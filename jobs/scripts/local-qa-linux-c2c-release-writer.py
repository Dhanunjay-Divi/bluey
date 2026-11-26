"""Root-only, one-shot publication for a locally admitted C2C QA actor.

This module deliberately does not start, stop, or configure a unit.  The
caller is a separately admitted root supervisor which owns both the actor
launcher and the independent observation channel.  An actor ``waiting`` frame
is only a request: every fact used for publication is re-observed below.
"""
from contextlib import ExitStack
import hashlib
import os
from pathlib import Path
import re
import stat
import sys
import time
import types


RELEASE_WRITER_SHA256 = "8aeeba6bb06e37976201ea4b88c8a532ab6dbb8ab4bc56615fb7e8c3ad1d6014"
# The protocol is supplied by the root-owned waiting actor workstream.  Leaving
# this unset is intentionally a refusal: parsing an actor frame locally would
# create a second, potentially divergent release grammar.
PROTOCOL_SHA256 = "850b48cd7e36778d7c1be4d4eaf8f5ead5f00eadd90f0ea18a42db366c5a57ec"
PROTOCOL_NAME = "local-qa-linux-c2c-release-protocol.py"
SPEC_NAME = ".local-qa-linux-c2c-native.json"
ROOT_MARKER = ".bluey-test-run-v2"
GATE_PARENT = Path("/run/bluey-jobs-c2c-gates")
SHA256 = re.compile(r"[0-9a-f]{64}")
MAX_SOURCE = 128 * 1024
MAX_SPEC = 65_536
MAX_PROC = 16_384
NINETY_SECONDS_NS = 90_000_000_000


def _need(value, code):
    if not value:
        raise RuntimeError(code)


def _identity(value):
    return (value.st_dev, value.st_ino, value.st_uid, value.st_gid,
            value.st_mode, value.st_nlink, value.st_size,
            value.st_mtime_ns, value.st_ctime_ns)


def _gate_identity(value):
    """The directory fields which publication itself cannot legitimately change."""
    return (value.st_dev, value.st_ino, value.st_uid, value.st_gid, value.st_mode)


def _regular_private(value, maximum):
    return (stat.S_ISREG(value.st_mode) and value.st_uid == 0
            and value.st_nlink == 1 and stat.S_IMODE(value.st_mode) in (0o600, 0o644)
            and 0 < value.st_size <= maximum)


def _read_pinned_source(name, expected, maximum=MAX_SOURCE):
    """Read a sibling once, bind its descriptor and execute retained bytes."""
    _need(isinstance(expected, str) and SHA256.fullmatch(expected), "c2c_release_helper_pin")
    path = Path(__file__).resolve().with_name(name)
    _need(path.parent == Path(__file__).resolve().parent and path.is_absolute(), "c2c_release_helper_path")
    flags = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
    fd = os.open(path, flags)
    try:
        before = os.fstat(fd)
        _need(_regular_private(before, maximum), "c2c_release_helper_metadata")
        chunks, total = [], 0
        while True:
            chunk = os.read(fd, min(65536, maximum + 1 - total))
            if not chunk:
                break
            total += len(chunk)
            _need(total <= maximum, "c2c_release_helper_size")
            chunks.append(chunk)
        body = b"".join(chunks)
        named = os.stat(path, follow_symlinks=False)
        _need(len(body) == before.st_size and _identity(before) == _identity(os.fstat(fd))
              == _identity(named) and hashlib.sha256(body).hexdigest() == expected,
              "c2c_release_helper_changed")
    finally:
        os.close(fd)
    module_name = "_verified_" + name.replace("-", "_").replace(".", "_")
    module = types.ModuleType(module_name)
    module.__file__ = str(path)
    # Avoid a second path open/import while preserving normal module globals.
    exec(compile(body, str(path), "exec"), module.__dict__)
    return module


def _protocol():
    _need(PROTOCOL_SHA256 is not None, "c2c_release_protocol_unpinned")
    module = _read_pinned_source(PROTOCOL_NAME, PROTOCOL_SHA256)
    _need(callable(getattr(module, "parse_waiting", None))
          and callable(getattr(module, "make_release", None)), "c2c_release_protocol_shape")
    return module


