"""Descriptor-relative verification of already installed immutable QA files.

Capture hashes each approved byte once before the waiting actor starts.
Recheck uses the full observed identities inside the actor's filesystem view.
Neither path creates files, issues a release, or starts a process.
"""
from contextlib import ExitStack, contextmanager
import hashlib
import os
from pathlib import PurePosixPath
import re
import stat
from types import MappingProxyType


DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
FILE = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
SHA = re.compile(r"[a-f0-9]{64}")
NAME = re.compile(r"[A-Za-z0-9._@%+,-]{1,255}")
# Match the deployed browser inventory's member bound. Chromium alone exceeds
# the smaller application-ELF ceiling; individual pinned lengths still apply.
MAX_FILE = 320 << 20
MAX_STATIC_FILE = 2 << 20
MAX_BYTES = 800 << 20
MAX_ENTRIES = 3_000


def need(value, label):
    if not value:
        raise ValueError("c2c_artifact_readback_refused:" + label)


def parts(value, *, absolute=False):
    need(type(value) is str and 0 < len(value) <= 512 and value.startswith("/") == absolute,
         "path_shape")
    raw = value.removeprefix("/") if absolute else value
    result = raw.split("/")
    need(0 < len(result) <= 32 and all(NAME.fullmatch(name) and name not in (".", "..")
                                         for name in result), "path_component")
    return tuple(result)


def stamp(row):
    return (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_gid,
            row.st_nlink, row.st_size, row.st_mtime_ns, row.st_ctime_ns)


def protected_directory(row, owner):
    return stat.S_ISDIR(row.st_mode) and row.st_uid == owner and not row.st_mode & 0o7022


@contextmanager
def directory_at(view, path, *, owner):
    """Follow no symlink below an already held filesystem root."""
    with ExitStack() as stack:
        fd = os.dup(view)
        stack.callback(os.close, fd)
        need(protected_directory(os.fstat(fd), owner), "view_directory")
        chain = []
        for name in parts(path, absolute=True):
            child = os.open(name, DIR, dir_fd=fd)
            stack.callback(os.close, child)
            held = os.fstat(child)
            named = os.stat(name, dir_fd=fd, follow_symlinks=False)
            need(protected_directory(held, owner) and stamp(held) == stamp(named), "path_custody")
            # Higher ancestors may get unrelated children on a running guest;
            # retain their exact identity, ownership and mode, not their times.
            chain.append((fd, name, child, stamp(held)[:5]))
            fd = child
        yield fd
        for parent, name, child, initial in chain:
            need(stamp(os.fstat(child))[:5] == stamp(os.stat(name, dir_fd=parent, follow_symlinks=False))[:5]
                 == initial, "path_changed")


def inventory(trees):
    """Make one closed tree map with canonical components and bounded size."""
    need(type(trees) is dict and 0 < len(trees) <= 8, "tree_shape")
    result, leaf_count, byte_limit = {}, 0, 0
    for path, (dirs, files) in trees.items():
        parts(path, absolute=True)
        need(type(dirs) is dict and type(files) is dict and "." in dirs
             and dirs.keys().isdisjoint(files) and files, "tree_entries")
        children = {name: {} for name in dirs}
        for rel, mode in dirs.items():
            need(type(mode) is int and mode in (0o555, 0o700, 0o755), "directory_mode")
            if rel == ".":
                continue
            name = parts(rel)
            parent = str(PurePosixPath(rel).parent)
            need(parent in children, "missing_parent")
            children[parent][name[-1]] = (rel, None)
        for rel, record in files.items():
            name = parts(rel)
            parent = str(PurePosixPath(rel).parent)
            need(type(record) is tuple and len(record) == 4 and parent in children, "member_shape")
            kind, mode, size, digest = record
            if kind == "file":
                need(type(mode) is int and mode in (0o400, 0o444, 0o500, 0o555, 0o600)
                     and (size is None or type(size) is int and 0 <= size <= MAX_FILE)
                     and type(digest) is str and SHA.fullmatch(digest), "file_spec")
                byte_limit += size if size is not None else MAX_STATIC_FILE
            else:
                need(kind == "link" and mode == 0o777 and size is None
                     and type(digest) is str and NAME.fullmatch(digest) and digest not in (".", ".."),
                     "link_spec")
                target = (PurePosixPath(rel).parent / digest).as_posix()
                need(target in files and files[target][0] == "file", "link_target")
            need(name[-1] not in children[parent], "duplicate_member")
            children[parent][name[-1]] = (rel, record)
        leaf_count += len(dirs) + len(files)
        need(leaf_count <= MAX_ENTRIES and byte_limit <= MAX_BYTES, "inventory_bounds")
        result[path] = (MappingProxyType(dict(dirs)), MappingProxyType({
            name: MappingProxyType(items) for name, items in children.items()}))
    return MappingProxyType(result)


