"""Prepare one synthetic C2C QA launcher root. This module never starts a unit."""
import hashlib
import os
from pathlib import Path
import platform
import re
import stat
import sys


SOURCE_BASE = Path("/opt/bluey-jobs-c2c-launcher")
TEMP = Path("/tmp")
LAUNCHER_SHA = "8ad1d3f2d3976b88da64a55b1de9111997603115c7a56ae2d5d0a0afdcbb55c8"
GIT_HEAD = b"ref: refs/heads/bluey-c2c-qa\n"
GIT_CONFIG = b"[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tlogallrefupdates = false\n"
DIRECTORIES = (".git", ".git/objects", ".git/refs", ".git/refs/heads", "scripts")
DIR = os.O_RDONLY | os.O_NOFOLLOW | os.O_DIRECTORY | os.O_CLOEXEC
FILE = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC


def need(ok, label):
    if not ok:
        raise ValueError("c2c_launcher_install_refused:" + label)


def whole(pattern, value):
    return type(value) is str and re.fullmatch(pattern, value) is not None


def launcher_bytes(raw):
    need(type(raw) is bytes and 0 < len(raw) <= 256 << 10 and hashlib.sha256(raw).hexdigest() == LAUNCHER_SHA,
         "launcher_pin")
    return {".git/HEAD": (GIT_HEAD, 0o444), ".git/config": (GIT_CONFIG, 0o444),
            "scripts/run-bluey-tests.sh": (raw, 0o555)}


def stamp(row):
    return (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_gid, row.st_nlink,
            row.st_size, row.st_mtime_ns, row.st_ctime_ns)


def no_entry(parent, leaf):
    try:
        os.stat(leaf, dir_fd=parent, follow_symlinks=False)
    except FileNotFoundError:
        return
    raise ValueError("c2c_launcher_install_refused:already_present")


def write_new(parent, leaf, raw, mode):
    flags = os.O_WRONLY | os.O_NOFOLLOW | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC
    with os.fdopen(os.open(leaf, flags, 0o600, dir_fd=parent), "wb") as stream:
        need(stream.write(raw) == len(raw), "short_write")
        stream.flush()
        os.fchmod(stream.fileno(), mode)
        os.fsync(stream.fileno())


def verify_source(top, files):
    owner = os.getuid()
    expected_dirs = {".", *DIRECTORIES}
    seen_dirs, seen_files = set(), set()
    for relative, dirs, leaves, directory in os.fwalk(".", follow_symlinks=False, dir_fd=top):
        here = Path(relative)
        name = here.as_posix()
        info = os.fstat(directory)
        need(name in expected_dirs and name not in seen_dirs and info.st_uid == owner
             and stat.S_IMODE(info.st_mode) == 0o555, "source_directory")
        seen_dirs.add(name)
        for leaf in dirs:
            need((here / leaf).as_posix() in expected_dirs
                 and stat.S_ISDIR(os.stat(leaf, dir_fd=directory, follow_symlinks=False).st_mode), "source_subdir")
        for leaf in leaves:
            path = (here / leaf).as_posix()
            need(path in files and path not in seen_files, "source_leaf")
            expected, mode = files[path]
            with os.fdopen(os.open(leaf, FILE, dir_fd=directory), "rb") as stream:
                before = os.fstat(stream.fileno())
                need(stat.S_ISREG(before.st_mode) and before.st_uid == owner and before.st_nlink == 1
                     and stat.S_IMODE(before.st_mode) == mode and before.st_size == len(expected)
                     and stream.read(len(expected) + 1) == expected
                     and stamp(before) == stamp(os.fstat(stream.fileno()))
                     == stamp(os.stat(leaf, dir_fd=directory, follow_symlinks=False)), "source_bytes")
            seen_files.add(path)
        need(stamp(info) == stamp(os.fstat(directory)), "source_directory_changed")
    need(seen_dirs == expected_dirs and seen_files == files.keys(), "source_inventory")


def materialize_source(parent, run, raw):
    """An exact fresh leaf. May be exercised with portable private test FDs."""
    need(whole(r"[a-f0-9]{32}", run) and run != "0" * 32, "run")
    files = launcher_bytes(raw)
    no_entry(parent, run)
    os.mkdir(run, mode=0o700, dir_fd=parent)
    top = os.open(run, DIR, dir_fd=parent)
    try:
        original = os.fstat(top)
        need(original.st_uid == os.getuid() and stat.S_IMODE(original.st_mode) == 0o700
             and os.path.samestat(original, os.stat(run, dir_fd=parent, follow_symlinks=False)), "source_root")
        os.fsync(parent)
        held = {".": top}
        try:
            for name in DIRECTORIES:
                path = Path(name)
                base = held[path.parent.as_posix()]
                os.mkdir(path.name, mode=0o700, dir_fd=base)
                held[name] = os.open(path.name, DIR, dir_fd=base)
            for name, (body, mode) in files.items():
                path = Path(name)
                write_new(held[path.parent.as_posix()], path.name, body, mode)
            for fd in reversed(list(held.values())):
                os.fchmod(fd, 0o555)
                os.fsync(fd)
        finally:
            for key, fd in held.items():
                if key != ".":
                    os.close(fd)
        verify_source(top, files)
        need(os.path.samestat(original, os.fstat(top))
             and os.path.samestat(original, os.stat(run, dir_fd=parent, follow_symlinks=False)), "source_root_changed")
        return top
    except BaseException:
        os.close(top)
        raise


