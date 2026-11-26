"""Permission changes are exercised only on inert synthetic owned files."""
import importlib.util
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "modes", Path(__file__).with_name("receipt-browser-native-modes.py")
)
MODES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODES)
STAGER = Path(__file__).with_name("stage-browser-receipt-runtime.py")
RUNTIME = {"__name__": "fixture_runtime", "__file__": str(STAGER)}
exec(compile(STAGER.read_bytes(), str(STAGER), "exec"), RUNTIME)


class NativeModes(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name).resolve()
        (self.root / "chromium").mkdir(mode=0o700)
        self.paths = sorted(set(MODES.EXECUTABLES) | {"chromium/chrome_sandbox", "driver.mjs"})
        self.rows = []
        for name in self.paths:
            payload = f"synthetic: {name}\n".encode()
            path = self.root / name
            path.write_bytes(payload)
            path.chmod(0o600)
            self.rows.append((name, MODES.sha(payload), len(payload), 0o755))
        self.fd = os.open(self.root, MODES.DIR)
        self.addCleanup(os.close, self.fd)

    def mode(self, name):
        return stat.S_IMODE((self.root / name).stat().st_mode)

    def test_only_two_selected_paths_transition(self):
        MODES.transition(self.fd, self.rows, RUNTIME)
        self.assertEqual(self.mode("chromium/chrome"), 0o700)
        self.assertEqual(self.mode("chromium/chrome_crashpad_handler"), 0o700)
        self.assertEqual(self.mode("chromium/chrome_sandbox"), 0o600)
        self.assertEqual(self.mode("driver.mjs"), 0o600)
        with self.assertRaisesRegex(RuntimeError, "^native_mode_file$"):
            MODES.readback(self.fd, self.rows, False, RUNTIME)

    def test_content_drift_refuses_whole_transition(self):
        path = self.root / "chromium/chrome_crashpad_handler"
        path.write_bytes(b"x" * path.stat().st_size)
        with self.assertRaisesRegex(RuntimeError, "^native_mode_content$"):
            MODES.transition(self.fd, self.rows, RUNTIME)
        self.assertEqual(self.mode("chromium/chrome"), 0o600)

    def test_setuid_or_wider_mode_refused(self):
        (self.root / "chromium/chrome_sandbox").chmod(0o4700)
        with self.assertRaisesRegex(RuntimeError, "^native_mode_file$"):
            MODES.transition(self.fd, self.rows, RUNTIME)
        self.assertEqual(self.mode("chromium/chrome"), 0o600)

    def test_additional_hardlink_refused(self):
        with tempfile.TemporaryDirectory() as other:
            os.link(self.root / "chromium/chrome", Path(other) / "link")
            with self.assertRaisesRegex(RuntimeError, "^native_mode_file$"):
                MODES.transition(self.fd, self.rows, RUNTIME)

    def test_fake_inventory_does_not_match_actual_fixture(self):
        with self.assertRaisesRegex(RuntimeError, "^native_capsule_inventory$"):
            MODES.expect_archive_inventory(self.rows)

    @unittest.skipIf(sys.platform == "linux", "negative control is a non-Linux host")
    def test_non_linux_actor_cannot_enter_materialization(self):
        with self.assertRaisesRegex(RuntimeError, "^native_linux_actor$"):
            MODES.evaluate(None, None, None, action="materialize", independently_admitted=lambda *_: True)


if __name__ == "__main__":
    unittest.main()
