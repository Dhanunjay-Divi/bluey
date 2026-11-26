"""Verify and stage the exact inert C2C PostgreSQL capsule.

This module never executes an archive member or starts PostgreSQL. The portable
materializer writes only beneath a caller-held directory descriptor; the public
installer additionally requires the initial Linux root namespace.
"""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import stat
import sys
import tarfile


POSTGRES_BASE = Path("/run/bluey-jobs-c2c-postgres")
ARCHIVE_SHA256 = "9a09356e9eb7ffa8ddf4e0dde66fbbb3c963ec1ce3e4b0c1a79a321cba94bdfa"
MANIFEST_SHA256 = "e0f68a27c049e69aff5d789051ad731ddce5f41d120e46bdd21ba994bb46dc92"
ARCHIVE_BYTES = 22_422_977
MANIFEST_BYTES = 240_820
MEMBERS = 1_640
REGULAR_FILES = 1_639
DIRECTORY_COUNT = 150
PAYLOAD_BYTES = 47_718_773
SONAME_LINK = "usr/lib/x86_64-linux-gnu/libpq.so.5"
SONAME_TARGET = "libpq.so.5.18"
MAX_FILE_BYTES = 24 << 20
CAPACITY_RESERVE = 64 << 20
DIR = os.O_RDONLY | os.O_NOFOLLOW | os.O_DIRECTORY | os.O_CLOEXEC
FILE = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
PATH = re.compile(r"[A-Za-z0-9_.+@%/-]{1,512}")
SHA = re.compile(r"[0-9a-f]{64}")
RUN = re.compile(r"[0-9a-f]{32}")


def need(ok, label):
    if not ok:
        raise ValueError("c2c_postgres_install_refused:" + label)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def file_stamp(info):
    return (
        info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid,
        info.st_nlink, info.st_size, info.st_mtime_ns, info.st_ctime_ns,
    )


def directory_authority(info):
    """Stable authority fields for a directory whose children may change."""
    return info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid


def object_authority(info):
    """Stable identity fields for a child whose mode and contents may change."""
    return info.st_dev, info.st_ino, info.st_uid, info.st_gid


