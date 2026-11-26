#!/usr/bin/env python3
"""Portable controls for the synthetic C2C test-launcher source root.

All Git state and launcher execution stay inside owned temporary directories.
The fixture cleanup restores write permission only on the exact directories
made read-only by ``materialize_source``.
"""
from copy import deepcopy
import hashlib
import os
from pathlib import Path
import runpy
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
MODULE_PATH = HERE / "local-qa-linux-c2c-install-launcher.py"
LAUNCHER_PATH = HERE.parent.parent / "scripts" / "run-bluey-tests.sh"
REFUSED = r"c2c_launcher_install_refused:"


def load_module():
    return runpy.run_path(str(MODULE_PATH), run_name="_c2c_launcher_install_test")


def metadata(path):
    info = path.lstat()
    return (
        info.st_dev,
        info.st_ino,
        info.st_mode,
        info.st_uid,
        info.st_gid,
        info.st_nlink,
        info.st_size,
        info.st_mtime_ns,
        info.st_ctime_ns,
    )


class LauncherInstallTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_module()
        cls.raw = LAUNCHER_PATH.read_bytes()
        git = os.environ.get("C2C_GIT") or shutil.which("git")
        if not git:
            raise AssertionError("an explicit or system Git is required")
        cls.git = Path(git).resolve(strict=True)
        if hashlib.sha256(cls.raw).hexdigest() != cls.module["LAUNCHER_SHA"]:
            raise AssertionError("workspace launcher does not match installer pin")

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.base = Path(self.temporary.name).resolve(strict=True)
        self.base_identity = (self.base.lstat().st_dev, self.base.lstat().st_ino)
        self.parent = os.open(self.base, self.module["DIR"])
        self.held = []
        self.addCleanup(self.temporary.cleanup)
        self.addCleanup(self.restore_owned_fixture)

    def restore_owned_fixture(self):
        for descriptor in self.held:
            try:
                os.close(descriptor)
            except OSError:
                pass
        self.held.clear()
        try:
            os.close(self.parent)
        except OSError:
            pass
        if not self.base.exists():
            return
        current = self.base.lstat()
        self.assertEqual((current.st_dev, current.st_ino), self.base_identity)
        self.assertEqual(current.st_uid, os.getuid())
        for root, directories, _files in os.walk(self.base, topdown=True, followlinks=False):
            path = Path(root)
            info = path.lstat()
            self.assertTrue(stat.S_ISDIR(info.st_mode))
            self.assertEqual(info.st_uid, os.getuid())
            self.assertTrue(path == self.base or self.base in path.parents)
            path.chmod(0o700)
            for name in directories:
                child = path / name
                child_info = child.lstat()
                if stat.S_ISLNK(child_info.st_mode):
                    continue
                self.assertTrue(stat.S_ISDIR(child_info.st_mode))
                self.assertEqual(child_info.st_uid, os.getuid())

    def refused(self, function, *args, **kwargs):
        with self.assertRaisesRegex(ValueError, REFUSED):
            function(*args, **kwargs)

    def materialize(self, run="1" * 32):
        descriptor = self.module["materialize_source"](self.parent, run, self.raw)
        self.held.append(descriptor)
        return self.base / run, descriptor

    def snapshot(self, root):
        rows = {}
        for current, directories, files in os.walk(root, followlinks=False):
            where = Path(current)
            relative = where.relative_to(root).as_posix()
            rows[relative] = ("dir", metadata(where))
            for name in sorted(directories):
                child = where / name
                self.assertFalse(child.is_symlink())
            for name in sorted(files):
                path = where / name
                body = path.read_bytes()
                rows[(where / name).relative_to(root).as_posix()] = (
                    "file", metadata(path), hashlib.sha256(body).hexdigest()
                )
        return rows

    def controlled_environment(self, run_parent, home):
        path = os.pathsep.join(dict.fromkeys([
            str(self.git.parent), "/usr/bin", "/bin", "/usr/sbin", "/sbin"
        ]))
        environment = {
            "PATH": path,
            "HOME": str(home),
            "LC_ALL": "C",
            "LANG": "C",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": "/dev/null",
            "BLUEY_TEST_RUN_PARENT": str(run_parent),
        }
        developer = os.environ.get("DEVELOPER_DIR")
        if developer:
            environment["DEVELOPER_DIR"] = developer
        return environment

    def test_launcher_bytes_are_a_closed_pinned_source_map(self):
        files = self.module["launcher_bytes"](self.raw)
        self.assertEqual(set(files), {".git/HEAD", ".git/config", "scripts/run-bluey-tests.sh"})
        self.assertEqual(files[".git/HEAD"], (self.module["GIT_HEAD"], 0o444))
        self.assertEqual(files[".git/config"], (self.module["GIT_CONFIG"], 0o444))
        self.assertEqual(files["scripts/run-bluey-tests.sh"], (self.raw, 0o555))

        same_size = bytes([self.raw[0] ^ 1]) + self.raw[1:]
        self.assertEqual(len(same_size), len(self.raw))
        for value in (b"", self.raw + b"\n", same_size, bytearray(self.raw)):
            with self.subTest(kind=type(value).__name__, size=len(value)):
                self.refused(self.module["launcher_bytes"], value)

    def test_materialized_tree_is_readonly_and_real_git_reports_one_worktree(self):
        root, descriptor = self.materialize()
        files = self.module["launcher_bytes"](self.raw)
        self.module["verify_source"](descriptor, files)
        self.assertEqual(stat.S_IMODE(root.lstat().st_mode), 0o555)
        for name in self.module["DIRECTORIES"]:
            self.assertEqual(stat.S_IMODE((root / name).lstat().st_mode), 0o555)
        for name, (body, mode) in files.items():
            path = root / name
            self.assertEqual((path.read_bytes(), stat.S_IMODE(path.lstat().st_mode)), (body, mode))

        git = subprocess.run(
            [str(self.git), "-C", str(root), "worktree", "list", "--porcelain"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
            env=self.controlled_environment(self.base, self.base),
        )
        self.assertEqual((git.returncode, git.stderr), (0, ""))
        worktrees = [line for line in git.stdout.splitlines() if line.startswith("worktree ")]
        self.assertEqual(worktrees, ["worktree " + str(root)])
        self.assertIn("branch refs/heads/bluey-c2c-qa", git.stdout.splitlines())

    def test_canonical_launcher_runs_benign_command_and_cleans_owned_scratch(self):
        root, _descriptor = self.materialize()
        with tempfile.TemporaryDirectory(prefix="bluey-test-runs-", dir="/tmp") as run_name:
            run_parent = Path(run_name).resolve(strict=True)
            run_parent.chmod(0o700)
            home = self.base / "home"
            home.mkdir(mode=0o700)
            result = subprocess.run(
                [str(root / "scripts/run-bluey-tests.sh"), "--", "/bin/pwd"],
                check=False,
                capture_output=True,
                text=True,
                timeout=30,
                env=self.controlled_environment(run_parent, home),
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, str(root) + "\n")
            self.assertEqual(result.stderr, "")
            self.assertEqual(list(run_parent.iterdir()), [])

    def test_duplicate_run_never_overwrites_first_source(self):
        root, _descriptor = self.materialize()
        before = self.snapshot(root)
        self.refused(self.module["materialize_source"], self.parent, "1" * 32, self.raw)
        self.assertEqual(self.snapshot(root), before)

        wrong = bytes([self.raw[0] ^ 1]) + self.raw[1:]
        self.refused(self.module["materialize_source"], self.parent, "2" * 32, wrong)
        self.assertFalse((self.base / ("2" * 32)).exists())

    def test_verify_source_rejects_mode_and_same_size_body_drift(self):
        root, descriptor = self.materialize()
        files = self.module["launcher_bytes"](self.raw)
        head = root / ".git/HEAD"
        head.chmod(0o644)
        self.refused(self.module["verify_source"], descriptor, files)
        head.chmod(0o444)

        launcher = root / "scripts/run-bluey-tests.sh"
        changed = bytes([self.raw[0] ^ 1]) + self.raw[1:]
        launcher.chmod(0o755)
        launcher.write_bytes(changed)
        launcher.chmod(0o555)
        self.assertEqual(launcher.lstat().st_size, len(self.raw))
        self.refused(self.module["verify_source"], descriptor, files)

    def test_install_launcher_non_linux_guard_precedes_admission_and_paths(self):
        calls = []
        original = self.module["platform_root"]
        self.module["platform_root"] = lambda: False
        try:
            self.refused(
                self.module["install_launcher"],
                "1" * 32,
                "2" * 64,
                1000,
                1000,
                "/tmp/bluey-test-runs-123456abcdef",
                self.raw,
                admitted=lambda value: calls.append(deepcopy(value)) or True,
            )
        finally:
            self.module["platform_root"] = original
        self.assertEqual(calls, [])
        if sys.platform == "darwin":
            self.assertIs(original(), False)


if __name__ == "__main__":
    unittest.main()