def _file(fd, name, before, digest):
    with os.fdopen(os.open(name, FILE, dir_fd=fd), "rb") as stream:
        need(stamp(os.fstat(stream.fileno())) == stamp(before), "file_open_changed")
        expected_bytes = before.st_size
        checksum = hashlib.sha256()
        while expected_bytes:
            data = stream.read(min(expected_bytes, 1 << 20))
            need(bool(data), "file_short")
            checksum.update(data)
            expected_bytes -= len(data)
        need(not stream.read(1) and checksum.hexdigest() == digest
             and stamp(before) == stamp(os.fstat(stream.fileno())), "file_pin")


def _walk(fd, rel, modes, children, seen, baseline, owner):
    status = os.fstat(fd)
    key = stamp(status)
    need(protected_directory(status, owner) and stat.S_IMODE(status.st_mode) == modes[rel]
         and (baseline is None or baseline.get(rel) == key), "directory_fingerprint")
    names = children[rel]
    need(set(os.listdir(fd)) == names.keys(), "member_inventory")
    seen[rel] = key
    for name, (path, record) in names.items():
        before = os.stat(name, dir_fd=fd, follow_symlinks=False)
        item = stamp(before)
        if record is None:
            sub = os.open(name, DIR, dir_fd=fd)
            try:
                need(item == stamp(os.fstat(sub)), "child_directory_changed")
                _walk(sub, path, modes, children, seen, baseline, owner)
            finally:
                os.close(sub)
        else:
            kind, mode, size, digest = record
            need(before.st_uid == owner and stat.S_IMODE(before.st_mode) == mode and before.st_nlink == 1
                 and (baseline is None or baseline.get(path) == item), "leaf_fingerprint")
            if kind == "link":
                need(stat.S_ISLNK(before.st_mode) and os.readlink(name, dir_fd=fd) == digest, "link_target")
            else:
                need(stat.S_ISREG(before.st_mode)
                     and (0 <= before.st_size <= MAX_STATIC_FILE if size is None else before.st_size == size),
                     "file_metadata")
                if baseline is None:
                    _file(fd, name, before, digest)
            seen[path] = item
        need(item == stamp(os.stat(name, dir_fd=fd, follow_symlinks=False)), "member_changed")
    need(key == stamp(os.fstat(fd)) and set(os.listdir(fd)) == names.keys(), "directory_changed")


def measure(view, layout, *, baseline=None, owner=0):
    """Hash or compare every expected entry using the same exact directory walk."""
    need(baseline is None or baseline.keys() == layout.keys(), "baseline_shape")
    result = {}
    for path, (modes, children) in layout.items():
        with directory_at(view, path, owner=owner) as fd:
            seen = {}
            old = None if baseline is None else baseline[path]
            _walk(fd, ".", modes, children, seen, old, owner)
            need(old is None or seen == old, "tree_changed")
            result[path] = MappingProxyType(seen)
    return MappingProxyType(result)


def plan_leaf(view, path, expected, *, baseline=None, owner=0):
    """Check plan.json alone; the publisher owns other names in its gate."""
    need(type(expected) is bytes and 0 < len(expected) <= 4096, "plan_bytes")
    with directory_at(view, path, owner=owner) as fd:
        status = os.fstat(fd)
        before = os.stat("plan.json", dir_fd=fd, follow_symlinks=False)
        anchor = (status.st_dev, status.st_ino, status.st_mode, status.st_uid, status.st_gid)
        result = (anchor, stamp(before))
        need(stat.S_IMODE(status.st_mode) == 0o755 and stat.S_ISREG(before.st_mode)
             and before.st_uid == owner and before.st_nlink == 1
             and stat.S_IMODE(before.st_mode) == 0o444 and before.st_size == len(expected)
             and (baseline is None or baseline == result), "plan_identity")
        if baseline is None:
            _file(fd, "plan.json", before, hashlib.sha256(expected).hexdigest())
        need(stamp(before) == stamp(os.stat("plan.json", dir_fd=fd, follow_symlinks=False)), "plan_changed")
        return result
