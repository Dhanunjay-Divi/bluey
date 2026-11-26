"""Prepare the f9 receipt driver on Linux; never start a browser or database."""
import errno
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import socket
import stat
import subprocess
import sys

BUILDER = "7afd76b981a5d70ca9bd74ec90d0ff7f4a3007ca778ff27e1bec155245854eb7"
ARCHIVE = "1805bcd35e6815559bd2eb97db438d3fdb0eb3a5e6a977b7c1badca76337e9ce"
NODE = "94efd062ed066c1a0b332e18249859c905e920eeada5df55d24c99b919629a90"
NODE_TREE = "ceaea4efc1f6465f5d8a892e48ae33c593573951cd87f5cb85eb3a609b481072"
NODE_TOP = "node-v22.23.4-linux-x64"
NODE_LINKS = {
    "bin/corepack": "../lib/node_modules/corepack/dist/corepack.js",
    "bin/npm": "../lib/node_modules/npm/bin/npm-cli.js",
    "bin/npx": "../lib/node_modules/npm/bin/npx-cli.js",
}
EXPECTED_DIST = "784ffb8974a9256414ecb5cd5ac38d8536bc1e0ebda7fe2194e3063daf073804"
SOURCE = "stage-browser-receipt-intake.py"
REQUESTS = ("measure-tool", "build")


def require(ok, reason):
    if not ok:
        raise RuntimeError(reason)


def canonical(obj):
    return json.dumps(obj, ensure_ascii=True, separators=(",", ":")).encode("ascii")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verified_helper(name, expected):
    path = Path(__file__).resolve().with_name(name)
    with path.open("rb") as stream:
        body = stream.read(32 * 1024)
    require(sha(body) == expected, "helper_pin")
    scope = {"__name__": "verified_f9_helper", "__file__": str(path)}
    exec(compile(body, str(path), "exec"), scope)
    return scope


def metadata(row):
    return (row.st_dev, row.st_ino, row.st_uid, row.st_mode, row.st_nlink, row.st_size,
            row.st_mtime_ns, row.st_ctime_ns)


def measure_tool(root):
    """Read back the exact tar projection, independent of the archive parser."""
    rows = []
    seen_links = {}
    file_bytes = 0
    for name, directories, files, fd in os.fwalk(root, follow_symlinks=False):
        location = Path(name).relative_to(root)
        location = "" if location == Path(".") else location.as_posix()
        info = os.fstat(fd)
        require(info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700, "tool_directory")
        rows.append([location, "dir", 0o700])
        for child in directories + files:
            relative = f"{location}/{child}" if location else child
            info = os.stat(child, dir_fd=fd, follow_symlinks=False)
            require(info.st_uid == os.getuid(), "tool_owner")
            mode = stat.S_IMODE(info.st_mode)
            if stat.S_ISLNK(info.st_mode):
                target = os.readlink(child, dir_fd=fd)
                require(NODE_LINKS.get(relative) == target and (root / relative).resolve().is_file(),
                        "tool_link")
                seen_links[relative] = target
                rows.append([relative, "symlink", 0o777, target])
            elif stat.S_ISREG(info.st_mode):
                require(mode in {0o600, 0o700} and info.st_nlink == 1, "tool_file_mode")
                file_bytes += info.st_size
                require(file_bytes <= 196_453_447, "tool_file_bound")
                flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
                with os.fdopen(os.open(child, flags, dir_fd=fd), "rb") as stream:
                    require(metadata(info) == metadata(os.fstat(stream.fileno())), "tool_file_changed")
                    digest = hashlib.sha256()
                    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                        digest.update(chunk)
                    require(metadata(info) == metadata(os.fstat(stream.fileno()))
                            == metadata(os.stat(child, dir_fd=fd, follow_symlinks=False)),
                            "tool_file_changed")
                rows.append([relative, "file", mode, info.st_size, digest.hexdigest()])
            else:
                require(stat.S_ISDIR(info.st_mode) and child in directories, "tool_member_type")
            require(len(rows) <= 5866, "tool_entry_bound")
    rows.sort(key=lambda row: row[0])
    require(len(rows) == 5866 and file_bytes == 196_453_447 and seen_links == NODE_LINKS
            and sha(canonical(rows)) == NODE_TREE, "tool_tree_mismatch")
    return {"entries": len(rows), "bytes": file_bytes, "treeSha256": NODE_TREE}


