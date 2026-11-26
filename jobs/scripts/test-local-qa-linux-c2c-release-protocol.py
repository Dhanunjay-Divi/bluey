#!/usr/bin/env python3
"""Portable pure controls for the Linux C2C release protocol."""
from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import unittest


_PATH = Path(__file__).with_name("local-qa-linux-c2c-release-protocol.py")
_SPEC = importlib.util.spec_from_file_location("release_protocol", _PATH)
assert _SPEC and _SPEC.loader
protocol = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(protocol)


def waiting() -> dict:
    run = "a" * 32
    return {"schema": "bluey-jobs-c2c-waiting-v1", "run": run, "unitName": f"bluey-jobs-qa-{run}.service",
            "bootId": "b" * 32, "invocationId": "c" * 32,
            "unitCgroup": f"/system.slice/bluey-jobs-qa-{run}.service", "networkNamespace": "net:[4026532555]",
            "root": "/tmp/bluey-test-runs-abcdef012345/run.1795250653.12345.32615190360",
            "rootDevice": 0, "rootInode": 44, "markerSha256": "d" * 64, "actorUid": 999,
            "mainPid": 12345, "mainStartTime": 100, "actorPid": 12346, "actorStartTime": 101,
            "nonce": "e" * 32, "waitingNs": 1_000_000_000}


class ProtocolTests(unittest.TestCase):
    def test_canonical_release_round_trip(self) -> None:
        raw = protocol.encode(waiting())
        parsed = protocol.parse_waiting(raw)
        observed = {key: value for key, value in parsed.items() if key not in {"schema", "nonce", "waitingNs"}}
        release = protocol.make_release(raw, observed, "f" * 64, 2_000_000_000)
        self.assertEqual(protocol.accept_release(release, raw, 2_000_000_000)["specSha256"], "f" * 64)
        self.assertEqual(hashlib.sha256(raw).hexdigest(), protocol.accept_release(release, raw, 2_000_000_000)["waitingSha256"])

    def test_refuses_noncanonical_malformed_mismatch_and_stale_records(self) -> None:
        raw = protocol.encode(waiting())
        with self.assertRaises(ValueError):
            protocol.parse_waiting(b'{"run":"a"}\n')
        with self.assertRaises(ValueError):
            protocol.parse_waiting(raw.replace(b'"run":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"', b'"run":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"'))
        for update in ({"mainPid": 1}, {"actorPid": 12345}, {"mainPid": 12344},
                       {"root": waiting()["root"][:-11] + "0"}):
            with self.subTest(update=update), self.assertRaises(ValueError):
                protocol.parse_waiting(protocol.encode(dict(waiting(), **update)))
        observed = {key: value for key, value in waiting().items() if key not in {"schema", "nonce", "waitingNs"}}
        observed["mainPid"] = 999
        with self.assertRaises(ValueError):
            protocol.make_release(raw, observed, "f" * 64, 2_000_000_000)
        valid = protocol.make_release(raw, {key: value for key, value in waiting().items() if key not in {"schema", "nonce", "waitingNs"}}, "f" * 64, 2_000_000_000)
        with self.assertRaises(ValueError):
            protocol.accept_release(valid, protocol.encode({**waiting(), "nonce": "1" * 32}), 2_000_000_000)
        with self.assertRaises(ValueError):
            protocol.accept_release(valid, raw, 91_000_000_000)
        with self.assertRaises(ValueError):
            protocol.make_release(raw, {key: value for key, value in waiting().items()
                                        if key not in {"schema", "nonce", "waitingNs"}}, "f" * 64, 91_000_000_000)


if __name__ == "__main__":
    unittest.main()
