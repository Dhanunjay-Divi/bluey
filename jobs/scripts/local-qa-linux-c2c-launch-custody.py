"""Pre-frame root custody for one separately admitted C2C systemd launch."""
from contextlib import contextmanager
import hashlib
import os
from pathlib import Path
import re
import selectors
import stat
import subprocess
import sys
import time
import types


OBSERVER = "local-qa-linux-c2c-manager-observer.py"
OBSERVER_SHA256 = "57d60374c15596bfe0594b707892f7a847c3c5a459da6564778525729c886530"
SHA = re.compile(r"[0-9a-f]{64}")
RUN = re.compile(r"[0-9a-f]{32}")
BOOT_DASHED = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
HEX32 = re.compile(r"[0-9a-f]{32}")
PARENT = re.compile(r"/tmp/bluey-test-runs-[0-9a-f]{12}")
MAX_SOURCE = 128 * 1024
MAX_PID = 2_147_483_647
REFUSED = "local_qa_linux_c2c_launch_custody_refused"
MANAGER_FIELDS = tuple(list((
    "Id", "InvocationID", "MainPID", "ControlGroup", "PrivateNetwork", "NoNewPrivileges",
    "CapabilityBoundingSet", "Restart", "KillMode", "ActiveState", "SubState", "Type",
    "NetworkNamespacePath", "JoinsNamespaceOf", "PrivateTmp", "AmbientCapabilities",
    "ProtectSystem", "ProtectHome", "ExitType", "ReadWritePaths", "RestrictAddressFamilies",
)) + ["LoadState", "Job"])


def _need(value):
    if not value:
        raise RuntimeError(REFUSED)


def _stamp(value):
    return (value.st_dev, value.st_ino, value.st_uid, value.st_gid, value.st_mode,
            value.st_nlink, value.st_size, value.st_mtime_ns, value.st_ctime_ns)


def _load_observer():
    path = Path(__file__).resolve().with_name(OBSERVER)
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        _need(stat.S_ISREG(before.st_mode) and before.st_uid == 0 and before.st_nlink == 1
              and stat.S_IMODE(before.st_mode) in (0o600, 0o644) and 0 < before.st_size <= MAX_SOURCE)
        chunks, total = [], 0
        while True:
            chunk = os.read(fd, min(65536, MAX_SOURCE + 1 - total))
            if not chunk:
                break
            total += len(chunk)
            _need(total <= MAX_SOURCE)
            chunks.append(chunk)
        raw = b"".join(chunks)
        named = os.stat(path, follow_symlinks=False)
        _need(len(raw) == before.st_size and _stamp(before) == _stamp(os.fstat(fd)) == _stamp(named)
              and hashlib.sha256(raw).hexdigest() == OBSERVER_SHA256)
    finally:
        os.close(fd)
    module = types.ModuleType("_pinned_c2c_manager_observer")
    module.__file__ = str(path)
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


def _plan(plan):
    _need(type(plan) is dict and set(plan) == {"run", "unit", "uid", "gid", "runParent"}
          and type(plan["run"]) is str and RUN.fullmatch(plan["run"]) and plan["run"] != "0" * 32
          and type(plan["unit"]) is str and plan["unit"] == "bluey-jobs-qa-{}.service".format(plan["run"])
          and type(plan["uid"]) is int and 0 < plan["uid"] <= MAX_PID
          and type(plan["gid"]) is int and 0 < plan["gid"] <= MAX_PID
          and type(plan["runParent"]) is str and PARENT.fullmatch(plan["runParent"]))
    return dict(plan)


def validate_launch_claim(value):
    """Pure claim-shape validator used by portable tests and later stop code."""
    _need(type(value) is dict and set(value) == {"run", "unit", "invocation", "boot", "actorUid",
                                                   "workloadNetns", "mainPid", "mainStartTime"}
          and type(value["run"]) is str and RUN.fullmatch(value["run"]) and value["run"] != "0" * 32
          and type(value["unit"]) is str and value["unit"] == "bluey-jobs-qa-{}.service".format(value["run"])
          and type(value["invocation"]) is str and HEX32.fullmatch(value["invocation"])
          and value["invocation"] != "0" * 32 and type(value["boot"]) is str and BOOT_DASHED.fullmatch(value["boot"])
          and type(value["actorUid"]) is int and 0 < value["actorUid"] <= MAX_PID
          and type(value["workloadNetns"]) is int and value["workloadNetns"] > 0
          and type(value["mainPid"]) is int and 1 < value["mainPid"] <= MAX_PID
          and type(value["mainStartTime"]) is int and value["mainStartTime"] > 0)
    return dict(value)