def _proc_text(path, maximum=MAX_PROC):
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
    try:
        data = bytearray()
        while True:
            part = os.read(fd, min(4096, maximum + 1 - len(data)))
            if not part:
                break
            data.extend(part)
            _need(len(data) <= maximum, "c2c_release_proc_size")
        _need(data and b"\0" not in data, "c2c_release_proc_bytes")
        return bytes(data).decode("ascii")
    finally:
        os.close(fd)


def _proc_stat(pid):
    raw = _proc_text("/proc/{}/stat".format(pid), 4096).rstrip("\n")
    right = raw.rfind(")")
    _need(raw.startswith("{} (".format(pid)) and right > len(str(pid)) + 2 and right + 2 < len(raw),
          "c2c_release_proc_stat")
    fields = raw[right + 2:].split()
    # /proc/<pid>/stat field 22 is index 19 after the state field.
    _need(len(fields) > 19 and fields[0].isalpha() and fields[2].isdigit() and fields[19].isdigit(),
          "c2c_release_proc_stat")
    _need(fields[0] != "Z", "c2c_release_proc_zombie")
    return int(fields[19]), int(fields[2])


def _root_marker_binding(bytes_value, waiting):
    """Validate the launcher/actor linkage in the actor-owned root marker."""
    try:
        text = bytes_value.decode("ascii")
    except UnicodeDecodeError:
        raise RuntimeError("c2c_release_marker")
    lines = text.split("\n")
    _need(text.endswith("\n") and len(lines) == 10 and lines[-1] == "" and lines[0] == "magic=bluey-test-run-v2",
          "c2c_release_marker")
    pairs = []
    for line in lines[1:9]:
        key, sep, value = line.partition("=")
        _need(bool(sep) and key and value and "=" not in value, "c2c_release_marker")
        pairs.append((key, value))
    names = [name for name, _ in pairs]
    _need(names == ["run_root", "run_name", "owner_uid", "owner_pid", "process_group_id",
                    "launcher_group_id", "created_epoch", "state"], "c2c_release_marker")
    value = dict(pairs)
    root_name = Path(waiting["root"]).name
    segments = root_name.split(".")
    _need(len(segments) == 4 and segments[0] == "run" and re.fullmatch(r"[1-9][0-9]{0,18}", segments[1])
          and segments[2] == str(waiting["mainPid"]), "c2c_release_marker")
    _need(value["run_root"] == waiting["root"] and value["run_name"] == root_name
          and value["owner_uid"] == str(waiting["actorUid"])
          and value["owner_pid"] == str(waiting["mainPid"])
          and value["process_group_id"] == str(_proc_stat(waiting["actorPid"])[1])
          and value["launcher_group_id"] == str(_proc_stat(waiting["mainPid"])[1])
          and value["launcher_group_id"] != value["process_group_id"]
          and re.fullmatch(r"[1-9][0-9]{0,18}", value["launcher_group_id"])
          and value["created_epoch"] == segments[1]
          and value["state"] == "running", "c2c_release_marker")


def _network_is_loopback_only(pid):
    base = Path("/proc/{}/net".format(pid))
    dev = _proc_text(base / "dev", 4096).splitlines()
    _need(len(dev) >= 3 and "Inter-|" in dev[0], "c2c_release_net_shape")
    names = []
    for line in dev[2:]:
        name, colon, rest = line.partition(":")
        _need(colon and rest.strip(), "c2c_release_net_shape")
        names.append(name.strip())
    _need(names == ["lo"], "c2c_release_interfaces")
    routes = _proc_text(base / "route", 4096).splitlines()
    _need(routes and routes[0].split()[:3] == ["Iface", "Destination", "Gateway"], "c2c_release_routes")
    for line in routes[1:]:
        fields = line.split()
        _need(len(fields) == 11 and fields[0] == "lo" and fields[1] != "00000000", "c2c_release_route")
    ipv6 = _proc_text(base / "ipv6_route", 8192).splitlines()
    for line in ipv6:
        fields = line.split()
        _need(len(fields) == 10 and fields[-1] == "lo" and re.fullmatch(r"[0-9a-f]{8}", fields[-2]),
              "c2c_release_ipv6_route")
        is_default = fields[0] == "0" * 32 and fields[1] == "00"
        # A private net namespace can retain Linux's loopback reject default.
        _need(not is_default or int(fields[-2], 16) & 0x200, "c2c_release_ipv6_default")
    return True


