"""Independent post-stop census for one isolated Linux Jobs fixture."""
from contextlib import ExitStack, contextmanager
import hashlib
import os
from pathlib import Path
import re
import stat
import sys
import time

OBSERVER_SHA = "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4"
DECIMAL = re.compile(r"[1-9][0-9]{0,9}")
HEX32 = re.compile(r"[0-9a-f]{32}")
BOOT = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")
DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW
FILE = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW
GONE = (FileNotFoundError, ProcessLookupError)


def require(ok, reason):
    if not ok:
        raise RuntimeError(reason)


@contextmanager
def opened(path, flags, parent=None):
    fd = os.open(path, flags, dir_fd=parent)
    try:
        yield fd
    finally:
        os.close(fd)


def identity(info):
    return info.st_dev, info.st_ino


def read_at(fd, name):
    with opened(name, FILE, fd) as source:
        with os.fdopen(os.dup(source), "rb") as file:
            raw = file.read(64 * 1024 + 1)
    require(0 < len(raw) <= 64 * 1024 and raw.endswith(b"\n"), "census_proc_read")
    return raw


def status(raw, tid):
    values = {}
    for line in raw.split(b"\n"):
        key, _, value = line.partition(b":")
        if key in {b"Uid", b"Kthread", b"State", b"Pid"}:
            require(key not in values, "census_status_duplicate")
            values[key] = value.split()
    require(set(values) == {b"Uid", b"Kthread", b"State", b"Pid"}
            and values[b"Pid"] == [tid.encode("ascii")]
            and len(values[b"Uid"]) == 4
            and all(re.fullmatch(rb"0|[1-9][0-9]{0,9}", uid) and int(uid) < 2**32 - 1
                    for uid in values[b"Uid"])
            and values[b"Kthread"] in ([b"0"], [b"1"])
            and values[b"State"] and re.fullmatch(rb"[A-Z]", values[b"State"][0]),
            "census_status_shape")
    return tuple(map(int, values[b"Uid"])), values[b"Kthread"] == [b"1"], values[b"State"][0]


def proc_view(raw):
    matched = []
    for line in raw.splitlines():
        left, sep, right = line.partition(" - ")
        parts, fs = left.split(), right.split()
        if sep and len(parts) >= 6 and parts[4] == "/proc":
            require(parts[3] == "/" and len(fs) == 3 and fs[0] == "proc", "census_proc_mount")
            options = parts[5].split(",") + fs[2].split(",")
            require(not any(o.startswith("subset=") or
                            (o.startswith("hidepid=") and o not in ("hidepid=0", "hidepid=off"))
                            for o in options), "census_proc_visibility")
            require(DECIMAL.fullmatch(parts[0]), "census_proc_mount")
            matched.append(int(parts[0]))
    require(len(matched) == 1, "census_proc_mount")
    return matched[0]


def task_names(fd, check, cap):
    # New directory stream: never reuse the offset from an earlier pass.
    with opened(".", DIR, fd) as scan:
        with os.scandir(scan) as entries:
            for n, entry in enumerate(entries, 1):
                check()
                require(n <= cap, "census_entry_limit")
                if DECIMAL.fullmatch(entry.name):
                    yield entry.name


def task_observation(fd, tid):
    values = status(read_at(fd, "status"), tid)
    uids, kernel, state = values
    if kernel or state in (b"Z", b"X"):
        # The actor census still counts zombies. These tasks can legitimately
        # lack an ns/net entry. Do not extend that exception to living users.
        return uids, None
    try:
        with opened("ns", DIR, fd) as ns:
            net = identity(os.stat("net", dir_fd=ns))
    except GONE:
        try:
            after = status(read_at(fd, "status"), tid)
        except GONE:
            return uids, None  # The same held descriptor proves departure.
        require(after[0] == uids and after[2] in (b"Z", b"X"), "census_netns_unreadable")
        return uids, None
    try:
        new_uids, new_kernel, _ = status(read_at(fd, "status"), tid)
    except GONE:
        return uids, net
    require(new_uids == uids and not new_kernel, "census_task_changed")
    return uids, net


def scan_proc(proc, actor_uid, network, check, cap=32768):
    actor, namespace, seen = set(), set(), 0
    for pid in task_names(proc, check, cap):
        try:
            leader = os.open(pid, DIR, dir_fd=proc)
        except GONE:
            continue
        try:
            group_seen = 0
            try:
                with opened("task", DIR, leader) as tasks:
                    for tid in task_names(tasks, check, cap):
                        seen += 1
                        require(seen <= cap, "census_task_limit")
                        try:
                            with opened(tid, DIR, tasks) as thread:
                                uids, net = task_observation(thread, tid)
                        except GONE:
                            continue
                        group_seen += 1
                        if actor_uid in uids:
                            actor.add(pid)
                        if net == network:
                            namespace.add(pid)
            except GONE:
                group_seen = 0
            if not group_seen:
                # A dead main thread can leave other threads alive while
                # /proc/PID/task is unavailable. Skip only a vanished leader.
                try:
                    read_at(leader, "status")
                except GONE:
                    continue
                raise RuntimeError("census_task_group_unobservable")
        finally:
            os.close(leader)
    require(seen > 0, "census_empty_proc")
    return {"actorProcesses": len(actor), "namespaceProcesses": len(namespace)}


