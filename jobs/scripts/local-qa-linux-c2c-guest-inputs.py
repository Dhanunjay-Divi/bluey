"""Hold the exact read-only payload set for one disposable Jobs QA guest."""
from contextlib import ExitStack, contextmanager
import hashlib
import os
from pathlib import Path
import re
import stat
from types import MappingProxyType


BASE = Path("/var/lib/bluey-jobs-c2c-intake")
PINS = MappingProxyType({
    "launcher.sh": (94_631, "8ad1d3f2d3976b88da64a55b1de9111997603115c7a56ae2d5d0a0afdcbb55c8"),
    "browser-inventory.json": (70_130, "9ecdff4561fe0924e499325c40a6b61ffbe7ea047044d3f2cc0a89f82e728dce"),
    "playwright.tgz": (2_859_694, "3740bba3a3e93b4c7bd7c0f6e3c0e5f8cdce498bef13511e4d534cc99a8ce198"),
    "chrome.zip": (185_646_494, "13113b963ac22fffdad898a677591028e4397c46c1daa9e61811258eed6e35b5"),
    "node.tar.xz": (31_029_996, "1805bcd35e6815559bd2eb97db438d3fdb0eb3a5e6a977b7c1badca76337e9ce"),
    "native-receipt.json": (2_047, "7995aef600a191f0b02d3f11012a4f503adf4c32219a0fbd7c9cd40473930e3e"),
    "resolver.json": (507, "f6572c944641495b8933bbffd463ad1e773bd38c426cf3f73549945b3685d541"),
    "bluey-server": (111_198_656, "8adff1b0bf73d451bc58ad7e7050658693a3dbae2b4f379ea0c73779b906351f"),
    "jobs-nonprod-browser-fixture": (29_931_688, "0c4f7908d8f3fece7369921385a44a7f7023796ccfc19409e0042c324710fa94"),
    "portal.tar.gz": (1_508_201, "9b1090ef7e0a5fc98e1bcfeb34ed9cac4f7e42d7b09734400028f33a97ab3199"),
    "portal-manifest.json": (4_568, "1441b6884155c4d78d0ecfeeff7709ba020a70f584ed29cbd2c07f248cf20469"),
    "portal-receipt.json": (1_608, "3d88967ea03dfac2e7cc704d2857f985247087c6819071144f270d5f47fc3586"),
    "postgres.tar.gz": (22_422_977, "9a09356e9eb7ffa8ddf4e0dde66fbbb3c963ec1ce3e4b0c1a79a321cba94bdfa"),
    "postgres-files.json": (240_820, "e0f68a27c049e69aff5d789051ad731ddce5f41d120e46bdd21ba994bb46dc92"),
})
ARCHIVES = frozenset({"playwright.tgz", "chrome.zip", "node.tar.xz", "postgres.tar.gz"})
RD = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
DIR = RD | os.O_DIRECTORY


def need(value, label):
    if not value:
        raise ValueError("c2c_guest_inputs_refused:" + label)


def stamp(row):
    return (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_gid, row.st_nlink,
            row.st_size, row.st_mtime_ns, row.st_ctime_ns)


def digest_fd(fd, size, *, keep):
    """Hash exactly size bytes at offset zero. Never buffers path-only archives."""
    digest = hashlib.sha256()
    records = [] if keep else None
    offset = 0
    while offset < size:
        chunk = os.pread(fd, min(1 << 20, size - offset), offset)
        need(bool(chunk), "truncated")
        digest.update(chunk)
        offset += len(chunk)
        if keep:
            records.append(chunk)
    need(not os.pread(fd, 1, size), "trailing_bytes")
    return digest.hexdigest(), b"".join(records) if keep else None


def measure_fd(fd, name, *, keep=False):
    """Portable byte control. Live intake also requires root mode and named custody."""
    size, expected = PINS[name]
    before = os.fstat(fd)
    need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and before.st_size == size,
         "file_metadata")
    digest, raw = digest_fd(fd, size, keep=keep)
    need(digest == expected and stamp(before) == stamp(os.fstat(fd)), "file_pin")
    return stamp(before), raw


@contextmanager
def open_intake(run):
    """Hold one exact flat, root-private guest intake. Acquire nothing and write nothing."""
    need(type(run) is str and re.fullmatch(r"[0-9a-f]{32}", run) is not None and run != "0" * 32
         and os.getuid() == os.geteuid() == 0, "intake_scope")
    path = BASE / run
    need(path.resolve(strict=True) == path, "intake_path")
    with ExitStack() as stack:
        directory = os.open("/", DIR)
        stack.callback(os.close, directory)
        current = Path("/")
        parents = []
        for leaf in path.parts[1:]:
            directory = os.open(leaf, DIR, dir_fd=directory)
            stack.callback(os.close, directory)
            current /= leaf
            item = os.fstat(directory)
            need(stat.S_ISDIR(item.st_mode) and item.st_uid == item.st_gid == 0
                 and not item.st_mode & 0o7022 and os.path.samestat(item, current.lstat()),
                 "intake_ancestry")
            parents.append((current, directory, stamp(item)[:5]))
        top = os.fstat(directory)
        need(stat.S_IMODE(top.st_mode) == 0o700 and set(os.listdir(directory)) == PINS.keys(),
             "intake_inventory")
        files, payloads = {}, {}
        for name in PINS:
            fd = os.open(name, RD, dir_fd=directory)
            stack.callback(os.close, fd)
            item = os.fstat(fd)
            need(item.st_uid == item.st_gid == 0 and stat.S_IMODE(item.st_mode) == 0o400
                 and item.st_dev == top.st_dev and stamp(item) ==
                 stamp(os.stat(name, dir_fd=directory, follow_symlinks=False)), "intake_leaf")
            fingerprint, raw = measure_fd(fd, name, keep=name not in ARCHIVES)
            need(fingerprint == stamp(item), "intake_changed")
            files[name] = (fd, fingerprint)
            if raw is not None:
                payloads[name] = raw

        def current_intake():
            need(path.resolve(strict=True) == path and set(os.listdir(directory)) == PINS.keys()
                 and stamp(top) == stamp(os.fstat(directory)), "intake_changed")
            for named, held, identity in parents:
                need(stamp(named.lstat())[:5] == stamp(os.fstat(held))[:5] == identity,
                     "intake_ancestry_changed")
            for leaf, (held, fingerprint) in files.items():
                need(stamp(os.fstat(held)) ==
                     stamp(os.stat(leaf, dir_fd=directory, follow_symlinks=False)) == fingerprint,
                     "intake_leaf_changed")
            return True

        current_intake()
        yield (MappingProxyType({name: path / name for name in ARCHIVES}),
               MappingProxyType(payloads), current_intake)
        current_intake()
