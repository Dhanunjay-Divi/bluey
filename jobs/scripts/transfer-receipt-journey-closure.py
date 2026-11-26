"""Seal, verify and copy the offline Linux receipt driver's test-only closure."""
import argparse
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import posixpath
import re
import stat
import sys
import tarfile

TREE = "f9b3cf55de39cd0c86a50b5de7546008bbd49210"
COMPILER = "3d37eba3d1fd4f3e7e046d8130e9133cf7f5f82cd8c5698d90cfc56ce574cbd8"
INTAKE = "07426bad5f55f7fe3ea99ccab025571ffabffe310e204c7cf64ec4c20dd682dc"
LOCK = "ac64330dca973a22beb4569c96551af683fabd5edb0ee5592c43554ac098b00b"
DRIVER = "c2a1b8e42a2f536e0aa6dcba42aa241ac72c154e82dd21f1a09aaf551bfd0898"
DIST = "784ffb8974a9256414ecb5cd5ac38d8536bc1e0ebda7fe2194e3063daf073804"
NODE = "94efd062ed066c1a0b332e18249859c905e920eeada5df55d24c99b919629a90"
NODE_PATH = "toolchain/bin/node"
DESC = {"schema": "phase658_receipt_journey_node_tool_v1", "platform": "linux", "arch": "x64",
        "version": "v22.23.4", "relativePath": NODE_PATH, "sha256": NODE}
SCHEMA = "bluey-jobs-f9-receipt-journey-transfer-v1"
CAPSULE = "receipt-journey-node"
ARCHIVE = "receipt-journey-closure.tar"
MANIFEST = "inventory.json"
MAX_INVENTORY = 12 * 1024**2
MAX_ENTRIES = 60_000
MAX_FILE = 128 * 1024**2
MAX_TOTAL = 2 * 1024**3
DIR = os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_RDONLY
FILE = os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
HEX = re.compile(r"[a-f0-9]{64}")


def insist(condition, reason):
    if not condition:
        raise RuntimeError(reason)


def encode(value):
    return json.dumps(value, ensure_ascii=True, separators=(",", ":")).encode("ascii")


def sha(value):
    return hashlib.sha256(value).hexdigest()


