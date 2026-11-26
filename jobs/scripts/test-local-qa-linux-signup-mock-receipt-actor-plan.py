"""Portable controls for the fixed mock-receipt actor-plan serializer only."""
import hashlib
import importlib.util
import os
from pathlib import Path
import unittest


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "signup_mock_plan", HERE / "local-qa-linux-signup-mock-receipt-actor-plan.py",
)
plan = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(plan)


class SignupMockReceiptActorPlanTests(unittest.TestCase):
    def test_make_plan_binds_actual_inert_browser_inventory(self):
        evidence = Path(os.environ["MOCK_RECEIPT_EVIDENCE_ROOT"])
        inventory = (evidence / "jobs-c2c-browser-inert-manifest-Auv0nTdj/inventory.json").read_bytes()
        digest = hashlib.sha256(inventory).hexdigest()
        self.assertEqual(digest, plan.BROWSER_INVENTORY)
        cap = {
            "run": "a" * 32,
            "reservationSha256": "b" * 64,
            "packetSha256": plan.PACKET_SHA256,
            "codeInventorySha256": plan.CODE_INVENTORY,
            "sourceTree": plan.SOURCE_TREE,
            "sourceInventorySha256": plan.SOURCE_INVENTORY,
        }
        stage = {
            "run": cap["run"], "packetSha256": cap["packetSha256"],
            "codeInventorySha256": cap["codeInventorySha256"], "actorFiles": 39,
            "managerFiles": 12, "releasePublished": False, "runtimeQualified": False,
        }
        raw = plan.make_plan(cap, stage, "/run/bluey-jobs-c2c-browser/" + cap["run"], inventory, digest)
        self.assertIn(b'"schema":"bluey-jobs-c2c-actor-plan-v1"', raw)
        self.assertIn(plan.CODE_INVENTORY.encode("ascii"), raw)

    def test_make_plan_rejects_old_31_11_receipt(self):
        cap = {
            "run": "a" * 32, "reservationSha256": "b" * 64,
            "packetSha256": plan.PACKET_SHA256, "codeInventorySha256": plan.CODE_INVENTORY,
            "sourceTree": plan.SOURCE_TREE, "sourceInventorySha256": plan.SOURCE_INVENTORY,
        }
        with self.assertRaisesRegex(RuntimeError, "code_stage_receipt"):
            plan.make_plan(cap, {"run": cap["run"], "packetSha256": cap["packetSha256"],
                                 "codeInventorySha256": cap["codeInventorySha256"], "actorFiles": 31,
                                 "managerFiles": 11, "releasePublished": False, "runtimeQualified": False},
                           "/run/bluey-jobs-c2c-browser/" + cap["run"], b"{}", "a" * 64)

    def test_make_plan_refuses_self_consistent_unadmitted_inventory_digest(self):
        cap = {
            "run": "a" * 32, "reservationSha256": "b" * 64,
            "packetSha256": plan.PACKET_SHA256, "codeInventorySha256": plan.CODE_INVENTORY,
            "sourceTree": plan.SOURCE_TREE, "sourceInventorySha256": plan.SOURCE_INVENTORY,
        }
        stage = {"run": cap["run"], "packetSha256": cap["packetSha256"],
                 "codeInventorySha256": cap["codeInventorySha256"], "actorFiles": 39,
                 "managerFiles": 12, "releasePublished": False, "runtimeQualified": False}
        with self.assertRaisesRegex(RuntimeError, "browser_inventory"):
            plan.make_plan(cap, stage, "/run/bluey-jobs-c2c-browser/" + cap["run"], b"{}", "a" * 64)

    def test_make_plan_refuses_boolean_file_counts(self):
        cap = {
            "run": "a" * 32, "reservationSha256": "b" * 64,
            "packetSha256": plan.PACKET_SHA256, "codeInventorySha256": plan.CODE_INVENTORY,
            "sourceTree": plan.SOURCE_TREE, "sourceInventorySha256": plan.SOURCE_INVENTORY,
        }
        receipt = {"run": cap["run"], "packetSha256": cap["packetSha256"],
                   "codeInventorySha256": cap["codeInventorySha256"], "actorFiles": True,
                   "managerFiles": 12, "releasePublished": False, "runtimeQualified": False}
        with self.assertRaisesRegex(RuntimeError, "code_stage_receipt"):
            plan.make_plan(cap, receipt, "/run/bluey-jobs-c2c-browser/" + cap["run"], b"{}", plan.BROWSER_INVENTORY)


if __name__ == "__main__":
    unittest.main()
