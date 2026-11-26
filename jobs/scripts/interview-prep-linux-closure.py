"""Read-only closure witness for the disposable Jobs browser-receipt unit."""
import contextlib
import json
import os
from pathlib import Path
import re
import selectors
import stat
import subprocess
import sys
import time

ROOT = "/sys/fs/cgroup"
BOOT = "/proc/sys/kernel/random/boot_id"
RUN = re.compile(r"[0-9a-f]{32}")
SECONDS = re.compile(r"[1-9][0-9]{0,3}")
UUID = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")
FIELDS = ("Id", "InvocationID", "ControlGroup", "LoadState", "ActiveState", "SubState", "MainPID")
TERMINAL = {("active", "exited"), ("failed", "failed"), ("inactive", "dead")}


class Refused(RuntimeError):
    pass


def require(condition, code):
    if not condition:
        raise Refused(code)


def parse_events(data):
    require(isinstance(data, bytes) and 0 < len(data) <= 256, "events_size")
    require(data.endswith(b"\n"), "events_incomplete")
    pairs = {}
    for row in data[:-1].split(b"\n"):
        m = re.fullmatch(rb"(populated|frozen) ([01])", row)
        require(m and m[1] not in pairs, "events_malformed")
        pairs[m[1]] = int(m[2])
    require(set(pairs) == {b"populated", b"frozen"}, "events_missing_fields")
    return pairs[b"populated"]


def mount_id(data):
    """Bind the FD to the exact cgroup2 filesystem, not the path spelling."""
    matches = []
    for line in data.splitlines():
        prefix, sep, suffix = line.partition(" - ")
        if not sep:
            continue
        parts, after = prefix.split(), suffix.split()
        if len(parts) >= 6 and parts[4] == ROOT:
            require(parts[3] == "/" and after and after[0] == "cgroup2", "cgroup_v2_mount")
            require(re.fullmatch(r"[1-9]\d{0,9}", parts[0]), "cgroup_mount_id")
            matches.append(int(parts[0]))
    require(len(matches) == 1, "cgroup_mount_not_unique")
    return matches[0]


def fd_mount_id(fdinfo):
    matches = [line.partition(":")[2].strip() for line in fdinfo.splitlines()
               if line.startswith("mnt_id:")]
    require(len(matches) == 1 and re.fullmatch(r"[1-9]\d{0,9}", matches[0]), "fd_mount_id")
    return int(matches[0])


def unit_properties(raw):
    require(len(raw) <= 4096 and raw.endswith("\n"), "manager_file_shape")
    pairs = {}
    for line in raw.splitlines():
        key, sep, value = line.partition("=")
        require(sep and key in FIELDS and key not in pairs, "manager_field")
        pairs[key] = value
    require(set(pairs) == set(FIELDS), "manager_fields_missing")
    return pairs


def bounded_manager_query(argv, timeout=3):
    deadline = time.monotonic() + timeout
    env = {"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"}
    child = subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    output = bytearray()
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ, True)
            selector.register(child.stderr, selectors.EVENT_READ, False)
            while selector.get_map():
                remaining = deadline - time.monotonic()
                require(remaining > 0, "manager_deadline")
                ready = selector.select(remaining)
                require(bool(ready), "manager_deadline")
                for key, _ in ready:
                    cap = 4097 - len(output) if key.data else 1
                    data = os.read(key.fd, cap)
                    if not data:
                        selector.unregister(key.fileobj)
                    elif not key.data:
                        raise Refused("manager_stderr")
                    else:
                        output.extend(data)
                        require(len(output) <= 4096, "manager_output_bound")
        require(child.wait(timeout=max(0, deadline - time.monotonic())) == 0, "manager_read")
        return bytes(output)
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=2)
        child.stdout.close()
        child.stderr.close()


def sample_events(fd, expected, ops=os):
    def verified_fd():
        info = ops.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and (info.st_dev, info.st_ino) == expected,
                "events_fd_changed")
    verified_fd()
    ops.lseek(fd, 0, os.SEEK_SET)
    data = ops.read(fd, 257)
    require(ops.read(fd, 1) == b"", "events_not_eof")
    verified_fd()
    return parse_events(data)


