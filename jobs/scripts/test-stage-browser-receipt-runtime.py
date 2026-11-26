"""Small custody controls; real archive staging is a separate acceptance run."""
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import tempfile
import unittest
from contextlib import ExitStack
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "runtime_intake", Path(__file__).with_name("stage-browser-receipt-runtime.py")
)
RUNTIME = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNTIME)


class RuntimeCustody(unittest.TestCase):
    def test_archive_fd_detects_same_length_replacement(self):
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
            archive = Path(directory).resolve() / "archive"
            archive.write_bytes(b"approved")
            stream, recheck = RUNTIME.open_verified(
                stack, archive, (8, hashlib.sha256(b"approved").hexdigest())
            )
            replacement = archive.with_name("replacement")
            replacement.write_bytes(b"replaced")
            replacement.rename(archive)
            self.assertEqual(stream.read(), b"approved")
            with self.assertRaisesRegex(RUNTIME.Refused, "^archive_changed$"):
                recheck()

    def test_archive_digest_refuses_same_length_drift(self):
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
            archive = Path(directory).resolve() / "archive"
            archive.write_bytes(b"replaced")
            with self.assertRaisesRegex(RUNTIME.Refused, "^archive_digest$"):
                RUNTIME.open_verified(stack, archive, (8, hashlib.sha256(b"approved").hexdigest()))

    def test_archive_path_rejects_traversal_and_non_member_prefix(self):
        self.assertEqual(RUNTIME.relative("package/lib/index.js", "package"), "lib/index.js")
        for invalid in ("package/../chrome", "package/.git/config", "packaged/file", "package//bin"):
            with self.subTest(invalid=invalid):
                with self.assertRaisesRegex(RUNTIME.Refused, "^member_path$"):
                    RUNTIME.relative(invalid, "package")

    def test_wrong_elf_architecture_is_refused(self):
        header = b"\x7fELF\x02\x01\x01" + b"\0" * 11
        x64 = header + b"\x3e\x00"
        self.assertEqual(RUNTIME.file_digest(io.BytesIO(x64), len(x64), elf=True),
                         hashlib.sha256(x64).hexdigest())
        with self.assertRaisesRegex(RUNTIME.Refused, "^member_architecture$"):
            RUNTIME.file_digest(io.BytesIO(header + b"\xb7\x00"), 20, elf=True)

    def test_stage_is_inert_and_readback_rejects_drift(self):
        name = "runtime/playwright/index.mjs"
        blob = b"unused runtime module"
        members = [(name, len(blob), 0o755, io.BytesIO(blob))]
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
            fd = os.open(directory, RUNTIME.DIR_FLAGS)
            stack.callback(os.close, fd)
            with patch.object(RUNTIME, "REQUIRED", {name}):
                rows = RUNTIME.consume([(members, 1, len(blob))], fd)
            self.assertEqual(rows, [(name, hashlib.sha256(blob).hexdigest(), len(blob), 0o755)])
            RUNTIME.readback(fd, rows)
            path = Path(directory) / name
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            path.chmod(0o700)
            with self.assertRaisesRegex(RUNTIME.Refused, "^stage_file_readback$"):
                RUNTIME.readback(fd, rows)
            path.chmod(0o600)
            path.with_name("unexpected.mjs").write_bytes(b"unexpected")
            with self.assertRaisesRegex(RUNTIME.Refused, "^stage_file_inventory$"):
                RUNTIME.readback(fd, rows)


if __name__ == "__main__":
    unittest.main()
