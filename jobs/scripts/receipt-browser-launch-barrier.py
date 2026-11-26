"""Hold the synthetic browser workload until a separate supervisor releases it."""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import secrets
import stat
import sys
import time

GATE_PARENT = Path("/run/bluey-jobs-receipt-gates")
INTAKE = "07426bad5f55f7fe3ea99ccab025571ffabffe310e204c7cf64ec4c20dd682dc"
CAPSULE = "194d2393e5b922807110f98009a9c6815cffd957f632dda2605cb616cdce4f30"
HEX32 = re.compile(r"[a-f0-9]{32}")
HEX64 = re.compile(r"[a-f0-9]{64}")
BOOT = re.compile(r"[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}")
DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
FILE = os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC
GATE_FILE = "release.json"
MAX_FRAME = 4096
WAIT_FIELDS = frozenset({
    "run", "unit", "boot", "invocation", "root", "rootDevice", "rootInode",
    "capsuleDevice", "capsuleInode", "actorPid", "actorPgid", "actorUid", "challenge",
    "waitingNs", "workloadNetns", "capsuleInventory", "journeyClosure",
})


class Refused(RuntimeError):
    pass


def kernel_monotonic_ns():
    return time.clock_gettime_ns(time.CLOCK_MONOTONIC)


def require(ok, code):
    if not ok:
        raise Refused(code)


