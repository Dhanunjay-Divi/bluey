"""Materialize only the independently pinned f9 browser receipt inputs."""
import argparse
import contextlib
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import sys
import tarfile

RECEIPT = "aa2ea7b02b17407e9a80871db8515002880da490b7ef144118d2003be6d34727"
TREE = "f9b3cf55de39cd0c86a50b5de7546008bbd49210"
PARTS = {
    "source": ("browser-receipt-source.tar.gz", "sourceArchive", "stagedRows", 981, {0o644, 0o755}),
    "portal": ("browser-receipt-portal.tar.gz", "browserArchive", "browserRows", 41, {0o600}),
}
INPUT_CAP = 24 * 1024 * 1024
EXPANDED_CAP = 64 * 1024 * 1024
NAME = re.compile(r"[A-Za-z0-9._@+/-]{1,240}")
RUN = re.compile(r"run\.([1-9]\d{0,15})\.([1-9]\d{0,9})\.\d{3,12}")
SOURCE_PARENT = re.compile(r"bluey-jobs-receipt-intake-[0-9a-f]{16,32}")
DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
REGULAR = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC


class Refused(RuntimeError):
    pass


def require(condition, code):
    if not condition:
        raise Refused(code)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode()


def identity(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_mode, info.st_nlink, info.st_size,
            info.st_mtime_ns, info.st_ctime_ns)


def physical_directory(path):
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_dir(), "physical_directory")


def pinned_read(parent, name, cap, expected=None, owned_mode=None):
    fd = os.open(name, REGULAR, dir_fd=parent)
    with os.fdopen(fd, "rb") as stream:
        before = os.fstat(fd)
        require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and before.st_size <= cap,
                "input_file_type_or_bound")
        if owned_mode is not None:
            require(before.st_uid == os.getuid() and stat.S_IMODE(before.st_mode) == owned_mode,
                    "owned_file_mode")
        data = stream.read(before.st_size + 1)
        named = os.stat(name, dir_fd=parent, follow_symlinks=False)
        require(len(data) == before.st_size
                and identity(before) == identity(os.fstat(fd)) == identity(named), "input_file_changed")
    if expected is not None:
        require(sha(data) == expected, "input_digest")
    return data


def unpack(archive, inventory, count, modes):
    require(isinstance(inventory, list) and len(inventory) == count, "inventory_count")
    expected = {}
    for row in inventory:
        require(isinstance(row, list) and len(row) == 4, "inventory_row")
        path, digest, mode, size = row
        require(isinstance(path, str) and NAME.fullmatch(path)
                and not {"", ".", ".."}.intersection(path.split("/"))
                and path not in expected and not any(part == ".git" for part in path.split("/")),
                "inventory_path")
        require(isinstance(digest, str) and re.fullmatch(r"[a-f0-9]{64}", digest)
                and type(mode) is int and mode in modes
                and type(size) is int and 0 <= size <= 16 * 1024 * 1024, "inventory_mode_size_or_hash")
        expected[path] = (digest, mode, size)
    require(sum(row[2] for row in expected.values()) <= EXPANDED_CAP, "inventory_total_bound")
    with gzip.GzipFile(fileobj=io.BytesIO(archive), mode="rb") as stream:
        payload = stream.read(EXPANDED_CAP + 1)
    require(0 < len(payload) <= EXPANDED_CAP, "archive_expansion_bound")
    result = {}
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:") as tar:
        for member in tar:
            require(member.name in expected and member.name not in result, "archive_member")
            digest, mode, size = expected[member.name]
            require(member.type == tarfile.REGTYPE and not member.pax_headers and not member.linkname
                    and (member.uid, member.gid, member.mtime, member.mode, member.size) == (0, 0, 0, mode, size),
                    "archive_member_metadata")
            data = tar.extractfile(member).read(size + 1)
            require(len(data) == size and sha(data) == digest, "archive_member_digest")
            result[member.name] = (data, mode)
    require(result.keys() == expected.keys(), "archive_missing_member")
    return result


def load_inputs(directory):
    physical_directory(directory)
    with contextlib.ExitStack() as stack:
        fd = os.open(directory, DIR)
        stack.callback(os.close, fd)
        receipt = json.loads(pinned_read(fd, "receipt.json", 256 * 1024, RECEIPT))
        require(receipt["schema"] == "jobs-f9-browser-receipt-input-v1" and receipt["tree"] == TREE
                and receipt["profile"] == "interview-prep-browser"
                and all(receipt[key] is False
                        for key in ("runtimeAdmitted", "rustCompiled", "postgresStarted", "chromiumStarted")),
                "receipt_identity")
        require(sha(canonical(receipt["stagedRows"])) == receipt["stagedSourceSha256"], "source_inventory_digest")
        data = {}
        for mode, (filename, arch, rows, count, modes) in PARTS.items():
            archive = pinned_read(fd, filename, INPUT_CAP, receipt[arch]["sha256"])
            require(len(archive) == receipt[arch]["bytes"], "archive_length")
            data[mode] = unpack(archive, receipt[rows], count, modes)
    return receipt, data


