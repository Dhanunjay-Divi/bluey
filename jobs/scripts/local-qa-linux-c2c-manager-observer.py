"""Root-side independent observation for a waiting local C2C QA actor.

The waiting frame selects an actor.  It never supplies a fact accepted here:
the unit manager, procfs, and bound launcher-root marker are read again by the
root manager before it may invoke the separate release writer.
"""
import hashlib
import os
from pathlib import Path
import re
import selectors
import stat
import subprocess
import sys
import time


REFUSED = "local_qa_linux_c2c_manager_observer_refused"
FIELDS = (
    "Id", "InvocationID", "MainPID", "ControlGroup", "PrivateNetwork", "NoNewPrivileges",
    "CapabilityBoundingSet", "Restart", "KillMode", "ActiveState", "SubState", "Type",
    "NetworkNamespacePath", "JoinsNamespaceOf", "PrivateTmp", "AmbientCapabilities",
    "ProtectSystem", "ProtectHome", "ExitType", "ReadWritePaths", "RestrictAddressFamilies",
)
WAITING = frozenset((
    "schema", "run", "unitName", "bootId", "invocationId", "unitCgroup", "networkNamespace",
    "root", "rootDevice", "rootInode", "markerSha256", "actorUid", "mainPid", "mainStartTime",
    "actorPid", "actorStartTime", "nonce", "waitingNs",
))
OBSERVED = WAITING - {"schema", "nonce", "waitingNs"}
RUN = re.compile(r"[0-9a-f]{32}")
BOOT = re.compile(r"[0-9a-f]{32}")
SHA = re.compile(r"[0-9a-f]{64}")
UNIT = re.compile(r"bluey-jobs-qa-[a-z0-9_.@-]{1,60}\.service")
ROOT = re.compile(r"/tmp/bluey-test-runs-[0-9a-f]{12}/run\.[1-9][0-9]{0,19}\.[1-9][0-9]{0,9}\.[0-9]{3,12}")
NETNS = re.compile(r"net:\[[1-9][0-9]{0,19}\]")
MAX_PID = 2_147_483_647


def _need(value):
    if not value:
        raise RuntimeError(REFUSED)


def _private_dir(value, uid, gid):
    return (stat.S_ISDIR(value.st_mode) and value.st_uid == uid and value.st_gid == gid
            and stat.S_IMODE(value.st_mode) == 0o700 and value.st_nlink >= 2)


def _stamp(value):
    return (value.st_dev, value.st_ino, value.st_uid, value.st_gid, value.st_mode,
            value.st_nlink, value.st_size, value.st_mtime_ns, value.st_ctime_ns)


def _directory_identity(value):
    return (value.st_dev, value.st_ino, value.st_uid, value.st_gid, value.st_mode)


def _stable_read(path, maximum, *, regular=False):
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        if regular:
            _need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and 0 < before.st_size <= maximum)
        parts, total = [], 0
        while True:
            part = os.read(fd, min(65536, maximum + 1 - total))
            if not part:
                break
            total += len(part)
            _need(total <= maximum)
            parts.append(part)
        body = b"".join(parts)
        if regular:
            named = os.stat(path, follow_symlinks=False)
            _need(len(body) == before.st_size and _stamp(before) == _stamp(os.fstat(fd)) == _stamp(named))
        else:
            _need(body and b"\0" not in body)
        return body, before
    finally:
        os.close(fd)


def parse_unit_show(raw):
    """Parse the exact, finite systemctl property projection used by the JS gate."""
    try:
        _need(isinstance(raw, bytes) and 0 < len(raw) <= 16384)
        text = raw.decode("ascii")
        _need(text.endswith("\n") and "\0" not in text and "\r" not in text)
        out = {}
        for line in text[:-1].split("\n"):
            key, sep, value = line.partition("=")
            _need(bool(sep) and key in FIELDS and key not in out and "\n" not in value and "\r" not in value)
            out[key] = value
        _need(set(out) == set(FIELDS))
        return out
    except (UnicodeDecodeError, RuntimeError):
        raise RuntimeError(REFUSED)


