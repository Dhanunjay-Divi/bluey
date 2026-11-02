#!/usr/bin/python3
"""Private Darwin process identity producer; no argv, environment or memory inspection.

An executable identity/class is neither code-signing attestation nor permission for
network/listener exemptions. Any uninspectable enumerated PID denies the whole capture.
Invoke only with /usr/bin/python3 -I -S -B and a closed snapshot selection.
"""

import ctypes
import hashlib
import json
import os
import platform
import stat
import sys
import time
from dataclasses import dataclass


MAX_PIDS = 4096
MAX_OUTPUT_BYTES = 1024 * 1024
MAX_SELECTION_BYTES = 65536
MAX_PID = 2147483647
BSD_INFO_SIZE = 136
PROC_RUID_ONLY = 5
PROC_PIDTBSDINFO = 3
REVIEWED_PYTHON_LAUNCHER = "/Applications/Xcode.app/Contents/Developer/usr/bin/python3"
REVIEWED_PYTHON_LAUNCHER_CANONICAL = (
    "/Applications/Xcode.app/Contents/Developer/Library/Frameworks/"
    "Python3.framework/Versions/3.9/bin/python3.9"
)
REVIEWED_PYTHON_NATIVE = (
    "/Applications/Xcode.app/Contents/Developer/Library/Frameworks/"
    "Python3.framework/Versions/3.9/Resources/Python.app/Contents/MacOS/Python"
)
ERROR_CODES = frozenset((
    "host_process_arguments", "host_process_platform", "host_process_layout",
    "host_process_self_probe", "host_process_native", "host_process_uninspectable",
    "host_process_protocol", "host_process_changed", "host_process_uid",
    "host_process_birth", "host_process_file", "host_process_limit",
    "host_process_internal",
))


class ClosedError(Exception):
    def __init__(self, code):
        self.code = code if code in ERROR_CODES else "host_process_internal"
        super().__init__(self.code)


