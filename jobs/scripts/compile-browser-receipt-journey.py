"""Offline Darwin build proof for the frozen f9 receipt-journey JavaScript."""
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

TREE = "f9b3cf55de39cd0c86a50b5de7546008bbd49210"
STAGER = "07426bad5f55f7fe3ea99ccab025571ffabffe310e204c7cf64ec4c20dd682dc"
NODE_TAR = "037650d0839f457747396f052073fb48333d31bf5abd3c98b74da20d89ebb1ad"
CACHE_TAR = "a0aafff9e673b302ef402e5d2e24c89fa7d0bd16cf37565091538ee004de1fc3"
LOCK = "ac64330dca973a22beb4569c96551af683fabd5edb0ee5592c43554ac098b00b"
DRIVER = "c2a1b8e42a2f536e0aa6dcba42aa241ac72c154e82dd21f1a09aaf551bfd0898"
ENTRIES = {
    "@bluey/jobs-automation": "automation/dist/index.js",
    "@bluey/jobs-automation/worker-auth": "automation/dist/worker-auth.js",
    **{f"../dist/{name}.js": f"runner/dist/{name}.js" for name in
       ("execution-lease", "leased-run", "result-store", "subject-storage-layout")},
}


def check(ok, why):
    if not ok:
        raise RuntimeError(why)


def digest(stream):
    h = hashlib.sha256()
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        h.update(chunk)
    return h.hexdigest()


def sha(path):
    with path.open("rb") as stream:
        return digest(stream)


def run(label, argv, cwd, env, *, stdin=None, timeout=120):
    # The canonical launcher owns the entire process group. Keep all child
    # writes and subprocesses in the outer network-denied Seatbelt profile.
    result = subprocess.run(argv, cwd=cwd, env=env, stdin=stdin or subprocess.DEVNULL,
                            capture_output=True, timeout=timeout)
    if result.returncode:
        msg = result.stderr + result.stdout
        codes = sorted(set(re.findall(rb"\b(?:E[A-Z]{4,20}|TS[0-9]{4})\b", msg)))[:20]
        raise RuntimeError(f"{label}: status={result.returncode}; codes="
                           + ",".join(code.decode("ascii") for code in codes))
    return result.stdout


def unpack(path, expected, target, env):
    check(path.is_absolute() and path.resolve(strict=True) == path, "archive_path")
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC)
    with os.fdopen(fd, "rb") as stream:
        before = os.fstat(stream.fileno())
        check(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
              and digest(stream) == expected, "archive_hash")
        stream.seek(0)
        run("archive_unpack", ["/usr/bin/tar", "--no-same-owner", "--no-same-permissions",
                               "-xf", "-", "-C", target], target, env, stdin=stream)
        after = os.fstat(stream.fileno())
        check((before.st_size, before.st_ino, before.st_dev, before.st_mtime_ns, before.st_ctime_ns)
              == (after.st_size, after.st_ino, after.st_dev, after.st_mtime_ns, after.st_ctime_ns)
              and os.path.samestat(after, path.stat()), "archive_changed")


def manifest(root):
    rows = []
    for workspace in ("automation", "runner"):
        dist = root / workspace / "dist"
        for path in sorted(dist.rglob("*")):
            check(not path.is_symlink(), "dist_link")
            if path.is_file():
                check(path.suffix in {".js", ".ts", ".map"}, "dist_extension")
                rows.append([path.relative_to(root).as_posix(), path.stat().st_size, sha(path)])
    check(all(any(row[0] == entry for row in rows) for entry in ENTRIES.values()), "dist_required")
    return rows