def _boot(observer):
    raw, _ = observer._stable_read("/proc/sys/kernel/random/boot_id", 128)
    value = raw.decode("ascii").strip().lower()
    _need(BOOT_DASHED.fullmatch(value))
    return value


def _opened_netns(main_pid):
    # This is the intentionally followed procfs namespace magic link.  No
    # workload pathname is followed here, and the returned FD pins the namespace
    # through the caller's release/teardown interval.
    path = "/proc/{}/ns/net".format(main_pid)
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NONBLOCK)
    try:
        inode = os.fstat(fd).st_ino
        _need(inode > 0 and os.stat(path).st_ino == inode and os.stat("/proc/self/ns/net").st_ino != inode)
        return fd, inode
    except Exception:
        os.close(fd)
        raise


def parse_manager_show(raw):
    """Strict manager projection: observer fields plus launch-state discriminants."""
    _need(type(raw) is bytes and 0 < len(raw) <= 16384)
    text = raw.decode("ascii")
    _need(text.endswith("\n") and "\0" not in text and "\r" not in text)
    value = {}
    for line in text[:-1].split("\n"):
        key, sep, item = line.partition("=")
        _need(bool(sep) and key in MANAGER_FIELDS and key not in value and "\n" not in item and "\r" not in item)
        value[key] = item
    _need(set(value) == set(MANAGER_FIELDS))
    return value


def _normalize_invocation(value):
    result = value.replace("-", "").lower()
    _need(HEX32.fullmatch(result) and result != "0" * 32)
    return result


def _phase(value, plan, remembered):
    """Return (transient, invocation) or refuse a loaded/incompatible unit."""
    _need(value["Id"] == plan["unit"])
    raw_invocation = value["InvocationID"]
    invocation = _normalize_invocation(raw_invocation) if raw_invocation else None
    if invocation is not None and remembered is not None:
        _need(invocation == remembered)
    remembered = invocation or remembered
    if value["LoadState"] == "not-found":
        _need(value["ActiveState"] == "inactive" and value["SubState"] == "dead"
              and value["MainPID"] == "0" and value["ControlGroup"] == "" and value["Job"] == "")
        return True, remembered
    if value["LoadState"] == "loaded" and value["ActiveState"] == "activating" and value["SubState"] == "start":
        _need(value["Job"] != "")
        return True, remembered
    _need(value["LoadState"] == "loaded" and value["ActiveState"] == "active" and value["SubState"] == "running")
    _need(remembered is not None)
    return False, remembered


def _manager_show(observer, unit, deadline_ns):
    """Bounded raw manager query; only closed vacant/start projections are transient."""
    argv = ["/usr/bin/systemctl", "--system", "--no-pager", "show", "--all"]
    argv.extend("--property={}".format(field) for field in MANAGER_FIELDS)
    argv.extend(["--", unit])
    try:
        child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"}, close_fds=True)
    except OSError:
        raise RuntimeError(REFUSED)
    selector, out, err = selectors.DefaultSelector(), bytearray(), bytearray()
    try:
        selector.register(child.stdout, selectors.EVENT_READ, out)
        selector.register(child.stderr, selectors.EVENT_READ, err)
        while selector.get_map():
            remain = (deadline_ns - time.clock_gettime_ns(time.CLOCK_MONOTONIC)) / 1_000_000_000
            ready = selector.select(max(0, min(5, remain)))
            _need(ready)
            for key, _ in ready:
                chunk = os.read(key.fileobj.fileno(), 4096)
                if chunk:
                    key.data.extend(chunk)
                    _need(len(key.data) <= 16384)
                else:
                    selector.unregister(key.fileobj)
                    key.fileobj.close()
        remain = (deadline_ns - time.clock_gettime_ns(time.CLOCK_MONOTONIC)) / 1_000_000_000
        code = child.wait(timeout=max(0, remain))
        _need(code == 0)
        _need(not err)
        return parse_manager_show(bytes(out))
    except (OSError, subprocess.TimeoutExpired, RuntimeError):
        if child.poll() is None:
            try:
                child.kill()
            except ProcessLookupError:
                pass
        try:
            child.wait()
        except OSError:
            pass
        raise RuntimeError(REFUSED)
    finally:
        for stream in (child.stdout, child.stderr):
            if stream is not None and not stream.closed:
                stream.close()
        selector.close()