def marker_for(root, group):
    match = RUN.fullmatch(root.name)
    require(match and re.fullmatch(r"bluey-test-runs-[a-f0-9]{12}", root.parent.name)
            and root.parent.parent == Path("/tmp").resolve(), "run_root_shape")
    fd = os.open(root, DIR)
    try:
        raw = pinned_read(fd, ".bluey-test-run-v2", 2048, owned_mode=0o600)
    finally:
        os.close(fd)
    rows = raw.decode("utf-8").splitlines(keepends=True)
    require(len(rows) == 9 and all(row.endswith("\n") for row in rows), "run_marker_shape")
    prefix = [
        "magic=bluey-test-run-v2", f"run_root={root}", f"run_name={root.name}",
        f"owner_uid={os.getuid()}", f"owner_pid={match[2]}", f"process_group_id={group}",
    ]
    require(group > 1 and rows[:6] == [line + "\n" for line in prefix], "run_marker_identity")
    other = re.fullmatch(r"launcher_group_id=([1-9]\d{0,9})\n", rows[6])
    require(other and int(other[1]) > 1 and int(other[1]) != group
            and rows[7:] == [f"created_epoch={match[1]}\n", "state=running\n"], "run_marker_state")
    return raw


def stage(parent, leaf, files, recheck):
    physical_directory(parent)
    with contextlib.ExitStack() as stack:
        parent_fd = os.open(parent, DIR)
        stack.callback(os.close, parent_fd)
        def proof():
            info = os.fstat(parent_fd)
            require(info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700
                    and (info.st_dev, info.st_ino) == actual_dir(parent), "private_destination_changed")
        proof()
        recheck()
        # A pre-existing destination, even empty, may belong to another run.
        # Refuse it rather than replacing it. On later failure keep the
        # incomplete tree for the owning controller; emit no success record.
        os.mkdir(leaf, mode=0o700, dir_fd=parent_fd)
        top = os.open(leaf, DIR, dir_fd=parent_fd)
        stack.callback(os.close, top)
        folders = {"": top}
        for name, (data, mode) in sorted(files.items()):
            *parents, filename = name.split("/")
            directory = ""
            for part in parents:
                prior = directory
                directory = (directory + "/" + part).lstrip("/")
                if directory not in folders:
                    os.mkdir(part, mode=0o700, dir_fd=folders[prior])
                    handle = os.open(part, DIR, dir_fd=folders[prior])
                    stack.callback(os.close, handle)
                    folders[directory] = handle
            flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
            fd = os.open(filename, flags, 0o600, dir_fd=folders[directory])
            with os.fdopen(fd, "wb") as destination:
                require(destination.write(data) == len(data), "stage_short_write")
                os.fchmod(fd, mode)
        visited = set()
        visited_folders = set()
        for directory, names, regulars in os.walk(parent / leaf, followlinks=False):
            rel = Path(directory).relative_to(parent / leaf).as_posix()
            rel = "" if rel == "." else rel
            require(rel in folders, "extra_staged_directory")
            visited_folders.add(rel)
            info = os.fstat(folders[rel])
            require((info.st_dev, info.st_ino) == actual_dir(directory)
                    and info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700,
                    "staged_directory_changed")
            for dirname in names:
                require((rel + "/" + dirname).lstrip("/") in folders, "extra_staged_directory")
            for filename in regulars:
                path = (rel + "/" + filename).lstrip("/")
                require(path in files and path not in visited, "extra_staged_file")
                data, mode = files[path]
                require(pinned_read(folders[rel], filename, len(data), sha(data), mode) == data, "staged_readback")
                visited.add(path)
        require(visited == files.keys() and visited_folders == folders.keys(), "staged_inventory_changed")
        proof()
        recheck()


def actual_dir(path):
    info = os.stat(path, follow_symlinks=False)
    require(stat.S_ISDIR(info.st_mode), "not_directory")
    return info.st_dev, info.st_ino


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("verify", "stage-source", "stage-portal"))
    parser.add_argument("inputs", type=Path)
    parser.add_argument("--source-parent", type=Path)
    args = parser.parse_args()
    require((args.source_parent is not None) == (args.mode == "stage-source"), "source_parent_option")
    receipt, files = load_inputs(args.inputs)
    if args.mode != "verify":
        os.umask(0o077)
    result = {"tree": TREE, "receiptSha256": RECEIPT, "runtimeAdmitted": False}
    if args.mode == "stage-source":
        parent = args.source_parent
        require(SOURCE_PARENT.fullmatch(parent.name) and not parent.is_relative_to(args.inputs)
                and not args.inputs.is_relative_to(parent), "source_parent_shape")
        stage(parent, "source", files["source"], lambda: None)
        result.update({"sourcePath": str(parent / "source"), "sourceFiles": len(files["source"]),
                       "stagedSourceSha256": receipt["stagedSourceSha256"], "gitInitialized": False})
    elif args.mode == "stage-portal":
        root = Path(os.environ.get("BLUEY_TEST_RUN_ROOT", ""))
        physical_directory(root)
        group = os.getpgrp()
        marker = marker_for(root, group)
        def running():
            require(marker_for(root, group) == marker, "run_marker_changed")
        stage(root, "receipt-prep-browser", files["portal"], running)
        result.update({"portalPath": str(root / "receipt-prep-browser"), "portalFiles": len(files["portal"]),
                       "browserRuntimeStaged": False})
    else:
        result.update({"sourceFiles": len(files["source"]), "portalFiles": len(files["portal"])})
    print(json.dumps(result))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, Refused, tarfile.TarError) as error:
        reason = str(error) if isinstance(error, Refused) else type(error).__name__
        raise SystemExit(f"Browser receipt intake refused: {reason}") from None
