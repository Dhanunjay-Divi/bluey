"""Order, refusal and custody coverage for the synthetic workload assembler."""
import hashlib
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "receipt_workload", Path(__file__).with_name("run-receipt-browser-workload.py")
)
ENTRY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ENTRY)


class Workload(unittest.TestCase):
    def setUp(self):
        self.events = []
        self.raw = []
        self.steps = []
        self.status = 0
        self.authorization = object()
        self.scope = {"run": "a" * 32, "boot": "b" * 36, "invocation": "c" * 32, "challenge": "d" * 32}
        self.browser = {"runtimeAdmitted": False, "chromiumLaunchEnabled": False, "fileMode": "0600",
                        "capsuleInventorySha256": "browser"}
        self.permission = {"runtimeAdmitted": False, "observedExecutableFiles": 2,
                           "inventorySha256": "browser"}

    def invoke(self, *, blocked=False, changed=False, lost_stream=False):
        def stage_journey():
            self.steps.append("journey")
            return {"runtimeAdmitted": False, "inventorySha256": "journey"}

        def stage_browser():
            self.steps.append("browser")
            return self.browser

        def gate(closure):
            self.assertEqual(closure, "journey")
            self.steps.append("hold")
            if blocked:
                raise RuntimeError("test_release_refused")
            return self.scope, self.authorization

        def modes(authorized):
            self.assertIs(authorized, self.authorization)
            self.steps.append("modes")
            return self.permission

        def run(authorized, journey, browser):
            self.assertIs(authorized, self.authorization)
            self.assertEqual((journey, browser), ("journey", "browser"))
            self.steps.append("wrapper")
            return self.status, b"exact stdout\n", b"exact stderr\n"

        def after(journey, browser):
            self.assertEqual((journey, browser), ("journey", "browser"))
            self.steps.append("readback")
            if changed:
                raise RuntimeError("test_postflight_drift")

        def export(scope, out, err):
            self.assertEqual(scope, self.scope)
            self.steps.append("stream-export")
            if lost_stream:
                return False
            self.raw.append((out, err))
            return True

        return ENTRY.run_sequence(stage_journey, stage_browser, gate, modes, run, after,
                                  self.events.append, export)

    def test_exact_order_and_lossless_streams(self):
        result = self.invoke()
        self.assertEqual(self.steps, ["journey", "browser", "hold", "modes", "wrapper",
                                      "readback", "stream-export"])
        self.assertIs(self.events[0], result)
        self.assertEqual(self.raw, [(b"exact stdout\n", b"exact stderr\n")])
        self.assertEqual(result["stdoutSha256"], hashlib.sha256(self.raw[0][0]).hexdigest())
        self.assertNotIn("stdoutBase64", result)
        self.assertEqual({name: result[name] for name in self.scope}, self.scope)
        self.assertFalse(result["runtimeAdmitted"])

    def test_wait_refusal_never_materializes_or_runs(self):
        with self.assertRaisesRegex(RuntimeError, "^test_release_refused$"):
            self.invoke(blocked=True)
        self.assertEqual(self.steps, ["journey", "browser", "hold"])
        self.assertEqual(self.events, [])

    def test_wrong_browser_materialization_never_runs(self):
        self.permission["inventorySha256"] = "other"
        with self.assertRaisesRegex(RuntimeError, "^workload_native_binding$"):
            self.invoke()
        self.assertEqual(self.steps, ["journey", "browser", "hold", "modes"])
        self.assertEqual(self.events, [])

    def test_failed_wrapper_is_reported_only_after_readback(self):
        self.status = 7
        with self.assertRaisesRegex(RuntimeError, "^workload_wrapper_nonzero$"):
            self.invoke()
        self.assertEqual(self.steps[-3:], ["wrapper", "readback", "stream-export"])
        self.assertEqual(self.events[0]["code"], 7)

    def test_drift_does_not_emit_success(self):
        with self.assertRaisesRegex(RuntimeError, "^test_postflight_drift$"):
            self.invoke(changed=True)
        self.assertEqual(self.events, [])

    def test_stream_export_failure_cannot_emit_success(self):
        with self.assertRaisesRegex(RuntimeError, "^workload_stream_export$"):
            self.invoke(lost_stream=True)
        self.assertEqual(self.events, [])

    def test_fixed_argv_closed_environment(self):
        source, root, tools_base = Path("/opt/source"), Path("/tmp/run"), Path("/opt/tools")
        tools = {key: tools_base / key for key in ("postgres", "cargo_bin", "cargo_home", "rustup_home")}
        argv = ENTRY.command(source, root, tools["postgres"])
        self.assertEqual(argv[4:7], ["--case-filter", ENTRY.CASE, "--"])
        self.assertEqual(argv[-6:], ["--locked", "--offline", "--features",
                                      "integration-test-support", "--jobs", "2"])
        env = ENTRY.closed_env(root, tools)
        self.assertEqual(env["HOME"], "/tmp/run/home")
        self.assertEqual(env["CARGO_TARGET_DIR"], "/tmp/run/cargo-target")
        self.assertEqual(env["BLUEY_INTERVIEW_PREP_OUTER_FENCE"], ENTRY.FENCE)
        for name in ("BLUEY_DATABASE_URL", "BLUEY_TEST_POSTGRES_URL", "OPENAI_API_KEY", "LD_PRELOAD"):
            self.assertNotIn(name, env)

    def test_real_child_streams_preserve_binary_and_exit_status(self):
        with tempfile.TemporaryDirectory() as scratch:
            command = [sys.executable, "-IB", "-c",
                       "import os; os.write(1,b'out\\0'); os.write(2,b'err\\n'); raise SystemExit(7)"]
            self.assertEqual(ENTRY.bounded_child(command, scratch, {}, 3), (7, b"out\0", b"err\n"))

    @unittest.skipIf(sys.platform == "linux", "non-Linux refusal control")
    def test_live_entry_refuses_this_host(self):
        with self.assertRaisesRegex(RuntimeError, "^workload_linux_actor$"):
            ENTRY.run_workload(run=None, inputs=None, playwright=None, chrome=None, journey_tar=None,
                               journey_sha256=None, tools=None, verify_tools=None, emit=None,
                               export_streams=None, seconds=3)


