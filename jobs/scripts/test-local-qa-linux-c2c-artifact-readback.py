"""Portable adversarial controls for descriptor-bound C2C artifact readback."""
import hashlib
import importlib.util
import os
from pathlib import Path
import stat
import tempfile
import unittest
from contextlib import contextmanager
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "c2c_artifact_readback", HERE / "local-qa-linux-c2c-artifact-readback.py")
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def digest(value):
    return hashlib.sha256(value).hexdigest()


class ArtifactReadback(unittest.TestCase):
    def setUp(self):
        # The test launcher supplies an owned TMPDIR below its canonical test
        # root.  The view FD makes the portable tree independent of host `/`.
        self.temporary = tempfile.TemporaryDirectory(prefix="bluey-c2c-artifact-", dir=os.environ["TMPDIR"])
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve(strict=True)
        self.owner = os.getuid()
        self.view = os.open(self.root, M.DIR)
        self.addCleanup(os.close, self.view)
        self.body, self.other = b"sealed-one\n", b"sealed-two\n"
        self._make_tree()

    def _mkdir(self, path):
        path.mkdir(mode=0o755)
        path.chmod(0o755)

    def _write(self, path, body, mode=0o444):
        with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600), "wb") as stream:
            self.assertEqual(stream.write(body), len(body))
        path.chmod(mode)

    def _make_tree(self):
        self.tree = self.root / "tree"
        self._mkdir(self.tree)
        self._mkdir(self.tree / "sub")
        self._write(self.tree / "one", self.body)
        self._write(self.tree / "sub" / "two", self.other)
        os.symlink("one", self.tree / "one-link")
        self.files = {
            "one": ("file", 0o444, len(self.body), digest(self.body)),
            "one-link": ("link", 0o777, None, "one"),
            "sub/two": ("file", 0o444, len(self.other), digest(self.other)),
        }
        self.layout = M.inventory({"/tree": ({".": 0o755, "sub": 0o755}, self.files)})

    def measure(self, *, baseline=None):
        with self.portable_link_mode():
            return M.measure(self.view, self.layout, baseline=baseline, owner=self.owner)

    @contextmanager
    def portable_link_mode(self):
        """Darwin reports symlink mode 0755; Linux's fixed mode is 0777."""
        if os.uname().sysname != "Darwin":
            yield
            return
        actual = M.os.stat

        def projected(path, *args, **kwargs):
            row = actual(path, *args, **kwargs)
            if path == "one-link" and kwargs.get("follow_symlinks") is False:
                values = list(row)
                values[stat.ST_MODE] = stat.S_IFLNK | 0o777
                return os.stat_result(values)
            return row

        with patch.object(M.os, "stat", side_effect=projected):
            yield

    @staticmethod
    def rewrite(path, body):
        path.chmod(0o600)
        with path.open("r+b") as stream:
            stream.write(body)
        path.chmod(0o444)

    def refused(self, callback, label):
        with self.assertRaisesRegex(ValueError, "c2c_artifact_readback_refused:" + label):
            callback()

    def test_capture_hashes_full_bytes_and_replay_uses_no_content_read(self):
        self.refused(lambda: self._capture_drift(), "file_pin")
        baseline = self.measure()
        with patch.object(M, "_file", side_effect=AssertionError("replay read bytes")) as read:
            self.assertEqual(self.measure(baseline=baseline), baseline)
        read.assert_not_called()

    def _capture_drift(self):
        self.rewrite(self.tree / "one", b"sealed-Xne\n")
        try:
            self.measure()
        finally:
            self.rewrite(self.tree / "one", self.body)

    def test_same_inode_same_size_content_drift_and_mode_or_owner_drift_refuse(self):
        baseline = self.measure()
        before = (self.tree / "one").stat()
        self.rewrite(self.tree / "one", b"sealed-Xne\n")
        after = (self.tree / "one").stat()
        self.assertEqual((before.st_dev, before.st_ino, before.st_size),
                         (after.st_dev, after.st_ino, after.st_size))
        self.refused(lambda: self.measure(baseline=baseline), "leaf_fingerprint")
        self.rewrite(self.tree / "one", self.body)
        (self.tree / "one").chmod(0o400)
        self.refused(self.measure, "leaf_fingerprint")
        (self.tree / "one").chmod(0o444)

        actual_stat = M.os.stat

        def wrong_owner(path, *args, **kwargs):
            result = actual_stat(path, *args, **kwargs)
            if path == "one" and kwargs.get("follow_symlinks") is False:
                values = list(result)
                values[stat.ST_UID] = self.owner + 1
                return os.stat_result(values)
            return result

        with patch.object(M.os, "stat", side_effect=wrong_owner):
            self.refused(self.measure, "leaf_fingerprint")

    def test_different_root_extra_file_and_symlink_substitutions_refuse(self):
        baseline = self.measure()
        twin_parent = self.root / "twin"
        self._mkdir(twin_parent)
        twin = twin_parent / "tree"
        self._mkdir(twin)
        self._mkdir(twin / "sub")
        self._write(twin / "one", self.body)
        self._write(twin / "sub" / "two", self.other)
        os.symlink("one", twin / "one-link")
        twin_fd = os.open(twin_parent, M.DIR)
        try:
            with self.portable_link_mode():
                self.refused(lambda: M.measure(twin_fd, self.layout, baseline=baseline, owner=self.owner),
                             "directory_fingerprint")
        finally:
            os.close(twin_fd)

        (self.tree / "extra").write_bytes(b"unexpected\n")
        self.refused(self.measure, "member_inventory")
        (self.tree / "extra").unlink()
        (self.tree / "one-link").unlink()
        os.symlink("sub/two", self.tree / "one-link")
        self.refused(self.measure, "link_target")
        (self.tree / "one-link").unlink()
        os.symlink("one", self.tree / "one-link")
        nested = self.tree / "sub"
        replacement = self.root / "elsewhere"
        self._mkdir(replacement)
        self._write(replacement / "two", self.other)
        nested.rename(self.root / "sealed-sub")
        os.symlink("../elsewhere", nested)
        with self.assertRaises((OSError, ValueError)):
            self.measure()

    def test_plan_leaf_allows_publisher_siblings_but_binds_exact_bytes_and_identity(self):
        publisher = self.root / "publisher"
        self._mkdir(publisher)
        expected = b'{"version":1}\n'
        self._write(publisher / "plan.json", expected)
        self._write(publisher / "publisher-owned.json", b"later\n")
        first = M.plan_leaf(self.view, "/publisher", expected, owner=self.owner)
        self.assertEqual(M.plan_leaf(self.view, "/publisher", expected, baseline=first, owner=self.owner), first)
        self.rewrite(publisher / "plan.json", b'{"version":2}\n')
        self.refused(lambda: M.plan_leaf(self.view, "/publisher", expected, baseline=first,
                                         owner=self.owner), "plan_identity")
        self.refused(lambda: M.plan_leaf(self.view, "/publisher", expected, owner=self.owner), "file_pin")


if __name__ == "__main__":
    unittest.main(verbosity=2)
