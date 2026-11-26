"""Prepare the staged C2C actor's exact, inert plan. No launch or release."""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import types


GATES = Path("/run/bluey-jobs-c2c-gates")
BROWSER = Path("/run/bluey-jobs-c2c-browser")
STAGER = "local-qa-linux-c2c-stage-code-packet.py"
STAGER_SHA = "1941e8ae352b1d3e0dd4cbf676e2b3d4f9c76a66d54c59a59d15afc1ab08a661"
NODE = "receipt-journey-node/toolchain/bin/node"
NODE_SHA = "94efd062ed066c1a0b332e18249859c905e920eeada5df55d24c99b919629a90"
NODE_SIZE = 124_902_584
PW = "receipt-prep-browser/node_modules/playwright-core/"
CR = "receipt-prep-browser/chromium/"
MANDATORY = {PW + "index.mjs": 0o444, PW + "package.json": 0o444, PW + "browsers.json": 0o444,
             CR + "chrome": 0o555, NODE: 0o555}
CAP = {"run", "reservationSha256", "packetSha256", "codeInventorySha256", "sourceTree", "sourceInventorySha256"}
INVENTORY_MAX = 1 << 20
DIR = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_DIRECTORY
FILE = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK


def need(ok, label):
    if not ok:
        raise ValueError("c2c_actor_plan_refused:" + label)


def hex_pin(value, width=64):
    return type(value) is str and re.fullmatch(f"[0-9a-f]{{{width}}}", value) and value != "0" * width


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def wire(value, *, sort_keys=True):
    return (json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=sort_keys) + "\n").encode("ascii")


def capability(value):
    need(type(value) is dict and value.keys() == CAP and hex_pin(value["run"], 32) and hex_pin(value["sourceTree"], 40)
         and all(hex_pin(value[key]) for key in CAP - {"run", "sourceTree"}), "capability")
    return dict(value)


def inventory_node(raw, expected):
    """Check the actor inventory grammar and locate the one approved Node."""
    need(type(raw) is bytes and 0 < len(raw) <= INVENTORY_MAX and hex_pin(expected) and sha(raw) == expected,
         "browser_inventory_pin")
    try:
        value = json.loads(raw.decode("ascii"))
        canonical = wire(value, sort_keys=False)  # Actor preserves input key order when verifying JSON bytes.
    except (ValueError, RecursionError) as error:
        raise ValueError("c2c_actor_plan_refused:browser_inventory") from error
    need(type(value) is dict and value.keys() == {"version", "files"} and type(value["version"]) is int
         and value["version"] == 1 and type(value["files"]) is list and len(value["files"]) <= 768
         and canonical == raw, "browser_inventory")
    prior, total, present = "", 0, {}
    for row in value["files"]:
        need(type(row) is dict and row.keys() == {"path", "sha256", "size", "mode"}, "browser_row")
        name, size, mode = row["path"], row["size"], row["mode"]
        need(type(name) is str and re.fullmatch(r"[A-Za-z0-9._@+/-]{1,240}", name) and name > prior
             and (name == NODE or name.startswith((PW, CR)))
             and not {"", ".", "..", ".git"}.intersection(name.split("/"))
             and type(size) is int and 0 <= size <= 320 << 20 and type(mode) is int and mode in (0o444, 0o555)
             and hex_pin(row["sha256"]), "browser_member")
        need(not any(parent.as_posix() in present for parent in Path(name).parents), "browser_member_collision")
        present[name] = row
        prior, total = name, total + size
        need(total <= 600 << 20, "browser_inventory_size")
    need(all(name in present and present[name]["mode"] == mode and present[name]["size"] > 0
             for name, mode in MANDATORY.items()), "browser_mandatory")
    row = present[NODE]
    need(row["sha256"] == NODE_SHA and row["size"] == NODE_SIZE, "node_pin")
    return dict(row)


def make_plan(capability_value, stage_receipt, closure_root, inventory_raw, inventory_sha256):
    """Serialize the existing waiter's plan from the paired-stage receipt."""
    cap = capability(capability_value)
    stage_keys = {"run", "packetSha256", "codeInventorySha256", "actorFiles", "managerFiles",
                  "releasePublished", "runtimeQualified"}
    need(type(stage_receipt) is dict and stage_receipt.keys() == stage_keys
         and all(stage_receipt[key] == cap[key] for key in ("run", "packetSha256", "codeInventorySha256"))
         and all(type(stage_receipt[key]) is int and stage_receipt[key] == number
                 for key, number in (("actorFiles", 31), ("managerFiles", 11)))
         and stage_receipt["releasePublished"] is False and stage_receipt["runtimeQualified"] is False,
         "code_stage_receipt")
    need(type(closure_root) is str and closure_root == str(BROWSER / cap["run"]), "browser_root")
    node = inventory_node(inventory_raw, inventory_sha256)
    return wire({"schema": "bluey-jobs-c2c-actor-plan-v1", "run": cap["run"],
                 "sourceTree": cap["sourceTree"], "sourceInventorySha256": cap["sourceInventorySha256"],
                 "codeInventorySha256": cap["codeInventorySha256"],
                 "node": {"path": closure_root + "/" + NODE, "sha256": node["sha256"]}})


def stamp(row):
    return (row.st_dev, row.st_ino, row.st_uid, row.st_gid, row.st_mode, row.st_nlink,
            row.st_size, row.st_mtime_ns, row.st_ctime_ns)


