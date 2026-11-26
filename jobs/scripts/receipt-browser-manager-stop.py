"""Retain and stop one already-admitted synthetic systemd unit."""
from contextlib import contextmanager
import ctypes
import hashlib
import os
from pathlib import Path
import re
import stat
import sys
import time

OBSERVER_SHA = "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4"
SOCKET = Path("/run/dbus/system_bus_socket")
SERVICE = b"org.freedesktop.systemd1"
MANAGER_PATH = b"/org/freedesktop/systemd1"
MANAGER = SERVICE + b".Manager"
UNIT = SERVICE + b".Unit"
FIELDS = ("Id", "InvocationID", "ControlGroup", "LoadState", "ActiveState", "SubState", "MainPID", "Job")
NONCE = re.compile(r"[0-9a-f]{32}")
BOOT = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")
SHA = re.compile(r"[0-9a-f]{64}")


def require(ok, reason):
    if not ok:
        raise RuntimeError(reason)


def clock():
    return time.clock_gettime_ns(time.CLOCK_MONOTONIC)


def remaining(deadline_ns, cap):
    seconds = (deadline_ns - clock()) / 1_000_000_000
    require(seconds > 0, "manager_stop_deadline")
    return min(cap, seconds)


def parse_show(raw):
    require(type(raw) is bytes and 0 < len(raw) <= 4096 and raw.endswith(b"\n"), "manager_stop_show_bound")
    result = {}
    for line in raw[:-1].decode("ascii").split("\n"):
        key, sep, value = line.partition("=")
        require(sep and key in FIELDS and key not in result and
                all(32 <= ord(c) < 127 for c in value), "manager_stop_show_field")
        result[key] = value
    require(set(result) == set(FIELDS), "manager_stop_show_fields")
    return result


def check_state(rows, unit, invocation, phase, mode):
    group = f"/system.slice/{unit}"
    pair = rows["ActiveState"], rows["SubState"]
    require(rows["Id"] == unit and rows["InvocationID"] == invocation and rows["LoadState"] == "loaded"
            and rows["Job"] == "", "manager_stop_unit_identity_or_pending_job")
    if phase == "live":
        require(pair == ("active", "running") and rows["ControlGroup"] == group
                and re.fullmatch(r"[1-9][0-9]{0,9}", rows["MainPID"]) and int(rows["MainPID"]) > 1,
                "manager_stop_unit_not_live")
    else:
        states = ({("inactive", "dead")} if phase == "stopped" else
                  ({("active", "exited"), ("inactive", "dead")} if mode == "case" else {("failed", "failed")}))
        require(pair in states and rows["MainPID"] == "0" and rows["ControlGroup"] in ("", group),
                "manager_stop_unexpected_state")


@contextmanager
def scoped_reference(unit, invocation, mode, deadline_ns, reference, read, stop, guard, pause=time.sleep):
    """Protocol core. The Linux caller supplies the authenticated adapters."""
    require(mode in ("case", "timeout-control"), "manager_stop_mode")

    def observed():
        guard()
        rows = read(deadline_ns)
        guard()
        return rows

    def state(phase):
        rows = observed()
        check_state(rows, unit, invocation, phase, mode)
        return rows

    before = state("live")
    active = False
    try:
        reference("RefUnit", unit, min(deadline_ns, clock() + 2_000_000_000))
        active = True
        require(state("live") == before, "manager_stop_bind_changed")
        used = False

        def stop_exact(expected_unit, expected_invocation, expected_deadline):
            nonlocal used
            require(active and not used and (expected_unit, expected_invocation, expected_deadline)
                    == (unit, invocation, deadline_ns), "manager_stop_scope")
            state("terminal")
            used = True  # An uncertain stop is not authority to retry it.
            job = stop(invocation, deadline_ns)
            require(type(job) is str and re.fullmatch(r"[1-9][0-9]{0,9}", job)
                    and int(job) < 2**32, "manager_stop_job")
            while True:
                rows = observed()
                if rows["Job"] == "":
                    check_state(rows, unit, invocation, "stopped", mode)
                    return True
                require(rows["Id"] == unit and rows["InvocationID"] == invocation
                        and rows["LoadState"] == "loaded" and rows["Job"] == job and rows["MainPID"] == "0"
                        and rows["ControlGroup"] in ("", f"/system.slice/{unit}"),
                        "manager_stop_changed_job")
                pause(min(0.05, remaining(deadline_ns, 1)))

        yield stop_exact
    finally:
        active = False
        # A lost admission or elapsed runtime deadline cannot strand a
        # reference. Attempt cleanup even on an uncertain Ref reply.
        reference("UnrefUnit", unit, clock() + 2_000_000_000)


