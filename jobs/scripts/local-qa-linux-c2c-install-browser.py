"""Measure and stage the exact Jobs C2C browser inputs without launching them."""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import stat
import sys


RUNTIME_HELPER = "stage-browser-receipt-runtime.py"
HELPER_SHA = "793273419b521bcdecaf4a649fa43d100a8c089940c073b781d522511b4a6936"
INVENTORY_SHA = "9ecdff4561fe0924e499325c40a6b61ffbe7ea047044d3f2cc0a89f82e728dce"
BROWSER_BASE = Path("/run/bluey-jobs-c2c-browser")
NODE = "receipt-journey-node/toolchain/bin/node"
CHROME = "receipt-prep-browser/chromium/chrome"
CRASHPAD = "receipt-prep-browser/chromium/chrome_crashpad_handler"
EXECUTE = {NODE, CHROME, CRASHPAD}
MAX_INVENTORY = 1 << 20
FILES = 410
PAYLOAD_BYTES = 533_939_096
DIR = os.O_RDONLY | os.O_NOFOLLOW | os.O_DIRECTORY | os.O_CLOEXEC
FILE = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
ARCHIVES = ("playwright", "chrome", "node")


def need(ok, label):
    if not ok:
        raise ValueError("c2c_browser_install_refused:" + label)


def whole(pattern, value):
    return type(value) is str and re.fullmatch(pattern, value) is not None


def sha(data):
    return hashlib.sha256(data).hexdigest()


def stamp(row):
    return (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_gid, row.st_nlink,
            row.st_size, row.st_mtime_ns, row.st_ctime_ns)


def validate_inventory(raw):
    need(type(raw) is bytes and 0 < len(raw) <= MAX_INVENTORY and sha(raw) == INVENTORY_SHA, "inventory_pin")
    inventory = json.loads(raw)
    canonical = (json.dumps(inventory, separators=(",", ":"), ensure_ascii=True) + "\n").encode("ascii")
    need(type(inventory) is dict and inventory.keys() == {"version", "files"} and type(inventory["version"]) is int
         and inventory["version"] == 1 and type(inventory["files"]) is list and len(inventory["files"]) == FILES
         and canonical == raw, "inventory_shape")
    rows, prior, total = {}, "", 0
    for row in inventory["files"]:
        need(type(row) is dict and row.keys() == {"path", "sha256", "size", "mode"}, "inventory_row")
        path = row["path"]
        need(whole(r"[A-Za-z0-9._@+/-]{1,240}", path) and path > prior
             and not {"", ".", "..", ".git"}.intersection(path.split("/"))
             and (path == NODE or path.startswith(("receipt-prep-browser/chromium/",
                                                   "receipt-prep-browser/node_modules/playwright-core/")))
             and not any(parent.as_posix() in rows for parent in Path(path).parents)
             and whole(r"[0-9a-f]{64}", row["sha256"]) and type(row["size"]) is int
             and 0 <= row["size"] <= 320 << 20 and type(row["mode"]) is int
             and row["mode"] == (0o555 if path in EXECUTE else 0o444), "inventory_member")
        rows[path] = dict(row)
        prior, total = path, total + row["size"]
    need(EXECUTE <= rows.keys() and total == PAYLOAD_BYTES, "inventory_totals")
    return rows


def helper():
    here = Path(__file__).resolve(strict=True).parent
    path = here / RUNTIME_HELPER
    need(path.resolve(strict=True) == path, "helper_path")
    with os.fdopen(os.open(path, FILE), "rb") as stream:
        info = os.fstat(stream.fileno())
        need(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_uid == os.getuid()
             and not info.st_mode & 0o7022 and 0 < info.st_size <= 64 << 10, "helper_metadata")
        raw = stream.read(info.st_size + 1)
        need(len(raw) == info.st_size and sha(raw) == HELPER_SHA
             and stamp(info) == stamp(os.fstat(stream.fileno())) == stamp(path.lstat()), "helper_pin")
    scope = {"__file__": str(path), "__name__": "_pinned_c2c_browser_archive_reader"}
    exec(compile(raw, str(path), "exec"), scope)
    return scope


def open_archives(conduit, paths, stack):
    need(type(paths) is dict and paths.keys() == set(ARCHIVES)
         and all(isinstance(path, Path) for path in paths.values()), "archive_paths")
    return {name: conduit["open_verified"](stack, paths[name], conduit["ARCHIVES"][name]) for name in ARCHIVES}