def _open_actor_root(waiting):
    """Descend below the intentional proc-root magic link without following children."""
    # /proc/<pid>/root is intentionally a procfs magic link; every workload
    # path component below it is opened through directory descriptors with
    # O_NOFOLLOW.
    fd = os.open("/proc/{}/root".format(waiting["actorPid"]),
                 os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NONBLOCK)
    try:
        for component in waiting["root"].lstrip("/").split("/"):
            _need(component not in ("", ".", ".."), "c2c_release_root_component")
            child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC
                            | os.O_NONBLOCK | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
        return fd
    except Exception:
        os.close(fd)
        raise


def _read_actor_leaf(root_fd, name, maximum):
    _need(name and "/" not in name, "c2c_release_actor_leaf")
    fd = os.open(name, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=root_fd)
    try:
        before = os.fstat(fd)
        _need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and 0 < before.st_size <= maximum,
              "c2c_release_file_metadata")
        data = bytearray()
        while True:
            part = os.read(fd, min(65536, maximum + 1 - len(data)))
            if not part:
                break
            data.extend(part)
            _need(len(data) <= maximum, "c2c_release_file_size")
        named = os.stat(name, dir_fd=root_fd, follow_symlinks=False)
        _need(len(data) == before.st_size and _identity(before) == _identity(os.fstat(fd))
              == _identity(named), "c2c_release_file_changed")
        return bytes(data), before
    finally:
        os.close(fd)


def _read_actor_inputs(waiting, spec_sha256):
    """Bind the actor-visible root, marker and native spec without trusting paths."""
    root_fd = _open_actor_root(waiting)
    try:
        run = os.fstat(root_fd)
        _need(stat.S_ISDIR(run.st_mode) and run.st_uid == waiting["actorUid"]
              and stat.S_IMODE(run.st_mode) == 0o700
              and run.st_dev == waiting["rootDevice"] and run.st_ino == waiting["rootInode"],
              "c2c_release_actor_root")
        marker, marker_info = _read_actor_leaf(root_fd, ROOT_MARKER, 8192)
        _need(marker_info.st_uid == waiting["actorUid"] and stat.S_IMODE(marker_info.st_mode) == 0o600
              and hashlib.sha256(marker).hexdigest() == waiting["markerSha256"], "c2c_release_marker")
        _root_marker_binding(marker, waiting)
        spec, spec_info = _read_actor_leaf(root_fd, SPEC_NAME, MAX_SPEC)
        _need(spec_info.st_uid == waiting["actorUid"] and stat.S_IMODE(spec_info.st_mode) == 0o600
              and hashlib.sha256(spec).hexdigest() == spec_sha256, "c2c_release_spec")
        return _identity(run), _identity(marker_info), _identity(spec_info)
    finally:
        os.close(root_fd)


def _observed_matches(waiting, observed):
    keys = set(waiting) - {"schema", "nonce", "waitingNs"}
    _need(type(observed) is dict and set(observed) == keys
          and all(type(observed[key]) is type(waiting[key]) and observed[key] == waiting[key] for key in keys),
          "c2c_release_observation_mismatch")


def _root_recheck(waiting, spec_sha256, initial):
    _need(_proc_text("/proc/sys/kernel/random/boot_id", 128).strip().replace("-", "") == waiting["bootId"],
          "c2c_release_boot")
    for role, pid, start in (("main", waiting["mainPid"], waiting["mainStartTime"]),
                             ("actor", waiting["actorPid"], waiting["actorStartTime"])):
        _need(_proc_stat(pid)[0] == start, "c2c_release_{}_birth".format(role))
        proc = Path("/proc/{}".format(pid))
        status = _proc_text(proc / "status", 8192)
        uid_line = [line for line in status.splitlines() if line.startswith("Uid:\t")]
        _need(len(uid_line) == 1, "c2c_release_{}_uid".format(role))
        uids = uid_line[0].split()[1:]
        expected_uid = str(waiting["actorUid"])
        _need(uids == [expected_uid] * 4, "c2c_release_{}_uid".format(role))
        _need(_proc_text(proc / "cgroup", 4096) == "0::{}\n".format(waiting["unitCgroup"]),
              "c2c_release_{}_cgroup".format(role))
        _need(os.readlink(proc / "ns/net") == waiting["networkNamespace"],
              "c2c_release_{}_netns".format(role))
    _need(_network_is_loopback_only(waiting["actorPid"]), "c2c_release_network")
    _need(os.stat("/proc/self/ns/net").st_ino != os.stat("/proc/{}/ns/net".format(waiting["actorPid"])).st_ino,
          "c2c_release_observer_namespace")
    now = _read_actor_inputs(waiting, spec_sha256)
    _need(now == initial, "c2c_release_actor_inputs_changed")


