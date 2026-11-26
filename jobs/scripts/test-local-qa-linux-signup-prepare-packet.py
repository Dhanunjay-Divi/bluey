import json
from pathlib import Path
import runpy
import unittest


HERE = Path(__file__).resolve().parent
PACKET = runpy.run_path(str(HERE / "local-qa-linux-signup-prepare-code-packet.py"))
CHECK = runpy.run_path(str(HERE / "local-qa-linux-signup-prepare-terminal.py"))["terminal"]
OLD = runpy.run_path(str(HERE / "local-qa-linux-signup-sample-terminal.py"))["terminal"]
TREE, INVENTORY = "1" * 40, "2" * 64
EVIDENCE = {"browserSignup": True, "candidateWorkspace": True, "signupMailCount": 1,
            "applicationCount": 1, "submittedCount": 0, "sampleResumeImported": True, "reviewFirstPrepared": True}
EVENT = "local_qa_linux_browser_sample_prepare_completed"


def frame(evidence=None, event=EVENT):
    return (json.dumps({"event": event, "sourceTree": TREE, "sourceInventorySha256": INVENTORY,
                        "evidence": evidence if evidence is not None else EVIDENCE},
                       separators=(",", ":")) + "\n").encode("ascii")


class SignupPrepareControls(unittest.TestCase):
    def test_only_the_new_frame_is_accepted(self):
        self.assertEqual(CHECK(frame(), TREE, INVENTORY)["evidence"], EVIDENCE)
        with self.assertRaisesRegex(ValueError, ":provenance"):
            OLD(frame(), TREE, INVENTORY)
        previous = frame({key: value for key, value in EVIDENCE.items() if key != "reviewFirstPrepared"}
                         | {"applicationCount": 0}, "local_qa_linux_browser_sample_resume_completed")
        OLD(previous, TREE, INVENTORY)
        with self.assertRaisesRegex(ValueError, ":provenance"):
            CHECK(previous, TREE, INVENTORY)

    def test_counts_types_and_authority_cannot_drift(self):
        for change in ({"applicationCount": 0}, {"applicationCount": True}, {"submittedCount": 1},
                       {"submittedCount": False}, {"reviewFirstPrepared": False}, {"accountId": "not-retained"}):
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, ":evidence_"):
                CHECK(frame(EVIDENCE | change), TREE, INVENTORY)
        for raw in (frame() + b"\n", frame() + frame(), frame().replace(b'{"event":', b'{"event":null,"event":')):
            with self.subTest(raw=raw[:30]), self.assertRaisesRegex(ValueError, ":frame_"):
                CHECK(raw, TREE, INVENTORY)
        with self.assertRaisesRegex(ValueError, ":provenance"):
            CHECK(frame(), TREE, "3" * 64)

    def test_refuses_unknown_predecessor_before_source_access(self):
        with self.assertRaisesRegex(ValueError, ":base_packet"):
            PACKET["compose"](b"x" * PACKET["BASE_BYTES"], HERE / "unused")

    def test_exact_replacement_rejects_ambiguous_input(self):
        self.assertEqual(PACKET["replace_once"](b"a.b", b".", b"-"), b"a-b")
        for raw in (b"ab", b"a..b"):
            with self.assertRaisesRegex(ValueError, ":render_anchor"):
                PACKET["replace_once"](raw, b".", b"-")


if __name__ == "__main__":
    unittest.main()
