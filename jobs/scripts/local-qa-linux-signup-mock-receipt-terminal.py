"""Validate one completed synthetic Linux signup-to-mock-receipt frame."""

import json
import re

MAX_FRAME = 16 << 10
TREE = re.compile(r"[0-9a-f]{40}\Z")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
EVENT = "local_qa_linux_browser_sample_mock_receipt_completed"


def _need(ok, reason):
    if not ok:
        raise ValueError("local_qa_linux_signup_mock_receipt_terminal:" + reason)


def terminal(raw, source_tree, source_inventory):
    """Return only a canonical, complete, current synthetic receipt frame."""
    _need(type(raw) is bytes and 0 < len(raw) <= MAX_FRAME, "frame_size")
    _need(type(source_tree) is str and TREE.fullmatch(source_tree), "source_tree")
    _need(type(source_inventory) is str and SHA256.fullmatch(source_inventory), "source_inventory")
    try:
        value = json.loads(raw.decode("ascii"))
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError("local_qa_linux_signup_mock_receipt_terminal:frame_json") from error
    _need(type(value) is dict and set(value) == {
        "event", "sourceTree", "sourceInventorySha256", "evidence"
    }, "frame_shape")
    canonical = (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
    _need(raw == canonical, "frame_canonical")
    _need(value["event"] == EVENT and value["sourceTree"] == source_tree
          and value["sourceInventorySha256"] == source_inventory, "provenance")

    evidence = value["evidence"]
    flags = {
        "browserSignup",
        "candidateWorkspace",
        "sampleResumeImported",
        "mockReceiptObserved",
    }
    counts = {"signupMailCount": 1, "applicationCount": 1, "submittedCount": 1}
    _need(type(evidence) is dict and set(evidence) == flags | counts.keys(), "evidence_shape")
    _need(all(evidence[key] is True for key in flags) and all(
        type(evidence[key]) is int and evidence[key] == expected
        for key, expected in counts.items()
    ), "evidence_values")
    return value