def wire(value):
    return (json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def identity(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_mode, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def positive(value):
    return type(value) is int and 0 < value < 2**63


def validate_release(body, waiting, now_ns):
    require(waiting.keys() == WAIT_FIELDS, "waiting_fields")
    require(isinstance(body, bytes) and 0 < len(body) <= MAX_FRAME, "release_size")
    result = json.loads(body)
    authority = {"schema", "issuedNs", "observerMountId", "observerEventsInode",
                 "supervisorNetns", "interfaces", "defaultRoutes"}
    require(isinstance(result, dict) and set(result) == (waiting.keys() | authority)
            and body == wire(result), "release_canonical")
    require(result["schema"] == "bluey-jobs-browser-release-v1"
            and all(type(result[key]) is type(value) and result[key] == value
                    for key, value in waiting.items()), "release_binding")
    require(all(positive(result[key]) for key in (
        "issuedNs", "observerMountId", "observerEventsInode", "supervisorNetns"
    )), "release_observer")
    require(result["supervisorNetns"] != waiting["workloadNetns"] and result["interfaces"] == ["lo"]
            and type(result["defaultRoutes"]) is int and result["defaultRoutes"] == 0,
            "release_network")
    require(waiting["waitingNs"] <= result["issuedNs"] <= now_ns, "release_time")
    return result


def render_release(waiting, supervised, bound, now_ns):
    """Build the exact record only after an independently bound observer event."""
    observed_fields = {"supervisorNetns", "interfaces", "defaultRoutes"}
    independent_fields = WAIT_FIELDS - {"challenge", "waitingNs"}
    require(waiting.keys() == WAIT_FIELDS and supervised.keys() == independent_fields | observed_fields
            and all(type(supervised[key]) is type(waiting[key])
                    and supervised[key] == waiting[key] for key in independent_fields),
            "supervisor_binding")
    require(bound.keys() == {"event", "run", "boot", "invocation", "mountId", "eventsInode"}
            and bound["event"] == "jobs_browser_closure_bound"
            and all(bound[key] == waiting[key] for key in ("run", "boot", "invocation")),
            "supervisor_observer_not_bound")
    require(HEX32.fullmatch(waiting["challenge"]) and positive(waiting["waitingNs"]),
            "waiting_challenge")
    record = {
        **waiting, "schema": "bluey-jobs-browser-release-v1", "issuedNs": now_ns,
        "observerMountId": bound["mountId"], "observerEventsInode": bound["eventsInode"],
        **{key: supervised[key] for key in observed_fields},
    }
    body = wire(record)
    validate_release(body, waiting, now_ns)
    return body


def read_release(directory, uid=0):
    try:
        fd = os.open(GATE_FILE, FILE, dir_fd=directory)
    except FileNotFoundError:
        return None
    with os.fdopen(fd, "rb") as stream:
        before = os.fstat(fd)
        require(stat.S_ISREG(before.st_mode) and before.st_uid == uid
                and stat.S_IMODE(before.st_mode) == 0o644 and before.st_nlink == 1
                and 0 < before.st_size <= MAX_FRAME, "release_file_identity")
        body = stream.read(MAX_FRAME + 1)
        require(len(body) == before.st_size and
                identity(before) == identity(os.fstat(fd))
                == identity(os.stat(GATE_FILE, dir_fd=directory, follow_symlinks=False)),
                "release_file_changed")
    return body, identity(before)


def wait_for_release(reader, waiting, limit_seconds, *, clock=kernel_monotonic_ns, pause=time.sleep):
    require(type(limit_seconds) is int and 1 <= limit_seconds <= 90, "release_timeout_bound")
    deadline = waiting["waitingNs"] + limit_seconds * 1_000_000_000
    while clock() < deadline:
        record = reader()
        now = clock()
        require(now < deadline, "release_timeout")
        if record is not None:
            return record, validate_release(record[0], waiting, now)
        pause(0.05)
    raise Refused("release_timeout")


def protected_run_gate(run, stack):
    require(HEX32.fullmatch(run), "release_run_name")
    current = os.open("/", DIR)
    stack.callback(os.close, current)
    for name in (*GATE_PARENT.parts[1:], run):
        current = os.open(name, DIR, dir_fd=current)
        stack.callback(os.close, current)
        st = os.fstat(current)
        require(st.st_uid == 0 and stat.S_IMODE(st.st_mode) == 0o755, "release_parent_owner")
    return current


def verified_marker(root):
    helper = Path(__file__).resolve().with_name("stage-browser-receipt-intake.py")
    with helper.open("rb") as stream:
        body = stream.read(32 * 1024)
    require(hashlib.sha256(body).hexdigest() == INTAKE, "release_marker_helper")
    scope = {"__name__": "verified_barrier_intake"}
    exec(compile(body, str(helper), "exec"), scope)
    scope["physical_directory"](root)
    marker = scope["marker_for"](root, os.getpgrp())
    return lambda: scope["marker_for"](root, os.getpgrp()) == marker


def owned_dir(fd):
    metadata = os.fstat(fd)
    require(stat.S_ISDIR(metadata.st_mode) and metadata.st_uid == os.getuid()
            and stat.S_IMODE(metadata.st_mode) == 0o700, "workload_directory")
    return metadata


def ensure_home(root_fd):
    # This is the HOME Rust supplies to its Node children, not the
    # PostgreSQL wrapper's separate compiler HOME.
    os.mkdir("home", mode=0o700, dir_fd=root_fd)
    fd = os.open("home", DIR, dir_fd=root_fd)
    try:
        return owned_dir(fd)
    finally:
        os.close(fd)


def linux_state(unit):
    boot = Path("/proc/sys/kernel/random/boot_id").read_text().strip()
    invocation = os.environ.get("INVOCATION_ID", "")
    expected = f"0::/system.slice/{unit}\n"
    require(BOOT.fullmatch(boot) and HEX32.fullmatch(invocation)
            and invocation != "0" * 32
            and Path("/proc/self/cgroup").read_text() == expected, "release_linux_state")
    return boot, invocation, os.stat("/proc/self/ns/net").st_ino


def hold(run, closure_digest, *, emit, stack, limit_seconds=90):
    """Library entry: return the exact release and a live attestation callback."""
    require(sys.platform == "linux" and platform.machine() == "x86_64"
            and os.getuid() == os.geteuid() and os.getuid() != 0, "release_linux_actor")
    require(HEX32.fullmatch(run) and HEX64.fullmatch(closure_digest), "release_input")
    unit = f"bluey-jobs-receipt-{run}.service"
    boot, invocation, namespace = linux_state(unit)
    gate = protected_run_gate(run, stack)
    gate_info = os.fstat(gate)
    require(read_release(gate) is None, "release_already_exists")
    root = Path(os.environ.get("BLUEY_TEST_RUN_ROOT", ""))
    marker_same = verified_marker(root)
    fd = os.open(root, DIR)
    stack.callback(os.close, fd)
    root_info = owned_dir(fd)
    leaf = os.open("receipt-prep-browser", DIR, dir_fd=fd)
    stack.callback(os.close, leaf)
    leaf_info = owned_dir(leaf)
    home_info = ensure_home(fd)
    waiting = {
        "run": run, "unit": unit, "boot": boot, "invocation": invocation, "root": str(root),
        "rootDevice": root_info.st_dev, "rootInode": root_info.st_ino,
        "capsuleDevice": leaf_info.st_dev, "capsuleInode": leaf_info.st_ino,
        "actorPid": os.getpid(), "actorPgid": os.getpgrp(), "actorUid": os.getuid(),
        "challenge": secrets.token_hex(16), "waitingNs": kernel_monotonic_ns(),
        "workloadNetns": namespace, "capsuleInventory": CAPSULE, "journeyClosure": closure_digest,
    }

    def current():
        require(linux_state(unit) == (boot, invocation, namespace) and marker_same()
                and os.path.samestat(gate_info, os.stat(GATE_PARENT / run, follow_symlinks=False))
                and os.path.samestat(root_info, os.stat(root, follow_symlinks=False))
                and os.path.samestat(home_info, os.stat("home", dir_fd=fd, follow_symlinks=False))
                and os.path.samestat(leaf_info, os.stat(
                    "receipt-prep-browser", dir_fd=fd, follow_symlinks=False)),
                "release_live_identity")
        owned_dir(fd)
        owned_dir(leaf)

    current()
    emit({"event": "jobs_browser_waiting_for_release", **waiting})
    record, release = wait_for_release(lambda: read_release(gate), waiting, limit_seconds)
    current()

    def still_admitted(inventory, observed_leaf):
        current()
        return (inventory == CAPSULE and os.path.samestat(leaf_info, observed_leaf)
                and kernel_monotonic_ns() < waiting["waitingNs"] + limit_seconds * 1_000_000_000
                and read_release(gate) == record)

    return release, still_admitted