def linux_build_fences():
    require(platform.system() == "Linux" and platform.machine() == "x86_64"
            and os.getuid() != 0 and os.getuid() == os.geteuid()
            and platform.libc_ver()[0] == "glibc", "linux_x64_glibc_unprivileged_required")
    status = Path("/proc/self/status").read_text()
    require(re.search(r"^NoNewPrivs:\s+1$", status, re.M)
            and re.search(r"^Seccomp:\s+2$", status, re.M), "linux_seccomp_required")
    for family in (socket.AF_INET, socket.AF_INET6, socket.AF_PACKET, socket.AF_NETLINK):
        # The compile-only unit may not create IP sockets at all. Check
        # TCP and UDP separately; neither probe sends or binds a packet.
        kinds = (socket.SOCK_STREAM, socket.SOCK_DGRAM) if family in (
            socket.AF_INET, socket.AF_INET6) else (socket.SOCK_DGRAM,)
        for kind in kinds:
            try:
                sock = socket.socket(family, kind)
            except OSError as error:
                require(error.errno in {errno.EAFNOSUPPORT, errno.EPERM, errno.EACCES}, "socket_probe")
            else:
                sock.close()
                raise RuntimeError("internet_socket_not_denied")
    for path in (__file__, Path(__file__).resolve().with_name(SOURCE)):
        try:
            fd = os.open(path, os.O_WRONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
        except OSError as error:
            require(error.errno in {errno.EROFS, errno.EPERM, errno.EACCES}, "source_write_probe")
        else:
            os.close(fd)
            raise RuntimeError("source_write_not_denied")


def build(base, source, inputs, root, box, tool, marker, env):
    jobs_rows = {name[5:]: payload for name, payload in inputs["source"].items()
                 if name.startswith("jobs/")}

    def recheck():
        require(source["marker_for"](root, os.getpgrp()) == marker, "run_marker_changed")

    capsule = box / "receipt-journey-node"
    capsule.mkdir(mode=0o700)
    source["stage"](capsule, "jobs", jobs_rows, recheck)
    jobs = capsule / "jobs"
    driver = jobs / "runner/tests/managed-receipt-journey-driver.mjs"
    require(base["sha"](jobs / "package-lock.json") == base["LOCK"]
            and base["sha"](driver) == base["DRIVER"], "compiler_source_pin")
    require(not any(os.path.lexists(path / ".npmrc") for path in (jobs, *jobs.parents)),
            "ambient_npmrc")
    base["unpack"](Path(sys.argv[4]), base["CACHE_TAR"], box / "cache", env)
    node = tool / "bin/node"
    npm = tool / "lib/node_modules/npm/bin/npm-cli.js"
    require(base["run"]("node_version", [node, "--version"], jobs, env).strip() == b"v22.23.4"
            and base["run"]("npm_version", [node, npm, "--version"], jobs, env).strip() == b"10.9.9",
            "compiler_tool_version")
    base["run"]("offline_npm_ci", [node, npm, "ci", "--offline", "--ignore-scripts",
                                     "--no-audit", "--no-fund"], jobs, env, timeout=240)
    tsc = jobs / "node_modules/typescript/bin/tsc"
    for workspace in ("automation", "runner"):
        base["run"](f"tsc_{workspace}", [node, tsc, "-p", jobs / workspace / "tsconfig.json"],
                    jobs, env)
    for name, (body, mode) in jobs_rows.items():
        current = jobs / name
        require(not current.is_symlink() and stat.S_IMODE(current.stat().st_mode) == mode
                and base["sha"](current) == sha(body), "compiler_source_changed")
    outputs = base["manifest"](jobs)
    require(sha(canonical(outputs)) == EXPECTED_DIST, "linux_darwin_emit_mismatch")
    measure_tool(tool)
    # Keep the build-only npm toolchain out of the published runtime layout.
    runtime_bin = capsule / "toolchain/bin"
    runtime_bin.mkdir(mode=0o700, parents=True)
    # Same owned filesystem, no relinking. The bundled Node/npm distribution
    # was measured after use; this does not attest the installed npm modules.
    os.rename(node, runtime_bin / "node")
    require(base["sha"](runtime_bin / "node") == NODE, "runtime_node_pin")
    desc = {"schema": "phase658_receipt_journey_node_tool_v1", "platform": "linux",
            "arch": "x64", "version": "v22.23.4", "relativePath": "toolchain/bin/node", "sha256": NODE}
    with (capsule / "node-tool.json").open("xb") as body:
        body.write(canonical(desc) + b"\n")
    resolver = """
import assert from 'node:assert/strict';
import { realpathSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
for (const [entry, target] of Object.entries(JSON.parse(process.argv[2]))) {
  const actual = fileURLToPath(import.meta.resolve(entry, pathToFileURL(process.argv[1])));
  assert.equal(realpathSync(actual), resolve(target));
  assert(statSync(actual).isFile());
}
"""
    base["run"]("linux_direct_resolution", [runtime_bin / "node", "--experimental-import-meta-resolve",
                "--input-type=module", "-e", resolver, driver, json.dumps(base["ENTRIES"])], jobs, env)
    startup = subprocess.run([str(runtime_bin / "node"), str(driver)], cwd=root, env=env,
                             stdin=subprocess.DEVNULL, capture_output=True, timeout=15)
    require(startup.returncode == 1 and not startup.stderr and startup.stdout.endswith(b"\n")
            and startup.stdout.count(b"\n") == 1 and json.loads(startup.stdout) == {
                "schema": "phase658_managed_receipt_journey_node_result_v1",
                "ok": False, "code": "unexpected_eof", "phase": "input",
            }, "linux_driver_input_boundary")
    recheck()
    return {"compiledFiles": len(outputs), "distSha256": EXPECTED_DIST,
            "directResolvedImports": len(base["ENTRIES"]),
            "driverStartup": "unexpected_eof", "outputsRetained": False}


def main():
    require(len(sys.argv) in {3, 5} and sys.argv[1] in REQUESTS, "usage")
    mode = sys.argv[1]
    require(len(sys.argv) == (3 if mode == "measure-tool" else 5), "usage")
    if mode == "build":
        linux_build_fences()
    base = verified_helper("compile-browser-receipt-journey.py", BUILDER)
    source = verified_helper(SOURCE, base["STAGER"])
    root = Path(os.environ["BLUEY_TEST_RUN_ROOT"])
    source["physical_directory"](root)
    require(root.stat().st_uid == os.getuid() and stat.S_IMODE(root.stat().st_mode) == 0o700,
            "run_root_owner")
    marker = source["marker_for"](root, os.getpgrp())
    usage = os.statvfs(root)
    require(usage.f_frsize * usage.f_bavail >= (32 if mode == "build" else 17) * 1024**3,
            "scratch_capacity")
    os.umask(0o077)
    box = root / "linux-receipt-offline-build"
    box.mkdir(mode=0o700)
    for name in ("tool", "tmp", "home", "cache", "global"):
        (box / name).mkdir(mode=0o700)
    env = {"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C", "CI": "true",
           "HOME": str(box / "home"), "TMPDIR": str(box / "tmp"),
           **{f"npm_config_{key}": value for key, value in {
               "cache": str(box / "cache"), "prefix": str(box / "global"),
               "userconfig": str(box / "home/user.npmrc"),
               "globalconfig": str(box / "global/global.npmrc"),
               "offline": "true", "ignore_scripts": "true", "audit": "false",
               "fund": "false", "update_notifier": "false",
           }.items()},
           "PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD": "1", "ELECTRON_SKIP_BINARY_DOWNLOAD": "1"}
    base["unpack"](Path(sys.argv[2]), ARCHIVE, box / "tool", env)
    tool = box / "tool" / NODE_TOP
    measured = measure_tool(tool)
    if mode == "build":
        receipt, inputs = source["load_inputs"](Path(sys.argv[3]))
        require(receipt["tree"] == base["TREE"], "frozen_tree")
        measured.update(build(base, source, inputs, root, box, tool, marker, env))
    require(source["marker_for"](root, os.getpgrp()) == marker, "run_marker_changed")
    print(json.dumps({"tree": base["TREE"], "mode": mode, **measured, "runtimeAdmitted": False,
                      "nodeExecuted": mode == "build"}))


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"linux receipt compiler refused: {error if isinstance(error, RuntimeError) else type(error).__name__}",
              file=sys.stderr)
        raise SystemExit(1)