def validate_unit_show(value, plan, main_pid, invocation, cgroup):
    _need(value["Id"] == plan["unit"] and value["InvocationID"] == invocation
          and value["MainPID"] == str(main_pid) and value["ControlGroup"] == cgroup
          and value["PrivateNetwork"] == "yes" and value["NoNewPrivileges"] == "yes"
          and value["CapabilityBoundingSet"] == "" and value["Restart"] == "no"
          and value["KillMode"] == "control-group" and value["ActiveState"] == "active"
          and value["SubState"] == "running" and value["Type"] == "exec"
          and value["NetworkNamespacePath"] == "" and value["JoinsNamespaceOf"] == ""
          and value["PrivateTmp"] == "no" and value["AmbientCapabilities"] == ""
          and value["ProtectSystem"] == "strict" and value["ProtectHome"] == "yes"
          and value["ExitType"] == "cgroup" and value["ReadWritePaths"] == plan["runParent"]
          and value["RestrictAddressFamilies"].split(" ") == ["AF_INET", "AF_INET6", "AF_NETLINK", "AF_UNIX"])


def parse_waiting_selector(value):
    """Strict selector validation; returned values remain untrusted until observed."""
    try:
        _need(type(value) is dict and set(value) == WAITING
              and value["schema"] == "bluey-jobs-c2c-waiting-v1"
              and isinstance(value["run"], str) and RUN.fullmatch(value["run"])
              and value["run"] != "0" * 32
              and isinstance(value["unitName"], str) and UNIT.fullmatch(value["unitName"])
              and type(value["actorPid"]) is int and 1 < value["actorPid"] <= MAX_PID
              and type(value["mainPid"]) is int and 1 < value["mainPid"] <= MAX_PID
              and type(value["actorStartTime"]) is int and value["actorStartTime"] > 0
              and type(value["mainStartTime"]) is int and value["mainStartTime"] > 0
              and isinstance(value["root"], str) and ROOT.fullmatch(value["root"])
              and type(value["actorUid"]) is int and 0 < value["actorUid"] <= MAX_PID
              and type(value["rootDevice"]) is int and value["rootDevice"] >= 0
              and type(value["rootInode"]) is int and value["rootInode"] > 0
              and isinstance(value["markerSha256"], str) and SHA.fullmatch(value["markerSha256"])
              and isinstance(value["nonce"], str) and RUN.fullmatch(value["nonce"]) and value["nonce"] != "0" * 32)
        return dict(value)
    except (KeyError, RuntimeError):
        raise RuntimeError(REFUSED)


def _parse_stat(raw, expected):
    try:
        text = raw.decode("ascii").rstrip("\n")
        _need(text.startswith("{} (".format(expected)))
        right = text.rfind(")")
        fields = text[right + 2:].split()
        _need(right > len(str(expected)) + 2 and len(fields) > 19 and fields[0] != "Z"
              and fields[0].isalpha() and fields[1].isdigit() and fields[2].isdigit() and fields[19].isdigit())
        return {"pid": expected, "ppid": int(fields[1]), "pgrp": int(fields[2]), "start": int(fields[19])}
    except (UnicodeDecodeError, RuntimeError):
        raise RuntimeError(REFUSED)


def parse_proc_ids(text):
    """Require every effective/saved/filesystem ID to retain the same identity."""
    try:
        _need(type(text) is str and "\0" not in text)
        uid = [line.split()[1:] for line in text.splitlines() if line.startswith("Uid:\t")]
        gid = [line.split()[1:] for line in text.splitlines() if line.startswith("Gid:\t")]
        _need(len(uid) == len(gid) == 1 and len(uid[0]) == len(gid[0]) == 4
              and all(value.isdigit() for value in uid[0] + gid[0])
              and len(set(uid[0])) == len(set(gid[0])) == 1)
        return int(uid[0][0]), int(gid[0][0])
    except RuntimeError:
        raise RuntimeError(REFUSED)


def _proc(pid):
    _need(isinstance(pid, int) and 1 < pid <= MAX_PID)
    base = Path("/proc/{}".format(pid))
    row = _parse_stat(_stable_read(base / "stat", 4096)[0], pid)
    status = _stable_read(base / "status", 8192)[0].decode("ascii")
    uid, gid = parse_proc_ids(status)
    cgroup = _stable_read(base / "cgroup", 4096)[0].decode("ascii")
    netns = os.readlink(base / "ns/net")
    _need(NETNS.fullmatch(netns))
    row.update(uid=uid, gid=gid, cgroup=cgroup, netns=netns)
    return row