class ProcBsdInfo(ctypes.Structure):
    _fields_ = [
        (name, ctypes.c_uint32) for name in (
            "flags", "status", "xstatus", "pid", "ppid", "uid", "gid",
            "ruid", "rgid", "svuid", "svgid", "reserved",
        )
    ] + [
        ("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32),
    ] + [
        (name, ctypes.c_uint32) for name in ("nfiles", "pgid", "pjobc", "tdev", "tpgid")
    ] + [
        ("nice", ctypes.c_int32), ("start_sec", ctypes.c_uint64),
        ("start_usec", ctypes.c_uint64),
    ]


@dataclass(frozen=True)
class BsdIdentity:
    pid: int
    ppid: int
    euid: int
    ruid: int
    birth_sec: int
    birth_usec: int


@dataclass(frozen=True)
class ExecutableObservation:
    canonical_path: str
    device: int
    inode: int
    mode: int
    uid: int


def require(condition, code):
    if not condition:
        raise ClosedError(code)


class NativeProvider:
    def __init__(self):
        require(ctypes.sizeof(ProcBsdInfo) == BSD_INFO_SIZE, "host_process_layout")
        self.library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self.library.proc_listpids.argtypes = (
            ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_int,
        )
        self.library.proc_listpids.restype = ctypes.c_int
        self.library.proc_pidinfo.argtypes = (
            ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int,
        )
        self.library.proc_pidinfo.restype = ctypes.c_int
        self.library.proc_pidpath.argtypes = (ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32)
        self.library.proc_pidpath.restype = ctypes.c_int

    def list_pids(self, uid):
        # The extra slot makes an exactly full/truncated native result a denial.
        capacity = (MAX_PIDS + 1) * ctypes.sizeof(ctypes.c_int32)
        buffer = (ctypes.c_int32 * (MAX_PIDS + 1))()
        returned = self.library.proc_listpids(PROC_RUID_ONLY, uid, buffer, capacity)
        return decode_pid_buffer(returned, capacity, list(buffer))

    def bsd_identity(self, pid):
        buffer = ProcBsdInfo()
        returned = self.library.proc_pidinfo(
            pid, PROC_PIDTBSDINFO, 0, ctypes.byref(buffer), BSD_INFO_SIZE,
        )
        return decode_bsd_buffer(returned, buffer)

    def executable_path(self, pid):
        buffer = ctypes.create_string_buffer(4096)
        returned = self.library.proc_pidpath(pid, buffer, len(buffer))
        return decode_path_buffer(returned, bytes(buffer))


class FilesystemObserver:
    def executable(self, path):
        require(valid_path(path), "host_process_file")
        canonical = os.path.realpath(path)
        require(valid_path(canonical), "host_process_file")
        metadata = os.lstat(canonical)
        require(stat.S_ISREG(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode),
                "host_process_file")
        require(metadata.st_dev >= 0 and metadata.st_ino > 0, "host_process_file")
        return ExecutableObservation(canonical, metadata.st_dev, metadata.st_ino,
                                     metadata.st_mode, metadata.st_uid)


def valid_path(path):
    return (isinstance(path, str) and path.startswith("/") and "\0" not in path
            and len(path.encode("utf-8", "strict")) < 4096)


def decode_pid_buffer(returned, capacity, values):
    require(type(returned) is int and type(capacity) is int
            and 0 < capacity <= (MAX_PIDS + 1) * 4 and capacity % 4 == 0
            and returned > 0 and returned < capacity and returned % 4 == 0,
            "host_process_protocol")
    count = returned // 4
    require(count <= MAX_PIDS and count <= len(values), "host_process_limit")
    return validate_pids(values[:count])


def decode_bsd_buffer(returned, buffer):
    require(type(returned) is int and returned == BSD_INFO_SIZE
            and isinstance(buffer, ProcBsdInfo), "host_process_uninspectable")
    # comm/name are never accessed or emitted.
    return BsdIdentity(buffer.pid, buffer.ppid, buffer.uid, buffer.ruid,
                       buffer.start_sec, buffer.start_usec)


def decode_path_buffer(returned, raw):
    require(type(returned) is int and isinstance(raw, bytes)
            and len(raw) == 4096 and 0 < returned < len(raw),
            "host_process_uninspectable")
    # proc_pidpath returns strlen(buffer), excluding its terminating NUL.
    terminator = raw.find(b"\0")
    require(terminator == returned, "host_process_protocol")
    try:
        path = raw[:terminator].decode("utf-8", "strict")
    except UnicodeError:
        raise ClosedError("host_process_protocol") from None
    require(valid_path(path), "host_process_protocol")
    return path


def validate_pids(values, max_count=MAX_PIDS):
    require(isinstance(values, list) and 0 < len(values) <= max_count, "host_process_limit")
    require(all(type(pid) is int and 0 < pid <= MAX_PID for pid in values),
            "host_process_protocol")
    require(len(set(values)) == len(values), "host_process_protocol")
    return values


def validate_selected_pids(values):
    require(isinstance(values, list) and 1 <= len(values) <= MAX_PIDS,
            "host_process_arguments")
    require(all(type(pid) is int and 1 < pid <= MAX_PID for pid in values)
            and all(left < right for left, right in zip(values, values[1:])),
            "host_process_arguments")
    return values


def parse_selection(raw):
    require(isinstance(raw, str) and 0 < len(raw) <= MAX_SELECTION_BYTES,
            "host_process_arguments")
    try:
        require(len(raw.encode("ascii", "strict")) <= MAX_SELECTION_BYTES,
                "host_process_arguments")
        values = json.loads(raw)
        validate_selected_pids(values)
        require(json.dumps(values, separators=(",", ":")) == raw, "host_process_arguments")
        return values
    except ClosedError:
        raise
    except (ValueError, TypeError, UnicodeError, RecursionError):
        raise ClosedError("host_process_arguments") from None


def snapshot_arguments(arguments):
    if arguments == ["--snapshot"]:
        return None
    require(len(arguments) == 2 and arguments[0] == "--snapshot-pids",
            "host_process_arguments")
    return parse_selection(arguments[1])


def validate_identity(identity, pid, uid, now_us):
    require(isinstance(identity, BsdIdentity), "host_process_protocol")
    require(all(type(value) is int for value in (
        identity.pid, identity.ppid, identity.euid, identity.ruid,
        identity.birth_sec, identity.birth_usec,
    )), "host_process_protocol")
    require(identity.pid == pid and 0 < pid <= MAX_PID
            and 0 <= identity.ppid <= MAX_PID and identity.ppid != pid,
            "host_process_protocol")
    require(identity.euid == uid and identity.ruid == uid, "host_process_uid")
    require(0 < identity.birth_sec <= 18446744073709551615
            and 0 <= identity.birth_usec < 1000000
            and identity.birth_sec * 1000000 + identity.birth_usec <= now_us,
            "host_process_birth")


def observe_stable_process(provider, filesystem, pid, uid, clock_us):
    try:
        first = provider.bsd_identity(pid)
        validate_identity(first, pid, uid, clock_us())
        first_path = provider.executable_path(pid)
        require(valid_path(first_path), "host_process_file")
        first_file = filesystem.executable(first_path)
        second = provider.bsd_identity(pid)
        validate_identity(second, pid, uid, clock_us())
        second_path = provider.executable_path(pid)
        require(valid_path(second_path), "host_process_file")
        second_file = filesystem.executable(second_path)
        require(first == second and first_path == second_path and first_file == second_file,
                "host_process_changed")
        require(isinstance(first_file, ExecutableObservation)
                and valid_path(first_file.canonical_path)
                and type(first_file.device) is int and first_file.device >= 0
                and type(first_file.inode) is int and first_file.inode > 0
                and stat.S_ISREG(first_file.mode) and not stat.S_ISLNK(first_file.mode),
                "host_process_file")
        return first, first_file
    except ClosedError:
        raise
    except (OSError, ValueError, TypeError, UnicodeError):
        raise ClosedError("host_process_uninspectable") from None


def executable_identity(observation):
    payload = (b"bluey-jobs-host-executable-identity-v1\0"
               + observation.canonical_path.encode("utf-8") + b"\0"
               + str(observation.device).encode("ascii") + b"\0"
               + str(observation.inode).encode("ascii"))
    return hashlib.sha256(payload).hexdigest()


def helper_class(path):
    # Classification is descriptive only; it never confers an exemption or authority.
    reviewed = (
        ("/opt/homebrew/Cellar/colima/0.10.3/bin/colima", "colima"),
        ("/opt/homebrew/Cellar/lima/2.2.1/bin/limactl", "lima"),
        ("/opt/homebrew/Cellar/lima/2.2.1/bin/lima-hostagent", "lima"),
        ("/opt/homebrew/Cellar/qemu/11.1.2/bin/qemu-system-x86_64", "qemu"),
        ("/opt/homebrew/Cellar/docker/29.8.1/bin/docker", "docker"),
        ("/usr/bin/ssh", "ssh"),
    )
    return dict(reviewed).get(path, "python-or-other")


def capture_snapshot(provider, filesystem, own_pid, uid, executable, clock_us,
                     selected_pids=None):
    require(type(uid) is int and 0 <= uid <= 4294967295, "host_process_uid")
    require(type(own_pid) is int and 0 < own_pid <= MAX_PID, "host_process_self_probe")
    _, self_file = observe_stable_process(provider, filesystem, own_pid, uid, clock_us)
    try:
        expected_self = filesystem.executable(executable)
    except (OSError, ValueError, TypeError):
        raise ClosedError("host_process_self_probe") from None
    require(self_file == expected_self and self_file.uid == 0
            and self_file.mode & 0o022 == 0, "host_process_self_probe")
    if selected_pids is None:
        try:
            pids = validate_pids(provider.list_pids(uid))
        except ClosedError:
            raise
        except (OSError, ValueError, TypeError):
            raise ClosedError("host_process_native") from None
    else:
        selected = validate_selected_pids(selected_pids)
        # At most 4096 caller-selected processes plus the independently probed producer.
        pids = validate_pids(sorted(set(selected) | {own_pid}), MAX_PIDS + 1)
    require(own_pid in pids, "host_process_self_probe")
    records = []
    for pid in sorted(pids):
        identity, observed = observe_stable_process(provider, filesystem, pid, uid, clock_us)
        records.append({
            "pid": str(identity.pid), "ppid": str(identity.ppid),
            "euid": str(identity.euid), "ruid": str(identity.ruid),
            "birthSec": str(identity.birth_sec), "birthUsec": str(identity.birth_usec),
            "executableIdentitySha256": executable_identity(observed),
            "helperClass": helper_class(observed.canonical_path),
        })
    encoded = (json.dumps({"version": 1, "records": records}, sort_keys=True,
                          separators=(",", ":"), ensure_ascii=True) + "\n").encode("ascii")
    require(len(encoded) <= MAX_OUTPUT_BYTES, "host_process_limit")
    return encoded


def capture_local(selected_pids=None):
    require(platform.system() == "Darwin" and platform.release() == "27.0.0"
            and platform.machine() == "arm64" and sys.version_info[:3] == (3, 9, 6),
            "host_process_platform")
    require(sys.flags.isolated == 1 and sys.flags.no_site == 1
            and sys.flags.dont_write_bytecode == 1 and sys.flags.ignore_environment == 1,
            "host_process_platform")
    require(os.getuid() == os.geteuid(), "host_process_uid")
    return capture_reviewed_python_snapshot(
        NativeProvider(), FilesystemObserver(), os.getpid(), os.getuid(),
        sys.executable, lambda: time.time_ns() // 1000, selected_pids,
    )


def reviewed_python_pair(filesystem, launcher):
    # The fixed /usr/bin/python3 invocation resolves through this observed Xcode launcher
    # to its distinct framework interpreter. No suffix, PATH or sys-prefix inference.
    require(launcher == REVIEWED_PYTHON_LAUNCHER, "host_process_self_probe")
    try:
        launcher_file = filesystem.executable(launcher)
        native_file = filesystem.executable(REVIEWED_PYTHON_NATIVE)
    except (OSError, ValueError, TypeError):
        raise ClosedError("host_process_self_probe") from None
    for observed, expected in (
        (launcher_file, REVIEWED_PYTHON_LAUNCHER_CANONICAL),
        (native_file, REVIEWED_PYTHON_NATIVE),
    ):
        require(isinstance(observed, ExecutableObservation)
                and observed.canonical_path == expected
                and type(observed.device) is int and observed.device >= 0
                and type(observed.inode) is int and observed.inode > 0
                and observed.uid == 0 and stat.S_ISREG(observed.mode)
                and not stat.S_ISLNK(observed.mode) and observed.mode & 0o022 == 0,
                "host_process_self_probe")
    return launcher_file, native_file


def capture_reviewed_python_snapshot(provider, filesystem, own_pid, uid, launcher, clock_us,
                                     selected_pids=None):
    before = reviewed_python_pair(filesystem, launcher)
    # Generic capture still compares the actual native self PID path against the exact
    # expected root-owned interpreter; the launcher is not substituted for that identity.
    result = capture_snapshot(provider, filesystem, own_pid, uid, REVIEWED_PYTHON_NATIVE,
                              clock_us, selected_pids)
    require(reviewed_python_pair(filesystem, launcher) == before, "host_process_changed")
    return result


def main():
    try:
        selected = snapshot_arguments(sys.argv[1:])
        result = capture_local(selected)
        offset = 0
        while offset < len(result):
            written = os.write(1, result[offset:])
            require(written > 0, "host_process_internal")
            offset += written
        return 0
    except ClosedError as error:
        emit_error(error.code)
        return 1
    except BaseException:
        emit_error("host_process_internal")
        return 1


def emit_error(code):
    try:
        sys.stderr.write(code + "\n")
        sys.stderr.flush()
    except BaseException:
        # A closed/broken error stream must not produce a secondary traceback.
        pass


if __name__ == "__main__":
    sys.exit(main())