def measure(conduit, inputs, directory=None):
    for stream, check in inputs.values():
        check()
        stream.seek(0)
    playwright, chrome, node = (inputs[name][0] for name in ARCHIVES)
    groups = [(conduit["tar_members"](playwright, False), 106, 12_701_224),
              (conduit["browser_members"](chrome), 303, 396_335_288),
              (conduit["tar_members"](node, True), 1, 124_902_584)]
    measured = conduit["consume"](groups, directory)
    for _, check in inputs.values():
        check()
    return measured


def expected_bytes(rows, measured):
    need([(row["path"], row["sha256"], row["size"]) for row in rows.values()]
         == [(path, digest, size) for path, digest, size, _ in measured], "archive_members")


def verify_archives(paths, inventory_raw):
    rows = validate_inventory(inventory_raw)
    conduit = helper()
    with ExitStack() as stack:
        inputs = open_archives(conduit, paths, stack)
        expected_bytes(rows, measure(conduit, inputs))
    return {"inventorySha256": INVENTORY_SHA, "files": FILES, "payloadBytes": PAYLOAD_BYTES,
            "installed": False, "runtimeQualified": False}


def readback(top, rows, inventory_raw, *, sealed, apply_modes=False):
    """Reject drift through fresh directory FDs and byte-level file readback."""
    owner = os.getuid()
    expected = {**rows, "inventory.json": {"sha256": INVENTORY_SHA, "size": len(inventory_raw), "mode": 0o444}}
    dirs = {parent.as_posix() for name in expected for parent in Path(name).parents}
    names_seen, dirs_seen = set(), set()
    for directory, children, files, descriptor in os.fwalk(".", topdown=False, follow_symlinks=False, dir_fd=top):
        relative = Path(directory)
        found = relative.as_posix()
        info = os.fstat(descriptor)
        want_dir = 0o555 if sealed else 0o700
        initial_dir = 0o700 if apply_modes else want_dir
        need(found in dirs and found not in dirs_seen and info.st_uid == owner
             and stat.S_IMODE(info.st_mode) == initial_dir, "stage_directory")
        dirs_seen.add(found)
        for child in children:
            child_stat = os.stat(child, dir_fd=descriptor, follow_symlinks=False)
            need((relative / child).as_posix() in dirs and stat.S_ISDIR(child_stat.st_mode), "stage_child")
        for leaf in files:
            name = (relative / leaf).as_posix()
            need(name in expected and name not in names_seen, "stage_member")
            row = expected[name]
            with os.fdopen(os.open(leaf, FILE, dir_fd=descriptor), "rb") as stream:
                fd = stream.fileno()
                before = os.fstat(fd)
                want_mode = row["mode"] if sealed else 0o600
                initial = 0o600 if apply_modes else want_mode
                need(stat.S_ISREG(before.st_mode) and before.st_uid == owner and before.st_nlink == 1
                     and stat.S_IMODE(before.st_mode) == initial and before.st_size == row["size"], "stage_file")
                digest = hashlib.sha256()
                remaining = before.st_size
                while remaining:
                    data = stream.read(min(remaining, 1 << 20))
                    need(bool(data), "stage_truncated")
                    remaining -= len(data)
                    digest.update(data)
                need(not stream.read(1) and digest.hexdigest() == row["sha256"]
                     and stamp(before) == stamp(os.fstat(fd))
                     == stamp(os.stat(leaf, dir_fd=descriptor, follow_symlinks=False)), "stage_hash")
                if apply_modes:
                    os.fchmod(fd, want_mode)
                    os.fsync(fd)
            names_seen.add(name)
        if apply_modes:
            os.fchmod(descriptor, want_dir)
            os.fsync(descriptor)
    need(names_seen == expected.keys() and dirs_seen == dirs, "stage_inventory")


