"""Invocation-scoped teardown and independent zero census for C2C QA.

Requires a separately admitted, disposable Linux guest and launch custody
captured before reading the actor's first frame. Never starts a service.
"""
from contextlib import ExitStack, contextmanager
import hashlib
import os
from pathlib import Path
import platform
import re
import stat
import sys
import time


RUN = re.compile(r"[0-9a-f]{32}")
BOOT = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")
SHA = re.compile(r"[0-9a-f]{64}")
DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
FILE = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
FIELDS = ("Id", "InvocationID", "ControlGroup", "LoadState", "ActiveState", "SubState", "MainPID",
          "Job", "Result", "ExecMainCode", "ExecMainStatus")
ZERO = {"actorProcesses": 0, "namespaceProcesses": 0}
PINS = {
    "receipt-browser-manager-stop.py": "b41eeb361e5511885995b1318c08beec22fc7699d41f67a915e4a8012ba4a1e9",
    "receipt-browser-post-stop.py": "26a34e0cb65ad3d34d2d72b3cfb9ae58f280fcb99fa3ff968ae8bce5f3d46a48",
    "interview-prep-linux-closure.py": "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4",
    "local-qa-linux-c2c-manager-observer.py": "57d60374c15596bfe0594b707892f7a847c3c5a459da6564778525729c886530",
}


def need(ok, code):
    if not ok:
        raise ValueError("c2c_teardown_refused:" + code)


def full(rex, value):
    return type(value) is str and rex.fullmatch(value) is not None


def clock():
    return time.clock_gettime_ns(time.CLOCK_MONOTONIC)


def exact_claim(value):
    required = {"run", "unit", "invocation", "boot", "actorUid", "workloadNetns", "mainPid", "mainStartTime"}
    need(type(value) is dict and value.keys() == required and full(RUN, value["run"])
         and value["run"] != "0" * 32 and value["unit"] == f"bluey-jobs-qa-{value['run']}.service"
         and full(RUN, value["invocation"]) and value["invocation"] != "0" * 32
         and full(BOOT, value["boot"]) and value["boot"].replace("-", "") != "0" * 32
         and type(value["actorUid"]) is int and 0 < value["actorUid"] < 2**31
         and type(value["mainPid"]) is int and 1 < value["mainPid"] < 2**31
         and all(type(value[key]) is int and 0 < value[key] < 2**63 for key in ("workloadNetns", "mainStartTime")),
         "launch_claim")
    return dict(value)


def parse_show(raw):
    need(type(raw) is bytes and 0 < len(raw) <= 4096 and raw.endswith(b"\n"), "show_bound")
    rows = {}
    for line in raw[:-1].decode("ascii").split("\n"):
        key, sep, value = line.partition("=")
        need(sep and key in FIELDS and key not in rows and all(32 <= ord(c) < 127 for c in value), "show_field")
        rows[key] = value
    need(set(rows) == set(FIELDS), "show_fields")
    return rows


def identity(rows, claim):
    need(type(rows) is dict and rows.keys() == set(FIELDS)
         and rows["LoadState"] == "loaded" and rows["Id"] == claim["unit"]
         and rows["InvocationID"] == claim["invocation"]
         and rows["ControlGroup"] in ("", f"/system.slice/{claim['unit']}"), "unit_identity")


def phase(rows, claim):
    identity(rows, claim)
    state = rows["ActiveState"], rows["SubState"]
    if state == ("active", "running"):
        need(rows["MainPID"] == str(claim["mainPid"])
             and rows["ControlGroup"] == f"/system.slice/{claim['unit']}", "live_main")
        return "running"
    need(state in {("active", "exited"), ("failed", "failed"), ("inactive", "dead")}
         and rows["MainPID"] == "0", "unit_state")
    return "stopped" if state == ("inactive", "dead") else "terminal"