def _network(pid):
    root = Path("/proc/{}/net".format(pid))
    dev = _stable_read(root / "dev", 4096)[0].decode("ascii").splitlines()
    _need(len(dev) >= 3 and "Inter-|" in dev[0])
    names = []
    for line in dev[2:]:
        name, sep, values = line.partition(":")
        _need(bool(sep) and values.strip())
        names.append(name.strip())
    _need(names == ["lo"])
    routes = _stable_read(root / "route", 4096)[0].decode("ascii").splitlines()
    _need(routes and routes[0].split()[:3] == ["Iface", "Destination", "Gateway"])
    for line in routes[1:]:
        fields = line.split()
        _need(len(fields) == 11 and fields[0] == "lo" and fields[1] != "00000000")
    for line in _stable_read(root / "ipv6_route", 8192)[0].decode("ascii").splitlines():
        fields = line.split()
        _need(len(fields) == 10 and fields[-1] == "lo" and re.fullmatch(r"[0-9a-f]{8}", fields[-2]))
        default = fields[0] == "0" * 32 and fields[1] == "00"
        _need(not default or int(fields[-2], 16) & 0x200)


def _root_marker(root, expected_uid, expected_main, expected_main_pgrp, expected_actor_pgrp):
    data, info = _stable_read(root / ".bluey-test-run-v2", 4096, regular=True)
    _need(info.st_uid == expected_uid and stat.S_IMODE(info.st_mode) == 0o600)
    lines = data.decode("ascii").split("\n")
    _need(data.endswith(b"\n") and len(lines) == 10 and lines[0] == "magic=bluey-test-run-v2" and lines[-1] == "")
    fields = []
    for line in lines[1:9]:
        key, sep, value = line.partition("=")
        _need(bool(sep) and key and value and "=" not in value)
        fields.append((key, value))
    names = [key for key, _ in fields]
    _need(names == ["run_root", "run_name", "owner_uid", "owner_pid", "process_group_id", "launcher_group_id", "created_epoch", "state"])
    row = dict(fields)
    parts = root.name.split(".")
    _need(len(parts) == 4 and parts[0] == "run" and row["run_root"] == str(root) and row["run_name"] == root.name
          and row["owner_uid"] == str(expected_uid) and row["owner_pid"] == str(expected_main)
          and row["process_group_id"] == str(expected_actor_pgrp)
          and row["launcher_group_id"] == str(expected_main_pgrp) and row["launcher_group_id"] != row["process_group_id"]
          and row["created_epoch"] == parts[1] and row["state"] == "running")
    return hashlib.sha256(data).hexdigest(), info


def _pin_systemctl(expected):
    _need(isinstance(expected, str) and SHA.fullmatch(expected))
    path = "/usr/bin/systemctl"
    before_data, before = _stable_read(path, 256 * 1024 * 1024, regular=True)
    _need(before.st_uid == 0 and (before.st_mode & 0o7022) == 0 and (before.st_mode & 0o111) != 0
          and hashlib.sha256(before_data).hexdigest() == expected)
    return _stamp(before)


def _show(expected, unit):
    argv = ["/usr/bin/systemctl", "--system", "--no-pager", "show", "--all"]
    argv.extend("--property={}".format(field) for field in FIELDS)
    argv.extend(["--", unit])
    try:
        child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"}, close_fds=True)
    except OSError:
        raise RuntimeError(REFUSED)
    selector, out, err, deadline = selectors.DefaultSelector(), bytearray(), bytearray(), time.monotonic() + 5
    try:
        selector.register(child.stdout, selectors.EVENT_READ, out)
        selector.register(child.stderr, selectors.EVENT_READ, err)
        while selector.get_map():
            ready = selector.select(max(0, deadline - time.monotonic()))
            _need(ready)
            for key, _ in ready:
                chunk = os.read(key.fileobj.fileno(), 4096)
                if chunk:
                    key.data.extend(chunk)
                    _need(len(key.data) <= 16384)
                else:
                    selector.unregister(key.fileobj)
                    key.fileobj.close()
        _need(child.wait(timeout=max(0, deadline - time.monotonic())) == 0 and not err)
        return parse_unit_show(bytes(out))
    except (OSError, subprocess.TimeoutExpired, RuntimeError):
        child.kill()
        child.wait()
        raise RuntimeError(REFUSED)
    finally:
        selector.close()


def _parse_plan(plan):
    _need(type(plan) is dict and set(plan) == {"run", "unit", "uid", "gid", "runParent"}
          and isinstance(plan["run"], str) and RUN.fullmatch(plan["run"]) and plan["run"] != "0" * 32
          and isinstance(plan["unit"], str) and plan["unit"] == "bluey-jobs-qa-{}.service".format(plan["run"])
          and type(plan["uid"]) is int and 0 < plan["uid"] <= MAX_PID
          and type(plan["gid"]) is int and 0 < plan["gid"] <= MAX_PID
          and isinstance(plan["runParent"], str) and re.fullmatch(r"/tmp/bluey-test-runs-[0-9a-f]{12}", plan["runParent"]))
    return dict(plan)


