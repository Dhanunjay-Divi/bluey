#!/usr/bin/env python3
"""Portable controls for the inert C2C PostgreSQL capsule installer.

The retained archive is only measured or copied into an owned temporary tree.
No archive member is executed and no PostgreSQL process is started.
"""
from contextlib import contextmanager
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import runpy
import stat
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
MODULE_PATH = HERE / "local-qa-linux-c2c-install-postgres.py"
REFUSED = r"c2c_postgres_install_refused:"


def load_module():
    return runpy.run_path(str(MODULE_PATH), run_name="_c2c_postgres_install_test")


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


class PostgresInstallTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_module()
        cls.archive = Path(os.environ["C2C_PG_ARCHIVE"])
        cls.manifest_path = Path(os.environ["C2C_PG_MANIFEST"])
        if (not cls.archive.is_absolute() or not cls.manifest_path.is_absolute()
                or cls.archive.resolve(strict=True) != cls.archive
                or cls.manifest_path.resolve(strict=True) != cls.manifest_path):
            raise AssertionError("PostgreSQL capsule fixtures must be explicit absolute paths")
        cls.manifest_raw = cls.manifest_path.read_bytes()

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.base = Path(self.temporary.name).resolve(strict=True)
        root = self.base.lstat()
        self.base_identity = (root.st_dev, root.st_ino, root.st_uid)
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
        self.assertEqual(
            (current.st_dev, current.st_ino, current.st_uid), self.base_identity
        )
        for root, directories, _files in os.walk(
                self.base, topdown=True, followlinks=False):
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

    @staticmethod
    def canonical(value):
        return (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode(
            "ascii"
        )

    def validate_with_matching_test_pin(self, raw):
        old_hash = self.module["MANIFEST_SHA256"]
        old_size = self.module["MANIFEST_BYTES"]
        self.module["MANIFEST_SHA256"] = hashlib.sha256(raw).hexdigest()
        self.module["MANIFEST_BYTES"] = len(raw)
        try:
            return self.module["validate_manifest"](raw)
        finally:
            self.module["MANIFEST_SHA256"] = old_hash
            self.module["MANIFEST_BYTES"] = old_size

    def test_manifest_is_exact_closed_postgres_inventory(self):
        rows = self.module["validate_manifest"](self.manifest_raw)
        directories = self.module["member_directories"](rows)
        link = rows[self.module["SONAME_LINK"]]
        self.assertEqual(hashlib.sha256(self.manifest_raw).hexdigest(),
                         self.module["MANIFEST_SHA256"])
        self.assertEqual(len(rows), 1_640)
        self.assertEqual(list(rows), sorted(rows))
        self.assertEqual(len(directories), 150)
        self.assertEqual(sum(row["type"] == "file" for row in rows.values()), 1_639)
        self.assertEqual(sum(row.get("size", 0) for row in rows.values()), 47_718_773)
        self.assertEqual(link, {"type": "link", "mode": 0o777,
                                "target": "libpq.so.5.18"})
        self.assertEqual(
            [name for name, row in rows.items() if row["type"] == "link"],
            ["usr/lib/x86_64-linux-gnu/libpq.so.5"],
        )

    def test_manifest_pin_precedes_shape_and_repinned_drift_is_refused(self):
        self.refused(self.module["validate_manifest"], self.manifest_raw[:-1])
        original = json.loads(self.manifest_raw)

        cases = {}
        unordered = deepcopy(original)
        unordered[0], unordered[1] = unordered[1], unordered[0]
        cases["unordered"] = self.canonical(unordered)
        changed_link = deepcopy(original)
        next(row for row in changed_link if row[1] == "link")[-1] = "libpq.so.5.17"
        cases["link-target"] = self.canonical(changed_link)
        changed_mode = deepcopy(original)
        regular = next(row for row in changed_mode if row[1] == "file")
        regular[2] = 0o644
        cases["writable-mode"] = self.canonical(changed_mode)
        traversal = deepcopy(original)
        traversal[0][0] = "usr/../" + traversal[0][0]
        cases["path-traversal"] = self.canonical(traversal)

        for name, raw in cases.items():
            with self.subTest(name=name):
                self.refused(self.validate_with_matching_test_pin, raw)

    def test_full_retained_archive_measurement_is_read_only(self):
        before = metadata(self.archive)
        result = self.module["verify_archive"](self.archive, self.manifest_raw)
        self.assertEqual(result, {
            "archiveSha256": self.module["ARCHIVE_SHA256"],
            "manifestSha256": self.module["MANIFEST_SHA256"],
            "files": 1_640,
            "regularFiles": 1_639,
            "links": 1,
            "directories": 150,
            "payloadBytes": 47_718_773,
            "installed": False,
            "runtimeQualified": False,
        })
        self.assertEqual(metadata(self.archive), before)

    def test_archive_path_contract_refuses_symlink_directory_and_tiny_file(self):
        self.refused(
            self.module["verify_archive"], Path("relative.tar.gz"), self.manifest_raw
        )
        alias = self.base / "capsule-link.tar.gz"
        alias.symlink_to(self.archive)
        directory = self.base / "capsule-directory.tar.gz"
        directory.mkdir()
        tiny = self.base / "capsule-tiny.tar.gz"
        tiny.write_bytes(b"not-a-capsule\n")
        tiny.chmod(0o600)
        for name, path in (("symlink", alias), ("directory", directory), ("tiny", tiny)):
            with self.subTest(name=name):
                with self.assertRaises((ValueError, OSError)):
                    self.module["verify_archive"](path, self.manifest_raw)

    def test_materialized_capsule_is_exact_readonly_and_duplicate_safe(self):
        run = "1" * 32
        calls = []
        prior_umask = os.umask(0)
        try:
            descriptor = self.module["materialize_postgres"](
                self.parent,
                run,
                self.archive,
                self.manifest_raw,
                live_check=lambda: calls.append(True) or True,
            )
        finally:
            os.umask(prior_umask)
        self.held.append(descriptor)
        root = self.base / run
        capsule = root / "capsule"
        rows = self.module["validate_manifest"](self.manifest_raw)
        self.assertEqual(calls, [True, True, True])
        self.assertEqual(stat.S_IMODE(root.lstat().st_mode), 0o555)
        self.assertEqual(stat.S_IMODE(capsule.lstat().st_mode), 0o555)
        self.assertEqual(list(root.iterdir()), [capsule])
        self.assertTrue(os.path.samestat(os.fstat(descriptor), capsule.lstat()))
        self.module["readback"](descriptor, rows, self.manifest_raw)
        self.assertEqual(hashlib.sha256((capsule / "files.json").read_bytes()).hexdigest(),
                         self.module["MANIFEST_SHA256"])
        link = capsule / self.module["SONAME_LINK"]
        self.assertTrue(link.is_symlink())
        self.assertEqual(os.readlink(link), self.module["SONAME_TARGET"])
        self.assertEqual(link.resolve(strict=True), link.parent / self.module["SONAME_TARGET"])

        before = hashlib.sha256((capsule / "files.json").read_bytes()).hexdigest()
        self.refused(
            self.module["materialize_postgres"],
            self.parent,
            run,
            self.archive,
            self.manifest_raw,
            live_check=lambda: True,
        )
        self.assertEqual(hashlib.sha256((capsule / "files.json").read_bytes()).hexdigest(),
                         before)

        name, row = min(
            ((name, row) for name, row in rows.items()
             if row["type"] == "file" and row["size"] > 0),
            key=lambda item: item[1]["size"],
        )
        target = capsule / name
        original = target.read_bytes()
        changed = bytes([original[0] ^ 1]) + original[1:]
        target.chmod(0o644)
        target.write_bytes(changed)
        target.chmod(row["mode"])
        self.assertEqual(target.lstat().st_size, row["size"])
        self.refused(self.module["readback"], descriptor, rows, self.manifest_raw)

    def test_failed_live_admission_leaves_no_destination(self):
        run = "2" * 32
        self.refused(
            self.module["materialize_postgres"],
            self.parent,
            run,
            self.archive,
            self.manifest_raw,
            live_check=lambda: False,
        )
        self.assertFalse((self.base / run).exists())

    def test_post_yield_archive_failure_closes_transferred_capsule_fd(self):
        original = {
            name: self.module[name]
            for name in ("pinned_archive", "validate_manifest", "member_directories",
                         "scan_archive", "create_directories", "readback")
        }
        descriptors_before = len(os.listdir("/dev/fd"))

        @contextmanager
        def late_failure(_path):
            yield object(), lambda: None
            raise ValueError("c2c_postgres_install_refused:archive_changed")

        measured = {"files": 1_640, "regularFiles": 1_639, "links": 1,
                    "directories": 150, "payloadBytes": 47_718_773}
        self.module["pinned_archive"] = late_failure
        self.module["validate_manifest"] = lambda _raw: {}
        self.module["member_directories"] = lambda _rows: set()
        self.module["scan_archive"] = lambda _stream, _rows, _folders=None: dict(measured)
        self.module["create_directories"] = lambda top, _directories: {".": top}
        self.module["readback"] = lambda _top, _rows, _raw: None
        try:
            self.refused(
                self.module["materialize_postgres"],
                self.parent,
                "3" * 32,
                self.archive,
                self.manifest_raw,
                live_check=lambda: True,
            )
        finally:
            self.module.update(original)
        self.assertEqual(len(os.listdir("/dev/fd")), descriptors_before)

    def test_root_guard_precedes_admission_and_fixed_path_access(self):
        calls = []
        original = self.module["platform_root"]
        self.module["platform_root"] = lambda: False
        try:
            self.refused(
                self.module["install_postgres"],
                "4" * 32,
                "5" * 64,
                self.archive,
                self.manifest_raw,
                admitted=lambda value: calls.append(value) or True,
            )
        finally:
            self.module["platform_root"] = original
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
