"""Workload-side assembly for the frozen, synthetic Linux browser receipt case."""
from contextlib import ExitStack
import hashlib
import os
from pathlib import Path
import platform
import re
import selectors
import stat
import subprocess
import sys
import time

PINS = {
    "stage-browser-receipt-intake.py": "07426bad5f55f7fe3ea99ccab025571ffabffe310e204c7cf64ec4c20dd682dc",
    "stage-receipt-browser-capsule.py": "7a6a3e7a5824bbd94ca5beac62be97b37c240d90b7d79da954d32b5208ea01b5",
    "transfer-receipt-journey-closure.py": "e99b4aeb805d2c3671aab4af8ff3ffb75b08cbf7cf005c09a2a906001dc7cf84",
    "receipt-browser-native-modes.py": "e515c61fd0b4e34196b596c8657011ca3352e5375fd87521ceaa127b377af7dc",
    "receipt-browser-launch-barrier.py": "8a043d7b14ea86cab295b58215b7773aafca7bd3fd4bbf3fa5aee6a8a454dadf",
}
WRAPPER = "jobs/scripts/run-nonprod-postgres-tests.mjs"
WRAPPER_SHA = "556dbce559c2547d8059c44d25fe7557a46ccf7471bb0f86f2ea0aab41878ffa"
CASE = ("jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::"
        "postgres_managed_receipt_browser_downloads_interview_prep")
FENCE = "qualified-linux-interview-prep-fixture-v1"
ROOT = re.compile(r"/tmp/bluey-test-runs-[a-f0-9]{12}/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}")
SAFE_DIR = re.compile(r"/[A-Za-z0-9._/-]+")
DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
CAP_OUT = 2 * 1024**2
CAP_ERR = 4 * 1024**2


def check(ok, code):
    if not ok:
        raise RuntimeError(code)


def helper(name):
    path = Path(__file__).resolve().with_name(name)
    with path.open("rb") as stream:
        body = stream.read(64 * 1024)
    check(hashlib.sha256(body).hexdigest() == PINS[name], "workload_helper_pin")
    scope = {"__name__": "verified_workload_helper", "__file__": str(path)}
    exec(compile(body, str(path), "exec"), scope)
    return scope


def command(source, root, postgres):
    return [
        str(root / "receipt-journey-node/toolchain/bin/node"), str(source / WRAPPER),
        "--pg-bin", str(postgres), "--case-filter", CASE, "--",
        "cargo", "test", "--manifest-path", "server/Cargo.toml", "--lib", "--locked",
        "--offline", "--features", "integration-test-support", "--jobs", "2",
    ]


def closed_env(root, tools):
    return {
        "PATH": f"{tools['cargo_bin']}:/usr/bin:/bin",
        "LANG": "C", "LC_ALL": "C", "HOME": str(root / "home"),
        "CARGO_HOME": str(tools["cargo_home"]), "RUSTUP_HOME": str(tools["rustup_home"]),
        "CARGO_TARGET_DIR": str(root / "cargo-target"), "CARGO_INCREMENTAL": "0",
        **{name: str(root / "tmp") for name in ("TMPDIR", "TMP", "TEMP", "SQLITE_TMPDIR")},
        "BLUEY_TEST_RUN_ROOT": str(root), "BLUEY_DB_PATH": str(root / "data/bluey-test.sqlite"),
        "BLUEY_SERVER_DB_BACKEND": "sqlite",
        **{f"{prefix}_{name.upper()}_DIR": str(root / subdir)
           for prefix in ("BLUEY", "CUE")
           for name, subdir in (("data", "data"), ("config", "config"),
                                ("runtime", "runtime"), ("log", "logs"))},
        **{f"CARGO_PROFILE_{item}_{key}": value
           for item in ("DEV", "TEST") for key, value in (("INCREMENTAL", "false"), ("DEBUG", "0"))},
        "BLUEY_INTERVIEW_PREP_OUTER_FENCE": FENCE,
    }