def _actor_root_identity(actor_pid, absolute_root):
    """Follow only proc's root magic link; every subsequent component is nofollow."""
    fd = os.open("/proc/{}/root".format(actor_pid), os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NONBLOCK)
    try:
        for component in absolute_root.lstrip("/").split("/"):
            _need(component not in ("", ".", ".."))
            child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NONBLOCK | os.O_NOFOLLOW,
                            dir_fd=fd)
            os.close(fd)
            fd = child
        return _directory_identity(os.fstat(fd))
    finally:
        os.close(fd)


def make_independent_observer(plan, systemctl_sha256):
    """Return the root-only observation callback consumed by the release writer."""
    _need(sys.platform == "linux" and os.uname().machine == "x86_64" and os.getuid() == os.geteuid() == 0)
    plan = _parse_plan(plan)
    systemctl_stamp = _pin_systemctl(systemctl_sha256)
    parent = Path(plan["runParent"])
    parent_info = os.stat(parent, follow_symlinks=False)
    _need(_private_dir(parent_info, plan["uid"], plan["gid"]))
    boot = _stable_read("/proc/sys/kernel/random/boot_id", 128)[0].decode("ascii").strip().replace("-", "")
    _need(BOOT.fullmatch(boot))

    def observe(untrusted_waiting):
        waiting = parse_waiting_selector(untrusted_waiting)
        _need(waiting["run"] == plan["run"] and waiting["unitName"] == plan["unit"] and waiting["actorUid"] == plan["uid"])
        _need(_directory_identity(os.stat(parent, follow_symlinks=False)) == _directory_identity(parent_info))
        _need(_pin_systemctl(systemctl_sha256) == systemctl_stamp)
        state = _show(systemctl_sha256, plan["unit"])
        main_pid = int(state["MainPID"])
        invocation = state["InvocationID"].replace("-", "").lower()
        cgroup = state["ControlGroup"]
        _need(BOOT.fullmatch(invocation) and main_pid > 1 and cgroup == "/system.slice/{}".format(plan["unit"]))
        validate_unit_show(state, plan, main_pid, invocation, cgroup)
        main, actor = _proc(main_pid), _proc(waiting["actorPid"])
        _need(main["uid"] == actor["uid"] == plan["uid"] and main["gid"] == actor["gid"] == plan["gid"]
              and main["cgroup"] == actor["cgroup"] == "0::{}\n".format(cgroup)
              and main["netns"] == actor["netns"] and actor["start"] == waiting["actorStartTime"]
              and os.stat("/proc/self/ns/net").st_ino != os.stat("/proc/{}/ns/net".format(actor["pid"])).st_ino)
        root = Path(waiting["root"])
        _need(root.parent == parent and os.path.realpath(parent) == str(parent) and os.path.realpath(root) == str(root))
        root_info = os.stat(root, follow_symlinks=False)
        _need(_private_dir(root_info, plan["uid"], plan["gid"]) and root_info.st_dev == waiting["rootDevice"]
              and root_info.st_ino == waiting["rootInode"]
              and _actor_root_identity(actor["pid"], str(root)) == _directory_identity(root_info))
        marker_sha, marker_info = _root_marker(root, plan["uid"], main_pid, main["pgrp"], actor["pgrp"])
        _need(marker_info.st_uid == plan["uid"] and marker_sha == waiting["markerSha256"])
        _network(actor["pid"])
        _need(_stable_read("/proc/sys/kernel/random/boot_id", 128)[0].decode("ascii").strip().replace("-", "") == boot)
        _need(_pin_systemctl(systemctl_sha256) == systemctl_stamp)
        # Every field is a fresh root observation, not a value copied from the frame.
        return {
            "run": plan["run"], "unitName": plan["unit"], "bootId": boot, "invocationId": invocation,
            "unitCgroup": cgroup, "networkNamespace": actor["netns"], "root": str(root),
            "rootDevice": root_info.st_dev, "rootInode": root_info.st_ino, "markerSha256": marker_sha,
            "actorUid": actor["uid"], "mainPid": main["pid"], "mainStartTime": main["start"],
            "actorPid": actor["pid"], "actorStartTime": actor["start"],
        }

    return observe
