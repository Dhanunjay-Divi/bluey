"""Publish one release for the synthetic Linux receipt job; never start a unit."""
from contextlib import ExitStack
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import time

BARRIER = "8a043d7b14ea86cab295b58215b7773aafca7bd3fd4bbf3fa5aee6a8a454dadf"
OBSERVER = "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4"
READY = "release.json"
PENDING = ".release.pending"
FLAGS = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
RUN = re.compile(r"[0-9a-f]{32}")
ROOT = re.compile(r"/tmp/bluey-test-runs-[a-f0-9]{12}/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}")


def check(ok, code):
    if not ok:
        raise RuntimeError(code)


def helper(filename, expected):
    path = Path(__file__).resolve().with_name(filename)
    with path.open("rb") as file:
        body = file.read(32 * 1024)
    check(hashlib.sha256(body).hexdigest() == expected, "release_helper_pin")
    scope = {"__name__": "verified_release_helper", "__file__": str(path)}
    exec(compile(body, str(path), "exec"), scope)
    return scope


def absent(fd, name):
    try:
        os.stat(name, dir_fd=fd, follow_symlinks=False)
    except FileNotFoundError:
        return True
    return False


def rename_noreplace(fd):
    libc = ctypes.CDLL(None, use_errno=True)
    # The Darwin primitive is used only for disposable filesystem tests;
    # write_after_bound refuses every non-Linux host before using it.
    if sys.platform == "linux":
        fn, flag = libc.renameat2, 1  # RENAME_NOREPLACE
    elif sys.platform == "darwin":
        fn, flag = libc.renameatx_np, 4  # RENAME_EXCL
    else:
        raise RuntimeError("release_atomic_rename_unsupported")
    fn.argtypes = (ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint)
    fn.restype = ctypes.c_int
    if fn(fd, os.fsencode(PENDING), fd, os.fsencode(READY), flag) != 0:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code))


def record_identity(item):
    return (item.st_dev, item.st_ino, item.st_uid, item.st_mode, item.st_nlink,
            item.st_size, item.st_mtime_ns, item.st_ctime_ns)


def publish_at(gate, body, recheck, *, owner, deadline_ns=None, clock=None):
    """Use an owned directory and an exclusive, retained single-writer claim.

    The caller must hold the already validated gate FD. Root is trusted;
    the workload has no write grant. Other supported callers all share
    .release.claim, which is intentionally not removed on error.
    """
    info = os.fstat(gate)
    clock = clock or (lambda: time.clock_gettime_ns(time.CLOCK_MONOTONIC))
    check(stat.S_ISDIR(info.st_mode) and info.st_uid == owner
          and stat.S_IMODE(info.st_mode) == 0o755, "release_writer_directory")
    check(absent(gate, READY), "release_already_published")
    recheck()
    claim = os.open(".release.claim", FLAGS, mode=0o600, dir_fd=gate)
    try:
        os.fsync(claim)
    finally:
        os.close(claim)
    os.fsync(gate)
    check(absent(gate, READY), "release_already_published")
    flags = (FLAGS & ~os.O_WRONLY) | os.O_RDWR
    with os.fdopen(os.open(PENDING, flags, mode=0o600, dir_fd=gate), "w+b") as file:
        check(file.write(body) == len(body), "release_short_write")
        file.flush()
        os.fchmod(file.fileno(), 0o644)
        os.fsync(file.fileno())
        pending = os.fstat(file.fileno())
        # Signaling too late or while the observer is gone is a refusal.
        recheck()
        check(absent(gate, READY), "release_already_published")
        check(record_identity(pending) == record_identity(os.fstat(file.fileno()))
              == record_identity(os.stat(PENDING, dir_fd=gate, follow_symlinks=False)),
              "release_pending_replaced")
        before_rename = clock()
        check(deadline_ns is None or before_rename < deadline_ns, "release_publication_deadline")
        rename_noreplace(gate)
        after_rename = clock()
        os.fsync(gate)
        check(deadline_ns is None or after_rename < deadline_ns, "release_publication_deadline")
        named = os.stat(READY, dir_fd=gate, follow_symlinks=False)
        check(os.path.samestat(pending, named) and named.st_nlink == 1
              and named.st_uid == owner and stat.S_IMODE(named.st_mode) == 0o644
              and named.st_size == len(body), "release_publication_identity")
        file.seek(0)
        check(file.read(len(body) + 1) == body, "release_publication_bytes")
        check(record_identity(named) == record_identity(os.fstat(file.fileno()))
              == record_identity(os.stat(READY, dir_fd=gate, follow_symlinks=False)),
              "release_publication_changed")
    return {"releaseSha256": hashlib.sha256(body).hexdigest(), "device": named.st_dev,
            "inode": named.st_ino, "bytes": named.st_size,
            "publicationBeforeNs": before_rename, "publicationAfterNs": after_rename}


def parse_bound(raw):
    check(isinstance(raw, bytes) and 0 < len(raw) <= 4096, "release_observer_frame")
    value = json.loads(raw)
    # The unchanged Python observer emits this precise JSON serialization.
    check(isinstance(value, dict) and raw == (json.dumps(value) + "\n").encode("ascii"),
          "release_observer_serialization")
    return value