def root_directory(path, stack, *, immutable_from=None):
    need(path.is_absolute() and path.resolve(strict=True) == path, "physical_directory")
    fd = os.open("/", DIR)
    stack.callback(os.close, fd)
    current = Path("/")
    for part in path.parts[1:]:
        current /= part
        fd = os.open(part, DIR, dir_fd=fd)
        stack.callback(os.close, fd)
        status = os.fstat(fd)
        need(stat.S_ISDIR(status.st_mode) and status.st_uid == 0 and not status.st_mode & 0o7022,
             "directory_authority")
        if immutable_from is not None and (current == immutable_from or immutable_from in current.parents):
            need(stat.S_IMODE(status.st_mode) == 0o555, "browser_directory_mode")
    need(os.path.samestat(os.fstat(fd), path.lstat()), "directory_identity")
    return fd


def pinned_file(parent, leaf, bound, mode, stack):
    fd = os.open(leaf, FILE, dir_fd=parent)
    stack.callback(os.close, fd)
    before = os.fstat(fd)
    need(stat.S_ISREG(before.st_mode) and stat.S_IMODE(before.st_mode) == mode and before.st_uid == 0
         and before.st_nlink == 1 and 0 < before.st_size <= bound, "file_metadata")

    def held():
        return stamp(before) == stamp(os.fstat(fd)) == stamp(os.stat(leaf, dir_fd=parent, follow_symlinks=False))

    need(held(), "file_changed")
    return fd, before.st_size, held


def read_small(parent, leaf, limit, mode, stack):
    fd, size, held = pinned_file(parent, leaf, limit, mode, stack)
    with os.fdopen(os.dup(fd), "rb") as stream:
        raw = stream.read(size + 1)
    need(len(raw) == size and held(), "file_changed")
    return raw, held


def assert_node(fd, size, held):
    need(size == NODE_SIZE, "node_size")
    header = os.pread(fd, 20, 0)
    need(len(header) == 20 and header[:7] == b"\x7fELF\x02\x01\x01"
         and header[16:18] in (b"\x02\x00", b"\x03\x00") and header[18:20] == b"\x3e\x00", "node_elf")
    digest, total = hashlib.sha256(), 0
    while total <= size:
        block = os.read(fd, min(1 << 20, size + 1 - total))
        if not block:
            break
        total += len(block)
        digest.update(block)
    need(total == size and digest.hexdigest() == NODE_SHA and held(), "node_bytes")


def load_stager(stack):
    parent = Path(__file__).resolve(strict=True).parent
    directory = root_directory(parent, stack)
    raw, _ = read_small(directory, STAGER, 64 << 10, 0o600, stack)
    need(sha(raw) == STAGER_SHA, "stager_pin")
    scope = types.ModuleType("_pinned_c2c_plan_stager")
    scope.__file__ = str(parent / STAGER)
    exec(compile(raw, scope.__file__, "exec"), scope.__dict__)
    return scope


def stage_code_and_plan(packet, capability_value, browser_inventory_sha256, *, admitted):
    """Root guest preparation only. Returns a plan pin, not launch authority.

    `admitted` must independently own the exact reservation, installed runtime
    closure and guest scope. An interrupted attempt retains all run leaves.
    """
    need(callable(admitted) and hex_pin(browser_inventory_sha256) and os.getuid() == os.geteuid() == 0,
         "root_admission")
    cap = capability(capability_value)
    closure = BROWSER / cap["run"]
    node = closure / NODE
    projection = {**cap, "browserInventorySha256": browser_inventory_sha256, "browserRoot": str(closure)}
    with ExitStack() as stack:
        stager = load_stager(stack)
        need(stager._root_linux() and admitted(dict(projection)) is True, "guest_admission")
        gate = stager._open_base(GATES, 0o755)
        stack.callback(os.close, gate)
        stager._absent(gate, cap["run"])
        browser_fd = root_directory(closure, stack, immutable_from=closure)
        node_parent = root_directory(node.parent, stack, immutable_from=closure)
        inventory, inventory_held = read_small(browser_fd, "inventory.json", INVENTORY_MAX, 0o444, stack)
        inventory_node(inventory, browser_inventory_sha256)
        node_fd, size, node_held = pinned_file(node_parent, node.name, NODE_SIZE, 0o555, stack)
        assert_node(node_fd, size, node_held)

        def current(binding):
            return (binding == cap and stager._root_linux() and admitted(dict(projection)) is True
                    and inventory_held() and node_held() and os.path.samestat(os.fstat(browser_fd), closure.lstat())
                    and os.path.samestat(os.fstat(node_parent), node.parent.lstat()) and node.resolve(strict=True) == node)

        result = stager.stage_code_packet(packet, cap["packetSha256"], cap, admitted=current)
        raw = make_plan(cap, result, str(closure), inventory, browser_inventory_sha256)
        stager._base_current(GATES, gate, 0o755)
        need(current(cap), "plan_admission_before")
        staged = stager._stage(gate, cap["run"], {"plan.json": raw}, 0o755)
        stack.callback(os.close, staged)
        need(current(cap), "plan_admission_after")
        stager._base_current(GATES, gate, 0o755)
        stager._verify_stage(gate, cap["run"], staged, {"plan.json": raw}, 0o755)
        return {**result, "reservationSha256": cap["reservationSha256"], "actorPlanSha256": sha(raw),
                "browserRoot": str(closure), "browserInventorySha256": browser_inventory_sha256, "launchAdmitted": False}