@contextmanager
def capture_launch_claim(plan, systemctl_sha256, *, independently_admitted, deadline_ns):
    """Hold a workload network namespace FD once the exact unit reaches exec/running.

    The context does not read or consume actor stdout.  It is intended to wrap
    the later waiter-frame protocol, so a malformed first frame still has a
    launch-bound claim available to its external cleanup adapter.
    """
    _need(sys.platform == "linux" and os.uname().machine == "x86_64" and os.getuid() == os.geteuid() == 0
          and callable(independently_admitted) and type(deadline_ns) is int)
    plan = _plan(plan)
    now = lambda: time.clock_gettime_ns(time.CLOCK_MONOTONIC)
    _need(now() < deadline_ns and independently_admitted(dict(plan)) is True)
    observer = _load_observer()
    _need(type(systemctl_sha256) is str and SHA.fullmatch(systemctl_sha256))
    tool_stamp = observer._pin_systemctl(systemctl_sha256)
    parent = Path(plan["runParent"])
    parent_initial = observer._directory_identity(os.stat(parent, follow_symlinks=False))
    _need(observer._private_dir(os.stat(parent, follow_symlinks=False), plan["uid"], plan["gid"]))
    boot = _boot(observer)
    held = None
    saw_running = False
    remembered_invocation = None
    try:
        while now() < deadline_ns:
            _need(independently_admitted(dict(plan)) is True
                  and observer._directory_identity(os.stat(parent, follow_symlinks=False)) == parent_initial
                  and observer._pin_systemctl(systemctl_sha256) == tool_stamp and _boot(observer) == boot)
            state = _manager_show(observer, plan["unit"], deadline_ns)
            transient, invocation = _phase(state, plan, remembered_invocation)
            remembered_invocation = invocation
            if transient:
                if saw_running:
                    raise RuntimeError(REFUSED)
                if now() >= deadline_ns:
                    break
                time.sleep(0.02)
                continue
            # A successful manager response with an unexpected property is
            # malformed/replaced, not a launch transient.
            main_pid = int(state["MainPID"])
            cgroup = state["ControlGroup"]
            _need(1 < main_pid <= MAX_PID and cgroup == "/system.slice/{}".format(plan["unit"]))
            checked = {key: state[key] for key in observer.FIELDS}
            checked["InvocationID"] = invocation
            observer.validate_unit_show(checked, plan, main_pid, invocation, cgroup)
            main = observer._proc(main_pid)
            _need(main["uid"] == plan["uid"] and main["gid"] == plan["gid"]
                  and main["cgroup"] == "0::{}\n".format(cgroup))
            held, net_inode = _opened_netns(main_pid)
            _need(observer._pin_systemctl(systemctl_sha256) == tool_stamp
                  and observer._directory_identity(os.stat(parent, follow_symlinks=False)) == parent_initial
                  and independently_admitted(dict(plan)) is True and _boot(observer) == boot)
            # Re-query after opening the namespace: a replacement must never
            # inherit a namespace FD from the previously observed MainPID.
            late = _manager_show(observer, plan["unit"], deadline_ns)
            late_transient, late_invocation = _phase(late, plan, remembered_invocation)
            _need(not late_transient and late_invocation == invocation and int(late["MainPID"]) == main_pid
                  and late["ControlGroup"] == cgroup)
            late_main = observer._proc(main_pid)
            _need(late_main["start"] == main["start"] and late_main["uid"] == main["uid"]
                  and late_main["gid"] == main["gid"] and late_main["netns"] == main["netns"]
                  and os.stat("/proc/{}/ns/net".format(main_pid)).st_ino == net_inode)
            claim = validate_launch_claim({"run": plan["run"], "unit": plan["unit"],
                                           "invocation": invocation, "boot": boot, "actorUid": plan["uid"],
                                           "workloadNetns": net_inode, "mainPid": main_pid,
                                           "mainStartTime": main["start"]})
            saw_running = True
            yield claim, held
            return
        raise RuntimeError(REFUSED)
    finally:
        if held is not None:
            os.close(held)