def sample_main(read, inspect, claim, bound):
    rows = read()
    identity(rows, claim)
    if (rows["ActiveState"], rows["SubState"]) != ("active", "running"):
        return rows
    phase(rows, claim)
    try:
        live = inspect(claim["mainPid"])
    except (FileNotFoundError, ProcessLookupError):
        # The exact launcher can exit after the manager sample. Missing
        # PID alone is not terminal or zero evidence.
        rows = read()
        need(phase(rows, claim) in ("terminal", "stopped"), "live_main_changed")
    else:
        need(live == bound, "live_main_changed")
    return rows


def stop_and_census(claim, *, normal, read, stop, populated, census, guard, pause):
    """Closed state machine. Its caller owns the one-shot capability."""
    claim = exact_claim(claim)
    need(type(normal) is bool, "teardown_mode")

    def sample():
        guard()
        rows = read()
        result = phase(rows, claim)
        guard()
        return rows, result

    rows, state = sample()
    need(rows["Job"] == "", "existing_job")
    launcher_failed = False
    if normal:
        # The actor's final stdout may precede the launcher's own cleanup.
        while state == "running":
            pause()
            rows, state = sample()
            need(rows["Job"] == "", "existing_job")
        while True:
            if (rows["Result"], rows["ExecMainCode"], rows["ExecMainStatus"]) != ("success", "1", "0"):
                launcher_failed = True
                break  # Still clean up the owned unit, but do not qualify the run.
            empty = populated()
            need(type(empty) is int and empty in (0, 1), "population_shape")
            latest = sample()
            if empty == 0 and latest == (rows, state):
                break
            pause()
            rows, state = latest
            need(state in ("terminal", "stopped") and rows["Job"] == "", "terminal_drift")
    if state != "stopped":
        guard()
        job = stop(claim["invocation"])
        need(full(re.compile(r"[1-9][0-9]{0,9}"), job) and int(job) < 2**32, "stop_reply")
        while True:
            guard()
            rows = read()
            identity(rows, claim)
            if rows["Job"] == "":
                break
            need(rows["Job"] == job, "stop_job_replaced")
            pause()
        need(phase(rows, claim) == "stopped", "not_stopped")
    terminal = dict(rows)
    for _ in range(3):
        guard()
        empty = populated()
        need(type(empty) is int and empty == 0 and sample() == (terminal, "stopped"), "cgroup_not_empty")
        counts = census()
        need(type(counts) is dict and counts.keys() == ZERO.keys()
             and all(type(counts[k]) is int and counts[k] == 0 for k in ZERO), "live_processes")
        guard()
    need(not launcher_failed, "launcher_failed_after_verified_cleanup")
    return {"unitStopped": True, **ZERO}


def bind_routes(plan, claim, teardown):
    """Produce the manager callbacks from one pre-frame custody record."""
    claim = exact_claim(claim)
    need(type(plan) is dict and plan.keys() == {"run", "unit", "uid", "gid", "runParent"}
         and plan["run"] == claim["run"] and plan["unit"] == claim["unit"]
         and type(plan["uid"]) is int and plan["uid"] == claim["actorUid"]
         and type(plan["gid"]) is int and 0 < plan["gid"] < 2**31
         and full(re.compile(r"/tmp/bluey-test-runs-[0-9a-f]{12}"), plan["runParent"])
         and callable(teardown), "route_plan")
    plan = dict(plan)
    used = False

    def consume(normal):
        nonlocal used
        need(not used, "teardown_consumed")
        used = True  # Never repeat an uncertain Stop, even through a different callback.
        return teardown(normal)

    def bound(waiting, observed, normal):
        expected = {"unitName": claim["unit"], "run": claim["run"], "invocationId": claim["invocation"],
                    "bootId": claim["boot"].replace("-", ""), "actorUid": claim["actorUid"],
                    "networkNamespace": f"net:[{claim['workloadNetns']}]", "mainPid": claim["mainPid"],
                    "mainStartTime": claim["mainStartTime"]}
        need(all(type(item) is dict and all(type(item.get(k)) is type(v) and item.get(k) == v
                                          for k, v in expected.items()) for item in (waiting, observed)),
             "route_claim")
        return consume(normal)

    def unbound(projection):
        need(type(projection) is dict and projection.keys() == plan.keys()
             and all(type(projection[k]) is type(v) and projection[k] == v for k, v in plan.items()), "route_plan_changed")
        return consume(False)

    return {"settle": lambda w, o: bound(w, o, True),
            "recover": lambda w, o: bound(w, o, False), "unbound_cleanup": unbound}


