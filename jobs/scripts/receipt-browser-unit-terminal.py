"""Read systemd status from the supervisor, never from the Jobs workload."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import time

OBSERVER_SHA = "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4"
BARRIER_SHA = "8a043d7b14ea86cab295b58215b7773aafca7bd3fd4bbf3fa5aee6a8a454dadf"
BOOT = Path("/proc/sys/kernel/random/boot_id")
NONCE = re.compile(r"[0-9a-f]{32}")
UUID = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")
ROOT = re.compile(
    r"/tmp/bluey-test-runs-[a-f0-9]{12}/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}"
)
SCOPE = ("run", "boot", "invocation", "challenge", "root", "actorUid", "unit")
TERMINAL_FILE = "unit-terminal.json"
FIELDS = (
    "Id", "InvocationID", "ControlGroup", "LoadState", "ActiveState", "SubState", "MainPID",
    "Restart", "NRestarts", "Result", "ExecMainCode", "ExecMainStatus", "ExecMainPID",
    "ExecMainStartTimestampMonotonic", "ExecMainExitTimestampMonotonic",
)


def require(condition, reason):
    if not condition:
        raise RuntimeError(reason)


def parse_manager(raw):
    require(type(raw) is bytes and 0 < len(raw) <= 4096 and raw.endswith(b"\n"),
            "unit_terminal_manager_size")
    rows = {}
    for line in raw[:-1].decode("ascii").split("\n"):
        key, sep, value = line.partition("=")
        require(sep and key in FIELDS and key not in rows and
                all(32 <= ord(c) < 127 for c in value), "unit_terminal_manager_field")
        rows[key] = value
    require(set(rows) == set(FIELDS), "unit_terminal_manager_fields")
    return rows


def closed_scope(scope):
    require(type(scope) is dict and set(scope) == set(SCOPE), "unit_terminal_scope_fields")
    for key in ("run", "challenge", "invocation"):
        require(type(scope[key]) is str and NONCE.fullmatch(scope[key])
                and scope[key] != "0" * 32, "unit_terminal_nonce")
    require(type(scope["boot"]) is str and UUID.fullmatch(scope["boot"])
            and type(scope["root"]) is str and ROOT.fullmatch(scope["root"])
            and type(scope["actorUid"]) is int and 0 < scope["actorUid"] < 2**32 - 1
            and scope["unit"] == f'bluey-jobs-receipt-{scope["run"]}.service',
            "unit_terminal_scope")
    return dict(scope)


def identity(rows, scope):
    require(rows["Id"] == scope["unit"] and rows["InvocationID"] == scope["invocation"]
            and rows["LoadState"] == "loaded" and rows["Restart"] == "no"
            and rows["NRestarts"] == "0", "unit_terminal_manager_identity")
    pid, start = rows["ExecMainPID"], rows["ExecMainStartTimestampMonotonic"]
    require(re.fullmatch(r"[1-9][0-9]{0,9}", pid) and int(pid) > 1
            and re.fullmatch(r"[1-9][0-9]{0,15}", start), "unit_terminal_exec_identity")
    return pid, int(start)


def bind_sample(raw, scope, observed_ns):
    rows = parse_manager(raw)
    pid, start = identity(rows, scope)
    require(rows["ActiveState"] == "active" and rows["SubState"] == "running"
            and rows["ControlGroup"] == "/system.slice/" + scope["unit"]
            and rows["MainPID"] == pid and rows["Result"] == "success"
            and rows["ExecMainCode"] == rows["ExecMainStatus"] == "0"
            and rows["ExecMainExitTimestampMonotonic"] == "0"
            and start * 1000 <= observed_ns, "unit_terminal_not_live")
    return pid, start


def terminal_sample(raw, scope, live_exec, mode, publication_ns, observed_ns):
    rows = parse_manager(raw)
    require(identity(rows, scope) == live_exec, "unit_terminal_exec_changed")
    require(rows["MainPID"] == "0"
            and rows["ControlGroup"] in ("", "/system.slice/" + scope["unit"]),
            "unit_terminal_manager_not_terminal")
    ended = rows["ExecMainExitTimestampMonotonic"]
    require(re.fullmatch(r"[1-9][0-9]{0,15}", ended)
            and publication_ns < int(ended) * 1000 <= observed_ns, "unit_terminal_exit_time")
    state = (rows["ActiveState"], rows["SubState"])
    if mode == "case":
        require(state in {("active", "exited"), ("inactive", "dead")}
                and (rows["Result"], rows["ExecMainCode"], rows["ExecMainStatus"])
                == ("success", "1", "0"), "unit_terminal_not_success")
        return {"code": 0, "signal": None, "deadline": False}
    require(mode == "timeout-control" and state == ("failed", "failed")
            and (rows["Result"], rows["ExecMainCode"]) == ("timeout", "2")
            and rows["ExecMainStatus"] in ("9", "15"), "unit_terminal_not_timeout")
    return {"code": None, "signal": {"9": "SIGKILL", "15": "SIGTERM"}[rows["ExecMainStatus"]],
            "deadline": True}


def prepare_reader(scope, mode, query, boot, now, admitted, deadline_ns):
    """Protocol core. The Linux entry supplies the privileged observations."""
    scope = closed_scope(scope)
    require(mode in ("case", "timeout-control"), "unit_terminal_mode")

    def check():
        instant = now()
        require(type(deadline_ns) is int and instant < deadline_ns
                and admitted() is True, "unit_terminal_no_supervision")
        require(boot() == scope["boot"], "unit_terminal_boot_changed")
        return instant

    def sample():
        check()
        raw = query()
        return raw, check()

    before, bound = sample()
    execution = bind_sample(before, scope, bound)

    def capture(publication_ns):
        require(type(publication_ns) is int and bound < publication_ns < check(),
                "unit_terminal_release_order")
        first, seen = sample()
        status = terminal_sample(first, scope, execution, mode, publication_ns, seen)
        second, confirmed = sample()
        require(parse_manager(first) == parse_manager(second)
                and status == terminal_sample(second, scope, execution, mode, publication_ns, confirmed),
                "unit_terminal_snapshot_changed")
        exit_ns = int(parse_manager(first)["ExecMainExitTimestampMonotonic"]) * 1000
        return {"record": {**scope, "status": status}, "publicationNs": publication_ns,
                "exitNs": exit_ns, "observedNs": seen,
                "confirmedNs": confirmed, "managerSamples": (before, first, second)}

    return capture


def pinned_helper(name, digest):
    path = Path(__file__).resolve().with_name(name)
    with path.open("rb") as source:
        body = source.read(16 * 1024)
        require(source.read(1) == b"" and hashlib.sha256(body).hexdigest() == digest,
                "unit_terminal_helper_pin")
    helper = {"__name__": "verified_terminal_helper", "__file__": str(path)}
    exec(compile(body, str(path), "exec"), helper)
    return helper


def bind_unit_terminal(scope, mode, *, independently_admitted, deadline_ns):
    """Bind before release; capture manager status before explicitly stopping the unit.

    The root caller owns dispatch, deadline policy, release timestamp, and the
    separate held cgroup observer. A timeout must be manager-enforced, not a
    signalled process relabelled by the caller. This helper never starts,
    stops or signals a service and cannot grant runtime admission.
    """
    require(sys.platform == "linux" and os.getuid() == os.geteuid() == 0,
            "unit_terminal_linux_root")
    scope = closed_scope(scope)
    require(callable(independently_admitted) and independently_admitted() is True,
            "unit_terminal_unadmitted")
    helper = pinned_helper("interview-prep-linux-closure.py", OBSERVER_SHA)
    argv = ["/usr/bin/systemctl", "--system", "--no-pager", "show", "--all",
            *[f"--property={field}" for field in FIELDS], "--", scope["unit"]]
    clock = lambda: time.clock_gettime_ns(time.CLOCK_MONOTONIC)
    return prepare_reader(scope, mode, lambda: helper["bounded_manager_query"](argv),
                          lambda: BOOT.read_text(encoding="ascii").removesuffix("\n"),
                          clock, independently_admitted, deadline_ns)


def render_terminal(observation, release_sha256):
    record = observation["record"]
    scope = closed_scope({key: value for key, value in record.items() if key != "status"})
    status = record["status"]
    require(type(status) is dict and set(status) == {"code", "deadline", "signal"} and (
        (type(status["code"]) is int and status["code"] == 0
         and status["signal"] is None and status["deadline"] is False)
        or (status["code"] is None and status["signal"] in ("SIGTERM", "SIGKILL")
            and status["deadline"] is True)
    ), "unit_terminal_export_status")
    require(type(release_sha256) is str and re.fullmatch(r"[0-9a-f]{64}", release_sha256),
            "unit_terminal_export_release")
    return (json.dumps({**scope, "status": status, "releaseSha256": release_sha256},
                       sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def _write_terminal_at(directory_fd, body, owner):
    """Local FD mechanism; the Linux entry below is the authority boundary."""
    directory = os.fstat(directory_fd)
    require(stat.S_ISDIR(directory.st_mode) and directory.st_uid == owner
            and stat.S_IMODE(directory.st_mode) == 0o700, "unit_terminal_export_directory")
    require(type(body) is bytes and 0 < len(body) <= 4096, "unit_terminal_export_bytes")
    flags = os.O_NOFOLLOW | os.O_CLOEXEC
    name = TERMINAL_FILE
    with os.fdopen(os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | flags,
                          0o600, dir_fd=directory_fd), "wb") as file:
        require(file.write(body) == len(body), "unit_terminal_export_write")
        file.flush()
        os.fsync(file.fileno())
        original = os.fstat(file.fileno())
    read_fd = os.open(name, os.O_RDONLY | os.O_NONBLOCK | flags, dir_fd=directory_fd)
    try:
        def compare():
            current = os.fstat(read_fd)
            require(stat.S_ISREG(current.st_mode) and current.st_uid == owner
                    and current.st_nlink == 1 and stat.S_IMODE(current.st_mode) == 0o600
                    and current.st_size == len(body) and os.path.samestat(original, current)
                    and (current.st_mtime_ns, current.st_ctime_ns)
                    == (original.st_mtime_ns, original.st_ctime_ns),
                    "unit_terminal_export_identity")

        compare()
        require(os.pread(read_fd, len(body) + 1, 0) == body, "unit_terminal_export_readback")
        compare()
        os.fsync(directory_fd)
        return {"fd": read_fd, "device": original.st_dev, "inode": original.st_ino,
                "bytes": len(body), "sha256": hashlib.sha256(body).hexdigest()}
    except BaseException:
        os.close(read_fd)
        raise


def release_hash(observation, published, release, waiting, validate):
    require(release is not None, "unit_terminal_release_missing")
    body, info = release
    actual = hashlib.sha256(body).hexdigest()
    require(actual == published["releaseSha256"]
            and (info[0], info[1], len(body)) == (published["device"], published["inode"], published["bytes"]),
            "unit_terminal_published_release_changed")
    require(observation["publicationNs"] == published["publicationAfterNs"] < observation["observedNs"],
            "unit_terminal_publication_time")
    record = validate(body, waiting, observation["observedNs"])
    require(all(type(record[key]) is type(observation["record"][key])
                and record[key] == observation["record"][key] for key in SCOPE),
            "unit_terminal_release_scope")
    return actual


def export_unit_terminal(directory_fd, gate_fd, observation, waiting, published):
    """Return an owned read-only FD for the trusted controller to pass to Node.

    The caller must retain the release and check held cgroup zero plus the
    later census before accepting evidence. A failed write retains its file.
    """
    require(sys.platform == "linux" and os.getuid() == os.geteuid() == 0,
            "unit_terminal_linux_root")
    gate = os.fstat(gate_fd)
    require(stat.S_ISDIR(gate.st_mode) and gate.st_uid == 0 and stat.S_IMODE(gate.st_mode) == 0o755,
            "unit_terminal_release_gate")
    barrier = pinned_helper("receipt-browser-launch-barrier.py", BARRIER_SHA)
    digest = release_hash(observation, published, barrier["read_release"](gate_fd), waiting,
                          barrier["validate_release"])
    return _write_terminal_at(directory_fd, render_terminal(observation, digest), 0)