def network_snapshot(pid):
    net = Path(f"/proc/{pid}/net")
    lines = (net / "dev").read_text(encoding="ascii").splitlines()
    check(len(lines) >= 3 and "Inter-|" in lines[0], "release_interfaces_shape")
    interfaces = sorted(line.partition(":")[0].strip() for line in lines[2:]
                        if ":" in line)
    check(len(interfaces) == len(lines) - 2 and interfaces == ["lo"], "release_interfaces")
    routes = (net / "route").read_text(encoding="ascii").splitlines()
    check(routes and routes[0].split()[:3] == ["Iface", "Destination", "Gateway"],
          "release_ipv4_route_shape")
    for line in routes[1:]:
        parts = line.split()
        check(len(parts) == 11 and parts[0] == "lo" and parts[1] != "00000000",
              "release_ipv4_route")
    for line in (net / "ipv6_route").read_text(encoding="ascii").splitlines():
        parts = line.split()
        check(len(parts) == 10 and parts[-1] == "lo" and re.fullmatch(r"[0-9a-f]{8}", parts[-2]),
              "release_ipv6_route_shape")
        # Linux reports a loopback reject entry for an absent default.
        is_default = parts[0] == "0" * 32 and parts[1] == "00"
        check(not is_default or int(parts[-2], 16) & 0x200, "release_ipv6_default")
    return interfaces


def write_after_bound(waiting, supervised, bound_line, observer_is_live, *,
                      deadline_ns, independently_admitted):
    """Root-only hook for an already authorized local supervisor.

    The caller owns systemd dispatch and the observer process. It must
    independently verify file/tree inputs and pass the live predicate for
    the exact pinned observer child; this hook never treats stdin or
    a unit-supplied frame as the source of the observer witness.
    """
    check(sys.platform == "linux" and os.getuid() == os.geteuid() == 0, "release_writer_linux_root")
    check(callable(observer_is_live) and callable(independently_admitted) and
          independently_admitted() is True, "release_writer_unadmitted")
    run = waiting.get("run")
    check(isinstance(run, str) and RUN.fullmatch(run), "release_writer_run")
    barrier = helper("receipt-browser-launch-barrier.py", BARRIER)
    closure = helper("interview-prep-linux-closure.py", OBSERVER)
    bound = parse_bound(bound_line)
    now = barrier["kernel_monotonic_ns"]
    started = now()
    check(type(deadline_ns) is int and started < deadline_ns
          <= waiting["waitingNs"] + 90_000_000_000, "release_writer_deadline")
    barrier["render_release"](waiting, supervised, bound, started)
    unit, pid = waiting["unit"], waiting["actorPid"]
    check(unit == f"bluey-jobs-receipt-{run}.service" and type(pid) is int and pid > 1
          and isinstance(waiting["root"], str) and ROOT.fullmatch(waiting["root"]),
          "release_writer_scope")
    argv = ["/usr/bin/systemctl", "--system", "--no-pager", "show", "--all", unit,
            *[f"--property={field}" for field in closure["FIELDS"]]]
    proc = Path(f"/proc/{pid}")
    with ExitStack() as stack:
        gate = barrier["protected_run_gate"](run, stack)
        gate_info = os.fstat(gate)
        group_mount = closure["mount_id"](Path("/proc/self/mountinfo").read_text())
        events, event_identity = closure["held_events"](unit, group_mount, stack)
        check((group_mount, event_identity[1]) == (bound["mountId"], bound["eventsInode"]),
              "release_observer_descriptor")
        proc_info = proc.stat()

        def current():
            check(now() < deadline_ns and observer_is_live() is True
                  and independently_admitted() is True, "release_supervisor_no_longer_live")
            check(Path(closure["BOOT"]).read_text().strip() == waiting["boot"],
                  "release_writer_boot")
            parent = barrier["GATE_PARENT"] / run
            check(os.path.samestat(gate_info, os.stat(parent, follow_symlinks=False)),
                  "release_gate_replaced")
            state = closure["unit_properties"](closure["bounded_manager_query"](argv).decode("ascii"))
            closure["check_manager"](state, unit, waiting["invocation"], True)
            check(os.path.samestat(proc_info, proc.stat()) and os.getpgid(pid) == waiting["actorPgid"]
                  and (proc / "cgroup").read_text() == f'0::{state["ControlGroup"]}\n',
                  "release_actor_process")
            uids = [row.split()[1:] for row in (proc / "status").read_text().splitlines()
                    if row.startswith("Uid:")]
            check(type(waiting["actorUid"]) is int and waiting["actorUid"] > 0
                  and uids == [[str(waiting["actorUid"])] * 4], "release_actor_uid")
            actor_root = proc / "root" / waiting["root"].lstrip("/")
            for path, device_key, inode_key in (
                (actor_root, "rootDevice", "rootInode"),
                (actor_root / "receipt-prep-browser", "capsuleDevice", "capsuleInode"),
            ):
                info = path.stat(follow_symlinks=False)
                check(stat.S_ISDIR(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o700
                      and info.st_uid == waiting["actorUid"]
                      and info.st_dev == waiting[device_key] and info.st_ino == waiting[inode_key],
                      "release_actor_directory")
            check((proc / "ns/net").stat().st_ino == waiting["workloadNetns"]
                  and Path("/proc/self/ns/net").stat().st_ino == supervised["supervisorNetns"]
                  and network_snapshot(pid) == supervised["interfaces"], "release_network_changed")
            check(closure["sample_events"](events, event_identity) == 1
                  and now() < deadline_ns and observer_is_live() is True, "release_observer_lost")

        current()
        issued = now()
        body = barrier["render_release"](waiting, supervised, bound, issued)
        receipt = publish_at(gate, body, current, owner=0, deadline_ns=deadline_ns, clock=now)
    # The original observer still owns its FD until terminal zero. A release
    # receipt cannot substitute for that evidence or a passed browser run.
    return {**receipt, "issuedNs": issued, "runtimeAdmitted": False}
