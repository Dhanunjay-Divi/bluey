"""Stage a selected, verified GitHub authorization ZIP as inert files."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import stat
import struct
import sys
import zipfile
import zlib

COMPONENTS = ("jobs-api", "jobs-portal", "jobs-runner", "jobs-workflows")
CANDIDATE = (
    "builder-policy.json", "candidate-set.json", "component-inventory.json",
    "config-contract.json", "inventory-attachments.json", "migration-contract.json",
    "protocol-contract.json", "release-manifest.json", "test-evidence.json",
    "verification-evidence.json", "evidence/build/jobs-package-lock.json",
    "evidence/build/server-cargo-lock",
)
ARCHIVE_FILES = {
    "authorization-set.json", "release-signatures.json", "trust-bundle.json",
    *(f"candidate/{name}" for name in CANDIDATE),
    *(f"candidate/evidence/{component}.{kind}.json"
      for component in COMPONENTS for kind in ("content-inventory", "provenance", "sbom")),
    *(f"candidate/payloads/{component}{'.tar' if component == 'jobs-portal' else '.oci.tar'}"
      for component in COMPONENTS),
}


def implied_directories(files):
    return {parent.as_posix() for name in files for parent in Path(name).parents} - {"."}


# Candidate is deliberately derived from the authorization structure, rather than
# duplicating its 28 names in a second hand-maintained allowlist.
CANDIDATE_ARCHIVE_FILES = {
    name.removeprefix("candidate/") for name in ARCHIVE_FILES if name.startswith("candidate/")
}
ARCHIVE_DIRECTORIES = implied_directories(ARCHIVE_FILES)
CANDIDATE_ARCHIVE_DIRECTORIES = implied_directories(CANDIDATE_ARCHIVE_FILES)
STAGE_KINDS = {
    "authorized": (ARCHIVE_FILES, ARCHIVE_DIRECTORIES, "authorized"),
    "candidate": (CANDIDATE_ARCHIVE_FILES, CANDIDATE_ARCHIVE_DIRECTORIES, "candidate"),
}
MAX_CENTRAL_BYTES = 32 * 1024
CHUNK = 1024 * 1024
MAX_INTEGER = 2**53 - 1
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
READ_FLAGS = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
WRITE_FLAGS = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC


class Refused(RuntimeError):
    """Closed staging refusal."""


def require(ok, reason):
    if not ok:
        raise Refused(reason)


def stamp(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_mode, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def bounded_int(value, minimum, maximum=MAX_INTEGER):
    return type(value) is int and minimum <= value <= maximum


def disk_room(fd):
    status = os.fstatvfs(fd)
    return status.f_bavail * status.f_frsize


def stream_bytes(source, length, target=None, room=None):
    digest = hashlib.sha256()
    remaining = length
    while remaining:
        data = source.read(min(CHUNK, remaining))
        require(0 < len(data) <= remaining, "unexpected stream size")
        digest.update(data)
        if target is not None:
            room(len(data))
            require(target.write(data) == len(data), "short staging write")
        remaining -= len(data)
    require(not source.read(1), "unexpected trailing stream data")
    return digest.hexdigest()


def read_record(stream, offset, pattern):
    record = struct.Struct(pattern)
    require(offset >= 0, "invalid ZIP record offset")
    stream.seek(offset)
    raw = stream.read(record.size)
    require(len(raw) == record.size, "incomplete ZIP record")
    return record.unpack(raw)


def bound_zip_directory(stream, length, archive_files, archive_directories):
    # Qualify the physical directory budget before ZipFile reads its bytes.
    end = length - 22
    signature, disk, disk_start, local, count, size, offset, comment = read_record(
        stream, end, "<4s4H2IH",
    )
    require(signature == b"PK\x05\x06" and disk == disk_start == comment == 0
            and local == count, "expected a single-disk ZIP without a comment")
    directory_end = end
    locator_start = end - 20
    if locator_start >= 0:
        marker, locator_disk, zip64_start, disks = read_record(stream, locator_start, "<4sIQI")
        if marker == b"PK\x06\x07":
            require(locator_disk == 0 and disks == 1 and zip64_start + 56 == locator_start,
                    "invalid ZIP64 locator")
            marker, record_size, _, _, disk, disk_start, local64, count64, size64, offset64 = read_record(
                stream, zip64_start, "<4sQ2H2I4Q",
            )
            require(marker == b"PK\x06\x06" and record_size == 44 and disk == disk_start == 0
                    and local64 == count64 and count in (count64, 0xFFFF)
                    and size in (size64, 0xFFFFFFFF) and offset in (offset64, 0xFFFFFFFF),
                    "invalid ZIP64 directory")
            count, size, offset, directory_end = count64, size64, offset64, zip64_start
    maximum_entries = len(archive_files) + len(archive_directories)
    require(len(archive_files) <= count <= maximum_entries and 0 < size <= MAX_CENTRAL_BYTES
            and 0 < offset < directory_end and offset + size == directory_end,
            "invalid ZIP central directory bounds")
    return count, offset


def validated_members(archive, entries, data_end, max_unpacked_bytes, archive_files, archive_directories):
    rows = archive.infolist()
    require(len(rows) == entries and archive.start_dir == data_end, "ZIP directory shape changed")
    paths, offsets, files = set(), set(), {}
    total = 0
    for entry in rows:
        name = entry.filename.removesuffix("/")
        mode = stat.S_IFMT(entry.external_attr >> 16)
        require(entry.filename == entry.orig_filename and name not in paths
                and 0 <= entry.header_offset < data_end and entry.header_offset not in offsets
                and not entry.flag_bits & 1
                and entry.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED),
                "unexpected ZIP entry metadata")
        paths.add(name)
        offsets.add(entry.header_offset)
        if entry.is_dir():
            require(name in archive_directories and mode in (0, stat.S_IFDIR) and entry.file_size == 0,
                    "unexpected ZIP directory")
            continue
        require(name in archive_files and mode in (0, stat.S_IFREG)
                and 0 < entry.file_size <= max_unpacked_bytes
                and 0 < entry.compress_size <= data_end - entry.header_offset,
                "unexpected ZIP file")
        total += entry.file_size
        require(total <= max_unpacked_bytes, "unpacked byte budget exceeded")
        files[name] = entry
    require(files.keys() == archive_files and min(offsets) == 0, "authorization ZIP file set is incomplete")
    return files, total


def inode(info):
    return (info.st_dev, info.st_ino)


def directory_identity(info):
    return f"{info.st_dev}:{info.st_ino}:{info.st_mode}:{info.st_uid}"


def discard(parent, parent_identity, attempt, attempt_identity, staged, staged_identity, kind="authorized"):
    require(kind in STAGE_KINDS, "invalid authorization artifact kind")
    archive_files, archive_directories, root_prefix = STAGE_KINDS[kind]
    require(parent.is_absolute() and parent.resolve(strict=True) == parent
            and re.fullmatch(r"intake-[a-zA-Z0-9]{6}", attempt)
            and ((not staged and not staged_identity) or re.fullmatch(rf"{root_prefix}-[0-9a-f]{{24}}", staged)),
            "invalid authorization cleanup scope")
    with ExitStack() as stack:
        def hold(container, name, expected=None):
            fd = os.open(name, DIR_FLAGS, dir_fd=container)
            stack.callback(os.close, fd)
            info = os.fstat(fd)
            require(info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700
                    and (expected is None or directory_identity(info) == expected)
                    and inode(info) == inode(os.stat(name, dir_fd=container, follow_symlinks=False)),
                    "authorization cleanup directory changed")
            return fd, inode(info)

        def remove_empty(container, name, fd, expected):
            require(not os.listdir(fd)
                    and inode(os.stat(name, dir_fd=container, follow_symlinks=False)) == expected,
                    "authorization cleanup directory has unexpected residue")
            os.rmdir(name, dir_fd=container)

        parent_fd, _ = hold(None, parent, parent_identity)
        attempt_fd, attempt_inode = hold(parent_fd, attempt, attempt_identity)
        require(set(os.listdir(attempt_fd)) == ({staged} if staged else set()),
                "authorization cleanup attempt has unexpected entries")
        if staged:
            root_fd, root_inode = hold(attempt_fd, staged, staged_identity)
            folders = {"": root_fd}
            inventory = {root_fd: set()}
            directories = []
            files = []
            for path in sorted(archive_directories, key=lambda name: (name.count("/"), name)):
                directory, _, name = path.rpartition("/")
                container = folders[directory]
                fd, original = hold(container, name)
                folders[path] = fd
                inventory[container].add(name)
                inventory[fd] = set()
                directories.append((container, name, fd, original))
            for path in sorted(archive_files):
                directory, _, name = path.rpartition("/")
                container = folders[directory]
                info = os.stat(name, dir_fd=container, follow_symlinks=False)
                require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
                        and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1,
                        "authorization cleanup file has unexpected metadata")
                files.append((container, name, stamp(info)))
                inventory[container].add(name)
            require(all(set(os.listdir(fd)) == names for fd, names in inventory.items()),
                    "authorization cleanup inventory changed")
            quarantine = "discard-" + secrets.token_hex(12)
            os.mkdir(quarantine, mode=0o700, dir_fd=attempt_fd)
            quarantined_fd, quarantine_inode = hold(attempt_fd, quarantine)
            for container, name, original in files:
                current = os.stat(name, dir_fd=container, follow_symlinks=False)
                require(stamp(current) == original, "authorization cleanup file changed")
                leaf = secrets.token_hex(12)
                os.rename(name, leaf, src_dir_fd=container, dst_dir_fd=quarantined_fd)
                moved = os.stat(leaf, dir_fd=quarantined_fd, follow_symlinks=False)
                require(stamp(moved)[:-1] == original[:-1], "authorization cleanup refused substituted file")
                os.unlink(leaf, dir_fd=quarantined_fd)
            for entry in reversed(directories):
                remove_empty(*entry)
            remove_empty(attempt_fd, staged, root_fd, root_inode)
            remove_empty(attempt_fd, quarantine, quarantined_fd, quarantine_inode)
        remove_empty(parent_fd, attempt, attempt_fd, attempt_inode)


def materialize(archive, files, parent, parent_fd, reserve_bytes, unchanged, archive_directories, root_prefix):
    created = []
    inventory = {}

    def make_directory(container, name):
        os.mkdir(name, mode=0o700, dir_fd=container)
        info = os.stat(name, dir_fd=container, follow_symlinks=False)
        created.append((container, name, inode(info), True))
        fd = os.open(name, DIR_FLAGS, dir_fd=container)
        stack.callback(os.close, fd)
        require(inode(os.fstat(fd)) == inode(info) and info.st_uid == os.getuid()
                and stat.S_IMODE(info.st_mode) == 0o700, "staging directory identity changed")
        inventory[fd] = set()
        if container in inventory:
            inventory[container].add(name)
        return fd

    def require_room(amount):
        require(disk_room(parent_fd) >= reserve_bytes + amount, "staging disk headroom fell below reserve")

    with ExitStack() as stack:
        try:
            leaf = root_prefix + "-" + secrets.token_hex(12)
            root_fd = make_directory(parent_fd, leaf)
            folders = {"": root_fd}
            for name in sorted(archive_directories, key=lambda path: (path.count("/"), path)):
                directory, _, child = name.rpartition("/")
                folders[name] = make_directory(folders[directory], child)
            for name, entry in sorted(files.items()):
                directory, _, child = name.rpartition("/")
                container = folders[directory]
                with archive.open(entry) as source:
                    with os.fdopen(os.open(child, WRITE_FLAGS, 0o600, dir_fd=container), "wb") as out:
                        info = os.fstat(out.fileno())
                        created.append((container, child, inode(info), False))
                        inventory[container].add(child)
                        stream_bytes(source, entry.file_size, out, require_room)
                        out.flush()
                        after = os.fstat(out.fileno())
                        require(after.st_size == entry.file_size and after.st_uid == os.getuid()
                                and stat.S_IMODE(after.st_mode) == 0o600 and after.st_nlink == 1
                                and stamp(after) == stamp(os.stat(child, dir_fd=container, follow_symlinks=False)),
                                "staged file changed")
            unchanged()
            require(inode(os.fstat(parent_fd)) == inode(os.stat(parent, follow_symlinks=False)),
                    "staging parent changed")
            for container, name, identity, directory in created:
                require(inode(os.stat(name, dir_fd=container, follow_symlinks=False)) == identity,
                        "staging tree identity changed")
            for fd, names in inventory.items():
                require(set(os.listdir(fd)) == names, "staging file inventory changed")
            require_room(0)
            return parent / leaf
        except BaseException:
            # Delete only the exact children created here, through held parent FDs.
            # On substitution or unexpected residue, stop and preserve evidence.
            for container, name, identity, directory in reversed(created):
                require(inode(os.stat(name, dir_fd=container, follow_symlinks=False)) == identity,
                        "staging cleanup refused changed entry")
                if directory:
                    os.rmdir(name, dir_fd=container)
                else:
                    os.unlink(name, dir_fd=container)
            raise


def stage(archive_path, parent, size, digest, max_unpacked_bytes, reserve_bytes, kind="authorized"):
    require(kind in STAGE_KINDS, "invalid authorization artifact kind")
    archive_files, archive_directories, root_prefix = STAGE_KINDS[kind]
    require(bounded_int(size, 22) and re.fullmatch(r"sha256:[0-9a-f]{64}", digest)
            and bounded_int(max_unpacked_bytes, 1) and bounded_int(reserve_bytes, 0),
            "invalid trusted archive bounds")
    require(archive_path.is_absolute() and archive_path.resolve(strict=True) == archive_path
            and parent.is_absolute() and parent.resolve(strict=True) == parent,
            "paths must be physical absolute paths")
    parent_fd = os.open(parent, DIR_FLAGS)
    try:
        directory = os.fstat(parent_fd)
        require(directory.st_uid == os.getuid() and stat.S_IMODE(directory.st_mode) == 0o700,
                "expected a private staging parent")
        with os.fdopen(os.open(archive_path, READ_FLAGS), "rb") as stream:
            original = os.fstat(stream.fileno())
            require(stat.S_ISREG(original.st_mode) and original.st_uid == os.getuid()
                    and original.st_nlink == 1 and original.st_size == size and not original.st_mode & 0o022,
                    "wrong archive type, mode, owner or size")

            def unchanged():
                require(stamp(original) == stamp(os.fstat(stream.fileno()))
                        == stamp(os.stat(archive_path, follow_symlinks=False)), "archive changed during staging")

            require("sha256:" + stream_bytes(stream, size) == digest, "archive digest mismatch")
            unchanged()
            entries, data_end = bound_zip_directory(stream, size, archive_files, archive_directories)
            with zipfile.ZipFile(stream) as archive:
                files, unpacked = validated_members(
                    archive, entries, data_end, max_unpacked_bytes, archive_files, archive_directories
                )
                require(disk_room(parent_fd) >= unpacked + reserve_bytes + CHUNK,
                        "insufficient staging disk headroom")
                root = materialize(
                    archive, files, parent, parent_fd, reserve_bytes, unchanged,
                    archive_directories, root_prefix,
                )
    finally:
        os.close(parent_fd)
    return {"root": str(root), "archiveDigest": digest, "archiveSizeBytes": size,
            "fileCount": len(files), "unpackedBytes": unpacked}


def main():
    parser = argparse.ArgumentParser(allow_abbrev=False)
    if sys.argv[1:2] == ["discard"]:
        parser.add_argument("--parent", type=Path, required=True)
        parser.add_argument("--parent-identity", required=True)
        parser.add_argument("--attempt", required=True)
        parser.add_argument("--attempt-identity", required=True)
        parser.add_argument("--staged", required=True)
        parser.add_argument("--staged-identity", required=True)
        parser.add_argument("--kind", choices=tuple(STAGE_KINDS), default="authorized")
        options = parser.parse_args(sys.argv[2:])
        try:
            discard(options.parent, options.parent_identity, options.attempt,
                    options.attempt_identity, options.staged, options.staged_identity, options.kind)
        except (OSError, Refused):
            parser.exit(1, "authorization archive cleanup refused; evidence retained\n")
        return
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--parent", type=Path, required=True)
    parser.add_argument("--size", type=int, required=True)
    parser.add_argument("--digest", required=True)
    parser.add_argument("--max-unpacked-bytes", type=int, required=True)
    parser.add_argument("--reserve-bytes", type=int, required=True)
    parser.add_argument("--kind", choices=tuple(STAGE_KINDS), default="authorized")
    options = parser.parse_args()
    try:
        result = stage(options.archive, options.parent, options.size, options.digest,
                       options.max_unpacked_bytes, options.reserve_bytes, options.kind)
    except Refused as error:
        parser.exit(1, f"authorization archive staging refused: {error}\n")
    except (OSError, ValueError, RuntimeError, EOFError, zipfile.BadZipFile, zlib.error) as error:
        # Do not echo member names or other untrusted archive diagnostics.
        parser.exit(1, f"authorization archive staging refused: {type(error).__name__}\n")
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
