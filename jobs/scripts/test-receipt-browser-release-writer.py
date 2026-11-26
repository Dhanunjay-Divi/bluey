"""Test the single-writer gate file on an owned fixture filesystem."""
import importlib.util
import json
import os
from pathlib import Path
from threading import Barrier, Thread
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("receipt-browser-release-writer.py")
SPEC = importlib.util.spec_from_file_location("release_writer", SCRIPT)
WRITER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WRITER)
BARRIER = WRITER.helper("receipt-browser-launch-barrier.py", WRITER.BARRIER)


class RootWriterFixture(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name).resolve()
        self.root.chmod(0o755)
        self.fd = os.open(self.root, BARRIER["DIR"])
        self.addCleanup(os.close, self.fd)

    def test_complete_publication_matches_the_existing_reader(self):
        visits = []
        body = b'{"fixture":"exact bytes"}\n'

        def recheck():
            self.assertIsNone(BARRIER["read_release"](self.fd, uid=os.getuid()))
            visits.append("held")

        receipt = WRITER.publish_at(self.fd, body, recheck, owner=os.getuid())
        result = BARRIER["read_release"](self.fd, uid=os.getuid())
        self.assertEqual(result[0], body)
        self.assertEqual((receipt["device"], receipt["inode"]), result[1][:2])
        self.assertEqual(len(visits), 2)
        self.assertLessEqual(receipt["publicationBeforeNs"], receipt["publicationAfterNs"])
        self.assertFalse((self.root / WRITER.PENDING).exists())

    def test_atomic_rename_does_not_replace_even_a_late_file(self):
        source = self.root / WRITER.PENDING
        target = self.root / WRITER.READY
        source.write_bytes(b"pending")
        target.write_bytes(b"prior")
        with self.assertRaises(FileExistsError):
            WRITER.rename_noreplace(self.fd)
        self.assertEqual(source.read_bytes(), b"pending")
        self.assertEqual(target.read_bytes(), b"prior")

    def test_observer_loss_before_publication_retains_only_non_signal_files(self):
        calls = []

        def recheck():
            calls.append(True)
            if len(calls) >= 2:
                raise RuntimeError("synthetic_observer_lost")

        with self.assertRaisesRegex(RuntimeError, "^synthetic_observer_lost$"):
            WRITER.publish_at(self.fd, b"fixture", recheck, owner=os.getuid())
        self.assertIsNone(BARRIER["read_release"](self.fd, uid=os.getuid()))
        self.assertTrue((self.root / WRITER.PENDING).exists())
        with self.assertRaises(FileExistsError):
            WRITER.publish_at(self.fd, b"retry", lambda: None, owner=os.getuid())

    def test_two_writers_allow_one_complete_release(self):
        start = Barrier(2, timeout=3)
        outcomes = []

        def publish(body):
            start.wait()
            try:
                WRITER.publish_at(self.fd, body, lambda: None, owner=os.getuid())
                outcomes.append(("published", body))
            except (RuntimeError, FileExistsError):
                outcomes.append(("refused", body))

        threads = [Thread(target=publish, args=(body,)) for body in (b"first", b"second")]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(timeout=4)
            self.assertFalse(thread.is_alive())
        self.assertEqual(sorted(status for status, _ in outcomes), ["published", "refused"])
        expected = next(body for status, body in outcomes if status == "published")
        self.assertEqual(BARRIER["read_release"](self.fd, uid=os.getuid())[0], expected)

    def test_published_same_inode_content_drift_cannot_return_success(self):
        rename = WRITER.rename_noreplace

        def mutate(fd):
            rename(fd)
            (self.root / WRITER.READY).write_bytes(b"other")

        with patch.object(WRITER, "rename_noreplace", mutate), self.assertRaisesRegex(
            RuntimeError, "^release_publication_bytes$"
        ):
            WRITER.publish_at(self.fd, b"exact", lambda: None, owner=os.getuid())

    def test_crossed_deadline_cannot_return_success(self):
        moments = iter((10, 20))
        with self.assertRaisesRegex(RuntimeError, "^release_publication_deadline$"):
            WRITER.publish_at(self.fd, b"fixture", lambda: None, owner=os.getuid(),
                              deadline_ns=20, clock=lambda: next(moments))
        # Publication may already be visible: the real watchdog owns abort.
        self.assertTrue((self.root / WRITER.READY).exists())

    def test_observer_requires_its_exact_original_serialization(self):
        item = {"event": "jobs_browser_closure_bound", "run": "00" * 16}
        self.assertEqual(WRITER.parse_bound((json.dumps(item) + "\n").encode()), item)
        with self.assertRaisesRegex(RuntimeError, "^release_observer_serialization$"):
            WRITER.parse_bound(b'{"event":"jobs_browser_closure_bound","event":"replaced"}\n')

    def test_live_writer_denies_darwin_without_calling_admission(self):
        if os.sys.platform != "darwin":
            self.skipTest("Darwin refusal control")
        with self.assertRaisesRegex(RuntimeError, "^release_writer_linux_root$"):
            WRITER.write_after_bound({}, {}, b"", None, deadline_ns=1, independently_admitted=None)


if __name__ == "__main__":
    unittest.main()
