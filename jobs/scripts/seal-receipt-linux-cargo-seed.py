"""Seal the frozen Linux receipt dependency graph and its offline replay cache."""
from contextlib import ExitStack
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import tomllib

INTAKE = "07426bad5f55f7fe3ea99ccab025571ffabffe310e204c7cf64ec4c20dd682dc"
LOCK = "23125668f1c5560bf5ffce020e81b4d2337f3b4c86be5a29ba06b063e45bffc9"
PRIOR = "caa1fac2eb7b0ce6321d8b63f1284e87777642cd4fb83091b1a7b1189e3c6340"
TREE = "f9b3cf55de39cd0c86a50b5de7546008bbd49210"
TARGET = "x86_64-unknown-linux-gnu"
FEATURE = "integration-test-support"
REGISTRY_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
INDEX = "index.crates.io-1949cf8c6b5b557f"
REGISTRY = Path("/Users/uno/.cargo/registry")
TOOL = Path("/Users/uno/.rustup/toolchains/stable-aarch64-apple-darwin/bin")
TOOLS = {
    "cargo": "c512bff73c86143b557463f021d0c3d5b0490d97d65040ba59ea2b3427784758",
    "rustc": "b829b733131d4e1673eeebd1f34d06ae1e9ff4977b051313cf42e2a9e79ecf1c",
}
CAP = 16 * 1024**2
DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
CREATE = os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC


