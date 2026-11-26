"""Measure or stage the exact f9 Linux browser tools without executing them."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tarfile
import zipfile

if sys.platform not in {"darwin", "linux"}:
    raise SystemExit("browser runtime custody refused: unsupported_platform")

INTAKE_SHA256 = "07426bad5f55f7fe3ea99ccab025571ffabffe310e204c7cf64ec4c20dd682dc"
TREE = "f9b3cf55de39cd0c86a50b5de7546008bbd49210"
ARCHIVES = {
    "playwright": (2_859_694, "3740bba3a3e93b4c7bd7c0f6e3c0e5f8cdce498bef13511e4d534cc99a8ce198"),
    "chrome": (185_646_494, "13113b963ac22fffdad898a677591028e4397c46c1daa9e61811258eed6e35b5"),
    "node": (31_029_996, "1805bcd35e6815559bd2eb97db438d3fdb0eb3a5e6a977b7c1badca76337e9ce"),
}
LEAF = "receipt-offline-runtime-inert"
MIN_FREE_AFTER_STAGE = 16 * 1024**3
NODE_MEMBER = "node-v22.23.4-linux-x64/bin/node"
PW_PREFIX = "receipt-prep-browser/node_modules/playwright-core"
CHROME_PREFIX = "receipt-prep-browser/chromium"
NODE_PATH = "receipt-journey-node/toolchain/bin/node"
REQUIRED = {f"{PW_PREFIX}/index.mjs", f"{PW_PREFIX}/package.json", f"{PW_PREFIX}/browsers.json",
            f"{CHROME_PREFIX}/chrome", NODE_PATH}
ELF = {f"{CHROME_PREFIX}/chrome", NODE_PATH}
FILE_FLAGS = os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
NAME = re.compile(r"[A-Za-z0-9._@+/-]{1,240}")


class Refused(RuntimeError):
    pass


def require(ok, reason):
    if not ok:
        raise Refused(reason)


def identity(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_mode, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def file_digest(stream, size, target=None, elf=False):
    digest = hashlib.sha256()
    remaining = size
    first = True
    while remaining:
        data = stream.read(min(remaining, 1024 * 1024))
        require(0 < len(data) <= remaining, "member_size")
        if first and elf:
            require(data.startswith(b"\x7fELF\x02\x01\x01") and data[18:20] == b"\x3e\x00",
                    "member_architecture")
        first = False
        digest.update(data)
        if target is not None:
            require(target.write(data) == len(data), "stage_short_write")
        remaining -= len(data)
    require(not stream.read(1) and not (first and elf), "member_size")
    return digest.hexdigest()


def open_verified(stack, path, expected):
    require(path.is_absolute() and path.resolve(strict=True) == path, "physical_input")
    fd = os.open(path, os.O_RDONLY | FILE_FLAGS)
    stream = stack.enter_context(os.fdopen(fd, "rb"))
    before = os.fstat(fd)
    size, digest = expected
    require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and before.st_size == size,
            "archive_type_or_size")

    def unchanged():
        require(identity(before) == identity(os.fstat(fd))
                == identity(os.stat(path, follow_symlinks=False)), "archive_changed")

    require(file_digest(stream, size) == digest, "archive_digest")
    unchanged()
    stream.seek(0)
    return stream, unchanged


def relative(name, prefix):
    require(NAME.fullmatch(name) and not {"", ".", "..", ".git"}.intersection(name.split("/"))
            and name.startswith(prefix + "/"), "member_path")
    return name[len(prefix) + 1:]


def browser_members(stream):
    with zipfile.ZipFile(stream) as archive:
        entries = archive.infolist()
        require(len(entries) == 308 and sum(m.file_size for m in entries) == 396_335_288,
                "chrome_inventory")
        seen = set()
        for member in entries:
            name = member.filename.removesuffix("/")
            suffix = relative(name, "chrome-linux64")
            require(name not in seen and member.filename == member.orig_filename, "member_duplicate")
            seen.add(name)
            mode = member.external_attr >> 16
            if member.is_dir():
                require(member.file_size == 0 and mode == (stat.S_IFDIR | 0o755), "chrome_directory")
                continue
            require(stat.S_ISREG(mode) and stat.S_IMODE(mode) in {0o600, 0o644, 0o755},
                    "chrome_member_type")
            with archive.open(member) as source:
                yield f"{CHROME_PREFIX}/{suffix}", member.file_size, stat.S_IMODE(mode), source


def tar_members(stream, node):
    with tarfile.open(fileobj=stream, mode="r|xz" if node else "r|gz") as archive:
        for member in archive:
            if node and member.name != NODE_MEMBER:
                continue
            target = NODE_PATH if node else f"{PW_PREFIX}/{relative(member.name, 'package')}"
            require(member.type == tarfile.REGTYPE and not member.linkname
                    and member.mode in {0o644, 0o755}, "tar_member_type")
            if node:
                require(member.mode == 0o755 and member.size == 124_902_584, "node_member_identity")
            with archive.extractfile(member) as source:
                yield target, member.size, member.mode, source


def readback(top_fd, rows):
    expected = {row[0]: row for row in rows}
    expected_dirs = {parent.as_posix() for name in expected for parent in Path(name).parents}
    seen = set()
    seen_dirs = set()
    for directory, dirs, names, fd in os.fwalk(".", follow_symlinks=False, dir_fd=top_fd):
        where = Path(directory)
        seen_dirs.add(where.as_posix())
        for subdir in dirs:
            info = os.stat(subdir, dir_fd=fd, follow_symlinks=False)
            require((where / subdir).as_posix() in expected_dirs and stat.S_ISDIR(info.st_mode)
                    and info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700,
                    "stage_directory_inventory")
        for filename in names:
            name = (where / filename).as_posix()
            require(name in expected and name not in seen, "stage_file_inventory")
            _, digest, size, _ = expected[name]
            with os.fdopen(os.open(filename, os.O_RDONLY | FILE_FLAGS, dir_fd=fd), "rb") as source:
                info = os.fstat(source.fileno())
                require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
                        and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1
                        and info.st_size == size and file_digest(source, size) == digest,
                        "stage_file_readback")
                require(identity(info) == identity(os.fstat(source.fileno()))
                        == identity(os.stat(filename, dir_fd=fd, follow_symlinks=False)),
                        "stage_file_changed")
            seen.add(name)
    require(seen == expected.keys() and seen_dirs == expected_dirs, "stage_missing_member")


def consume(groups, top_fd=None):
    rows = {}
    with contextlib.ExitStack() as stack:
        folders = {"": top_fd}
        for group, files, size_expected in groups:
            count = size_total = 0
            for name, size, mode, source in group:
                count += 1
                size_total += size
                require(name not in rows and size >= 0 and count <= files
                        and size_total <= size_expected, "member_inventory")
                if top_fd is None:
                    digest = file_digest(source, size, elf=name in ELF)
                else:
                    parent_parts = name.split("/")[:-1]
                    for index, part in enumerate(parent_parts):
                        path = "/".join(parent_parts[:index + 1])
                        if path not in folders:
                            parent = folders["/".join(parent_parts[:index])]
                            os.mkdir(part, mode=0o700, dir_fd=parent)
                            fd = os.open(part, DIR_FLAGS, dir_fd=parent)
                            stack.callback(os.close, fd)
                            folders[path] = fd
                    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | FILE_FLAGS
                    base = name.rsplit("/", 1)[1]
                    fd = os.open(base, flags, 0o600, dir_fd=folders["/".join(parent_parts)])
                    with os.fdopen(fd, "wb") as target:
                        digest = file_digest(source, size, target, elf=name in ELF)
                rows[name] = (name, digest, size, mode)
            require(count == files and size_total == size_expected, "member_inventory")
        require(REQUIRED <= rows.keys(), "required_runtime_entry")
    return [rows[name] for name in sorted(rows)]


def running_root(stack):
    helper = Path(__file__).resolve().with_name("stage-browser-receipt-intake.py")
    with helper.open("rb") as stream:
        body = stream.read(32 * 1024)
    require(hashlib.sha256(body).hexdigest() == INTAKE_SHA256, "intake_helper_digest")
    contract = {"__name__": "verified_intake_contract"}
    exec(compile(body, str(helper), "exec"), contract)
    root = Path(os.environ.get("BLUEY_TEST_RUN_ROOT", ""))
    contract["physical_directory"](root)
    fd = os.open(root, DIR_FLAGS)
    stack.callback(os.close, fd)
    original = os.fstat(fd)
    require(original.st_uid == os.getuid() and stat.S_IMODE(original.st_mode) == 0o700,
            "private_run_root")
    group = os.getpgrp()
    marker = contract["marker_for"](root, group)

    def recheck():
        info = os.stat(root, follow_symlinks=False)
        require((info.st_dev, info.st_ino, info.st_uid, info.st_mode)
                == (original.st_dev, original.st_ino, original.st_uid, original.st_mode)
                and contract["marker_for"](root, group) == marker, "run_root_changed")

    recheck()
    return root, fd, recheck


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("verify", "stage-inert"))
    for name in ARCHIVES:
        parser.add_argument(name, type=Path)
    parser.add_argument("--inventory", action="store_true")
    args = parser.parse_args()
    os.umask(0o077)
    with contextlib.ExitStack() as stack:
        verified = {name: open_verified(stack, getattr(args, name), spec)
                    for name, spec in ARCHIVES.items()}
        playwright, chrome, node = (verified[name][0] for name in ARCHIVES)
        groups = [(tar_members(playwright, False), 106, 12_701_224),
                  (browser_members(chrome), 303, 396_335_288),
                  (tar_members(node, True), 1, 124_902_584)]
        top = None
        if args.mode == "stage-inert":
            root, parent, recheck = running_root(stack)
            space = os.fstatvfs(parent)
            require(space.f_frsize * space.f_bavail >= MIN_FREE_AFTER_STAGE
                    + sum(size for _, _, size in groups), "stage_disk_floor")
            os.mkdir(LEAF, mode=0o700, dir_fd=parent)
            top = os.open(LEAF, DIR_FLAGS, dir_fd=parent)
            stack.callback(os.close, top)
        rows = consume(groups, top)
        if top is not None:
            recheck()
            info = os.fstat(top)
            require((info.st_dev, info.st_ino) == inode(root / LEAF)
                    and info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700,
                    "stage_root_changed")
            readback(top, rows)
            require((info.st_dev, info.st_ino) == inode(root / LEAF), "stage_root_changed")
            recheck()
        for _, unchanged in verified.values():
            unchanged()
    encoded = json.dumps(rows, separators=(",", ":")).encode()
    receipt = {"tree": TREE, "runtimeAdmitted": False, "publisherTrustEstablished": False,
               "stagedInert": args.mode == "stage-inert", "files": len(rows),
               "bytes": sum(row[2] for row in rows),
               "inventorySha256": hashlib.sha256(encoded).hexdigest()}
    if args.inventory:
        receipt["rows"] = rows
    print(json.dumps(receipt))


def inode(path):
    info = os.stat(path, follow_symlinks=False)
    return info.st_dev, info.st_ino


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, tarfile.TarError, zipfile.BadZipFile) as error:
        reason = str(error) if isinstance(error, Refused) else type(error).__name__
        print(f"browser runtime custody refused: {reason}", file=sys.stderr)
        raise SystemExit(1)