def post_stop(manager, census, check):
    check()
    initial = manager()
    for _ in range(2):
        counts = census()
        require(counts == {"actorProcesses": 0, "namespaceProcesses": 0},
                "census_remaining_processes")
        check()
        require(manager() == initial, "census_unit_changed")
    check()
    return {"unitStopped": True, **counts}


def stopped_state(rows, unit, invocation):
    require(rows["Id"] == unit and rows["LoadState"] == "loaded"
            and rows["InvocationID"] == invocation
            and rows["ControlGroup"] in ("", f"/system.slice/{unit}")
            and (rows["ActiveState"], rows["SubState"], rows["MainPID"]) == ("inactive", "dead", "0"),
            "census_unit_not_stopped")
    return rows


def observer_helper():
    path = Path(__file__).resolve().with_name("interview-prep-linux-closure.py")
    with path.open("rb") as file:
        source = file.read(32 * 1024)
        require(file.read(1) == b"" and hashlib.sha256(source).hexdigest() == OBSERVER_SHA,
                "census_observer_pin")
    scope = {"__name__": "verified_census_helper", "__file__": str(path)}
    exec(compile(source, str(path), "exec"), scope)
    return scope


@contextmanager
def bind_post_stop(waiting, *, independently_admitted, deadline_ns):
    """Bind before release. Yield a readback hook for after the exact unit stop.

    Requires the independently admitted, dedicated, non-delegated Linux guest
    and actor UID. The caller owns release/zero and the exact systemd stop.
    No signal, stop, namespace entry, or general-purpose host scan is exposed.
    """
    require(sys.platform == "linux" and os.getuid() == os.geteuid() == 0, "census_linux_root")
    unit, invocation, boot, nonce = [waiting[k] for k in ("unit", "invocation", "boot", "run")]
    require(type(nonce) is str and HEX32.fullmatch(nonce)
            and type(invocation) is str and HEX32.fullmatch(invocation) and invocation != "0" * 32
            and type(boot) is str and BOOT.fullmatch(boot)
            and unit == f"bluey-jobs-receipt-{nonce}.service", "census_scope")
    pid, actor_uid = waiting["actorPid"], waiting["actorUid"]
    require(type(pid) is int and 1 < pid < 2**31
            and type(actor_uid) is int and 0 < actor_uid < 2**32 - 1, "census_actor_scope")
    clock = lambda: time.clock_gettime_ns(time.CLOCK_MONOTONIC)
    now = clock()
    require(type(deadline_ns) is int and now < deadline_ns <= now + 1_800_000_000_000
            and callable(independently_admitted), "census_deadline")
    helper = observer_helper()
    boot_file = Path(helper["BOOT"])
    mountinfo = Path("/proc/self/mountinfo")
    argv = ["/usr/bin/systemctl", "--system", "--no-pager", "show", "--all",
            *[f"--property={k}" for k in helper["FIELDS"]], "--", unit]

    def manager():
        return helper["unit_properties"](helper["bounded_manager_query"](argv).decode("ascii"))

    def lease():
        require(clock() < deadline_ns and independently_admitted() is True
                and boot_file.read_text().strip() == boot, "census_lease")

    lease()
    with ExitStack() as stack:
        proc = stack.enter_context(opened("/proc", DIR))
        mount = proc_view(mountinfo.read_text())
        require(helper["fd_mount_id"](Path(f"/proc/self/fdinfo/{proc}").read_text()) == mount,
                "census_proc_descriptor")
        for kind in ("pid", "user"):
            require(identity(os.stat(f"/proc/self/ns/{kind}")) == identity(os.stat(f"/proc/1/ns/{kind}")),
                    "census_guest_namespace")
        require(Path("/proc/self/uid_map").read_text().split() == ["0", "0", "4294967295"],
                "census_guest_uid_map")
        with opened(str(pid), DIR, proc) as actor, opened("ns", DIR, actor) as ns:
            uids, kernel, _ = status(read_at(actor, "status"), str(pid))
            require(uids == (actor_uid,) * 4 and not kernel
                    and read_at(actor, "cgroup") == f"0::/system.slice/{unit}\n".encode("ascii"),
                    "census_actor_identity")
            network_fd = stack.enter_context(opened("net", os.O_RDONLY | os.O_CLOEXEC, ns))
            network = identity(os.fstat(network_fd))
            require(network == identity(os.stat("net", dir_fd=ns))
                    and network[1] == waiting["workloadNetns"]
                    and network != identity(os.stat("/proc/self/ns/net")), "census_held_namespace")
            helper["check_manager"](manager(), unit, invocation, True)

        def check():
            lease()
            require(proc_view(mountinfo.read_text()) == mount
                    and identity(os.fstat(proc)) == identity(os.stat("/proc"))
                    and identity(os.fstat(network_fd)) == network, "census_view_changed")

        yield lambda: post_stop(lambda: stopped_state(manager(), unit, invocation),
                                lambda: scan_proc(proc, actor_uid, network, lease), check)