def held_events(unit, expected_mount, stack):
    ds = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
    fs = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC
    parent = os.open(ROOT, ds)
    stack.callback(os.close, parent)
    for part in ("system.slice", unit):
        parent = os.open(part, ds, dir_fd=parent)
        stack.callback(os.close, parent)
        require(fd_mount_id(Path(f"/proc/self/fdinfo/{parent}").read_text()) == expected_mount,
                "cgroup_ancestor_mount")
    fd = os.open("cgroup.events", fs, dir_fd=parent)
    stack.callback(os.close, fd)
    require(fd_mount_id(Path(f"/proc/self/fdinfo/{fd}").read_text()) == expected_mount,
            "events_wrong_mount")
    info = os.fstat(fd)
    return fd, (info.st_dev, info.st_ino)


def check_manager(properties, unit, invocation, live):
    require(properties["Id"] == unit and properties["InvocationID"] == invocation
            and properties["LoadState"] == "loaded", "manager_identity")
    current = (properties["ActiveState"], properties["SubState"])
    group = "/system.slice/" + unit
    if live:
        require(current == ("active", "running") and properties["ControlGroup"] == group
                and re.fullmatch(r"[1-9]\d{0,9}", properties["MainPID"])
                and int(properties["MainPID"]) > 1, "manager_not_live")
    else:
        require(properties["ControlGroup"] in ("", group), "manager_other_cgroup")
    return current in TERMINAL and properties["MainPID"] == "0"


def require_zero(read, identify, check_boot, deadline, now=time.monotonic, sleep=time.sleep):
    """Terminal metadata is necessary but never substitutes for a real zero read."""
    while True:
        require(now() < deadline, "closure_deadline")
        check_boot()
        terminal = identify()
        if terminal:
            empty = read() == 0
            check_boot()
            still_terminal = identify()
            require(now() < deadline, "closure_deadline")
            if empty and still_terminal:
                return
        sleep(0.05)


def observe(run, boot, invocation, timeout):
    require(sys.platform == "linux", "linux_required")
    require(RUN.fullmatch(run) and RUN.fullmatch(invocation) and invocation != "0" * 32
            and UUID.fullmatch(boot), "identity_shape")
    require(SECONDS.fullmatch(timeout) and int(timeout) <= 1800, "deadline_shape")
    unit = f"bluey-jobs-receipt-{run}.service"
    deadline = time.monotonic() + int(timeout)

    def check_boot():
        require(Path(BOOT).read_text().strip() == boot, "boot_changed")

    def manager():
        argv = ["/usr/bin/systemctl", "--system", "--no-pager", "show", "--all",
                unit, *[f"--property={k}" for k in FIELDS]]
        return unit_properties(bounded_manager_query(argv).decode("ascii"))

    check_boot()
    live = manager()
    check_manager(live, unit, invocation, True)
    pid = int(live["MainPID"])
    membership = f'0::{live["ControlGroup"]}\n'
    require(Path(f"/proc/{pid}/cgroup").read_text() == membership, "main_cgroup_membership")
    bound_mount = mount_id(Path("/proc/self/mountinfo").read_text())
    with contextlib.ExitStack() as stack:
        fd, identity = held_events(unit, bound_mount, stack)
        require(sample_events(fd, identity) == 1, "initial_not_populated")
        after_open = manager()
        check_boot()
        check_manager(after_open, unit, invocation, True)
        require(live["MainPID"] == after_open["MainPID"]
                and Path(f"/proc/{pid}/cgroup").read_text() == membership, "main_changed_during_open")
        # The outer controller may only release its launch barrier on this
        # exact first output. This process never signals or reopens the unit.
        print(json.dumps({"event": "jobs_browser_closure_bound", "run": run, "boot": boot,
                          "invocation": invocation, "mountId": bound_mount, "eventsInode": identity[1]}),
              flush=True)
        require_zero(lambda: sample_events(fd, identity),
                     lambda: check_manager(manager(), unit, invocation, False),
                     check_boot, deadline)
    print(json.dumps({"event": "jobs_browser_closure_observed", "run": run, "boot": boot,
                      "invocation": invocation, "populated": 0, "sameEventsDescriptor": True}),
          flush=True)


if __name__ == "__main__":
    try:
        require(len(sys.argv) == 5, "expected_run_boot_invocation_timeout")
        observe(*sys.argv[1:])
    except (Refused, OSError, ValueError, subprocess.TimeoutExpired) as error:
        reason = str(error) if isinstance(error, Refused) else "observer_io"
        raise SystemExit(f"Jobs browser closure unverified: {reason}") from None
