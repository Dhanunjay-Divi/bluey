import json
from pathlib import Path
import runpy
import unittest


HERE = Path(__file__).resolve().parent
CHECK = runpy.run_path(str(HERE / "local-qa-linux-signup-mock-receipt-terminal.py"))["terminal"]
PREPARE = runpy.run_path(str(HERE / "local-qa-linux-signup-prepare-terminal.py"))["terminal"]
TREE, INVENTORY = "1" * 40, "2" * 64
EVENT = "local_qa_linux_browser_sample_mock_receipt_completed"
EVIDENCE = {
    "browserSignup": True,
    "candidateWorkspace": True,
    "sampleResumeImported": True,
    "mockReceiptObserved": True,
    "signupMailCount": 1,
    "applicationCount": 1,
    "submittedCount": 1,
}


def frame(evidence=None, event=EVENT, tree=TREE, inventory=INVENTORY):
    return (json.dumps({
        "event": event,
        "sourceTree": tree,
        "sourceInventorySha256": inventory,
        "evidence": evidence if evidence is not None else EVIDENCE,
    }, separators=(",", ":")) + "\n").encode("ascii")


class SignupMockReceiptTerminalControls(unittest.TestCase):
    def test_accepts_only_the_completed_mock_receipt_frame(self):
        self.assertEqual(CHECK(frame(), TREE, INVENTORY)["evidence"], EVIDENCE)
        prepare_evidence = {
            "browserSignup": True,
            "candidateWorkspace": True,
            "sampleResumeImported": True,
            "reviewFirstPrepared": True,
            "signupMailCount": 1,
            "applicationCount": 1,
            "submittedCount": 0,
        }
        prepare = frame(prepare_evidence, "local_qa_linux_browser_sample_prepare_completed")
        with self.assertRaisesRegex(ValueError, ":provenance"):
            CHECK(prepare, TREE, INVENTORY)
        self.assertEqual(PREPARE(prepare, TREE, INVENTORY)["evidence"], prepare_evidence)

    def test_rejects_drifted_effect_evidence_and_extra_fields(self):
        for change in (
            {"browserSignup": False},
            {"candidateWorkspace": False},
            {"sampleResumeImported": False},
            {"mockReceiptObserved": False},
            {"signupMailCount": 0},
            {"signupMailCount": True},
            {"applicationCount": 0},
            {"applicationCount": 2},
            {"submittedCount": 0},
            {"submittedCount": True},
            {"accountId": "not-retained"},
        ):
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, ":evidence_"):
                CHECK(frame(EVIDENCE | change), TREE, INVENTORY)

    def test_rejects_noncanonical_duplicate_or_stale_frames(self):
        for raw in (
            frame() + b"\n",
            frame() + frame(),
            frame().replace(b'{"event":', b'{"event":null,"event":'),
            b"{\n" + frame()[1:],
        ):
            with self.subTest(raw=raw[:30]), self.assertRaisesRegex(ValueError, ":frame_"):
                CHECK(raw, TREE, INVENTORY)
        for tree, inventory in (("3" * 40, INVENTORY), (TREE, "4" * 64)):
            with self.subTest(tree=tree, inventory=inventory), self.assertRaisesRegex(ValueError, ":provenance"):
                CHECK(frame(), tree, inventory)

    def test_rejects_invalid_frame_and_expected_provenance_inputs(self):
        for raw in (b"", b"[]\n", b"{bad}\n", b"\xff"):
            with self.subTest(raw=raw), self.assertRaisesRegex(ValueError, ":frame_"):
                CHECK(raw, TREE, INVENTORY)
        for tree, inventory in (("A" * 40, INVENTORY), (TREE, "A" * 64), ("1" * 39, INVENTORY)):
            with self.subTest(tree=tree, inventory=inventory), self.assertRaisesRegex(ValueError, ":source_"):
                CHECK(frame(), tree, inventory)


if __name__ == "__main__":
    unittest.main()