def controlled_path(path):
    require(path.is_absolute() and path.resolve(strict=True) == path, "manager_stop_path")
    for parent in path.parents:
        item = parent.stat()
        require(stat.S_ISDIR(item.st_mode) and item.st_uid == 0 and not item.st_mode & 0o022,
                "manager_stop_parent")


def load_library(path, expected):
    controlled_path(path)
    require(type(expected) is str and SHA.fullmatch(expected), "manager_stop_library_pin")
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC), "rb") as file:
        info = os.fstat(file.fileno())
        require(stat.S_ISREG(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o022
                and 0 < info.st_size <= 16 * 1024**2, "manager_stop_library_identity")
        data = file.read(16 * 1024**2 + 1)
        after = os.fstat(file.fileno())
        require(len(data) == info.st_size and hashlib.sha256(data).hexdigest() == expected
                and (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)
                == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns),
                "manager_stop_library_pin")
        return ctypes.CDLL(f"/proc/self/fd/{file.fileno()}", mode=os.RTLD_NOW | os.RTLD_LOCAL)


def invocation_path(invocation):
    require(type(invocation) is str and NONCE.fullmatch(invocation) and invocation != "0" * 32,
            "manager_stop_invocation_path")
    # sd_bus_path_encode's bus label rule escapes a leading digit.
    first = f"_{ord(invocation[0]):02x}" if invocation[0].isdigit() else invocation[0]
    return MANAGER_PATH + b"/unit/" + (first + invocation[1:]).encode("ascii")


@contextmanager
def bus_manager(lib):
    """One fixed system bus connection; Ref/Unref and invocation-addressed Stop."""
    p, c, i, u = ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int, ctypes.c_uint64
    pp = ctypes.POINTER(p)
    definitions = {
        "sd_bus_new": (i, [pp]), "sd_bus_set_address": (i, [p, c]), "sd_bus_set_bus_client": (i, [p, i]),
        "sd_bus_start": (i, [p]), "sd_bus_message_new_method_call": (i, [p, pp, c, c, c, c]),
        "sd_bus_message_append_basic": (i, [p, ctypes.c_char, p]),
        "sd_bus_call": (i, [p, p, u, p, pp]), "sd_bus_message_at_end": (i, [p, i]),
        "sd_bus_message_read_basic": (i, [p, ctypes.c_char, p]),
        "sd_bus_message_unref": (p, [p]), "sd_bus_close_unref": (p, [p]),
    }
    for name, (result, args) in definitions.items():
        method = getattr(lib, name)
        method.restype, method.argtypes = result, args
    bus = p()
    try:
        require(lib.sd_bus_new(ctypes.byref(bus)) >= 0 and bus, "manager_stop_bus_new")
        require(lib.sd_bus_set_address(bus, b"unix:path=/run/dbus/system_bus_socket") >= 0
                and lib.sd_bus_set_bus_client(bus, 1) >= 0 and lib.sd_bus_start(bus) >= 0,
                "manager_stop_bus_start")

        def request(path, interface, method, argument, deadline_ns, job_reply=False):
            message, reply = p(), p()
            try:
                require(lib.sd_bus_message_new_method_call(bus, ctypes.byref(message), SERVICE,
                        path, interface, method) >= 0, "manager_stop_request")
                value = ctypes.c_char_p(argument)
                require(lib.sd_bus_message_append_basic(message, b"s", ctypes.cast(value, p)) >= 0,
                        "manager_stop_arguments")
                budget = max(1, int(remaining(deadline_ns, 2) * 1_000_000))
                require(lib.sd_bus_call(bus, message, budget, None, ctypes.byref(reply)) >= 0
                        and reply, "manager_stop_response")
                result = None
                if job_reply:
                    out = c()
                    require(lib.sd_bus_message_read_basic(reply, b"o", ctypes.byref(out)) > 0
                            and out.value is not None, "manager_stop_job_response")
                    m = re.fullmatch(rb"/org/freedesktop/systemd1/job/([1-9][0-9]{0,9})", out.value)
                    require(m, "manager_stop_job_response")
                    result = m[1].decode("ascii")
                require(lib.sd_bus_message_at_end(reply, 1) == 1, "manager_stop_response_trailing")
                return result
            finally:
                if reply:
                    lib.sd_bus_message_unref(reply)
                if message:
                    lib.sd_bus_message_unref(message)

        def reference(method, unit, deadline_ns):
            require(method in ("RefUnit", "UnrefUnit"), "manager_stop_reference_method")
            return request(MANAGER_PATH, MANAGER, method.encode("ascii"), unit.encode("ascii"), deadline_ns)

        def stop(invocation, deadline_ns):
            # systemd resolves this path against its current invocation index
            # before enqueuing the job. A replacement with the same unit name
            # cannot be selected by the old invocation.
            return request(invocation_path(invocation), UNIT, b"Stop", b"fail", deadline_ns, True)

        yield reference, stop
    finally:
        if bus:
            lib.sd_bus_close_unref(bus)


