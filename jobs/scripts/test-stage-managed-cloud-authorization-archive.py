"""Synthetic archive fixtures; no registry or GitHub connectivity."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import warnings
import zipfile

SCRIPT = Path(__file__).resolve().with_name("stage-managed-cloud-authorization-archive.py")
SPEC = importlib.util.spec_from_file_location("authorization_archive_fixture", SCRIPT)
stage_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stage_module)


def archive_identity(path):
    data = path.read_bytes()
    return len(data), "sha256:" + hashlib.sha256(data).hexdigest()


class AuthorizationArchiveTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="bluey-authorization-zip-test-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name).resolve()
        self.destination = self.root / "private"
        self.destination.mkdir(mode=0o700)

    def archive(self, names=None, folders=False, non_file=False, kind="authorized"):
        files, directories, _ = stage_module.STAGE_KINDS[kind]
        path = self.root / "fixture.zip"
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            if folders:
                for name in sorted(directories):
                    archive.writestr(name + "/", b"")
            for name in names if names is not None else sorted(files):
                entry = zipfile.ZipInfo(name)
                entry.compress_type = zipfile.ZIP_DEFLATED
                if non_file and name == sorted(files)[0]:
                    entry.external_attr = (stat.S_IFLNK | 0o700) << 16
                else:
                    entry.external_attr = (stat.S_IFREG | 0o600) << 16
                archive.writestr(entry, name.encode("ascii"))
        return path

    def staged(self, archive, *, digest=None, limit=50_000, reserve=0, path=None, kind="authorized"):
        size, expected = archive_identity(archive)
        return stage_module.stage(path or archive, self.destination, size, digest or expected, limit, reserve, kind)

    def assert_no_effect(self):
        self.assertEqual(list(self.destination.iterdir()), [])

    def assert_files(self, result, kind="authorized", parent=None):
        files, directories, prefix = stage_module.STAGE_KINDS[kind]
        material = Path(result["root"])
        self.assertEqual(material.parent, self.destination if parent is None else parent)
        self.assertRegex(material.name, rf"^{prefix}-[0-9a-f]{{24}}$")
        self.assertEqual(result["fileCount"], len(files))
        self.assertEqual(result["unpackedBytes"], sum(len(name) for name in files))
        self.assertEqual({path.relative_to(material).as_posix() for path in material.rglob("*") if path.is_file()},
                         files)
        for name in files:
            path = material / name
            self.assertEqual(path.read_bytes(), name.encode("ascii"))
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        for name in {"."} | directories:
            self.assertEqual(stat.S_IMODE((material / name).stat().st_mode), 0o700)

    def test_actual_cli_extracts_only_the_closed_authorization_file_set(self):
        archive = self.archive(folders=True)
        size, digest = archive_identity(archive)
        result = subprocess.run([
            sys.executable, "-I", "-S", "-B", str(SCRIPT), "--archive", str(archive),
            "--parent", str(self.destination), "--size", str(size), "--digest", digest,
            "--max-unpacked-bytes", "50000", "--reserve-bytes", "0",
        ], capture_output=True, text=True, timeout=10, check=True)
        self.assertEqual(result.stderr, "")
        evidence = json.loads(result.stdout)
        self.assertEqual((evidence["archiveSizeBytes"], evidence["archiveDigest"]), (size, digest))
        self.assert_files(evidence)

    def test_candidate_kind_cli_and_discard_use_only_the_stripped_closed_inventory(self):
        self.assertEqual(len(stage_module.CANDIDATE_ARCHIVE_FILES), 28)
        archive = self.archive(folders=True, kind="candidate")
        size, digest = archive_identity(archive)
        attempt = self.destination / "intake-Ab123Z"
        attempt.mkdir(mode=0o700)
        result = subprocess.run([
            sys.executable, "-I", "-S", "-B", str(SCRIPT), "--archive", str(archive),
            "--parent", str(attempt), "--size", str(size), "--digest", digest,
            "--max-unpacked-bytes", "50000", "--reserve-bytes", "0", "--kind", "candidate",
        ], capture_output=True, text=True, timeout=10, check=True)
        self.assertEqual(result.stderr, "")
        evidence = json.loads(result.stdout)
        self.assert_files(evidence, "candidate", attempt)
        staged = Path(evidence["root"])
        inputs = [self.destination, stage_module.directory_identity(self.destination.stat()),
                  attempt.name, stage_module.directory_identity(attempt.stat()),
                  staged.name, stage_module.directory_identity(staged.stat())]
        stage_module.discard(*inputs, kind="candidate")
        self.assertFalse(attempt.exists())

    def test_candidate_and_authorized_kinds_refuse_cross_kind_archives_and_cleanup(self):
        candidate = self.archive(kind="candidate")
        with self.assertRaisesRegex(stage_module.Refused, "bounds|incomplete|unexpected"):
            self.staged(candidate, kind="authorized")
        authorized = self.archive(kind="authorized")
        with self.assertRaisesRegex(stage_module.Refused, "bounds|incomplete|unexpected"):
            self.staged(authorized, kind="candidate")
        candidate = self.archive(kind="candidate")
        attempt = self.destination / "intake-Ab123Z"
        attempt.mkdir(mode=0o700)
        size, digest = archive_identity(candidate)
        staged = Path(stage_module.stage(candidate, attempt, size, digest, 50_000, 0, "candidate")["root"])
        inputs = [self.destination, stage_module.directory_identity(self.destination.stat()),
                  attempt.name, stage_module.directory_identity(attempt.stat()),
                  staged.name, stage_module.directory_identity(staged.stat())]
        with self.assertRaisesRegex(stage_module.Refused, "cleanup scope"):
            stage_module.discard(*inputs)
        self.assertTrue(staged.exists())
        stage_module.discard(*inputs, kind="candidate")
        self.assertFalse(attempt.exists())
        self.assert_no_effect()

    def test_bounded_zip64_directory_stages_the_same_files(self):
        with patch.object(zipfile, "ZIP64_LIMIT", 128):
            archive = self.archive()
        self.assertIn(b"PK\x06\x07", archive.read_bytes()[-42:-22])
        self.assert_files(self.staged(archive))

    def test_bad_digest_or_symlink_never_creates_output(self):
        archive = self.archive()
        with self.assertRaisesRegex(stage_module.Refused, "archive digest mismatch"):
            self.staged(archive, digest="sha256:" + "0" * 64)
        link = self.root / "not-the-opened-archive.zip"
        link.symlink_to(archive)
        with self.assertRaisesRegex(stage_module.Refused, "physical absolute"):
            self.staged(archive, path=link)
        self.assert_no_effect()

    def test_nonregular_unknown_missing_and_duplicate_members_are_refused(self):
        names = sorted(stage_module.ARCHIVE_FILES)
        for contents, error in [
            (names[:-1], "bounds"),
            (names[:-1] + ["unrecognized.json"], "unexpected ZIP file"),
            (names + [names[0]], "unexpected ZIP entry metadata"),
        ]:
            with self.subTest(error=error), warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                with self.assertRaisesRegex(stage_module.Refused, error):
                    self.staged(self.archive(names=contents))
                self.assert_no_effect()
        with self.assertRaisesRegex(stage_module.Refused, "unexpected ZIP file"):
            self.staged(self.archive(non_file=True))
        self.assert_no_effect()

    def test_directory_unpacked_and_disk_limits_refuse_before_output(self):
        archive = self.archive()
        with patch.object(stage_module, "MAX_CENTRAL_BYTES", 1):
            with self.assertRaisesRegex(stage_module.Refused, "central directory bounds"):
                self.staged(archive)
        with self.assertRaisesRegex(stage_module.Refused, "unpacked byte budget exceeded"):
            self.staged(archive, limit=200)
        with patch.object(stage_module, "disk_room", return_value=0):
            with self.assertRaisesRegex(stage_module.Refused, "insufficient staging disk headroom"):
                self.staged(archive)
        self.assert_no_effect()

    def test_mid_copy_refusal_cleans_only_created_fixture_children(self):
        archive = self.archive()
        sibling = self.destination / "existing-note"
        sibling.write_bytes(b"preserve")
        original = stage_module.stream_bytes
        calls = 0

        def fail_after_one(source, length, target=None, room=None):
            nonlocal calls
            if target is not None:
                calls += 1
                if calls == 2:
                    raise stage_module.Refused("synthetic interrupted copy")
            return original(source, length, target, room)

        with patch.object(stage_module, "stream_bytes", side_effect=fail_after_one):
            with self.assertRaisesRegex(stage_module.Refused, "interrupted copy"):
                self.staged(archive)
        self.assertEqual(calls, 2)
        self.assertEqual(list(self.destination.iterdir()), [sibling])
        self.assertEqual(sibling.read_bytes(), b"preserve")

    def test_archive_change_after_hash_refuses_and_removes_partial_stage(self):
        archive = self.archive()
        original = stage_module.stream_bytes
        complete_files = 0

        def change_before_readback(source, length, target=None, room=None):
            nonlocal complete_files
            result = original(source, length, target, room)
            if target is not None:
                complete_files += 1
                if complete_files == len(stage_module.ARCHIVE_FILES):
                    with archive.open("r+b") as changed:
                        first = changed.read(1)
                        changed.seek(0)
                        changed.write(bytes([first[0] ^ 1]))
            return result

        with patch.object(stage_module, "stream_bytes", side_effect=change_before_readback):
            with self.assertRaisesRegex(stage_module.Refused, "archive changed during staging"):
                self.staged(archive)
        self.assertEqual(complete_files, 31)
        self.assert_no_effect()

    def test_discard_refuses_unexpected_inventory_without_deleting_it(self):
        archive = self.archive()
        attempt = self.destination / "intake-Ab123Z"
        attempt.mkdir(mode=0o700)
        size, digest = archive_identity(archive)
        staged = Path(stage_module.stage(archive, attempt, size, digest, 50_000, 0)["root"])
        sibling = self.destination / "preserved"
        sibling.write_bytes(b"outside attempt")
        extra = staged / "unexpected-note"
        extra.write_bytes(b"preserve on refusal")
        inputs = [self.destination, stage_module.directory_identity(self.destination.stat()),
                  attempt.name, stage_module.directory_identity(attempt.stat()),
                  staged.name, stage_module.directory_identity(staged.stat())]
        with self.assertRaisesRegex(stage_module.Refused, "inventory changed"):
            stage_module.discard(*inputs)
        self.assertEqual(extra.read_bytes(), b"preserve on refusal")
        self.assertTrue((staged / "authorization-set.json").is_file())
        extra.unlink()
        stage_module.discard(*inputs)
        self.assertEqual(sibling.read_bytes(), b"outside attempt")
        self.assertFalse(attempt.exists())


if __name__ == "__main__":
    unittest.main()