def _open_gate(run):
    _need(GATE_PARENT.is_dir(), "c2c_release_gate_parent")
    parent = os.open(GATE_PARENT, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW)
    try:
        parent_info = os.fstat(parent)
        _need(parent_info.st_uid == 0 and stat.S_IMODE(parent_info.st_mode) in (0o755, 0o750),
              "c2c_release_gate_parent")
        fd = os.open(run, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW, dir_fd=parent)
    finally:
        os.close(parent)
    try:
        info = os.fstat(fd)
        _need(stat.S_ISDIR(info.st_mode) and info.st_uid == 0 and stat.S_IMODE(info.st_mode) == 0o755
              and info.st_nlink >= 2, "c2c_release_gate")
        return fd, _gate_identity(info)
    except BaseException:
        os.close(fd)
        raise


def issue_release(waiting_raw, spec_sha256, *, independently_observe, verify_admitted_artifacts):
    """Publish exactly once after root re-observation of an actor request.

    ``independently_observe(waiting)`` returns the exact observation map
    (waiting minus schema/nonce/waitingNs). ``verify_admitted_artifacts`` is a
    root-manager predicate invoked with ``(waiting, spec_sha256)`` and must
    return exactly True. Neither callback can supply publication fields.
    """
    _need(sys.platform == "linux" and os.uname().machine == "x86_64"
          and os.getuid() == os.geteuid() == 0, "c2c_release_linux_root")
    _need(callable(independently_observe) and callable(verify_admitted_artifacts),
          "c2c_release_callbacks")
    protocol = _protocol()
    _need(isinstance(waiting_raw, bytes) and 0 < len(waiting_raw) <= 8192,
          "c2c_release_waiting_bytes")
    waiting = protocol.parse_waiting(waiting_raw)
    _need(type(spec_sha256) is str and SHA256.fullmatch(spec_sha256), "c2c_release_spec_sha")
    clock = lambda: time.clock_gettime_ns(time.CLOCK_MONOTONIC)
    started = clock()
    deadline = waiting["waitingNs"] + NINETY_SECONDS_NS
    _need(started >= waiting["waitingNs"] and started < deadline, "c2c_release_deadline")
    writer = _read_pinned_source("receipt-browser-release-writer.py", RELEASE_WRITER_SHA256)
    _need(callable(getattr(writer, "publish_at", None)), "c2c_release_writer_shape")
    observed = independently_observe(dict(waiting))
    _observed_matches(waiting, observed)
    _need(verify_admitted_artifacts(dict(waiting), spec_sha256) is True, "c2c_release_artifacts")
    initial = _read_actor_inputs(waiting, spec_sha256)
    with ExitStack() as stack:
        gate, gate_identity = _open_gate(waiting["run"])
        stack.callback(os.close, gate)

        def current():
            _need(clock() < deadline, "c2c_release_deadline")
            gate_info = os.fstat(gate)
            named = os.stat(GATE_PARENT / waiting["run"], follow_symlinks=False)
            _need(_gate_identity(gate_info) == gate_identity == _gate_identity(named),
                  "c2c_release_gate_changed")
            _observed_matches(waiting, independently_observe(dict(waiting)))
            _need(verify_admitted_artifacts(dict(waiting), spec_sha256) is True, "c2c_release_artifacts")
            _root_recheck(waiting, spec_sha256, initial)

        current()
        issued = clock()
        body = protocol.make_release(waiting_raw, observed, spec_sha256, issued)
        _need(isinstance(body, bytes) and 0 < len(body) <= 8192, "c2c_release_body")
        receipt = writer.publish_at(gate, body, current, owner=0, deadline_ns=deadline, clock=clock)
    return {**receipt, "issuedNs": issued, "runtimeQualified": False, "productionQualified": False}