def check(ok, reason):
    if not ok:
        raise RuntimeError(reason)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def wire(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def stable(path, limit=CAP):
    check(path.is_absolute() and path.resolve(strict=True) == path, "cargo_seed_nonphysical_input")
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
    with os.fdopen(os.open(path, flags), "rb") as stream:
        def identity(item):
            return (item.st_dev, item.st_ino, item.st_mode, item.st_uid, item.st_nlink,
                    item.st_size, item.st_mtime_ns, item.st_ctime_ns)
        before = os.fstat(stream.fileno())
        check(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and before.st_uid == os.getuid()
              and 0 < before.st_size <= limit, "cargo_seed_input_identity")
        data = stream.read(before.st_size + 1)
        check(len(data) == before.st_size and identity(before) == identity(os.fstat(stream.fileno()))
              == identity(path.lstat()), "cargo_seed_input_changed")
    return data


def entry_path(name):
    name = name.lower()
    if len(name) == 1 or len(name) == 2:
        return f"{len(name)}/{name}"
    if len(name) == 3:
        return f"3/{name[0]}/{name}"
    return f"{name[:2]}/{name[2:4]}/{name}"


def write_new(path, data):
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with path.open("xb") as stream:
        check(stream.write(data) == len(data), "cargo_seed_write")


def cargo_graph(source, cache, root, pins):
    env = {"PATH": "/usr/bin:/bin", "HOME": str(cache), "CARGO_HOME": str(cache),
           "CARGO_TARGET_DIR": str(root / "cargo-target"), "RUSTC": str(TOOL / "rustc"),
           "TMPDIR": str(root / "tmp"), "CARGO_NET_OFFLINE": "true", "CARGO_TERM_COLOR": "never",
           "LANG": "C", "LC_ALL": "C"}
    args = [str(TOOL / "cargo"), "metadata", "--locked", "--offline", "--filter-platform", TARGET,
            "--features", FEATURE, "--manifest-path", str(source / "server/Cargo.toml"),
            "--format-version", "1"]
    run = subprocess.run(args, env=env, cwd=source, capture_output=True, timeout=90)
    if run.returncode != 0:
        first = run.stderr.splitlines()[:1]
        raise RuntimeError("cargo_seed_metadata: " + (first[0].decode("utf-8", "replace") if first else str(run.returncode)))
    check(0 < len(run.stdout) <= CAP and len(run.stderr) <= 64 * 1024, "cargo_seed_metadata_bound")
    metadata = json.loads(run.stdout)
    check(metadata["version"] == 1 and metadata["workspace_root"] == str(source / "server"),
          "cargo_seed_workspace_binding")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    check(len(nodes) == len(metadata["packages"]) == 353
          and {pkg["id"] for pkg in metadata["packages"]} == nodes.keys()
          and FEATURE in nodes[metadata["resolve"]["root"]]["features"], "cargo_seed_graph")
    selected = set()
    for package in metadata["packages"]:
        if package["source"] is None:
            check(Path(package["manifest_path"]).is_relative_to(source), "cargo_seed_local_graph")
            continue
        key = package["name"], package["version"]
        check(package["source"] == REGISTRY_SOURCE and key in pins and key not in selected,
              "cargo_seed_registry_graph")
        selected.add(key)
    check(len(selected) == 351, "cargo_seed_selected_count")
    return sorted(selected)


def package_seed(spool_fd, registry, paths, receipt, replay):
    rows = []
    with os.fdopen(os.open("cargo-seed.tar", CREATE, 0o600, dir_fd=spool_fd), "w+b") as stream:
        with tarfile.open(fileobj=stream, mode="w") as archive:
            for name in paths:
                data = stable(registry / name)
                item = tarfile.TarInfo("registry/" + name)
                item.size, item.mode = len(data), 0o600
                archive.addfile(item, io.BytesIO(data))
                rows.append([item.name, len(data), sha(data)])
        check(sum(row[1] for row in rows) < 128 * 1024**2, "cargo_seed_archive_bound")
        stream.flush()
        os.fsync(stream.fileno())
        archive_identity = os.fstat(stream.fileno())
        # Copy only manifest-selected rows into a fresh Cargo home from
        # the same still-held file; never reopen a pathname for replay.
        stream.seek(0)
        with tarfile.open(fileobj=stream, mode="r:") as archive:
            for row in rows:
                member = archive.next()
                check(member is not None and member.name == row[0] and member.type == tarfile.REGTYPE
                      and member.size == row[1] and member.mode == 0o600 and not member.pax_headers
                      and not member.linkname and (member.uid, member.gid, member.mtime) == (0, 0, 0),
                      "cargo_seed_archive_member")
                with archive.extractfile(member) as source:
                    data = source.read(row[1] + 1)
                check(sha(data) == row[2] and len(data) == row[1], "cargo_seed_archive_hash")
                write_new(replay / row[0], data)
                check(stable(replay / row[0]) == data, "cargo_seed_restored_bytes")
            check(archive.next() is None, "cargo_seed_extra_member")
        stream.seek(0)
        body = stream.read(128 * 1024**2 + 1)
        current = os.fstat(stream.fileno())
        named = os.stat("cargo-seed.tar", dir_fd=spool_fd, follow_symlinks=False)
        check(len(body) == archive_identity.st_size and current.st_nlink == named.st_nlink == 1
              and os.path.samestat(archive_identity, current) and os.path.samestat(current, named)
              and archive_identity.st_mtime_ns == current.st_mtime_ns == named.st_mtime_ns
              and archive_identity.st_ctime_ns == current.st_ctime_ns == named.st_ctime_ns,
              "cargo_seed_archive_changed")
    receipt.update({"archiveSha256": sha(body), "archiveBytes": len(body), "rows": rows,
                    "registryInventorySha256": sha(wire(rows))})


def main(inputs, retained, spool, stack):
    check(sys.platform == "darwin", "cargo_seed_resolver_platform")
    os.umask(0o077)
    root = Path(os.environ["BLUEY_TEST_RUN_ROOT"])
    check(spool.is_absolute() and spool.resolve(strict=True) == spool and not spool.is_relative_to(root)
          and re.fullmatch(r"jobs-f9-linux-cargo-seed-[A-Za-z0-9]{8}", spool.name)
          and stat.S_IMODE(spool.stat().st_mode) == 0o700 and spool.stat().st_uid == os.getuid()
          and not list(spool.iterdir()), "cargo_seed_output")
    root_fd = os.open(root, DIR)
    stack.callback(os.close, root_fd)
    spool_fd = os.open(spool, DIR)
    stack.callback(os.close, spool_fd)
    saved_root, saved_spool = os.fstat(root_fd), os.fstat(spool_fd)
    check(saved_root.st_uid == saved_spool.st_uid == os.getuid()
          and stat.S_IMODE(saved_root.st_mode) == stat.S_IMODE(saved_spool.st_mode) == 0o700
          and os.listdir(spool_fd) == [], "cargo_seed_directory_binding")
    os.mkdir("cargo-seed-attempt", mode=0o700, dir_fd=root_fd)
    box = root / "cargo-seed-attempt"
    box_fd = os.open("cargo-seed-attempt", DIR, dir_fd=root_fd)
    stack.callback(os.close, box_fd)
    saved_box = os.fstat(box_fd)
    tool = Path(__file__).resolve().with_name("stage-browser-receipt-intake.py")
    raw = stable(tool, 32 * 1024)
    check(sha(raw) == INTAKE, "cargo_seed_intake_pin")
    intake = {"__name__": "verified_cargo_seed_intake"}
    exec(compile(raw, str(tool), "exec"), intake)
    _, payload = intake["load_inputs"](inputs)
    marker = intake["marker_for"](root, os.getpgrp())
    source = box / "source"
    def same():
        check(intake["marker_for"](root, os.getpgrp()) == marker, "cargo_seed_marker_changed")
        for item, path in ((saved_root, root), (saved_box, box), (saved_spool, spool)):
            check(os.path.samestat(item, os.stat(path, follow_symlinks=False)),
                  "cargo_seed_directory_replaced")
    intake["stage"](box, "source", payload["source"], same)
    lock = stable(source / "server/Cargo.lock", 512 * 1024)
    check(sha(lock) == LOCK, "cargo_seed_lock")
    pins = {}
    for row in tomllib.loads(lock.decode("utf-8"))["package"]:
        if "source" not in row:
            continue
        name, version, digest = (row[x] for x in ("name", "version", "checksum"))
        check(row["source"] == REGISTRY_SOURCE and re.fullmatch(r"[A-Za-z0-9_-]+", name)
              and re.fullmatch(r"[0-9][A-Za-z0-9.+-]*", version) and re.fullmatch(r"[a-f0-9]{64}", digest)
              and (name, version) not in pins, "cargo_seed_registry_tuple")
        pins[name, version] = digest
    check(len(pins) == 437, "cargo_seed_registry_count")
    for name, digest in TOOLS.items():
        check(sha(stable(TOOL / name, 64 * 1024**2)) == digest, "cargo_seed_host_tool")
    retained_receipt = stable(retained / "receipt.json", 8192)
    check(sha(retained_receipt) == PRIOR, "cargo_seed_prior_receipt")
    retained_rows = {row["file"]: row for row in json.loads(retained_receipt)["archives"]}
    check(len(retained_rows) == 10, "cargo_seed_prior_rows")
    home = box / "resolver"
    registry = home / "registry"
    prefix = f"index/{INDEX}"
    indexes = [prefix + "/config.json", *(
        prefix + "/.cache/" + key for key in sorted({entry_path(name) for name, _ in pins})
    )]
    for name in indexes:
        write_new(registry / name, stable(REGISTRY / name))
    archive_names = {}
    used_retained = set()
    for (name, version), digest in sorted(pins.items()):
        file = f"{name}-{version}.crate"
        relative = f"cache/{INDEX}/{file}"
        original = REGISTRY / relative
        if file in retained_rows:
            data = stable(retained / file)
            check(len(data) == retained_rows[file]["bytes"] and digest == retained_rows[file]["sha256"],
                  "cargo_seed_prior_bytes")
            used_retained.add(file)
        elif os.path.lexists(original):
            data = stable(original)
        else:
            continue
        check(sha(data) == digest, "cargo_seed_crate_digest")
        write_new(registry / relative, data)
        archive_names[name, version] = relative
    check(used_retained == retained_rows.keys(), "cargo_seed_reused_closure")
    same()
    selected = cargo_graph(source, home, root, pins)
    same()
    # Report the target graph separately from the replay cache: the host
    # Cargo resolver can open additional archives before reporting that
    # graph. The target-only trial refused core-foundation on fresh replay.
    files = sorted(indexes + list(archive_names.values()))
    receipt = {"tree": TREE, "lockSha256": LOCK, "platform": TARGET, "feature": FEATURE,
               "selectedCrates": len(selected), "priorCrates": len(used_retained),
               "archiveCrates": len(archive_names),
               "resolverSupportCrates": [
                   filename for key, filename in sorted(archive_names.items()) if key not in selected
               ],
               "sparseFiles": len(indexes), "nativeCompilerQualified": False, "compiled": False,
               "networkUsed": False}
    replay = box / "portable-replay"
    package_seed(spool_fd, registry, files, receipt, replay)
    same()
    check(cargo_graph(source, replay, root, pins) == selected, "cargo_seed_replay_graph")
    check(stable(source / "server/Cargo.lock", 512 * 1024) == lock, "cargo_seed_lock_changed")
    same()
    receipt["offlineReplay"] = True
    body = wire(receipt)
    with os.fdopen(os.open("receipt.json", CREATE, 0o600, dir_fd=spool_fd), "w+b") as stream:
        check(stream.write(body) == len(body), "cargo_seed_receipt_write")
        stream.flush()
        os.fsync(stream.fileno())
        stream.seek(0)
        check(stream.read(len(body) + 1) == body and os.path.samestat(os.fstat(stream.fileno()),
              os.stat("receipt.json", dir_fd=spool_fd, follow_symlinks=False)), "cargo_seed_receipt")
    same()
    print(json.dumps({k: v for k, v in receipt.items() if k != "rows"}, sort_keys=True))


if __name__ == "__main__":
    try:
        check(len(sys.argv) == 4, "cargo_seed_usage")
        with ExitStack() as stack:
            main(*(Path(value) for value in sys.argv[1:]), stack)
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired, KeyError, tarfile.TarError) as error:
        print(f"cargo seed refused: {error if isinstance(error, RuntimeError) else type(error).__name__}",
              file=sys.stderr)
        raise SystemExit(1)