def source_check(source, source_rows, intake):
    fd = os.open(source, DIR)
    try:
        info = os.fstat(fd)
        check(info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700,
              "workload_source_owner")
        expected_dirs = {p.as_posix() for name in source_rows for p in Path(name).parents}
        seen, seen_dirs = set(), set()
        for directory, dirs, files, held in os.fwalk(".", dir_fd=fd, follow_symlinks=False):
            rel = Path(directory)
            check(rel.as_posix() in expected_dirs, "workload_source_directory")
            seen_dirs.add(rel.as_posix())
            if rel == Path("."):
                # Canonical launcher needs disposable metadata. It must
                # not be an owner checkout .git pointer or symlink.
                check(".git" in dirs, "workload_source_git")
                meta = os.stat(".git", dir_fd=held, follow_symlinks=False)
                check(stat.S_ISDIR(meta.st_mode) and meta.st_uid == os.getuid()
                      and not stat.S_IMODE(meta.st_mode) & 0o022, "workload_source_git")
                dirs.remove(".git")
            for child in dirs:
                item = os.stat(child, dir_fd=held, follow_symlinks=False)
                check(stat.S_ISDIR(item.st_mode) and item.st_uid == os.getuid()
                      and stat.S_IMODE(item.st_mode) == 0o700
                      and (rel / child).as_posix() in expected_dirs, "workload_source_directory")
            for child in files:
                name = (rel / child).as_posix()
                check(name in source_rows, "workload_source_entry")
                data, mode = source_rows[name]
                observed = intake["pinned_read"](held, child, len(data), owned_mode=mode)
                check(observed == data, "workload_source_bytes")
                seen.add(name)
        check(seen == source_rows.keys() and seen_dirs == expected_dirs
              and os.path.samestat(info, os.stat(source, follow_symlinks=False)),
              "workload_source_inventory")
    finally:
        os.close(fd)