def materialize(parent, run, paths, raw, *, live_check, sealed):
    """Write only a fresh named leaf of a held base; retain any partials."""
    need(whole(r"[a-f0-9]{32}", run) and run != "0" * 32 and callable(live_check), "stage_run")
    need(type(sealed) is bool and (not sealed or linux_root()), "sealed_linux")
    rows = validate_inventory(raw)
    conduit = helper()
    parent_start = os.fstat(parent)
    with ExitStack() as stack:
        inputs = open_archives(conduit, paths, stack)
        expected_bytes(rows, measure(conduit, inputs))
        need(live_check() is True and os.path.samestat(parent_start, os.fstat(parent)), "stage_admission")
        capacity = os.fstatvfs(parent)
        need(capacity.f_bavail * capacity.f_frsize >= PAYLOAD_BYTES + (1 << 30), "stage_capacity")
        os.mkdir(run, mode=0o700, dir_fd=parent)
        top = os.open(run, DIR, dir_fd=parent)
        stack.callback(os.close, top)
        identity = os.fstat(top)
        need(identity.st_uid == os.getuid() and stat.S_IMODE(identity.st_mode) == 0o700
             and os.path.samestat(identity, os.stat(run, dir_fd=parent, follow_symlinks=False)), "stage_root")
        # Make the occupied name durable before writing the payload. Neither
        # this installer nor the owner reservation permits automatic retry.
        os.fsync(parent)
        expected_bytes(rows, measure(conduit, inputs, top))
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
        with os.fdopen(os.open("inventory.json", flags, 0o600, dir_fd=top), "wb") as stream:
            need(stream.write(raw) == len(raw), "manifest_write")
            stream.flush()
            os.fsync(stream.fileno())
        need(live_check() is True, "stage_admission_after_copy")
        readback(top, rows, raw, sealed=sealed, apply_modes=sealed)
        need(live_check() is True, "stage_admission_after_mode")
        for _, check in inputs.values():
            check()
        if sealed:
            readback(top, rows, raw, sealed=True)
        need(live_check() is True and os.path.samestat(identity, os.fstat(top))
             and os.path.samestat(identity, os.stat(run, dir_fd=parent, follow_symlinks=False)), "stage_readback")
        os.fsync(top)
        os.fsync(parent)


def linux_root():
    return (sys.platform == "linux" and platform.machine() == "x86_64" and os.getuid() == os.geteuid() == 0
            and all(os.path.samestat(os.stat(f"/proc/self/ns/{kind}"), os.stat(f"/proc/1/ns/{kind}"))
                    for kind in ("mnt", "pid", "user")))


def install_browser(run, reservation_sha256, paths, inventory_raw, *, admitted):
    """Linux root only; install at the exact actor-plan closure path, no CLI."""
    need(linux_root() and callable(admitted) and whole(r"[a-f0-9]{64}", reservation_sha256)
         and reservation_sha256 != "0" * 64 and whole(r"[a-f0-9]{32}", run) and run != "0" * 32,
         "root_admission")
    root = BROWSER_BASE / run
    scope = {"run": run, "reservationSha256": reservation_sha256, "browserRoot": str(root),
             "browserInventorySha256": INVENTORY_SHA}
    need(BROWSER_BASE.resolve(strict=True) == BROWSER_BASE, "base_path")
    for item in (BROWSER_BASE, *BROWSER_BASE.parents):
        row = item.lstat()
        need(stat.S_ISDIR(row.st_mode) and row.st_uid == 0 and not row.st_mode & 0o7022, "base_ancestry")
    parent = os.open(BROWSER_BASE, DIR)
    try:
        initial = os.fstat(parent)

        def live():
            if not linux_root() or admitted(dict(scope)) is not True:
                return False
            named, held = BROWSER_BASE.lstat(), os.fstat(parent)
            authority = (initial.st_dev, initial.st_ino, 0, 0, stat.S_IFDIR | 0o755)
            return (BROWSER_BASE.resolve(strict=True) == BROWSER_BASE
                    and authority
                    == (initial.st_dev, initial.st_ino, initial.st_uid, initial.st_gid, initial.st_mode)
                    == (named.st_dev, named.st_ino, named.st_uid, named.st_gid, named.st_mode)
                    == (held.st_dev, held.st_ino, held.st_uid, held.st_gid, held.st_mode))

        need(live() is True, "guest_admission")
        materialize(parent, run, paths, inventory_raw, live_check=live, sealed=True)
    finally:
        os.close(parent)
    return {**scope, "inventoryFile": str(root / "inventory.json"), "files": FILES,
            "payloadBytes": PAYLOAD_BYTES, "installed": True, "runtimeQualified": False}
