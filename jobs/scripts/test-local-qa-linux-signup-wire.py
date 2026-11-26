#!/usr/bin/env python3
"""Portable controls for the inert Linux signup-wire observer.

These tests use tiny synthetic blobs.  They do not claim that a mocked seal is
a kernel seal, and they never start a guest or execute the observed packet.
"""

import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import tempfile
import types
import unittest
from unittest import mock


HERE = Path(__file__).resolve().parent
WIRE = runpy.run_path(str(HERE / "local-qa-linux-signup-wire.py"), run_name="_signup_wire_test")
GLOBALS = WIRE["main"].__globals__
REFUSED = WIRE["WireRefused"]


class Fragmented:
    """A bytes source that deliberately returns less than requested."""

    def __init__(self, raw):
        self._raw = raw
        self._offset = 0

    def read(self, size):
        if self._offset == len(self._raw):
            return b""
        take = min(size, 1, len(self._raw) - self._offset)
        result = self._raw[self._offset:self._offset + take]
        self._offset += take
        return result


@contextlib.contextmanager
def anonymous_descriptor(raw):
    """Create a one-link-free regular descriptor under a test-owned root."""
    with tempfile.TemporaryDirectory(prefix="bluey-signup-wire-test-") as root:
        path = os.path.join(root, "packet")
        fd = os.open(path, os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
        try:
            os.write(fd, raw)
            os.unlink(path)
            yield fd
        finally:
            os.close(fd)


class SignupWireTests(unittest.TestCase):
    def verify(self, source, size, digest):
        return WIRE["_verify_wire"](source, size, digest)

    def test_exact_reader_refuses_truncation_trailing_and_same_size_digest_drift(self):
        raw = b"abcdef"
        digest = hashlib.sha256(raw).hexdigest()
        self.verify(Fragmented(raw), len(raw), digest)

        with self.assertRaisesRegex(REFUSED, "size"):
            self.verify(io.BytesIO(raw[:-1]), len(raw), digest)
        with self.assertRaisesRegex(REFUSED, "trailing_bytes"):
            self.verify(io.BytesIO(raw + b"!"), len(raw), digest)
        drift = b"abcxef"
        self.assertEqual(len(drift), len(raw))
        with self.assertRaisesRegex(REFUSED, "digest"):
            self.verify(Fragmented(drift), len(raw), digest)

    def test_sealed_input_requires_all_mocked_kernel_seals_and_anonymous_regular_fd(self):
        raw = b"sealed-test"
        flags = {"F_GET_SEALS": 101, "F_SEAL_WRITE": 1, "F_SEAL_GROW": 2,
                 "F_SEAL_SHRINK": 4, "F_SEAL_SEAL": 8}
        mask = flags["F_SEAL_WRITE"] | flags["F_SEAL_GROW"] | flags["F_SEAL_SHRINK"] | flags["F_SEAL_SEAL"]
        with anonymous_descriptor(raw) as fd, \
             mock.patch.object(WIRE["sys"], "platform", "linux"), \
             mock.patch.multiple(WIRE["fcntl"], create=True, **flags), \
             mock.patch.object(WIRE["fcntl"], "fcntl", return_value=mask):
            WIRE["sealed_input"](fd, len(raw))
            with mock.patch.object(WIRE["fcntl"], "fcntl", return_value=mask - 1):
                with self.assertRaisesRegex(REFUSED, "seals"):
                    WIRE["sealed_input"](fd, len(raw))

        with tempfile.TemporaryDirectory(prefix="bluey-signup-wire-linked-") as root:
            path = os.path.join(root, "packet")
            fd = os.open(path, os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
            try:
                os.write(fd, raw)
                with mock.patch.object(WIRE["sys"], "platform", "linux"), \
                     mock.patch.multiple(WIRE["fcntl"], create=True, **flags), \
                     mock.patch.object(WIRE["fcntl"], "fcntl", return_value=mask):
                    with self.assertRaisesRegex(REFUSED, "sealed_metadata"):
                        WIRE["sealed_input"](fd, len(raw))
            finally:
                os.close(fd)

    def test_borrowed_descriptor_keeps_its_offset_and_owned_duplicate_is_closed(self):
        raw = b"abcdef"
        digest = hashlib.sha256(raw).hexdigest()
        original_close = os.close
        close_calls = []

        def recording_close(fd):
            close_calls.append(fd)
            original_close(fd)

        with anonymous_descriptor(raw) as fd, \
             mock.patch.object(WIRE["sys"], "platform", "linux"), \
             mock.patch.dict(GLOBALS, {"sealed_input": lambda _fd, _size: None}), \
             mock.patch.object(WIRE["os"], "close", side_effect=recording_close):
            os.lseek(fd, 2, os.SEEK_SET)
            with WIRE["_hold_packet"](fd, lambda: True, len(raw), digest) as reader:
                self.assertEqual(reader.read(2), b"ab")
                self.assertEqual(os.read(fd, 1), b"c")
                self.assertEqual(reader.read(4), b"cdef")
                self.assertEqual(reader.read(1), b"")
            self.assertTrue(any(closed != fd for closed in close_calls))
            with self.assertRaisesRegex(REFUSED, "reader_closed"):
                reader.read(1)
            self.assertEqual(os.read(fd, 1), b"d")

    def test_consumer_must_read_exact_wire_then_confirm_eof(self):
        raw = b"abcdef"
        digest = hashlib.sha256(raw).hexdigest()
        negatives = {
            "no_consumption": lambda _reader: None,
            "partial": lambda reader: self.assertEqual(reader.read(2), b"ab"),
            "full_without_eof": lambda reader: self.assertEqual(reader.read(len(raw)), raw),
        }
        for name, consume in negatives.items():
            with self.subTest(name=name), anonymous_descriptor(raw) as fd, \
                 mock.patch.object(WIRE["sys"], "platform", "linux"), \
                 mock.patch.dict(GLOBALS, {"sealed_input": lambda _fd, _size: None}), \
                 self.assertRaisesRegex(REFUSED, "consumer_eof"):
                with WIRE["_hold_packet"](fd, lambda: True, len(raw), digest) as reader:
                    consume(reader)

        with anonymous_descriptor(raw) as fd, \
             mock.patch.object(WIRE["sys"], "platform", "linux"), \
             mock.patch.dict(GLOBALS, {"sealed_input": lambda _fd, _size: None}):
            with WIRE["_hold_packet"](fd, lambda: True, len(raw), digest) as reader:
                self.assertEqual(reader.read(len(raw)), raw)
                self.assertEqual(reader.read(1), b"")

    def test_lost_owner_callback_revokes_reader_permanently(self):
        raw = b"abcdef"
        state = {"held": True}
        with anonymous_descriptor(raw) as fd, \
             mock.patch.object(WIRE["sys"], "platform", "linux"), \
             mock.patch.dict(GLOBALS, {"sealed_input": lambda _fd, _size: None}):
            with self.assertRaisesRegex(REFUSED, "owner_or_read_failed"):
                with WIRE["_hold_packet"](fd, lambda: state["held"], len(raw),
                                          hashlib.sha256(raw).hexdigest()) as reader:
                    state["held"] = False
                    with self.assertRaisesRegex(REFUSED, "owner_lost"):
                        reader.read(1)
                    state["held"] = True
                    with self.assertRaisesRegex(REFUSED, "reader_closed"):
                        reader.read(1)

    def test_cli_refuses_default_and_non_linux_sealed_modes_without_running_packet(self):
        fake_flags = types.SimpleNamespace(isolated=1, no_site=1)
        with mock.patch.object(WIRE["sys"], "argv", ["wire"]), \
             mock.patch.dict(GLOBALS, {"verify_wire": mock.Mock(side_effect=AssertionError("ran"))}):
            self.assertEqual(WIRE["main"](), 64)

        stderr = io.StringIO()
        with mock.patch.object(WIRE["sys"], "argv", ["wire", "--observe-sealed-fd"]), \
             mock.patch.object(WIRE["sys"], "flags", fake_flags), \
             mock.patch.object(WIRE["sys"], "dont_write_bytecode", True), \
             mock.patch.object(WIRE["sys"], "platform", "darwin"), \
             contextlib.redirect_stderr(stderr):
            self.assertEqual(WIRE["main"](), 69)
        self.assertEqual(stderr.getvalue(), '{"event":"jobs_native_qa_wire_refused"}\n')

    def test_stream_observation_has_exact_non_authority_provenance_flags(self):
        fake_flags = types.SimpleNamespace(isolated=1, no_site=1)
        stdout = io.StringIO()
        with mock.patch.object(WIRE["sys"], "argv", ["wire", "--verify-stream"]), \
             mock.patch.object(WIRE["sys"], "flags", fake_flags), \
             mock.patch.object(WIRE["sys"], "dont_write_bytecode", True), \
             mock.patch.dict(GLOBALS, {"verify_wire": lambda _source: None}), \
             contextlib.redirect_stdout(stdout):
            self.assertEqual(WIRE["main"](), 0)
        self.assertEqual(json.loads(stdout.getvalue()), {
            "schema": "bluey-jobs-native-qa-wire-observation-v1",
            "bytes": 490_699_941,
            "sha256": "ca2ef640ece49304d71cac545d990a4f1e2431691b0e823ba7b41f7984bf5d92",
            "osSealedDescriptorObserved": False, "ownerObserved": False,
            "executionAdmitted": False, "productionQualified": False,
        })


if __name__ == "__main__":
    unittest.main()