def pinned_module(name):
    directory = Path(__file__).resolve(strict=True).parent
    path = directory / name
    need(path.resolve(strict=True) == path, "module_path")
    for parent in (directory, *directory.parents):
        info = parent.lstat()
        need(stat.S_ISDIR(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o022, "module_parent")
    with os.fdopen(os.open(path, FILE), "rb") as stream:
        before = os.fstat(stream.fileno())
        need(stat.S_ISREG(before.st_mode) and before.st_uid == 0 and before.st_nlink == 1
             and not before.st_mode & 0o7022 and 0 < before.st_size <= 128 << 10, "module_metadata")
        body = stream.read(before.st_size + 1)
        check = lambda st: (st.st_dev, st.st_ino, st.st_uid, st.st_mode, st.st_nlink, st.st_size,
                            st.st_mtime_ns, st.st_ctime_ns)
        need(len(body) == before.st_size and hashlib.sha256(body).hexdigest() == PINS[name]
             and check(before) == check(os.fstat(stream.fileno())) == check(path.lstat()), "module_pin")
    scope = {"__name__": "_pinned_c2c_teardown", "__file__": str(path)}
    exec(compile(body, str(path), "exec"), scope)
    return scope


@contextmanager
def bind_teardown(plan, claim, namespace_fd, *, systemctl_sha256, library_path, library_sha256,
                  independently_admitted, bind_deadline_ns):
    """Hold the Linux unit, cgroup and namespace before permitting release.

    The independent admission predicate is a dedicated-guest/launch lease,
    not a test-success predicate. It must remain available for recovery of an
    admitted invocation after artifact staging or the actor has failed.
    """
    need(sys.platform == "linux" and platform.machine() == "x86_64"
         and os.getuid() == os.geteuid() == 0 and callable(independently_admitted)
         and type(bind_deadline_ns) is int and clock() < bind_deadline_ns <= clock() + 90_000_000_000,
         "linux_root_or_deadline")
    need(all(os.path.samestat(os.stat(f"/proc/self/ns/{kind}"), os.stat(f"/proc/1/ns/{kind}"))
             for kind in ("mnt", "pid", "user", "net", "cgroup"))
         and Path("/proc/self/uid_map").read_text().split() == ["0", "0", "4294967295"], "guest_namespaces")
    claim = exact_claim(claim)
    manager = pinned_module("receipt-browser-manager-stop.py")
    watcher = pinned_module("receipt-browser-post-stop.py")
    proc_observer = pinned_module("interview-prep-linux-closure.py")
    current = pinned_module("local-qa-linux-c2c-manager-observer.py")
    # Establish the root manager authority and invocation before any Stop
    # adapter is returned. Child stdout is not an input to this binding.
    systemctl_stamp = current["_pin_systemctl"](systemctl_sha256)
    systemctl = ["/usr/bin/systemctl", "--system", "--no-pager", "show", "--all",
                 *[f"--property={key}" for key in FIELDS], "--", claim["unit"]]
    sock = manager["SOCKET"]
    manager["controlled_path"](sock)
    sock_stat = sock.lstat()
    need(stat.S_ISSOCK(sock_stat.st_mode) and sock_stat.st_uid == 0, "system_bus")
    lib = manager["load_library"](Path(library_path), library_sha256)

    def read(deadline):
        return parse_show(proc_observer["bounded_manager_query"](systemctl, manager["remaining"](deadline, 3)))

    def lease(deadline, *, check_tool=True):
        manager["remaining"](deadline, 3)
        need(independently_admitted() is True and Path(proc_observer["BOOT"]).read_text().strip() == claim["boot"]
             and os.path.samestat(sock_stat, sock.lstat())
             and (not check_tool or current["_pin_systemctl"](systemctl_sha256) == systemctl_stamp), "guest_lease")

    lease(bind_deadline_ns)
    mountinfo = Path("/proc/self/mountinfo")
    with ExitStack() as stack:
        proc = stack.enter_context(watcher["opened"]("/proc", DIR))
        proc_id = watcher["proc_view"](mountinfo.read_text())
        need(proc_observer["fd_mount_id"](Path(f"/proc/self/fdinfo/{proc}").read_text()) == proc_id, "full_proc_mount")
        net = os.dup(namespace_fd)
        stack.callback(os.close, net)
        network = watcher["identity"](os.fstat(net))
        main_proc = current["_proc"](claim["mainPid"])
        need(network[1] == claim["workloadNetns"]
             and network == watcher["identity"](os.stat(f"/proc/{claim['mainPid']}/ns/net"))
             and network != watcher["identity"](os.stat("/proc/self/ns/net"))
             and main_proc["start"] == claim["mainStartTime"] and main_proc["uid"] == claim["actorUid"]
             and main_proc["cgroup"] == f"0::/system.slice/{claim['unit']}\n", "launch_custody")
        mount_id = proc_observer["mount_id"](mountinfo.read_text())
        events, events_id = proc_observer["held_events"](claim["unit"], mount_id, stack)
        before = read(bind_deadline_ns)
        need(phase(before, claim) == "running" and before["Job"] == ""
             and proc_observer["sample_events"](events, events_id) == 1, "bind_unit_not_live")
        reference, stopper = stack.enter_context(manager["bus_manager"](lib))
        # Even a lost reference reply may have taken effect. Bus closure is
        # the final lease fence; do not issue a Stop after a failed rebind.
        stack.callback(lambda: reference("UnrefUnit", claim["unit"], clock() + 2_000_000_000))
        reference("RefUnit", claim["unit"], min(bind_deadline_ns, clock() + 2_000_000_000))
        lease(bind_deadline_ns)
        need(read(bind_deadline_ns) == before and current["_proc"](claim["mainPid"]) == main_proc
             and proc_observer["sample_events"](events, events_id) == 1, "bind_unit_changed")
        active = True

        def teardown(normal):
            need(active, "scope_closed")
            deadline = clock() + 30_000_000_000

            def guard():
                lease(deadline)
                raw_mounts = mountinfo.read_text()
                need(watcher["proc_view"](raw_mounts) == proc_id and proc_observer["mount_id"](raw_mounts) == mount_id
                     and os.path.samestat(os.fstat(proc), Path("/proc").stat())
                     and watcher["identity"](os.fstat(net)) == network
                     and proc_observer["fd_mount_id"](Path(f"/proc/self/fdinfo/{events}").read_text()) == mount_id,
                     "held_view_changed")

            def sample():
                return sample_main(lambda: read(deadline), current["_proc"], claim, main_proc)

            def population():
                guard()
                return proc_observer["sample_events"](events, events_id)

            def stop(invocation):
                guard()
                return stopper(invocation, deadline)

            return stop_and_census(
                claim, normal=normal, read=sample, stop=stop, populated=population,
                census=lambda: watcher["scan_proc"](proc, claim["actorUid"], network,
                                                     lambda: lease(deadline, check_tool=False)),
                guard=guard, pause=lambda: time.sleep(manager["remaining"](deadline, 0.05)),
            )

        try:
            yield bind_routes(plan, claim, teardown)
        finally:
            active = False
