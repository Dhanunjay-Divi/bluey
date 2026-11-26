"""Checks for sealed intake, without executing any staged byte."""
import gzip
import io
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tarfile
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("stage-browser-receipt-intake.py")
INTAKE = Path(os.environ["BLUEY_BROWSER_INTAKE_TEST_INPUTS"])
CODE = runpy.run_path(str(SCRIPT))
Refused = CODE["Refused"]


def run(mode, inputs=INTAKE, *args):
    return subprocess.run([sys.executable, "-I", "-B", SCRIPT, mode, inputs, *args],
                          capture_output=True, check=False, timeout=15)


class BrowserReceiptIntakeTests(unittest.TestCase):
    def test_actual_sealed_input_and_portal_destination(self):
        verified = run("verify")
        self.assertEqual((verified.returncode, verified.stderr), (0, b""))
        self.assertEqual(json.loads(verified.stdout)["sourceFiles"], 981)
        staged = run("stage-portal")
        self.assertEqual((staged.returncode, staged.stderr), (0, b""))
        receipt = json.loads(staged.stdout)
        portal = Path(os.environ["BLUEY_TEST_RUN_ROOT"]) / "receipt-prep-browser"
        self.assertEqual(Path(receipt["portalPath"]), portal)
        self.assertEqual(receipt["portalFiles"], 41)
        self.assertFalse(receipt["runtimeAdmitted"])
        self.assertFalse(receipt["browserRuntimeStaged"])
        self.assertTrue((portal / "driver.mjs").is_file())
        self.assertTrue((portal / "portal" / "index.html").is_file())
        self.assertFalse((portal / "chromium").exists())
        self.assertFalse((portal / "node_modules").exists())
        repeated = run("stage-portal")
        self.assertEqual((repeated.returncode, repeated.stdout), (1, b""))
        self.assertEqual(repeated.stderr, b"Browser receipt intake refused: FileExistsError\n")

    def test_actual_source_stage_matches_case_and_wrapper(self):
        with tempfile.TemporaryDirectory() as name:
            parent = Path(name) / ("bluey-jobs-receipt-intake-" + "a" * 16)
            parent.mkdir(mode=0o700)
            staged = run("stage-source", INTAKE, "--source-parent", parent)
            self.assertEqual((staged.returncode, staged.stderr), (0, b""))
            receipt = json.loads(staged.stdout)
            self.assertFalse(receipt["gitInitialized"])
            self.assertFalse(receipt["runtimeAdmitted"])
            self.assertEqual(receipt["sourceFiles"], 981)
            source = Path(receipt["sourcePath"])
            self.assertEqual(source, parent / "source")
            self.assertFalse((source / ".git").exists())
            wrapper = (source / "jobs/scripts/run-nonprod-postgres-tests.mjs").read_text()
            self.assertIn("postgres_managed_receipt_browser_downloads_interview_prep", wrapper)
            self.assertTrue((source / "server/Cargo.toml").is_file())
            self.assertTrue(os.stat(source / "scripts/run-bluey-tests.sh").st_mode & 0o100)
            # Isolated Git metadata is a separate launcher prerequisite.
            # Prove it can be initialized without changing any staged id
            # or starting the shell test launcher under this Python test.
            git = ("/Library/Developer/CommandLineTools/usr/bin/git" if sys.platform == "darwin"
                   else "/usr/bin/git")
            def command(*argv):
                return subprocess.run([git, *argv], cwd=source,
                                      env={"PATH": "/usr/bin:/bin", "HOME": name,
                                           "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null"},
                                      capture_output=True, check=False, timeout=5)
            self.assertNotEqual(command("worktree", "list", "--porcelain").returncode, 0)
            self.assertEqual(command("init", "-q", "--template=").returncode, 0)
            listing = command("worktree", "list", "--porcelain")
            self.assertEqual(listing.returncode, 0)
            self.assertEqual([line for line in listing.stdout.decode().splitlines()
                              if line.startswith("worktree ")], [f"worktree {source}"])
            self.assertEqual((source / "jobs/scripts/run-nonprod-postgres-tests.mjs").read_text(), wrapper)

    def test_mutated_archive_does_not_create_destination(self):
        with tempfile.TemporaryDirectory() as name:
            input_copy = Path(name) / "inputs"
            input_copy.mkdir()
            for path in INTAKE.iterdir():
                data = path.read_bytes()
                if path.name == "browser-receipt-portal.tar.gz":
                    data = data[:-1] + bytes([data[-1] ^ 1])
                (input_copy / path.name).write_bytes(data)
            target = Path(name) / ("bluey-jobs-receipt-intake-" + "b" * 16)
            target.mkdir(mode=0o700)
            refused = run("stage-source", input_copy, "--source-parent", target)
            self.assertEqual((refused.returncode, refused.stdout), (1, b""))
            self.assertEqual(refused.stderr, b"Browser receipt intake refused: input_digest\n")
            self.assertEqual(list(target.iterdir()), [])

    def test_regular_members_must_match_the_exact_inventory(self):
        expected = [["file", CODE["sha"](b"hello"), 0o600, 5]]
        def fixture(mode=0o600, data=b"hello", name="file", extra=False):
            buffer = io.BytesIO()
            with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as tar:
                info = tarfile.TarInfo(name)
                info.mode, info.size = mode, len(data)
                tar.addfile(info, io.BytesIO(data))
                if extra:
                    tar.addfile(info, io.BytesIO(data))
            return gzip.compress(buffer.getvalue(), mtime=0)
        self.assertEqual(CODE["unpack"](fixture(), expected, 1, {0o600}), {"file": (b"hello", 0o600)})
        for payload, reason in [(fixture(mode=0o644), "archive_member_metadata"),
                                (fixture(data=b"wrong"), "archive_member_digest"),
                                (fixture(name="extra"), "archive_member"),
                                (fixture(extra=True), "archive_member")]:
            with self.assertRaisesRegex(Refused, reason):
                CODE["unpack"](payload, expected, 1, {0o600})

    def test_portal_entry_retains_running_marker_binding(self):
        root = Path(os.environ["BLUEY_TEST_RUN_ROOT"])
        valid = CODE["marker_for"](root, os.getpgrp())
        self.assertTrue(valid.endswith(b"state=running\n"))
        with self.assertRaisesRegex(Refused, "run_marker_identity"):
            CODE["marker_for"](root, os.getpgrp() + 1)


if __name__ == "__main__":
    unittest.main()
