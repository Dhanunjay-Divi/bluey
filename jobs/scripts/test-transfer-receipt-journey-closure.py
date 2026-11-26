"""Boundary coverage for the receipt transfer; fixtures are never executed."""
import importlib.util
import io
import os
from pathlib import Path
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "receipt_transfer", Path(__file__).with_name("transfer-receipt-journey-closure.py")
)
TRANSFER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRANSFER)


class ClosureTransfer(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.base = Path(scratch.name).resolve()
        self.source = self.base / "source"
        self.spool = self.base / "spool"
        self.dest = self.base / "run"
        for path in (
            self.source, self.spool, self.dest, self.source / "jobs",
            self.source / "jobs/automation", self.source / "jobs/node_modules",
            self.source / "jobs/node_modules/@bluey",
        ):
            path.mkdir(mode=0o700)
        self.source_file = self.source / "jobs/automation/package.json"
        fd = os.open(self.source_file, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(fd, "wb") as f:
            f.write(b"fixture")
        self.link = self.source / "jobs/node_modules/@bluey/jobs-automation"
        self.link.symlink_to("../../automation")

    def receipt(self):
        with patch.object(TRANSFER, "check_pins"):
            return TRANSFER.publish(self.spool, self.source)

    def intake(self, mode, result):
        # The tiny fixture does not claim any of the production-build pins.
        # The checked receiver otherwise runs real archive/descriptor I/O.
        with patch.object(TRANSFER, "check_pins"), patch.object(
            TRANSFER, "run_marker", return_value=lambda: None
        ), patch.object(TRANSFER.os, "fstatvfs", return_value=SimpleNamespace(
            f_frsize=1, f_bavail=64 * 1024**3
        )):
            return TRANSFER.receive(mode, self.spool / TRANSFER.ARCHIVE,
                                    result["archiveSha256"], self.dest)

    def test_seal_stage_readback_in_distinct_roots_and_reject_mode_drift(self):
        receipt = self.receipt()
        self.assertEqual(receipt["archiveSha256"], self.intake("verify", receipt)["archiveSha256"])
        result = self.intake("stage", receipt)
        self.assertFalse(result["runtimeAdmitted"])
        self.assertEqual(self.intake("readback", receipt)["inventorySha256"],
                         result["inventorySha256"])
        staged = self.dest / TRANSFER.CAPSULE / "jobs"
        alias = staged / "node_modules/@bluey/jobs-automation"
        self.assertEqual(os.readlink(alias), "../../automation")
        self.assertEqual((alias / "package.json").read_bytes(), b"fixture")
        self.assertEqual((staged / "automation/package.json").stat().st_nlink, 1)
        (staged / "automation/package.json").chmod(0o644)
        with self.assertRaisesRegex(RuntimeError, "^stage_mode$"):
            self.intake("readback", receipt)

    def test_wrong_external_digest_refused_before_destination_exists(self):
        receipt = self.receipt()
        receipt["archiveSha256"] = "0" * 64
        with self.assertRaisesRegex(RuntimeError, "^archive_digest$"):
            self.intake("stage", receipt)
        self.assertEqual(list(self.dest.iterdir()), [])

    def test_refuse_existing_destination(self):
        receipt = self.receipt()
        (self.dest / TRANSFER.CAPSULE).mkdir(mode=0o700)
        with self.assertRaises(FileExistsError):
            self.intake("stage", receipt)

    def test_spool_substitution_refused_before_archive_write(self):
        before = self.spool.stat()
        self.spool.rename(self.base / "old-spool")
        self.spool.mkdir(mode=0o700)
        with self.assertRaisesRegex(RuntimeError, "^spool_replaced$"):
            TRANSFER.publish(self.spool, self.source, expected_spool=before)
        self.assertEqual(list(self.spool.iterdir()), [])

    def test_link_can_neither_escape_nor_point_to_a_symlink(self):
        self.link.unlink()
        self.link.symlink_to("../../../../outside")
        with self.assertRaisesRegex(RuntimeError, "^link_escape$"):
            self.receipt()
        self.link.unlink()
        self.link.symlink_to("../../automation")
        self.link.with_name("second").symlink_to("jobs-automation")
        with self.assertRaisesRegex(RuntimeError, "^link_target$"):
            self.receipt()

    def test_file_hash_detects_archive_tampering_with_new_outer_digest(self):
        self.receipt()
        path = self.spool / TRANSFER.ARCHIVE
        data = path.read_bytes()
        self.assertEqual(data.count(b"fixture"), 1)
        path.write_bytes(data.replace(b"fixture", b"altered"))
        with self.assertRaisesRegex(RuntimeError, "^archive_file_digest$"):
            self.intake("stage", {"archiveSha256": TRANSFER.sha(path.read_bytes())})
        self.assertEqual(list(self.dest.iterdir()), [])

    def test_extra_member_refused(self):
        self.receipt()
        path = self.spool / TRANSFER.ARCHIVE
        with tarfile.open(path, mode="a") as bundle:
            bundle.addfile(TRANSFER.header("extra", "f", 0o600, 1, None), io.BytesIO(b"x"))
        with self.assertRaisesRegex(RuntimeError, "^archive_extra_entry$"):
            self.intake("stage", {"archiveSha256": TRANSFER.sha(path.read_bytes())})
        self.assertEqual(list(self.dest.iterdir()), [])

    def test_trailing_bytes_refused_even_with_new_digest(self):
        self.receipt()
        path = self.spool / TRANSFER.ARCHIVE
        with path.open("ab") as stream:
            stream.write(b"extra")
        with self.assertRaisesRegex(RuntimeError, "^archive_terminator$"):
            self.intake("stage", {"archiveSha256": TRANSFER.sha(path.read_bytes())})
        self.assertEqual(list(self.dest.iterdir()), [])

    def test_real_pin_checker_refuses_the_synthetic_fixture(self):
        with self.assertRaisesRegex(RuntimeError, "^required_payload_pin$"):
            TRANSFER.publish(self.spool, self.source)
        self.assertEqual(list(self.spool.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
