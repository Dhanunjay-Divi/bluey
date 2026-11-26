"""Portable controls for the fixed signup/mock-receipt code stager.

These tests never claim Linux-root staging authority.  They exercise only the
closed tar parser and the fail-closed non-Linux entry condition.
"""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import types
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "signup_mock_stage", HERE / "local-qa-linux-signup-mock-receipt-code-stage.py",
)
stage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stage)


def _wire(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def _packet():
    actor = {"local-qa-linux-c2c-release-protocol.py": b"protocol"}
    actor.update({f"actor-{index:02d}.mjs": f"actor-{index}".encode("ascii") for index in range(38)})
    inventory = _wire({"version": 1, "files": [
        {"name": name, "bytes": len(body), "sha256": hashlib.sha256(body).hexdigest()}
        for name, body in sorted(actor.items())
    ]})
    manager = {"local-qa-linux-c2c-release-protocol.py": b"protocol"}
    manager.update({f"manager-{index:02d}.py": f"manager-{index}".encode("ascii") for index in range(11)})
    meta = {
        "schema": stage.SCHEMA,
        "basePacketSha256": "fffacd898962bf5d4bc86e09dc6e51879d7088aa79ca6866c647800056fe6af3",
        "baseCodeInventorySha256": "30be415457e3a83607c0d2a49b542b9ebb95937a767227555deffec6c8de9fd1",
        "codeInventorySha256": hashlib.sha256(inventory).hexdigest(),
        "actorFiles": 39,
        "managerFiles": 12,
        "actor": "local-qa-linux-signup-mock-receipt-native.mjs",
        "waiter": "local-qa-linux-signup-mock-receipt-wait-actor.py",
        "manager": "local-qa-linux-signup-mock-receipt-guest-manager.py",
        "physicalSourceAdmitted": False,
        "releaseAdmitted": False,
        "runtimeQualified": False,
    }
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, mode, body in (
            [("candidate.json", 0o444, _wire(meta)), ("actor/inventory.json", 0o444, inventory)]
            + [("actor/" + name, 0o444, body) for name, body in sorted(actor.items())]
            + [("manager/" + name, 0o600, body) for name, body in sorted(manager.items())]
        ):
            entry = tarfile.TarInfo(name)
            entry.mode, entry.size, entry.uid, entry.gid, entry.mtime = mode, len(body), 0, 0, 0
            archive.addfile(entry, io.BytesIO(body))
    return output.getvalue(), hashlib.sha256(inventory).hexdigest()


class SignupMockReceiptCodeStageTests(unittest.TestCase):
    def test_pure_packet_unpack_is_closed_and_tamper_refuses(self):
        raw, inventory = _packet()
        digest = hashlib.sha256(raw).hexdigest()
        with patch.multiple(stage, PACKET_SHA256=digest, PACKET_BYTES=len(raw), CODE_INVENTORY=inventory):
            actor, manager, receipt = stage.verify_packet(raw, digest)
            self.assertEqual((len(actor) - 1, len(manager)), (39, 12))
            self.assertEqual(receipt["codeInventorySha256"], inventory)
            tampered = raw[:-1] + bytes([raw[-1] ^ 1])
            with self.assertRaisesRegex(RuntimeError, "packet_pin"):
                stage.verify_packet(tampered, digest)

    def test_stage_refuses_before_packet_or_callback_off_linux(self):
        cap = {
            "run": "a" * 32,
            "reservationSha256": "b" * 64,
            "packetSha256": stage.PACKET_SHA256,
            "codeInventorySha256": stage.CODE_INVENTORY,
            "sourceTree": stage.SOURCE_TREE,
            "sourceInventorySha256": stage.SOURCE_INVENTORY,
        }
        protected = types.SimpleNamespace(_root_linux=lambda: False)
        with patch.object(stage, "_load_protected_stager", return_value=protected):
            with self.assertRaisesRegex(RuntimeError, "platform"):
                stage.stage_code_packet(b"not-a-packet", stage.PACKET_SHA256, cap, admitted=lambda _: True)


if __name__ == "__main__":
    unittest.main()