def exact_source(inputs):
    intake = ENTRY.helper("stage-browser-receipt-intake.py")
    _, payload = intake["load_inputs"](inputs)
    root = Path(os.environ["BLUEY_TEST_RUN_ROOT"])
    marker = intake["marker_for"](root, os.getpgrp())
    run = unittest.TestCase()
    intake["stage"](root, "source", payload["source"], lambda: run.assertEqual(
        intake["marker_for"](root, os.getpgrp()), marker))
    source = root / "source"
    git = "/Library/Developer/CommandLineTools/usr/bin/git" if sys.platform == "darwin" else "/usr/bin/git"
    result = subprocess.run([git, "init", "-q", "--template="], cwd=source, check=True,
                            env={"PATH": "/usr/bin:/bin", "HOME": str(root / "tmp"),
                                 "TMPDIR": str(root / "tmp")}, capture_output=True)
    run.assertEqual((result.stdout, result.stderr), (b"", b""))
    ENTRY.source_check(source, payload["source"], intake)
    run.assertEqual(len(payload["source"]), 981)
    row = source / ENTRY.WRAPPER
    original = row.read_bytes()
    run.assertEqual(hashlib.sha256(original).hexdigest(), ENTRY.WRAPPER_SHA)
    for expected in (ENTRY.CASE, "nonprod_postgres_test_inventory",
                     "owned receipt browser requires explicit outer controller admission",
                     "managed receipt wrapper must run from its staged source root"):
        run.assertIn(expected.encode(), original)
    row.write_bytes(b"!" + original[1:])
    with run.assertRaisesRegex(RuntimeError, "^workload_source_bytes$"):
        ENTRY.source_check(source, payload["source"], intake)
    print("981 exact source rows verified; same-size wrapper drift refused")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--exact-source":
        exact_source(Path(sys.argv[2]))
    else:
        unittest.main()