@contextmanager
def bind_manager_stop(waiting, mode, *, library_path, library_sha256, independently_admitted, deadline_ns):
    """Linux-only stop reference; never dispatches, restarts or kills a PID."""
    require(sys.platform == "linux" and os.getuid() == os.geteuid() == 0, "manager_stop_linux_root")
    run, unit, invocation, boot = (waiting[k] for k in ("run", "unit", "invocation", "boot"))
    require(type(run) is str and NONCE.fullmatch(run) and run != "0" * 32
            and unit == f"bluey-jobs-receipt-{run}.service"
            and type(invocation) is str and NONCE.fullmatch(invocation) and invocation != "0" * 32
            and type(boot) is str and BOOT.fullmatch(boot), "manager_stop_scope")
    require(callable(independently_admitted) and type(deadline_ns) is int
            and 0 < deadline_ns - clock() <= 1_800_000_000_000, "manager_stop_admission")

    def guard():
        remaining(deadline_ns, 3)
        require(independently_admitted() is True
                and Path("/proc/sys/kernel/random/boot_id").read_text().strip() == boot, "manager_stop_lease")

    guard()
    controlled_path(SOCKET)
    sock = SOCKET.stat()
    require(stat.S_ISSOCK(sock.st_mode) and sock.st_uid == 0, "manager_stop_bus_socket")
    lib = load_library(Path(library_path), library_sha256)
    path = Path(__file__).resolve().with_name("interview-prep-linux-closure.py")
    with path.open("rb") as file:
        body = file.read(32768)
        require(file.read(1) == b"" and hashlib.sha256(body).hexdigest() == OBSERVER_SHA,
                "manager_stop_query_pin")
    helper = {"__name__": "verified_stop_query", "__file__": str(path)}
    exec(compile(body, str(path), "exec"), helper)
    query = helper["bounded_manager_query"]
    show = ["/usr/bin/systemctl", "--system", "--no-pager", "show", "--all",
            *[f"--property={field}" for field in FIELDS], "--", unit]
    with bus_manager(lib) as (reference, stop):
        with scoped_reference(unit, invocation, mode, deadline_ns, reference,
                              lambda d: parse_show(query(show, remaining(d, 3))),
                              stop, guard) as action:
            yield action
