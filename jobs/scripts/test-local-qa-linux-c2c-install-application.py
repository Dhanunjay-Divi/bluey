"""Portable source-input checks for the inert C2C application installer."""
import hashlib
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import unittest


I = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("application", I / "local-qa-linux-c2c-install-application.py")
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


class ApplicationInputs(unittest.TestCase):
    def setUp(self):
        self.native = Path(os.environ["C2C_NATIVE_SPOOL"])
        self.portal = Path(os.environ["C2C_PORTAL_ARCHIVE"])

    def test_exact_current_receipts_and_portal_archive_parse(self):
        receipt = (self.native / "receipt.json").read_bytes()
        resolver = (self.native / "resolver.json").read_bytes()
        manifest = (self.portal / "static-manifest.json").read_bytes()
        portal_receipt = (self.portal / "receipt.json").read_bytes()
        archive = (self.portal / "static-roles.tar.gz").read_bytes()
        native, resolved = M._native(receipt, resolver)
        parsed_manifest, portal, files = M._portal(manifest, portal_receipt, archive)
        self.assertEqual(native["sourceTree"], M.TREE)
        self.assertEqual(resolved["lockSha256"], M.LOCK)
        self.assertEqual(len(parsed_manifest), len(files), 44)
        self.assertFalse(portal["releaseAdmitted"])

    def test_hash_or_schema_drift_refuses_before_staging(self):
        receipt = bytearray((self.native / "receipt.json").read_bytes())
        receipt[-2] ^= 1
        with self.assertRaisesRegex(RuntimeError, "native_wire_pin"):
            M._native(bytes(receipt), (self.native / "resolver.json").read_bytes())
        manifest = (self.portal / "static-manifest.json").read_bytes()
        receipt = (self.portal / "receipt.json").read_bytes()
        archive = (self.portal / "static-roles.tar.gz").read_bytes()
        with self.assertRaisesRegex(RuntimeError, "portal_manifest"):
            M._portal(manifest, receipt, archive[:-1])
        self.assertEqual(hashlib.sha256(manifest).hexdigest(), M.PORTAL_MANIFEST)

    def test_actual_combined_server_and_portal_readback_is_inert_and_closed(self):
        native = (self.native / "receipt.json").read_bytes()
        resolver = (self.native / "resolver.json").read_bytes()
        portal = (self.portal / "receipt.json").read_bytes()
        manifest = (self.portal / "static-manifest.json").read_bytes()
        _, _, files = M._portal(manifest, portal, (self.portal / "static-roles.tar.gz").read_bytes())
        server = (self.native / "bluey-server").read_bytes()
        fixture = (self.native / "jobs-nonprod-browser-fixture").read_bytes()
        for name, raw in (("bluey-server", server), ("jobs-nonprod-browser-fixture", fixture)):
            self.assertEqual((len(raw), hashlib.sha256(raw).hexdigest()), M.NATIVE_ARTIFACTS[name])
        payloads = {"api/bluey-server": server, "fixture/jobs-nonprod-browser-fixture": fixture,
                    "evidence/native-receipt.json": native, "evidence/resolver.json": resolver,
                    "evidence/portal-receipt.json": portal, "portal/static-manifest.json": manifest,
                    **{"portal/" + key: data for key, data in files.items()}}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            for name, raw in payloads.items():
                leaf = root / name
                for directory in reversed(list(leaf.parents)):
                    if root in directory.parents:
                        directory.mkdir(mode=0o700, exist_ok=True)
                with os.fdopen(os.open(leaf, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600), "wb") as out:
                    self.assertEqual(out.write(raw), len(raw))
            fd = os.open(root, M.DIR)
            group = root.stat().st_gid if sys.platform == "darwin" else os.getegid()

            def readback():
                M._verify_inventory(fd, server, fixture, native, resolver, portal, manifest, files,
                                    owner=os.getuid(), group=group, inert=True)

            try:
                readback()
                extra = root / "portal/web/surprise.html"
                extra.write_bytes(b"unexpected")
                with self.assertRaisesRegex(RuntimeError, "final_entries"):
                    readback()
                extra.unlink()
                with (root / "api/bluey-server").open("r+b") as stream:
                    stream.seek(0)
                    stream.write(bytes([server[0] ^ 1]))
                with self.assertRaisesRegex(RuntimeError, "final_leaf"):
                    readback()
            finally:
                os.close(fd)


if __name__ == "__main__":
    unittest.main(verbosity=2)
