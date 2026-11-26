#!/usr/bin/env python3
"""Offline hostile-input tests for the packet-only mock-receipt transport."""
from copy import deepcopy
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import unittest


HERE = Path(__file__).resolve().parent
TRANSPORT = runpy.run_path(str(HERE / "local-qa-linux-signup-mock-receipt-transport.py"), run_name="_test_transport")
REFUSED = r"signup_mock_receipt_transport_refused:"


class MockReceiptTransportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        evidence = Path(os.environ["MOCK_RECEIPT_EVIDENCE_ROOT"])
        cls.packet = (evidence / "jobs-signup-mock-receipt-source.Lu4NPZR1/"
                                  "signup-mock-receipt-source-candidate.tar").read_bytes()
        assert hashlib.sha256(cls.packet).hexdigest() == TRANSPORT["PACKET_SHA256"]
        assert len(cls.packet) == TRANSPORT["PACKET_BYTES"]

    def setUp(self):
        self.cap = {**TRANSPORT["PINNED"], "run": "1" * 32, "reservationSha256": "2" * 64}
        self.binding = {**TRANSPORT["PINNED"], "projection": {
            "run": "1" * 32, "unit": "bluey-jobs-qa-" + "1" * 32 + ".service",
            "uid": 1000, "gid": 1000, "runParent": "/tmp/bluey-test-runs-123456789abc",
        }}

    def refused(self, function, *args):
        with self.assertRaisesRegex(ValueError, REFUSED):
            function(*args)

    def frame(self):
        return TRANSPORT["encode_request"](deepcopy(self.cap), self.packet)

    def test_sealed_packet_round_trip_is_canonical_and_requires_eof(self):
        frame = self.frame()
        packet, request_sha = TRANSPORT["read_request"](io.BytesIO(frame), deepcopy(self.cap))
        self.assertEqual(packet, self.packet)
        self.assertEqual(request_sha, hashlib.sha256(frame).hexdigest())
        self.assertEqual(frame, self.frame())

    def test_request_refuses_hostile_framing_scope_suffix_and_packet_drift(self):
        frame = self.frame()
        size = int.from_bytes(frame[:4], "big")
        packet_at = 4 + size
        cases = [b"\0\0\0\0" + frame[4:], frame[:-1], frame + b"x"]
        bad_scope = bytearray(frame)
        bad_scope[packet_at] ^= 1
        cases.append(bytes(bad_scope))
        for raw in cases:
            with self.subTest(length=len(raw)):
                self.refused(TRANSPORT["read_request"], io.BytesIO(raw), deepcopy(self.cap))
        stale = deepcopy(self.cap)
        stale["run"] = "9" * 32
        self.refused(TRANSPORT["read_request"], io.BytesIO(frame), stale)

    def test_response_binds_capability_request_and_fixed_false_flags(self):
        frame = self.frame()
        request_sha = hashlib.sha256(frame).hexdigest()
        raw = TRANSPORT["encode_response"](TRANSPORT["expected_result"](self.cap), self.cap, request_sha)
        result = TRANSPORT["read_response"](raw, self.cap, request_sha)
        self.assertEqual((result["actorFiles"], result["managerFiles"]), (39, 12))
        self.assertFalse(result["releasePublished"] or result["runtimeQualified"] or result["launchAdmitted"])
        self.assertEqual(result["actorPlanSha256"], TRANSPORT["plan_digest"](self.cap))
        self.assertEqual(result["browserRoot"], "/run/bluey-jobs-c2c-browser/" + "1" * 32)
        self.assertNotIn("sourceTree", result)
        self.assertNotIn("sourceInventorySha256", result)
        value = json.loads(raw)
        value["stage"]["launchAdmitted"] = True
        self.refused(TRANSPORT["read_response"], TRANSPORT["wire"](value), self.cap, request_sha)

    def test_reserved_exchange_checks_hold_before_and_after_once_without_completion(self):
        calls = {"reserve": 0, "exchange": 0, "held": 0, "note": 0}
        states = iter((True, True))

        def reserve(binding):
            self.assertEqual(binding, self.binding)
            calls["reserve"] += 1
            return {"reservationSha256": "2" * 64, "held": lambda: (calls.__setitem__("held", calls["held"] + 1) or next(states)),
                    "note_workload": lambda _: calls.__setitem__("note", calls["note"] + 1)}

        def exchange(frame, cap):
            calls["exchange"] += 1
            packet, request_sha = TRANSPORT["read_request"](io.BytesIO(frame), cap)
            self.assertEqual(packet, self.packet)
            return TRANSPORT["encode_response"](TRANSPORT["expected_result"](cap), cap, request_sha)

        result = TRANSPORT["prepare_at_reserved_guest"](deepcopy(self.binding), self.packet, reserve=reserve, exchange=exchange)
        self.assertEqual(result, TRANSPORT["expected_result"](self.cap))
        self.assertEqual(calls, {"reserve": 1, "exchange": 1, "held": 2, "note": 0})

    def test_failed_or_unheld_exchange_has_no_retry_or_completion(self):
        for held, exchange_calls in ((False, 0), (True, 1)):
            calls = {"exchange": 0, "note": 0}
            hold = {"reservationSha256": "2" * 64, "held": lambda: held,
                    "note_workload": lambda _: calls.__setitem__("note", calls["note"] + 1)}
            def reserve(_): return hold
            def exchange(_, __):
                calls["exchange"] += 1
                raise RuntimeError("ambiguous")
            with self.subTest(held=held):
                with self.assertRaises((ValueError, RuntimeError)):
                    TRANSPORT["prepare_at_reserved_guest"](deepcopy(self.binding), self.packet, reserve=reserve, exchange=exchange)
                self.assertEqual(calls, {"exchange": exchange_calls, "note": 0})


if __name__ == "__main__":
    unittest.main()
