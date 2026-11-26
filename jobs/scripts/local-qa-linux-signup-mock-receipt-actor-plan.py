"""Prepare one inert 39/12 signup/mock-receipt actor plan.

The immutable C2C actor-plan owns the physical descriptor and browser-closure
checks.  This successor only narrows its code-packet receipt and capability to
the current fixed mock-receipt candidate; it never launches a process.
"""
from contextlib import ExitStack
import hashlib
import os
from pathlib import Path
import re
import stat
import types


GATES = Path("/run/bluey-jobs-c2c-gates")
BROWSER = Path("/run/bluey-jobs-c2c-browser")
PROTECTED_PLAN = "local-qa-linux-c2c-actor-plan.py"
PROTECTED_PLAN_SHA256 = "3c7846a6b6a442229b8fbefedb10d5de89b658fd531e3c0807225689332f7676"
STAGER = "local-qa-linux-signup-mock-receipt-code-stage.py"
STAGER_SHA256 = "f67b2d888da5ca0bc81fb65eef434ee308aec04b935d478f0365782dbaa0dec5"
PACKET_SHA256 = "ff7df61110b4521faadef1a10f6f58a5396657d8f8ad537a7ab65648f1bebd29"
SOURCE_TREE = "6fe9eab0180160bc70079f8b9020fa21abf0db62"
SOURCE_INVENTORY = "08459c30b037e839c798b4c5859dcbd2bb271905da36bcc3cdd7aaa4241eb7cc"
CODE_INVENTORY = "99035f76562ad58570f6f79e71907c02e53a0a14d69fae5b2d4a19e439930c56"
BROWSER_INVENTORY = "9ecdff4561fe0924e499325c40a6b61ffbe7ea047044d3f2cc0a89f82e728dce"
ACTOR_COUNT = 39
MANAGER_COUNT = 12
CAP = {"run", "reservationSha256", "packetSha256", "codeInventorySha256", "sourceTree", "sourceInventorySha256"}
SHA = re.compile(r"^[0-9a-f]{64}$")
RUN = re.compile(r"^[0-9a-f]{32}$")
RD = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK


def _need(value, label):
    if not value:
        raise RuntimeError("local_qa_linux_signup_mock_receipt_actor_plan_refused:" + label)


def _sha(raw):
    return hashlib.sha256(raw).hexdigest()


def _capability(value):
    _need(type(value) is dict and set(value) == CAP and type(value["run"]) is str
          and RUN.fullmatch(value["run"]) is not None and value["run"] != "0" * 32
          and value["packetSha256"] == PACKET_SHA256 and value["codeInventorySha256"] == CODE_INVENTORY
          and value["sourceTree"] == SOURCE_TREE and value["sourceInventorySha256"] == SOURCE_INVENTORY
          and type(value["reservationSha256"]) is str and SHA.fullmatch(value["reservationSha256"]) is not None
          and value["reservationSha256"] != "0" * 64, "capability")
    return dict(value)


def _load_sibling(name, digest, label):
    path = Path(__file__).resolve(strict=True).with_name(name)
    _need(path.resolve(strict=True) == path, label + "_path")
    fd = os.open(path, RD)
    try:
        before = os.fstat(fd)
        _need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and not before.st_mode & 0o022
              and before.st_uid in (0, os.geteuid()) and 0 < before.st_size <= 64 << 10, label + "_metadata")
        parts, remaining = [], before.st_size + 1
        while remaining:
            chunk = os.read(fd, min(65536, remaining))
            if not chunk:
                break
            parts.append(chunk)
            remaining -= len(chunk)
        raw = b"".join(parts)
        after = os.fstat(fd)
        named = path.lstat()
        stamp = lambda row: (row.st_dev, row.st_ino, row.st_uid, row.st_gid, row.st_mode,
                             row.st_nlink, row.st_size, row.st_mtime_ns, row.st_ctime_ns)
        _need(len(raw) == before.st_size and _sha(raw) == digest and stamp(before) == stamp(after) == stamp(named),
              label + "_pin")
    finally:
        os.close(fd)
    module = types.ModuleType("_pinned_" + label)
    module.__file__ = str(path)
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