def validate_manifest(raw):
    """Return the ordered, closed member map for the pinned ``files.json``."""
    need(type(raw) is bytes and len(raw) == MANIFEST_BYTES and sha(raw) == MANIFEST_SHA256,
         "manifest_pin")
    try:
        value = json.loads(raw.decode("ascii"))
    except (UnicodeError, ValueError, RecursionError) as error:
        raise ValueError("c2c_postgres_install_refused:manifest_json") from error
    canonical = (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
    need(type(value) is list and len(value) == MEMBERS and canonical == raw, "manifest_shape")
    rows = {}
    prior = ""
    regular = links = total = 0
    for entry in value:
        need(type(entry) is list and len(entry) in (4, 5) and type(entry[0]) is str,
             "manifest_row")
        name = entry[0]
        parts = name.split("/")
        need(PATH.fullmatch(name) is not None and name > prior and parts[0] == "usr"
             and all(part not in ("", ".", "..", ".git") for part in parts)
             and name not in rows, "manifest_path")
        if entry[1] == "file":
            need(len(entry) == 5 and type(entry[2]) is int and entry[2] in (0o444, 0o555)
                 and type(entry[3]) is int and 0 <= entry[3] <= MAX_FILE_BYTES
                 and type(entry[4]) is str and SHA.fullmatch(entry[4]) is not None,
                 "manifest_file")
            rows[name] = {"type": "file", "mode": entry[2], "size": entry[3],
                          "sha256": entry[4]}
            regular += 1
            total += entry[3]
        else:
            need(entry == [SONAME_LINK, "link", 0o777, SONAME_TARGET], "manifest_link")
            rows[name] = {"type": "link", "mode": 0o777, "target": SONAME_TARGET}
            links += 1
        prior = name
    directories = member_directories(rows)
    target = str(Path(SONAME_LINK).with_name(SONAME_TARGET))
    need(regular == REGULAR_FILES and links == 1 and total == PAYLOAD_BYTES
         and len(directories) == DIRECTORY_COUNT and target in rows
         and rows[target]["type"] == "file", "manifest_totals")
    return rows


def member_directories(rows):
    directories = {
        parent.as_posix()
        for name in rows
        for parent in Path(name).parents
        if parent.as_posix() != "."
    }
    need(len(directories) == DIRECTORY_COUNT and not directories.intersection(rows),
         "manifest_directories")
    return directories


def no_entry(parent, leaf):
    try:
        os.stat(leaf, dir_fd=parent, follow_symlinks=False)
    except FileNotFoundError:
        return
    raise ValueError("c2c_postgres_install_refused:already_present")


def write_all(descriptor, raw):
    view = memoryview(raw)
    while view:
        count = os.write(descriptor, view)
        need(count > 0, "write_progress")
        view = view[count:]


def write_new(parent, leaf, raw, mode):
    flags = os.O_WRONLY | os.O_NOFOLLOW | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC
    descriptor = os.open(leaf, flags, 0o600, dir_fd=parent)
    try:
        write_all(descriptor, raw)
        os.fchmod(descriptor, mode)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


@contextmanager
def pinned_archive(path):
    need(isinstance(path, Path) and path.is_absolute() and path.resolve(strict=True) == path,
         "archive_path")
    descriptor = os.open(path, FILE)
    try:
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            before = os.fstat(descriptor)
            named = path.lstat()
            need(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid()
                 and before.st_nlink == 1 and not before.st_mode & 0o022
                 and before.st_size == ARCHIVE_BYTES
                 and file_stamp(before) == file_stamp(named), "archive_metadata")
            digest = hashlib.sha256()
            remaining = ARCHIVE_BYTES
            while remaining:
                chunk = stream.read(min(remaining, 1 << 20))
                need(type(chunk) is bytes and 0 < len(chunk) <= remaining, "archive_truncated")
                digest.update(chunk)
                remaining -= len(chunk)
            need(stream.read(1) == b"" and digest.hexdigest() == ARCHIVE_SHA256,
                 "archive_pin")

            def unchanged():
                need(file_stamp(before) == file_stamp(os.fstat(descriptor))
                     == file_stamp(os.stat(path, follow_symlinks=False)), "archive_changed")

            unchanged()
            stream.seek(0)
            yield stream, unchanged
            unchanged()
    finally:
        os.close(descriptor)


def create_directories(top, directories):
    held = {".": top}
    try:
        for name in sorted(directories, key=lambda item: (item.count("/"), item)):
            path = Path(name)
            parent = held[path.parent.as_posix()]
            os.mkdir(path.name, mode=0o700, dir_fd=parent)
            held[name] = os.open(path.name, DIR, dir_fd=parent)
        return held
    except BaseException:
        for name, descriptor in held.items():
            if name != ".":
                os.close(descriptor)
        raise


def stream_file(source, size, expected_sha, target=-1):
    digest = hashlib.sha256()
    remaining = size
    while remaining:
        chunk = source.read(min(remaining, 1 << 20))
        need(type(chunk) is bytes and 0 < len(chunk) <= remaining, "member_truncated")
        digest.update(chunk)
        if target >= 0:
            write_all(target, chunk)
        remaining -= len(chunk)
    need(source.read(1) == b"" and digest.hexdigest() == expected_sha, "member_digest")


def scan_archive(stream, rows, folders=None):
    """Measure, and optionally write, every exact member without extraction APIs."""
    directories = member_directories(rows)
    expected = set(rows) | directories
    seen = set()
    total = regular = links = 0
    stream.seek(0)
    try:
        with tarfile.open(fileobj=stream, mode="r|gz") as archive:
            for item in archive:
                name = item.name
                need(type(name) is str and name in expected and name not in seen
                     and item.uid == item.gid == item.mtime == 0 and item.uname == item.gname == ""
                     and not item.pax_headers, "archive_member")
                seen.add(name)
                if name in directories:
                    need(item.type == tarfile.DIRTYPE and item.mode == 0o755 and item.size == 0
                         and item.linkname == "", "archive_directory")
                    continue
                row = rows[name]
                if row["type"] == "link":
                    need(item.type == tarfile.SYMTYPE and item.mode == 0o777 and item.size == 0
                         and item.linkname == SONAME_TARGET, "archive_link")
                    if folders is not None:
                        path = Path(name)
                        parent = folders[path.parent.as_posix()]
                        no_entry(parent, path.name)
                        os.symlink(SONAME_TARGET, path.name, dir_fd=parent)
                    links += 1
                    continue
                need(item.type == tarfile.REGTYPE and item.mode == row["mode"]
                     and item.size == row["size"] and item.linkname == "" and item.sparse is None,
                     "archive_file")
                source = archive.extractfile(item)
                need(source is not None, "archive_file_stream")
                with source:
                    target = -1
                    if folders is not None:
                        path = Path(name)
                        flags = os.O_WRONLY | os.O_NOFOLLOW | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC
                        target = os.open(path.name, flags, 0o600,
                                         dir_fd=folders[path.parent.as_posix()])
                    try:
                        stream_file(source, row["size"], row["sha256"], target)
                        if target >= 0:
                            os.fchmod(target, row["mode"])
                            os.fsync(target)
                    finally:
                        if target >= 0:
                            os.close(target)
                regular += 1
                total += row["size"]
                need(total <= PAYLOAD_BYTES, "archive_expansion")
    except (tarfile.TarError, EOFError) as error:
        raise ValueError("c2c_postgres_install_refused:archive_invalid") from error
    need(seen == expected and regular == REGULAR_FILES and links == 1
         and total == PAYLOAD_BYTES, "archive_inventory")
    return {"files": MEMBERS, "regularFiles": regular, "links": links,
            "directories": len(directories), "payloadBytes": total}


def receipt(measured, installed):
    return {"archiveSha256": ARCHIVE_SHA256, "manifestSha256": MANIFEST_SHA256,
            **measured, "installed": installed, "runtimeQualified": False}


def verify_archive(path, manifest_raw):
    """Read and hash the complete retained capsule without writing or executing."""
    rows = validate_manifest(manifest_raw)
    with pinned_archive(path) as (stream, unchanged):
        measured = scan_archive(stream, rows)
        unchanged()
    return receipt(measured, False)


def readback(top, rows, manifest_raw):
    expected_dirs = {".", *member_directories(rows)}
    expected_files = dict(rows)
    expected_files["files.json"] = {
        "type": "file", "mode": 0o444, "size": len(manifest_raw),
        "sha256": MANIFEST_SHA256,
    }
    seen_dirs, seen_files = set(), set()
    owner = os.getuid()
    for relative, children, leaves, descriptor in os.fwalk(
            ".", topdown=True, follow_symlinks=False, dir_fd=top):
        here = Path(relative)
        name = here.as_posix()
        before = os.fstat(descriptor)
        need(name in expected_dirs and name not in seen_dirs and before.st_uid == owner
             and stat.S_IMODE(before.st_mode) == 0o555, "stage_directory")
        seen_dirs.add(name)
        for child in children:
            info = os.stat(child, dir_fd=descriptor, follow_symlinks=False)
            need((here / child).as_posix() in expected_dirs and stat.S_ISDIR(info.st_mode)
                 and info.st_uid == owner, "stage_child")
        for leaf in leaves:
            path = (here / leaf).as_posix()
            need(path in expected_files and path not in seen_files, "stage_leaf")
            row = expected_files[path]
            info = os.stat(leaf, dir_fd=descriptor, follow_symlinks=False)
            if row["type"] == "link":
                need(stat.S_ISLNK(info.st_mode) and info.st_uid == owner and info.st_nlink == 1
                     and stat.S_IMODE(info.st_mode) == 0o777
                     and os.readlink(leaf, dir_fd=descriptor) == row["target"], "stage_link")
            else:
                with os.fdopen(os.open(leaf, FILE, dir_fd=descriptor), "rb") as source:
                    held = os.fstat(source.fileno())
                    need(stat.S_ISREG(held.st_mode) and held.st_uid == owner and held.st_nlink == 1
                         and stat.S_IMODE(held.st_mode) == row["mode"]
                         and held.st_size == row["size"] and file_stamp(held) == file_stamp(info),
                         "stage_file")
                    stream_file(source, row["size"], row["sha256"])
                    need(file_stamp(held) == file_stamp(os.fstat(source.fileno()))
                         == file_stamp(os.stat(leaf, dir_fd=descriptor, follow_symlinks=False)),
                         "stage_file_changed")
            seen_files.add(path)
        need(file_stamp(before) == file_stamp(os.fstat(descriptor)), "stage_directory_changed")
    need(seen_dirs == expected_dirs and seen_files == expected_files.keys(), "stage_inventory")


def materialize_postgres(parent, run, archive_path, manifest_raw, *, live_check):
    """Create one fresh, read-only capsule beneath a held private parent FD."""
    need(type(run) is str and RUN.fullmatch(run) is not None and run != "0" * 32
         and callable(live_check), "stage_binding")
    rows = validate_manifest(manifest_raw)
    parent_start = os.fstat(parent)
    need(stat.S_ISDIR(parent_start.st_mode) and parent_start.st_uid == os.getuid()
         and stat.S_IMODE(parent_start.st_mode) in (0o700, 0o755), "stage_parent")
    no_entry(parent, run)
    result = -1
    try:
        with pinned_archive(archive_path) as (stream, unchanged):
            measured = scan_archive(stream, rows)
            unchanged()
            need(live_check() is True and file_stamp(parent_start) == file_stamp(os.fstat(parent)),
                 "stage_admission")
            space = os.fstatvfs(parent)
            need(space.f_bavail * space.f_frsize
                 >= PAYLOAD_BYTES + MANIFEST_BYTES + CAPACITY_RESERVE, "stage_capacity")
            no_entry(parent, run)
            os.mkdir(run, mode=0o700, dir_fd=parent)
            os.fsync(parent)
            run_fd = os.open(run, DIR, dir_fd=parent)
            capsule_fd = -1
            folders = None
            try:
                run_info = os.fstat(run_fd)
                run_authority = object_authority(run_info)
                need(run_info.st_uid == os.getuid()
                     and stat.S_IMODE(run_info.st_mode) == 0o700
                     and os.path.samestat(
                         run_info, os.stat(run, dir_fd=parent, follow_symlinks=False)
                     ) and directory_authority(parent_start)
                     == directory_authority(os.fstat(parent)), "stage_run")
                os.mkdir("capsule", mode=0o700, dir_fd=run_fd)
                os.fsync(run_fd)
                capsule_fd = os.open("capsule", DIR, dir_fd=run_fd)
                capsule_info = os.fstat(capsule_fd)
                capsule_authority = object_authority(capsule_info)
                need(capsule_info.st_uid == os.getuid()
                     and stat.S_IMODE(capsule_info.st_mode) == 0o700
                     and os.path.samestat(
                         capsule_info,
                         os.stat("capsule", dir_fd=run_fd, follow_symlinks=False),
                     ) and object_authority(os.fstat(run_fd)) == run_authority,
                     "stage_capsule")
                folders = create_directories(capsule_fd, member_directories(rows))
                staged = scan_archive(stream, rows, folders)
                need(staged == measured, "stage_measurement")
                write_new(capsule_fd, "files.json", manifest_raw, 0o444)
                need(live_check() is True and directory_authority(parent_start)
                     == directory_authority(os.fstat(parent))
                     and object_authority(os.fstat(run_fd)) == run_authority
                     and object_authority(os.fstat(capsule_fd)) == capsule_authority,
                     "stage_admission_after_copy")
                for name in sorted((item for item in folders if item != "."),
                                   key=lambda item: (item.count("/"), item), reverse=True):
                    os.fchmod(folders[name], 0o555)
                    os.fsync(folders[name])
                os.fchmod(capsule_fd, 0o555)
                os.fsync(capsule_fd)
                os.fchmod(run_fd, 0o555)
                os.fsync(run_fd)
                for name, descriptor in list(folders.items()):
                    if name != ".":
                        os.close(descriptor)
                        del folders[name]
                readback(capsule_fd, rows, manifest_raw)
                unchanged()
                final_run = os.fstat(run_fd)
                final_capsule = os.fstat(capsule_fd)
                need(live_check() is True and directory_authority(parent_start)
                     == directory_authority(os.fstat(parent))
                     and object_authority(final_run) == run_authority
                     and object_authority(final_capsule) == capsule_authority
                     and stat.S_IMODE(final_run.st_mode)
                     == stat.S_IMODE(final_capsule.st_mode) == 0o555
                     and os.path.samestat(
                         final_run, os.stat(run, dir_fd=parent, follow_symlinks=False)
                     ) and os.path.samestat(
                         final_capsule,
                         os.stat("capsule", dir_fd=run_fd, follow_symlinks=False),
                     ) and os.listdir(run_fd) == ["capsule"], "stage_readback")
                os.fsync(parent)
                result = os.dup(capsule_fd)
            finally:
                if folders is not None:
                    for name, descriptor in folders.items():
                        if name != ".":
                            os.close(descriptor)
                if capsule_fd >= 0:
                    os.close(capsule_fd)
                os.close(run_fd)
        return result
    except BaseException:
        if result >= 0:
            os.close(result)
        raise


def platform_root():
    return (sys.platform == "linux" and platform.machine() == "x86_64"
            and os.getuid() == os.geteuid() == 0
            and all(os.path.samestat(os.stat("/proc/self/ns/" + kind), os.stat("/proc/1/ns/" + kind))
                    for kind in ("mnt", "pid", "user")))


def install_postgres(run, reservation_sha256, archive_path, manifest_raw, *, admitted):
    """Root-only installation of the exact inert capsule; no process launch."""
    need(platform_root() and callable(admitted) and type(run) is str
         and RUN.fullmatch(run) is not None and run != "0" * 32
         and type(reservation_sha256) is str and SHA.fullmatch(reservation_sha256) is not None
         and reservation_sha256 != "0" * 64, "root_admission")
    capsule = POSTGRES_BASE / run / "capsule"
    scope = {"run": run, "reservationSha256": reservation_sha256,
             "capsuleRoot": str(capsule), "archiveSha256": ARCHIVE_SHA256,
             "manifestSha256": MANIFEST_SHA256}
    need(POSTGRES_BASE.resolve(strict=True) == POSTGRES_BASE, "base_path")
    for path in (POSTGRES_BASE, *POSTGRES_BASE.parents):
        info = path.lstat()
        need(stat.S_ISDIR(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o7022,
             "base_ancestry")
    parent = os.open(POSTGRES_BASE, DIR)
    capsule_fd = -1
    try:
        initial = os.fstat(parent)

        def current():
            named = POSTGRES_BASE.lstat()
            held = os.fstat(parent)
            return (platform_root() and admitted(dict(scope)) is True
                    and initial.st_uid == 0 and stat.S_IMODE(initial.st_mode) == 0o755
                    and POSTGRES_BASE.resolve(strict=True) == POSTGRES_BASE
                    and directory_authority(initial) == directory_authority(named)
                    == directory_authority(held) and os.path.samestat(initial, named)
                    and os.path.samestat(initial, held))

        need(current(), "guest_admission")
        capsule_fd = materialize_postgres(
            parent, run, archive_path, manifest_raw, live_check=current
        )
    finally:
        if capsule_fd >= 0:
            os.close(capsule_fd)
        os.close(parent)
    return {**scope, "manifestPath": str(capsule / "files.json"), "files": MEMBERS,
            "directories": DIRECTORY_COUNT, "payloadBytes": PAYLOAD_BYTES,
            "installed": True, "runtimeQualified": False}
