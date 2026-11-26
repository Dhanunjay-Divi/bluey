#!/usr/bin/env python3
"""Portable controls for the one-shot C2C preparation transport.

No guest, service, or network is used.  The positive integration case uses
the real write-once host reservation in a private temporary slot and loops the
request through the real in-memory request/response decoders.
"""
from copy import deepcopy
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import stat
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
TRANSPORT_PATH = HERE / "local-qa-linux-c2c-preparation-transport.py"
RESERVATION_PATH = HERE / "local-qa-linux-c2c-host-reservation.py"
REFUSED = r"c2c_preparation_transport_refused:"


def load(path):
    return runpy.run_path(str(path), run_name="_c2c_preparation_transport_test")


class ObservedStream(io.BytesIO):
    def __init__(self, raw):
        super().__init__(raw)
        self.eof_observed = False

    def read(self, size=-1):
        raw = super().read(size)
        if raw == b"":
            self.eof_observed = True
        return raw


class PreparationTransportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.transport = load(TRANSPORT_PATH)
        cls.reservation = load(RESERVATION_PATH)
        packet_path = Path(os.environ["C2C_CODE_PACKET"])
        if not packet_path.is_absolute():
            raise ValueError("C2C_CODE_PACKET must be absolute")
        cls.packet = packet_path.read_bytes()
        cls.bootstrap = {
            name: (HERE / name).read_bytes()
            for name in cls.transport["BOOTSTRAP"]
        }
        if hashlib.sha256(cls.packet).hexdigest() != cls.transport["PACKET_SHA"]:
            raise AssertionError("C2C_CODE_PACKET digest does not match the fixed transport input")
        if len(cls.packet) != cls.transport["PACKET_BYTES"]:
            raise AssertionError("C2C_CODE_PACKET size does not match the fixed transport input")
        pins = {name: hashlib.sha256(raw).hexdigest() for name, raw in cls.bootstrap.items()}
        if pins != cls.transport["BOOTSTRAP"]:
            raise AssertionError("bootstrap sources do not match the fixed transport input")

    def setUp(self):
        self.run = "1" * 32
        self.cap = {
            **self.transport["PINNED"],
            "run": self.run,
            "reservationSha256": "2" * 64,
        }
        self.binding = {
            "instance": "bluey-c2c-guest",
            "instanceIdentity": [101, 102],
            "diskIdentity": [103, 104],
            "configSha256": "3" * 64,
            "imageSha256": "4" * 64,
            "packetSha256": self.transport["PACKET_SHA"],
            "sourceTree": self.transport["SOURCE_TREE"],
            "sourceInventorySha256": self.transport["SOURCE_INVENTORY_SHA"],
            "projection": {
                "run": self.run,
                "unit": "bluey-jobs-qa-" + self.run + ".service",
                "uid": 1000,
                "gid": 1000,
                "runParent": "/tmp/bluey-test-runs-123456789abc",
            },
        }

    def refused(self, function, *args, **kwargs):
        with self.assertRaisesRegex(ValueError, REFUSED):
            function(*args, **kwargs)

    def frame(self):
        return self.transport["encode_request"](
            deepcopy(self.cap), self.packet, deepcopy(self.bootstrap)
        )

    def rewrite_header(self, frame, change):
        size = int.from_bytes(frame[:4], "big")
        header = json.loads(frame[4:4 + size])
        change(header)
        raw = self.transport["wire"](header)
        return len(raw).to_bytes(4, "big") + raw + frame[4 + size:]

    def response(self, cap=None, request_sha=None):
        cap = self.cap if cap is None else cap
        request_sha = hashlib.sha256(self.frame()).hexdigest() if request_sha is None else request_sha
        return self.transport["encode_response"](
            self.transport["expected_result"](cap), cap, request_sha
        )

    def test_actual_inputs_round_trip_canonically_and_require_eof(self):
        frame = self.frame()
        self.assertEqual(frame, self.frame())
        stream = ObservedStream(frame)
        packet, bootstrap, request_sha = self.transport["read_request"](stream, deepcopy(self.cap))
        self.assertTrue(stream.eof_observed)
        self.assertEqual(packet, self.packet)
        self.assertEqual(bootstrap, self.bootstrap)
        self.assertIsNot(bootstrap, self.bootstrap)
        self.assertEqual(request_sha, hashlib.sha256(frame).hexdigest())

    def test_request_rejects_length_suffix_stale_scope_and_body_drift(self):
        frame = self.frame()
        header_size = int.from_bytes(frame[:4], "big")
        packet_at = 4 + header_size
        bootstrap_at = packet_at + len(self.packet)

        def flip(raw, offset):
            return raw[:offset] + bytes([raw[offset] ^ 1]) + raw[offset + 1:]

        cases = {
            "zero-header": b"\0\0\0\0" + frame[4:],
            "truncated": frame[:-1],
            "suffix": frame + b"x",
            "stale-capability": self.rewrite_header(
                frame, lambda value: value["capability"].update(run="9" * 32)
            ),
            "wrong-packet": flip(frame, packet_at),
            "wrong-helper": flip(frame, bootstrap_at),
            "wrong-packet-length": self.rewrite_header(
                frame, lambda value: value.update(packetBytes=len(self.packet) - 1)
            ),
        }
        for name, raw in cases.items():
            with self.subTest(name=name):
                self.refused(self.transport["read_request"], io.BytesIO(raw), deepcopy(self.cap))

    def test_response_binds_request_capability_plan_counts_and_no_launch(self):
        frame = self.frame()
        request_sha = hashlib.sha256(frame).hexdigest()
        raw = self.response(request_sha=request_sha)
        result = self.transport["read_response"](raw, deepcopy(self.cap), request_sha)
        self.assertEqual(result, self.transport["expected_result"](self.cap))
        self.assertEqual(result["actorPlanSha256"], self.transport["plan_digest"](self.cap))
        self.assertEqual((result["actorFiles"], result["managerFiles"]), (31, 11))
        self.assertIs(result["releasePublished"], False)
        self.assertIs(result["runtimeQualified"], False)
        self.assertIs(result["launchAdmitted"], False)

        for name, mutate in {
            "request-replay": lambda value: value.update(requestSha256="8" * 64),
            "plan": lambda value: value["stage"].update(actorPlanSha256="7" * 64),
            "actor-count": lambda value: value["stage"].update(actorFiles=29),
            "manager-count": lambda value: value["stage"].update(managerFiles=10),
            "release": lambda value: value["stage"].update(releasePublished=True),
            "runtime": lambda value: value["stage"].update(runtimeQualified=True),
            "launch": lambda value: value["stage"].update(launchAdmitted=True),
            "extra-start": lambda value: value["stage"].update(started=True),
        }.items():
            value = json.loads(raw)
            mutate(value)
            with self.subTest(name=name):
                self.refused(
                    self.transport["read_response"],
                    self.transport["wire"](value),
                    deepcopy(self.cap),
                    request_sha,
                )

        stale = deepcopy(self.cap)
        stale["run"] = "9" * 32
        self.refused(self.transport["read_response"], raw, stale, request_sha)

    def test_actual_reservation_and_simulated_guest_exchange_are_one_shot(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        slot = Path(temporary.name) / "guest-slot"
        slot.mkdir(mode=0o700)
        slot.chmod(0o700)
        slot = slot.resolve(strict=True)
        identity = (slot.lstat().st_dev, slot.lstat().st_ino)
        reserve_calls = []
        exchange_calls = []

        def observe(value):
            self.assertEqual(value, self.binding)
            self.assertIsNot(value, self.binding)
            return True

        def reserve(value):
            reserve_calls.append(deepcopy(value))
            return self.reservation["reserve_guest"](
                slot, identity, value, stopped=observe, verify_instance=observe
            )

        def exchange(frame, cap):
            exchange_calls.append((frame, deepcopy(cap)))
            packet, bootstrap, request_sha = self.transport["read_request"](
                io.BytesIO(frame), deepcopy(cap)
            )
            self.assertEqual(packet, self.packet)
            self.assertEqual(bootstrap, self.bootstrap)
            return self.transport["encode_response"](
                self.transport["expected_result"](cap), cap, request_sha
            )

        result = self.transport["prepare_at_reserved_guest"](
            deepcopy(self.binding), self.packet, deepcopy(self.bootstrap),
            reserve=reserve, exchange=exchange,
        )
        self.assertEqual(len(reserve_calls), 1)
        self.assertEqual(len(exchange_calls), 1)
        self.assertEqual(result, self.transport["expected_result"](exchange_calls[0][1]))
        reservation = slot / "reservation.json"
        info = reservation.lstat()
        self.assertTrue(stat.S_ISREG(info.st_mode))
        self.assertEqual((stat.S_IMODE(info.st_mode), info.st_nlink), (0o600, 1))
        self.assertEqual(json.loads(reservation.read_bytes())["binding"], self.binding)
        self.assertFalse((slot / "workload-closed.json").exists())

    def mock_hold(self, held_values, calls):
        values = iter(held_values)

        def held():
            calls["held"] += 1
            return next(values)

        def note_workload(_proof):
            calls["note"] += 1
            raise AssertionError("preparation must not complete the reservation")

        return {
            "reservationSha256": "2" * 64,
            "held": held,
            "note_workload": note_workload,
        }

    def exercise_failure(self, held_values, exchange):
        calls = {"reserve": 0, "held": 0, "exchange": 0, "note": 0}

        def reserve(value):
            self.assertEqual(value, self.binding)
            calls["reserve"] += 1
            return self.mock_hold(held_values, calls)

        def once(frame, cap):
            calls["exchange"] += 1
            return exchange(frame, cap)

        with self.assertRaises((ValueError, RuntimeError)):
            self.transport["prepare_at_reserved_guest"](
                deepcopy(self.binding), self.packet, deepcopy(self.bootstrap),
                reserve=reserve, exchange=once,
            )
        self.assertEqual(calls["reserve"], 1)
        self.assertLessEqual(calls["exchange"], 1)
        self.assertEqual(calls["note"], 0)
        return calls

    def test_ambiguous_or_invalid_exchange_never_retries_or_completes(self):
        def ambiguous(_frame, _cap):
            raise RuntimeError("ambiguous transport close")

        calls = self.exercise_failure([True], ambiguous)
        self.assertEqual((calls["held"], calls["exchange"]), (1, 1))

        calls = self.exercise_failure([True], lambda _frame, _cap: b"not-json\n")
        self.assertEqual((calls["held"], calls["exchange"]), (1, 1))

        def replay(frame, cap):
            return self.transport["encode_response"](
                self.transport["expected_result"](cap), cap, "8" * 64
            )

        calls = self.exercise_failure([True], replay)
        self.assertEqual((calls["held"], calls["exchange"]), (1, 1))

    def test_hold_is_required_before_and_after_the_single_exchange(self):
        calls = self.exercise_failure([False], lambda _frame, _cap: self.fail("exchange called"))
        self.assertEqual((calls["held"], calls["exchange"]), (1, 0))

        def valid(frame, cap):
            return self.transport["encode_response"](
                self.transport["expected_result"](cap), cap, hashlib.sha256(frame).hexdigest()
            )

        calls = self.exercise_failure([True, False], valid)
        self.assertEqual((calls["held"], calls["exchange"]), (2, 1))


if __name__ == "__main__":
    unittest.main()