def require_fences():
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
            probe.bind(("127.0.0.1", 0))
    except PermissionError:
        pass
    else:
        raise RuntimeError("no_network_fence")
    try:
        fd = os.open(__file__, os.O_WRONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    except PermissionError:
        pass
    else:
        os.close(fd)
        raise RuntimeError("no_source_write_fence")


def main():
    check(platform.system() == "Darwin" and platform.machine() == "arm64", "darwin_arm64_only")
    check(len(sys.argv) == 4, "usage: SOURCE_INPUT NODE_ARCHIVE CACHE_ARCHIVE")
    os.umask(0o077)
    root = Path(os.environ["BLUEY_TEST_RUN_ROOT"])
    helper = Path(__file__).resolve().with_name("stage-browser-receipt-intake.py")
    body = helper.read_bytes()
    check(hashlib.sha256(body).hexdigest() == STAGER, "stager_pin")
    contract = {"__name__": "verified_intake_contract"}
    exec(compile(body, str(helper), "exec"), contract)
    contract["physical_directory"](root)
    check(root.stat().st_uid == os.getuid() and root.stat().st_mode & 0o7777 == 0o700,
          "run_root_owner")
    marker = contract["marker_for"](root, os.getpgrp())
    space = os.statvfs(root)
    check(space.f_bavail * space.f_frsize >= 17 * 1024**3, "compiler_disk_floor")
    require_fences()
    work = root / "receipt-journey-offline-build"
    work.mkdir(mode=0o700)
    for directory in ("home", "tmp", "tool", "cache", "global"):
        (work / directory).mkdir(mode=0o700)
    env = {
        "PATH": "/usr/bin:/bin", "LANG": "C", "HOME": str(work / "home"),
        "TMPDIR": str(work / "tmp"), "CI": "true",
        "npm_config_cache": str(work / "cache"), "npm_config_prefix": str(work / "global"),
        "npm_config_userconfig": str(work / "home" / "user.npmrc"),
        "npm_config_globalconfig": str(work / "global" / "global.npmrc"),
        "npm_config_offline": "true", "npm_config_ignore_scripts": "true",
        "npm_config_audit": "false", "npm_config_fund": "false",
        "npm_config_update_notifier": "false",
        "PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD": "1", "ELECTRON_SKIP_BINARY_DOWNLOAD": "1",
    }
    receipt, inputs = contract["load_inputs"](Path(sys.argv[1]))
    check(receipt["tree"] == TREE, "frozen_tree")
    rows = {name[5:]: payload for name, payload in inputs["source"].items() if name.startswith("jobs/")}
    # A fixed source stage provides every workspace root needed by npm ci.
    contract["stage"](work, "jobs", rows, lambda: check(
        contract["marker_for"](root, os.getpgrp()) == marker, "run_marker_changed"))
    jobs = work / "jobs"
    driver = jobs / "runner/tests/managed-receipt-journey-driver.mjs"
    check(sha(jobs / "package-lock.json") == LOCK and sha(driver) == DRIVER, "compiler_source_pin")
    check(not any((base / ".npmrc").exists() for base in (jobs, *jobs.parents)), "ambient_npmrc")
    unpack(Path(sys.argv[2]), NODE_TAR, work / "tool", env)
    unpack(Path(sys.argv[3]), CACHE_TAR, work / "cache", env)
    tool = work / "tool/node-v22.23.4-darwin-arm64"
    node = tool / "bin/node"
    npm = tool / "lib/node_modules/npm/bin/npm-cli.js"
    check(run("node_version", [node, "--version"], jobs, env).strip() == b"v22.23.4", "node_version")
    check(run("npm_version", [node, npm, "--version"], jobs, env).strip() == b"10.9.9", "npm_version")
    run("offline_npm_ci", [node, npm, "ci", "--offline", "--ignore-scripts", "--no-audit",
                           "--no-fund"], jobs, env, timeout=240)
    tsc = jobs / "node_modules/typescript/bin/tsc"
    for workspace in ("automation", "runner"):
        run(f"tsc_{workspace}", [node, tsc, "-p", jobs / workspace / "tsconfig.json"], jobs, env)
    probe = """
import assert from 'node:assert/strict';
import { statSync, realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
const driver = pathToFileURL(process.argv[1]);
for (const [specifier, path] of Object.entries(JSON.parse(process.argv[2]))) {
  const actual = fileURLToPath(import.meta.resolve(specifier, driver));
  assert.equal(realpathSync(actual), resolve(path));
  assert(statSync(actual).isFile());
}
"""
    run("static_module_resolution", [node, "--experimental-import-meta-resolve", "--input-type=module",
                                     "-e", probe, driver, json.dumps(ENTRIES)], jobs, env)
    # The exact fixture must load its dependency graph and refuse empty input
    # before a claim, HTTP request, browser, or write can be attempted.
    startup = subprocess.run([node, str(driver)], cwd=root, env=env, stdin=subprocess.DEVNULL,
                             capture_output=True, timeout=15)
    refused = {"schema": "phase658_managed_receipt_journey_node_result_v1",
               "ok": False, "code": "unexpected_eof", "phase": "input"}
    check(startup.returncode == 1 and not startup.stderr and startup.stdout.endswith(b"\n")
          and startup.stdout.count(b"\n") == 1 and json.loads(startup.stdout) == refused,
          "driver_empty_input_refusal")
    for name, (body, mode) in rows.items():
        path = jobs / name
        check(not path.is_symlink() and path.stat().st_mode & 0o7777 == mode
              and sha(path) == hashlib.sha256(body).hexdigest(), "compiler_source_changed")
    check(contract["marker_for"](root, os.getpgrp()) == marker, "run_marker_changed")
    outputs = manifest(jobs)
    inventory = json.dumps(outputs, separators=(",", ":")).encode()
    print(json.dumps({"tree": TREE, "platform": "darwin-arm64", "cacheTarSha256": CACHE_TAR,
                      "lockSha256": LOCK, "driverSha256": DRIVER, "offlineInstall": True,
                      "directResolvedImports": len(ENTRIES), "driverStartup": "unexpected_eof",
                      "compiledFiles": len(outputs),
                      "compiledBytes": sum(row[1] for row in outputs),
                      "inventorySha256": hashlib.sha256(inventory).hexdigest(),
                      "runtimeAdmitted": False, "outputsRetained": False}))


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"receipt journey build refused: {error if isinstance(error, RuntimeError) else type(error).__name__}",
              file=sys.stderr)
        raise SystemExit(1)
