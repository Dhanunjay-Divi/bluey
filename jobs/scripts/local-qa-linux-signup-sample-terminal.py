"""Pure terminal-frame validator for the synthetic Linux sample-resume journey."""

import json
import re

MAX_FRAME = 16 << 10
TREE = re.compile(r"[0-9a-f]{40}\Z")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
EVENT = "local_qa_linux_browser_sample_resume_completed"


def _need(value, reason):
    if not value:
        raise ValueError(f"local_qa_linux_signup_sample_terminal:{reason}")


def terminal(raw, source_tree, source_inventory):
    """Accept exactly one canonical, provenance-bound sample-resume terminal frame."""
    _need(type(raw) is bytes and 0 < len(raw) <= MAX_FRAME, "frame_size")
    _need(type(source_tree) is str and TREE.fullmatch(source_tree), "source_tree")
    _need(type(source_inventory) is str and SHA256.fullmatch(source_inventory), "source_inventory")
    try:
        value = json.loads(raw.decode("ascii"))
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError("local_qa_linux_signup_sample_terminal:frame_json") from error
    _need(type(value) is dict and set(value) == {
        "event", "sourceTree", "sourceInventorySha256", "evidence"
    }, "frame_shape")
    canonical = (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
    _need(raw == canonical, "frame_canonical")
    _need(value["event"] == EVENT and value["sourceTree"] == source_tree
          and value["sourceInventorySha256"] == source_inventory, "provenance")
    evidence = value["evidence"]
    _need(type(evidence) is dict and set(evidence) == {
        "browserSignup", "candidateWorkspace", "signupMailCount", "applicationCount",
        "submittedCount", "sampleResumeImported"
    }, "evidence_shape")
    _need(evidence["browserSignup"] is True and evidence["candidateWorkspace"] is True
          and evidence["sampleResumeImported"] is True
          and type(evidence["signupMailCount"]) is int and evidence["signupMailCount"] == 1
          and type(evidence["applicationCount"]) is int and evidence["applicationCount"] == 0
          and type(evidence["submittedCount"]) is int and evidence["submittedCount"] == 0,
          "evidence_values")
    return value
