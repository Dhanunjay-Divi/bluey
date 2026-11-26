import hashlib
import importlib.util
import json
from pathlib import Path
import unittest


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "signup_sample_terminal", HERE / "local-qa-linux-signup-sample-terminal.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

TREE = "a" * 40
INVENTORY = "b" * 64


def frame(**changes):
    value = {
        "event": "local_qa_linux_browser_sample_resume_completed",
        "sourceTree": TREE,
        "sourceInventorySha256": INVENTORY,
        "evidence": {
            "browserSignup": True,
            "candidateWorkspace": True,
            "signupMailCount": 1,
            "applicationCount": 0,
            "submittedCount": 0,
            "sampleResumeImported": True,
        },
    }
    value.update(changes)
    return (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")


class SignupSampleTerminalTests(unittest.TestCase):
    def test_accepts_exact_worker_event(self):
        raw = frame()
        result = MODULE.terminal(raw, TREE, INVENTORY)
        self.assertEqual(result["event"], "local_qa_linux_browser_sample_resume_completed")
        self.assertEqual(hashlib.sha256(raw).hexdigest(), hashlib.sha256(frame()).hexdigest())

    def test_rejects_wrong_provenance_and_legacy_event(self):
        for raw, tree, inventory in [
            (frame(sourceTree="c" * 40), TREE, INVENTORY),
            (frame(), "c" * 40, INVENTORY),
            (frame(sourceInventorySha256="c" * 64), TREE, INVENTORY),
            (frame(event="local_qa_linux_browser_signup_completed"), TREE, INVENTORY),
            (frame(event="local_qa_linux_c2c_native_completed"), TREE, INVENTORY),
        ]:
            with self.assertRaises(ValueError):
                MODULE.terminal(raw, tree, inventory)

    def test_rejects_noncanonical_and_malformed_frames(self):
        for raw in [frame()[:-1], b"{}\n", b"{\xff}\n", frame() + b"\n", b"x" * 65_537]:
            with self.assertRaises(ValueError):
                MODULE.terminal(raw, TREE, INVENTORY)

    def test_rejects_extra_keys_and_boolean_numeric_swaps(self):
        base = json.loads(frame())
        variants = []
        extra = dict(base); extra["legacy"] = True; variants.append(extra)
        for key, value in [("browserSignup", 1), ("candidateWorkspace", 1),
                           ("sampleResumeImported", 1), ("signupMailCount", True),
                           ("applicationCount", False), ("submittedCount", False)]:
            changed = json.loads(frame())
            changed["evidence"][key] = value
            variants.append(changed)
        for value in variants:
            raw = (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
            with self.assertRaises(ValueError):
                MODULE.terminal(raw, TREE, INVENTORY)


if __name__ == "__main__":
    unittest.main()