def identity(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def valid_name(name):
    return isinstance(name, str) and 0 < len(name.encode("utf-8")) <= 700 and all(
        part and part not in {".", "..", ".git"} and all(c.isprintable() and c != "\\" for c in part)
        for part in name.split("/")
    )


def canonical_rel_target(name, target):
    parts = name.split("/")
    node_modules = [i for i, part in enumerate(parts) if part == "node_modules"]
    insist(isinstance(target, str) and 0 < len(target.encode("utf-8")) <= 700
           and node_modules and parts[0] == "jobs"
           and not target.startswith("/") and all(
               part and part != ".git" and all(c.isprintable() and c != "\\" for c in part)
               for part in target.split("/")), "link_scope")
    resolved = posixpath.normpath(posixpath.join(posixpath.dirname(name), target))
    insist(valid_name(resolved) and resolved.startswith("jobs/"), "link_escape")
    return resolved


def verify_rows(rows):
    insist(isinstance(rows, list) and 0 < len(rows) <= MAX_ENTRIES, "inventory_bound")
    table = {}
    total = 0
    for row in rows:
        insist(isinstance(row, list) and len(row) == 5, "inventory_row")
        name, kind, mode, size, detail = row
        insist(valid_name(name) and (name == "jobs" or name.startswith("jobs/") or name in {
            "toolchain", "toolchain/bin", NODE_PATH, "node-tool.json",
        }) and name not in table, "inventory_name")
        insist(type(mode) is int and type(size) is int, "inventory_metadata")
        if kind == "f":
            insist(mode in {0o600, 0o700} and 0 <= size <= MAX_FILE
                   and isinstance(detail, str) and HEX.fullmatch(detail), "inventory_file")
            total += size
        elif kind == "d":
            insist(mode == 0o700 and size == 0 and detail is None, "inventory_dir")
        elif kind == "l":
            insist(mode == 0 and size == 0, "inventory_link")
            canonical_rel_target(name, detail)
        else:
            raise RuntimeError("inventory_kind")
        table[name] = row
    insist(list(table) == sorted(table) and total <= MAX_TOTAL, "inventory_order_or_bytes")
    for name, kind, _, _, detail in rows:
        parent = posixpath.dirname(name)
        insist(not parent or (parent in table and table[parent][1] == "d"), "inventory_parent")
        if kind == "l":
            target = canonical_rel_target(name, detail)
            insist(target in table and table[target][1] in {"f", "d"}, "link_target")
            insist(not name.startswith(target + "/"), "link_ancestor")
    return table


def check_pins(table):
    expected = {
        NODE_PATH: NODE, "node-tool.json": sha(encode(DESC) + b"\n"),
        "jobs/package-lock.json": LOCK,
        "jobs/runner/tests/managed-receipt-journey-driver.mjs": DRIVER,
    }
    for name, digest in expected.items():
        row = table.get(name)
        insist(row is not None and row[1] == "f" and row[4] == digest, "required_payload_pin")
    insist(table[NODE_PATH][2:4] == [0o700, 124_902_584], "node_mode_size")
    dist = [[name[5:], size, digest] for name, kind, _, size, digest in
            sorted(table.values(), key=lambda row: Path(row[0]).parts)
            if kind == "f" and name.startswith(("jobs/runner/dist/", "jobs/automation/dist/"))]
    insist(len(dist) == 130 and sha(encode(dist)) == DIST, "compiled_dist_pin")
    insist("jobs/node_modules" in table and table["jobs/node_modules"][1] == "d"
           and any(name.startswith("jobs/node_modules/") for name in table), "npm_closure_missing")
    automation = "jobs/node_modules/@bluey/jobs-automation"
    insist(table.get(automation) == [automation, "l", 0, 0, "../../automation"],
           "automation_workspace_binding")


def copy_digest(stream, size, out=None):
    digest = hashlib.sha256()
    remaining = size
    while remaining:
        chunk = stream.read(min(remaining, 1024**2))
        insist(0 < len(chunk) <= remaining, "short_member")
        digest.update(chunk)
        if out is not None:
            insist(out.write(chunk) == len(chunk), "short_write")
        remaining -= len(chunk)
    insist(not stream.read(1), "long_member")
    return digest.hexdigest()


@contextlib.contextmanager
def read_at(fd, name):
    with os.fdopen(os.open(name, os.O_RDONLY | FILE, dir_fd=fd), "rb") as stream:
        info = os.fstat(stream.fileno())
        insist(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_nlink == 1,
               "physical_file")
        yield stream, info
        insist(identity(info) == identity(os.fstat(stream.fileno()))
               == identity(os.stat(name, dir_fd=fd, follow_symlinks=False)), "file_changed")


def scan(root_fd, *, seal=False):
    rows = []
    total = 0
    for dirname, dirs, files, fd in os.fwalk(".", dir_fd=root_fd, follow_symlinks=False):
        folder = Path(dirname)
        info = os.fstat(fd)
        mode = stat.S_IMODE(info.st_mode)
        insist(info.st_uid == os.getuid() and (mode == 0o700 or (
            seal and mode == 0o755 and folder != Path("."))), "physical_dir")
        if folder != Path("."):
            rows.append([folder.as_posix(), "d", 0o700, 0, None])
        for child in dirs + files:
            path = (folder / child).as_posix()
            info = os.stat(child, follow_symlinks=False, dir_fd=fd)
            insist(info.st_uid == os.getuid() and valid_name(path), "physical_member")
            if stat.S_ISLNK(info.st_mode):
                rows.append([path, "l", 0, 0, os.readlink(child, dir_fd=fd)])
            elif stat.S_ISREG(info.st_mode):
                mode = stat.S_IMODE(info.st_mode)
                insist(not mode & 0o7022 and info.st_size <= MAX_FILE, "physical_mode_or_size")
                normalized = 0o700 if mode & 0o100 else 0o600
                insist(seal or normalized == mode, "stage_mode")
                total += info.st_size
                insist(total <= MAX_TOTAL, "stage_byte_bound")
                with read_at(fd, child) as (stream, actual):
                    insist(identity(info) == identity(actual), "file_changed")
                    rows.append([path, "f", normalized, info.st_size, copy_digest(stream, info.st_size)])
            else:
                insist(stat.S_ISDIR(info.st_mode) and child in dirs, "physical_special")
            insist(len(rows) <= MAX_ENTRIES, "stage_entry_bound")
    rows.sort(key=lambda item: item[0])
    verify_rows(rows)
    return rows


@contextlib.contextmanager
def parent_at(root_fd, name):
    parts = name.split("/")
    with contextlib.ExitStack() as stack:
        fd = root_fd
        for part in parts[:-1]:
            fd = os.open(part, DIR, dir_fd=fd)
            stack.callback(os.close, fd)
        yield fd, parts[-1]


def inventory_bytes(rows):
    return encode({"schema": SCHEMA, "tree": TREE, "compilerSha256": COMPILER, "rows": rows})


def header(name, kind, mode, size, detail):
    entry = tarfile.TarInfo(name)
    entry.type = {"f": tarfile.REGTYPE, "d": tarfile.DIRTYPE, "l": tarfile.SYMTYPE}[kind]
    entry.mode, entry.size, entry.linkname = mode, size, detail if kind == "l" else ""
    return entry


def write_archive(root_fd, stream, rows):
    body = inventory_bytes(rows)
    insist(len(body) <= MAX_INVENTORY, "inventory_size")
    with tarfile.open(fileobj=stream, mode="w", format=tarfile.PAX_FORMAT) as archive:
        archive.addfile(header(MANIFEST, "f", 0o600, len(body), None), io.BytesIO(body))
        for name, kind, mode, size, detail in rows:
            record = header(name, kind, mode, size, detail)
            if kind == "f":
                with parent_at(root_fd, name) as (parent, leaf), read_at(parent, leaf) as (file, stat_):
                    insist(stat_.st_size == size, "seal_file_size")
                    archive.addfile(record, file)
            else:
                archive.addfile(record)


def member_matches(member, entry):
    return member is not None and (
        member.name, member.type, member.mode, member.size, member.linkname,
        member.uid, member.gid, member.mtime, member.uname, member.gname
    ) == (
        entry.name, entry.type, entry.mode, entry.size, entry.linkname, 0, 0, 0, "", ""
    ) and all(key in {"path", "linkpath"} for key in member.pax_headers)


def inspect_archive(stream, *, dest_fd=None, pin=True):
    stream.seek(0)
    with tarfile.open(fileobj=stream, mode="r:") as archive:
        member = archive.next()
        insist(member is not None and 0 < member.size <= MAX_INVENTORY
               and member_matches(member, header(MANIFEST, "f", 0o600, member.size, None)),
               "manifest_member")
        with archive.extractfile(member) as f:
            raw = f.read(MAX_INVENTORY + 1)
        data = json.loads(raw)
        insist(isinstance(data, dict) and "rows" in data and raw == inventory_bytes(data["rows"]),
               "manifest_canonical")
        rows = data["rows"]
        table = verify_rows(rows)
        if pin:
            check_pins(table)
        for name, kind, mode, size, detail in rows:
            member = archive.next()
            insist(member_matches(member, header(name, kind, mode, size, detail)), "archive_member")
            if dest_fd is None and kind == "f":
                with archive.extractfile(member) as source:
                    insist(copy_digest(source, size) == detail, "archive_file_digest")
            elif dest_fd is not None:
                with parent_at(dest_fd, name) as (parent, leaf):
                    if kind == "d":
                        os.mkdir(leaf, mode=mode, dir_fd=parent)
                    elif kind == "l":
                        os.symlink(detail, leaf, dir_fd=parent)
                    else:
                        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | FILE
                        with os.fdopen(os.open(leaf, flags, mode, dir_fd=parent), "wb") as dst, \
                                archive.extractfile(member) as source:
                            insist(copy_digest(source, size, dst) == detail, "archive_file_digest")
        end_of_members = archive.offset
        insist(archive.next() is None, "archive_extra_entry")
        stream.seek(end_of_members)
        terminator = ((end_of_members + 2 * tarfile.BLOCKSIZE + tarfile.RECORDSIZE - 1)
                      // tarfile.RECORDSIZE * tarfile.RECORDSIZE - end_of_members)
        insist(stream.read(terminator + 1) == bytes(terminator), "archive_terminator")
    return rows


def helper(name, digest):
    path = Path(__file__).resolve().with_name(name)
    with path.open("rb") as source:
        data = source.read(32 * 1024)
    insist(sha(data) == digest, "helper_digest")
    scope = {"__name__": "verified_transfer_helper", "__file__": str(path)}
    exec(compile(data, str(path), "exec"), scope)
    return scope


@contextlib.contextmanager
def private_dir(path):
    insist(path.is_absolute() and path.resolve(strict=True) == path, "nonphysical_directory")
    fd = os.open(path, DIR)
    try:
        before = os.fstat(fd)
        insist(before.st_uid == os.getuid() and stat.S_IMODE(before.st_mode) == 0o700,
               "private_directory_owner")
        yield fd
        insist(os.path.samestat(before, os.stat(path, follow_symlinks=False)), "directory_changed")
    finally:
        os.close(fd)


def run_marker(root):
    intake = helper("stage-browser-receipt-intake.py", INTAKE)
    group = os.getpgrp()
    marker = intake["marker_for"](root, group)

    def same():
        insist(intake["marker_for"](root, group) == marker, "run_marker_changed")
    return same


def publish(dir_path, capsule, *, expected_spool=None):
    with private_dir(capsule) as source, private_dir(dir_path) as dest:
        if expected_spool is not None:
            insist(os.path.samestat(expected_spool, os.fstat(dest)), "spool_replaced")
        insist(not os.listdir(dest), "spool_not_empty")
        rows = scan(source, seal=True)
        check_pins(verify_rows(rows))
        flags = os.O_RDWR | os.O_CREAT | os.O_EXCL | FILE
        fd = os.open(ARCHIVE, flags, mode=0o600, dir_fd=dest)
        with os.fdopen(fd, "w+b") as stream:
            write_archive(source, stream, rows)
            stream.flush()
            os.fsync(fd)
            insist(inspect_archive(stream) == rows and scan(source, seal=True) == rows,
                   "archive_readback")
            size = os.fstat(fd).st_size
            stream.seek(0)
            digest = copy_digest(stream, size)
            insist(identity(os.fstat(fd)) == identity(os.stat(ARCHIVE, dir_fd=dest, follow_symlinks=False)),
                   "archive_replaced")
        os.fsync(dest)
    return {"archiveSha256": digest, "archiveBytes": size, "filesAndDirectories": len(rows),
            "inventorySha256": sha(inventory_bytes(rows))}


def compile_and_seal(node, source, cache, spool, root):
    insist(not spool.is_relative_to(root) and not root.is_relative_to(spool), "spool_overlap")
    insist(re.fullmatch(r"bluey-receipt-transfer-[a-f0-9]{16}", spool.name), "spool_name")
    with private_dir(spool) as dest:
        insist(not os.listdir(dest), "spool_not_empty")
        spool_identity = os.fstat(dest)
    compiler = helper("compile-linux-browser-receipt-journey.py", COMPILER)
    original = compiler["build"]
    same = run_marker(root)

    def transfer(*args):
        result = original(*args)
        same()
        payload = publish(spool, root / "linux-receipt-offline-build" / CAPSULE,
                          expected_spool=spool_identity)
        same()
        return {**result, **payload, "outputsRetained": True}

    compiler["build"] = transfer
    old = sys.argv
    try:
        sys.argv = [old[0], "build", str(node), str(source), str(cache)]
        compiler["main"]()
    finally:
        sys.argv = old


def receive(mode, archive, digest, root):
    insist(archive.name == ARCHIVE and HEX.fullmatch(digest), "archive_pin")
    same = run_marker(root) if mode != "verify" else lambda: None
    with private_dir(archive.parent) as parent, read_at(parent, archive.name) as (stream, info):
        insist(stat.S_IMODE(info.st_mode) == 0o600 and 0 < info.st_size <= MAX_TOTAL + 64 * 1024**2,
               "archive_size_mode")
        insist(copy_digest(stream, info.st_size) == digest, "archive_digest")
        rows = inspect_archive(stream)
        if mode != "verify":
            insist(not archive.is_relative_to(root), "source_overlaps_run")
            with private_dir(root) as base:
                same()
                if mode == "stage":
                    space = os.fstatvfs(base)
                    insist(space.f_bavail * space.f_frsize >= 32 * 1024**3
                           + sum(row[3] for row in rows), "scratch_capacity")
                    os.mkdir(CAPSULE, mode=0o700, dir_fd=base)
                fd = os.open(CAPSULE, DIR, dir_fd=base)
                try:
                    if mode == "stage":
                        insist(inspect_archive(stream, dest_fd=fd) == rows, "stage_inventory")
                    insist(scan(fd) == rows, "stage_readback")
                    insist(os.path.samestat(os.fstat(fd), os.stat(CAPSULE, dir_fd=base,
                                                                follow_symlinks=False)),
                           "stage_directory_changed")
                finally:
                    os.close(fd)
                same()
        stream.seek(0)
        insist(copy_digest(stream, info.st_size) == digest, "archive_post_digest")
    return {"tree": TREE, "archiveSha256": digest, "inventorySha256": sha(inventory_bytes(rows)),
            "entries": len(rows), "mode": mode, "runtimeAdmitted": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    seal = modes.add_parser("compile-and-seal")
    for name in ("node", "source", "cache", "spool"):
        seal.add_argument(name, type=Path)
    for mode in ("verify", "stage", "readback"):
        verifier = modes.add_parser(mode)
        verifier.add_argument("archive", type=Path)
        verifier.add_argument("digest")
    args = parser.parse_args()
    os.umask(0o077)
    root = Path(os.environ.get("BLUEY_TEST_RUN_ROOT", ""))
    if args.mode == "compile-and-seal":
        compile_and_seal(args.node, args.source, args.cache, args.spool, root)
    else:
        print(json.dumps(receive(args.mode, args.archive, args.digest, root)))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, tarfile.TarError) as error:
        print(f"receipt closure refused: {error if isinstance(error, RuntimeError) else type(error).__name__}",
              file=sys.stderr)
        raise SystemExit(1)