def platform_root():
    return (sys.platform == "linux" and platform.machine() == "x86_64" and os.getuid() == os.geteuid() == 0
            and all(os.path.samestat(os.stat(f"/proc/self/ns/{key}"), os.stat(f"/proc/1/ns/{key}"))
                    for key in ("mnt", "user", "pid")))


def install_launcher(run, reservation_sha256, uid, gid, run_parent, raw, *, admitted):
    """Root-only, never executable discovery, Git initialization or unit start."""
    need(platform_root() and callable(admitted) and whole(r"[a-f0-9]{32}", run) and run != "0" * 32
         and whole(r"[a-f0-9]{64}", reservation_sha256) and reservation_sha256 != "0" * 64
         and all(type(value) is int and 0 < value < 2**31 for value in (uid, gid))
         and whole(r"/tmp/bluey-test-runs-[a-f0-9]{12}", run_parent), "install_binding")
    files = launcher_bytes(raw)
    scope = {"run": run, "reservationSha256": reservation_sha256, "actorUid": uid, "actorGid": gid,
             "runParent": run_parent, "launcherSha256": LAUNCHER_SHA}
    need(admitted(dict(scope)) is True, "guest_admission")
    need(SOURCE_BASE.resolve(strict=True) == SOURCE_BASE and TEMP.resolve(strict=True) == TEMP, "base_path")
    for path in (SOURCE_BASE, *SOURCE_BASE.parents):
        info = path.lstat()
        need(stat.S_ISDIR(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o7022, "protected_parent")
    source_fd = os.open(SOURCE_BASE, DIR)
    temp_fd = root_fd = scratch_fd = -1
    try:
        temp_fd = os.open(TEMP, DIR)
        source, temp = os.fstat(source_fd), os.fstat(temp_fd)

        def current():
            if not platform_root() or admitted(dict(scope)) is not True:
                return False
            source_named, source_held = SOURCE_BASE.lstat(), os.fstat(source_fd)
            temp_named, temp_held = TEMP.lstat(), os.fstat(temp_fd)
            source_authority = (source.st_dev, source.st_ino, 0, 0, stat.S_IFDIR | 0o755)
            temp_authority = (temp.st_dev, temp.st_ino, 0, 0, stat.S_IFDIR | 0o1777)
            return (source_authority
                    == (source.st_dev, source.st_ino, source.st_uid, source.st_gid, source.st_mode)
                    == (source_named.st_dev, source_named.st_ino, source_named.st_uid,
                        source_named.st_gid, source_named.st_mode)
                    == (source_held.st_dev, source_held.st_ino, source_held.st_uid,
                        source_held.st_gid, source_held.st_mode)
                    and temp_authority
                    == (temp.st_dev, temp.st_ino, temp.st_uid, temp.st_gid, temp.st_mode)
                    == (temp_named.st_dev, temp_named.st_ino, temp_named.st_uid,
                        temp_named.st_gid, temp_named.st_mode)
                    == (temp_held.st_dev, temp_held.st_ino, temp_held.st_uid,
                        temp_held.st_gid, temp_held.st_mode))

        need(current(), "install_before")
        scratch = Path(run_parent).name
        no_entry(source_fd, run)
        no_entry(temp_fd, scratch)
        root_fd = materialize_source(source_fd, run, raw)
        need(current(), "scratch_admission")
        os.mkdir(scratch, mode=0o700, dir_fd=temp_fd)
        scratch_fd = os.open(scratch, DIR, dir_fd=temp_fd)
        first = os.fstat(scratch_fd)
        need(first.st_uid == 0 and stat.S_IMODE(first.st_mode) == 0o700
             and os.path.samestat(first, os.stat(scratch, dir_fd=temp_fd, follow_symlinks=False)), "scratch_name")
        os.fsync(temp_fd)
        os.fchown(scratch_fd, uid, gid)
        os.fsync(scratch_fd)
        need(current(), "install_after")
        verify_source(root_fd, files)
        final = os.fstat(scratch_fd)
        need((final.st_uid, final.st_gid) == (uid, gid) and stat.S_IMODE(final.st_mode) == 0o700
             and not os.listdir(scratch_fd) and os.path.samestat(final, first)
             and os.path.samestat(final, os.stat(scratch, dir_fd=temp_fd, follow_symlinks=False))
             and os.path.samestat(os.fstat(root_fd), os.stat(run, dir_fd=source_fd, follow_symlinks=False)), "install_readback")
        os.fsync(temp_fd)
    finally:
        for fd in (scratch_fd, root_fd, temp_fd, source_fd):
            if fd >= 0:
                os.close(fd)
    return {**scope, "sourceRoot": str(SOURCE_BASE / run), "launcherPath": str(SOURCE_BASE / run / "scripts/run-bluey-tests.sh"),
            "launchAdmitted": False, "runtimeQualified": False}
