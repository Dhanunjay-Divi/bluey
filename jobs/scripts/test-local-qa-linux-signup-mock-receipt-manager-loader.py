"""Portable packet-only controls for the staged manager binding helper."""
import importlib.util
import os
from pathlib import Path
import unittest


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "manager_loader", HERE / "local-qa-linux-signup-mock-receipt-manager-loader.py",
)
loader = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(loader)


class StagedManagerLoaderTests(unittest.TestCase):
    @staticmethod
    def packet():
        evidence = Path(os.environ["MOCK_RECEIPT_EVIDENCE_ROOT"])
        return (evidence / "jobs-signup-mock-receipt-source.Lu4NPZR1/"
                "signup-mock-receipt-source-candidate.tar").read_bytes()

    def test_actual_sealed_packet_has_exact_twelve_manager_members(self):
        raw = self.packet()
        receipt = loader.verify_sealed_packet(raw)
        self.assertEqual(receipt, {"packetSha256": loader.PACKET_SHA256, "managerFiles": 12,
                                   "managerSha256": loader.MANAGER[loader.MANAGER_NAME][1]})

    def test_packet_tamper_refuses_before_unpack(self):
        raw = self.packet()
        with self.assertRaisesRegex(RuntimeError, "packet_pin"):
            loader.verify_sealed_packet(raw[:-1] + bytes([raw[-1] ^ 1]))


if __name__ == "__main__":
    unittest.main()