def bounded_child(argv, source, env, seconds):
    """Separate raw wrapper streams from the supervisor's own event channel."""
    deadline = time.monotonic() + seconds
    captured = [bytearray(), bytearray()]
    with subprocess.Popen(argv, cwd=source, env=env, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(child.stdout, selectors.EVENT_READ, 0)
                selector.register(child.stderr, selectors.EVENT_READ, 1)
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    check(remaining > 0, "workload_child_deadline")
                    for key, _ in selector.select(remaining):
                        chunk = os.read(key.fd, 65536)
                        if not chunk:
                            selector.unregister(key.fileobj)
                        else:
                            buf = captured[key.data]
                            buf.extend(chunk)
                            check(len(buf) <= (CAP_OUT, CAP_ERR)[key.data], "workload_child_output_bound")
            remaining = deadline - time.monotonic()
            check(remaining > 0, "workload_child_deadline")
            status = child.wait(timeout=remaining)
        except BaseException:
            # Direct child only. The external watchdog and canonical
            # group/cgroup proof still own the descendants and timeout case.
            if child.poll() is None:
                child.kill()
            child.wait(timeout=5)
            raise
    return status, bytes(captured[0]), bytes(captured[1])


def run_sequence(stage_journey, stage_browser, hold, materialize, run, postflight, emit,
                 export_streams):
    journey = stage_journey()
    browser = stage_browser()
    check(journey.get("runtimeAdmitted") is False and browser.get("runtimeAdmitted") is False
          and browser.get("chromiumLaunchEnabled") is False and browser.get("fileMode") == "0600",
          "workload_inert_intake")
    release, authorized = hold(journey["inventorySha256"])
    scope = {key: release[key] for key in ("run", "boot", "invocation", "challenge")}
    permission = materialize(authorized)
    check(permission["runtimeAdmitted"] is False and permission["observedExecutableFiles"] == 2
          and permission["inventorySha256"] == browser["capsuleInventorySha256"],
          "workload_native_binding")
    try:
        status, out, err = run(authorized, journey["inventorySha256"], browser["capsuleInventorySha256"])
    finally:
        postflight(journey["inventorySha256"], browser["capsuleInventorySha256"])
    check(export_streams(scope, out, err) is True, "workload_stream_export")
    event = {
        "event": "jobs_browser_owned_wrapper_terminal", **scope, "code": status if status >= 0 else None,
        "signalNumber": -status if status < 0 else None,
        "stdoutBytes": len(out), "stderrBytes": len(err),
        "stdoutSha256": hashlib.sha256(out).hexdigest(), "stderrSha256": hashlib.sha256(err).hexdigest(),
        "inputReadback": True, "runtimeAdmitted": False,
    }
    emit(event)
    check(status == 0, "workload_wrapper_nonzero")
    return event


def run_workload(*, run, inputs, playwright, chrome, journey_tar, journey_sha256,
                 tools, verify_tools, emit, export_streams, seconds):
    """For a separately admitted synthetic controller. No standalone CLI."""
    check(sys.platform == "linux" and platform.machine() == "x86_64" and os.getuid() != 0
          and os.geteuid() == os.getuid(), "workload_linux_actor")
    check(type(seconds) is int and 1 <= seconds <= 1700 and callable(verify_tools)
          and callable(emit) and callable(export_streams) and emit is not export_streams,
          "workload_parameters")
    check(set(tools) == {"postgres", "cargo_bin", "cargo_home", "rustup_home"}
          and all(isinstance(path, Path) and SAFE_DIR.fullmatch(str(path))
                  and path.resolve(strict=True) == path and path.is_dir() for path in tools.values()),
          "workload_tool_paths")
    check(verify_tools(tools) is True, "workload_tool_authority")
    os.umask(0o077)
    root = Path(os.environ.get("BLUEY_TEST_RUN_ROOT", ""))
    source = Path.cwd()
    check(ROOT.fullmatch(str(root)) and root.resolve(strict=True) == root
          and not source.is_relative_to(root) and not root.is_relative_to(source), "workload_root")
    root_info = os.stat(root, follow_symlinks=False)
    check(root_info.st_uid == os.getuid() and stat.S_IMODE(root_info.st_mode) == 0o700, "workload_root_mode")
    intake = helper("stage-browser-receipt-intake.py")
    marker = intake["marker_for"](root, os.getpgrp())
    _, data = intake["load_inputs"](inputs)
    check(hashlib.sha256(data["source"][WRAPPER][0]).hexdigest() == WRAPPER_SHA,
          "workload_wrapper_pin")
    source_check(source, data["source"], intake)
    transfer = helper("transfer-receipt-journey-closure.py")
    capsule = helper("stage-receipt-browser-capsule.py")
    barrier = helper("receipt-browser-launch-barrier.py")
    native = helper("receipt-browser-native-modes.py")

    def postflight(journey_inventory, browser_inventory):
        check(transfer["receive"]("readback", journey_tar, journey_sha256, root)["inventorySha256"]
              == journey_inventory, "workload_journey_postflight")
        check(native["evaluate"](inputs, playwright, chrome,
                                action="readback-executable")["inventorySha256"]
              == browser_inventory, "workload_browser_postflight")
        source_check(source, data["source"], intake)
        check(verify_tools(tools) is True and intake["marker_for"](root, os.getpgrp()) == marker,
              "workload_postflight_authority")

    def admitted_start(authorized, journey_inventory, browser_inventory):
        # Inputs may not drift while waiting for the supervisor. Re-use
        # the same complete checks immediately before the wrapper is run.
        postflight(journey_inventory, browser_inventory)
        leaf = os.stat(root / "receipt-prep-browser", follow_symlinks=False)
        check(authorized(browser_inventory, leaf) is True, "workload_release_expired")
        return bounded_child(command(source, root, tools["postgres"]), source,
                             closed_env(root, tools), seconds)

    with ExitStack() as stack:
        return run_sequence(
            lambda: transfer["receive"]("stage", journey_tar, journey_sha256, root),
            lambda: capsule["assemble"]("stage", inputs, playwright, chrome),
            lambda closure: barrier["hold"](run, closure, stack=stack, emit=emit),
            lambda authorized: native["evaluate"](inputs, playwright, chrome,
                                                   action="materialize", independently_admitted=authorized),
            admitted_start,
            postflight, emit, export_streams,
        )