def _helpers():
    plan = _load_sibling(PROTECTED_PLAN, PROTECTED_PLAN_SHA256, "protected_plan")
    _need(all(callable(getattr(plan, name, None)) for name in (
        "inventory_node", "root_directory", "read_small", "pinned_file", "assert_node", "wire",
    )), "protected_plan_contract")
    stager = _load_sibling(STAGER, STAGER_SHA256, "mock_stager")
    _need(callable(getattr(stager, "stage_code_packet", None))
          and callable(getattr(stager, "_load_protected_stager", None)), "mock_stager_contract")
    physical = stager._load_protected_stager()
    _need(all(callable(getattr(physical, name, None)) for name in (
        "_root_linux", "_open_base", "_base_current", "_absent", "_stage", "_verify_stage",
    )), "protected_stage_contract")
    return plan, stager, physical


def make_plan(cap, stage_receipt, closure_root, inventory_raw, inventory_sha256):
    """Create the unchanged waiter-readable plan after exact 39/12 staging."""
    value = _capability(cap)
    stage_keys = {"run", "packetSha256", "codeInventorySha256", "actorFiles", "managerFiles",
                  "releasePublished", "runtimeQualified"}
    _need(type(stage_receipt) is dict and set(stage_receipt) == stage_keys
          and all(stage_receipt[key] == value[key] for key in ("run", "packetSha256", "codeInventorySha256"))
          and type(stage_receipt["actorFiles"]) is int and stage_receipt["actorFiles"] == ACTOR_COUNT
          and type(stage_receipt["managerFiles"]) is int and stage_receipt["managerFiles"] == MANAGER_COUNT
          and stage_receipt["releasePublished"] is False and stage_receipt["runtimeQualified"] is False,
          "code_stage_receipt")
    _need(type(closure_root) is str and closure_root == str(BROWSER / value["run"]), "browser_root")
    _need(inventory_sha256 == BROWSER_INVENTORY, "browser_inventory")
    plan, _, _ = _helpers()
    node = plan.inventory_node(inventory_raw, inventory_sha256)
    return plan.wire({"schema": "bluey-jobs-c2c-actor-plan-v1", "run": value["run"],
                      "sourceTree": value["sourceTree"], "sourceInventorySha256": value["sourceInventorySha256"],
                      "codeInventorySha256": value["codeInventorySha256"],
                      "node": {"path": closure_root + "/" + plan.NODE, "sha256": node["sha256"]}})


def stage_code_and_plan(packet, cap, browser_inventory_sha256, *, admitted):
    """Root guest preparation only; returns inert plan metadata, never launch authority."""
    _need(callable(admitted) and browser_inventory_sha256 == BROWSER_INVENTORY,
          "root_admission")
    value = _capability(cap)
    plan, stager, physical = _helpers()
    closure = BROWSER / value["run"]
    node = closure / plan.NODE
    projection = {**value, "browserInventorySha256": browser_inventory_sha256, "browserRoot": str(closure)}
    _need(physical._root_linux() and admitted(dict(projection)) is True, "guest_admission")
    with ExitStack() as stack:
        gate = physical._open_base(GATES, 0o755)
        stack.callback(os.close, gate)
        physical._absent(gate, value["run"])
        browser_fd = plan.root_directory(closure, stack, immutable_from=closure)
        node_parent = plan.root_directory(node.parent, stack, immutable_from=closure)
        inventory, inventory_held = plan.read_small(browser_fd, "inventory.json", plan.INVENTORY_MAX, 0o444, stack)
        plan.inventory_node(inventory, browser_inventory_sha256)
        node_fd, size, node_held = plan.pinned_file(node_parent, node.name, plan.NODE_SIZE, 0o555, stack)
        plan.assert_node(node_fd, size, node_held)

        def current(binding):
            return (binding == value and physical._root_linux() and admitted(dict(projection)) is True
                    and inventory_held() and node_held() and os.path.samestat(os.fstat(browser_fd), closure.lstat())
                    and os.path.samestat(os.fstat(node_parent), node.parent.lstat()) and node.resolve(strict=True) == node)

        result = stager.stage_code_packet(packet, value["packetSha256"], value, admitted=current)
        raw = make_plan(value, result, str(closure), inventory, browser_inventory_sha256)
        physical._base_current(GATES, gate, 0o755)
        _need(current(value), "plan_admission_before")
        staged = physical._stage(gate, value["run"], {"plan.json": raw}, 0o755)
        stack.callback(os.close, staged)
        _need(current(value), "plan_admission_after")
        physical._base_current(GATES, gate, 0o755)
        physical._verify_stage(gate, value["run"], staged, {"plan.json": raw}, 0o755)
        return {**result, "reservationSha256": value["reservationSha256"], "actorPlanSha256": _sha(raw),
                "browserRoot": str(closure), "browserInventorySha256": browser_inventory_sha256,
                "launchAdmitted": False}
