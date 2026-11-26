#!/usr/bin/env python3
"""Portable controls for the inert C2C browser installer.

The positive archive case only measures the three explicitly supplied source
archives.  Negative filesystem cases use symlinks, directories, or tiny files;
they never copy the large inputs or materialize the browser closure.
"""
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import runpy
import stat
import sys
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
MODULE_PATH = HERE / "local-qa-linux-c2c-install-browser.py"
REFUSED = r"c2c_browser_install_refused:"


def load_module():
    return runpy.run_path(str(MODULE_PATH), run_name="_c2c_browser_install_test")


def file_stamp(path):
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


class BrowserInstallTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_module()
        cls.inventory_path = Path(os.environ["C2C_BROWSER_INVENTORY"])
        cls.inventory_raw = cls.inventory_path.read_bytes()
        cls.paths = {
            "playwright": Path(os.environ["C2C_PLAYWRIGHT_ARCHIVE"]),
            "chrome": Path(os.environ["C2C_CHROME_ARCHIVE"]),
            "node": Path(os.environ["C2C_NODE_ARCHIVE"]),
        }
        if not cls.inventory_path.is_absolute() or not all(path.is_absolute() for path in cls.paths.values()):
            raise AssertionError("all browser-install fixtures must be explicit absolute paths")

    def refused(self, function, *args, **kwargs):
        with self.assertRaisesRegex(ValueError, REFUSED):
            function(*args, **kwargs)

    def archive_refused(self, paths):
        # The pinned archive reader has its own RuntimeError refusal type;
        # caller-shape failures remain the installer's ValueError type.
        with self.assertRaises((ValueError, RuntimeError, OSError)):
            self.module["verify_archives"](paths, self.inventory_raw)

    def canonical(self, value):
        return (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")

    def validate_with_matching_test_pin(self, raw):
        old = self.module["INVENTORY_SHA"]
        self.module["INVENTORY_SHA"] = hashlib.sha256(raw).hexdigest()
        try:
            return self.module["validate_inventory"](raw)
        finally:
            self.module["INVENTORY_SHA"] = old

    def test_inventory_is_exact_ordered_closure_with_three_launch_executables(self):
        rows = self.module["validate_inventory"](self.inventory_raw)
        expected_exec = {
            "receipt-journey-node/toolchain/bin/node",
            "receipt-prep-browser/chromium/chrome",
            "receipt-prep-browser/chromium/chrome_crashpad_handler",
        }
        self.assertEqual(hashlib.sha256(self.inventory_raw).hexdigest(), self.module["INVENTORY_SHA"])
        self.assertEqual(len(rows), 410)
        self.assertEqual(list(rows), sorted(rows))
        self.assertEqual(self.module["EXECUTE"], expected_exec)
        self.assertEqual({name for name, row in rows.items() if row["mode"] == 0o555}, expected_exec)
        self.assertTrue(all(row["mode"] == (0o555 if name in expected_exec else 0o444)
                            for name, row in rows.items()))
        self.assertEqual(sum(row["size"] for row in rows.values()), 533_939_096)

    def test_inventory_pin_precedes_shape_and_shape_rejects_drift(self):
        self.refused(self.module["validate_inventory"], self.inventory_raw[:-1])
        original = json.loads(self.inventory_raw)

        def changed(mutator):
            value = deepcopy(original)
            mutator(value)
            return self.canonical(value)

        cases = {
            "version": changed(lambda value: value.update(version=2)),
            "extra-row-key": changed(lambda value: value["files"][0].update(extra=True)),
            "path-traversal": changed(lambda value: value["files"][0].update(path="../node")),
            "unordered": changed(lambda value: value["files"].__setitem__(
                slice(0, 2), list(reversed(value["files"][:2]))
            )),
            "bad-digest": changed(lambda value: value["files"][0].update(sha256="0" * 64)),
            "bool-size": changed(lambda value: value["files"][0].update(size=True)),
            "executable-mode": changed(lambda value: value["files"][0].update(mode=0o444)),
            "payload-total": changed(lambda value: value["files"][-1].update(
                size=value["files"][-1]["size"] + 1
            )),
        }
        for name, raw in cases.items():
            with self.subTest(name=name):
                self.refused(self.validate_with_matching_test_pin, raw)

    def test_archive_measurement_matches_whole_inventory_without_installing(self):
        before = {name: file_stamp(path) for name, path in self.paths.items()}
        result = self.module["verify_archives"](dict(self.paths), self.inventory_raw)
        self.assertEqual(result, {
            "inventorySha256": self.module["INVENTORY_SHA"],
            "files": 410,
            "payloadBytes": 533_939_096,
            "installed": False,
            "runtimeQualified": False,
        })
        self.assertEqual({name: file_stamp(path) for name, path in self.paths.items()}, before)

    def test_archive_path_contract_refuses_before_large_input_copy(self):
        self.archive_refused({"playwright": self.paths["playwright"]})
        self.archive_refused({**self.paths, "extra": self.paths["node"]})
        self.archive_refused({**self.paths, "playwright": str(self.paths["playwright"])})
        self.archive_refused({**self.paths, "playwright": Path("relative.tgz")})

        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name).resolve(strict=True)
        alias = root / "playwright-link.tgz"
        alias.symlink_to(self.paths["playwright"])
        tiny = root / "tiny.tgz"
        tiny.write_bytes(b"not-an-archive\n")
        tiny.chmod(0o600)
        directory = root / "directory.tgz"
        directory.mkdir()
        for name, replacement in (("symlink", alias), ("tiny", tiny), ("directory", directory)):
            with self.subTest(name=name):
                self.archive_refused({**self.paths, "playwright": replacement})

    def test_sealed_materialization_is_denied_before_any_non_linux_write(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name).resolve(strict=True)
        parent = os.open(root, self.module["DIR"])
        original = self.module["linux_root"]
        self.module["linux_root"] = lambda: False
        try:
            self.refused(
                self.module["materialize"],
                parent,
                "1" * 32,
                dict(self.paths),
                self.inventory_raw,
                live_check=lambda: True,
                sealed=True,
            )
        finally:
            self.module["linux_root"] = original
            os.close(parent)
        self.assertEqual(list(root.iterdir()), [])
        if sys.platform == "darwin":
            self.assertIs(original(), False)


if __name__ == "__main__":
    unittest.main()
