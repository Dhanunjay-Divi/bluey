"""Host owner's write-once reservation for one disposable Jobs QA guest.

The owner supplies a previously provisioned private persistent slot and verifies
the exact stopped instance. This is the host storage primitive, not the guest
manager's in-guest callback contract. No CLI, guest control, restart adoption,
record replacement, release or expiry is provided. Workload zero is not VM closure.
"""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import stat


RESERVATION = "reservation.json"
WORKLOAD = "workload-closed.json"
LIMIT = 4096
FILE = os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
DIR = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_DIRECTORY
ZERO = {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0}


def need(ok, label):
    if not ok:
        raise ValueError("c2c_host_reservation_refused:" + label)


def match(pattern, value):
    return type(value) is str and re.fullmatch(pattern, value) is not None


def digest(value, *, width=64):
    return match(f"[0-9a-f]{{{width}}}", value) and value != "0" * width


def encode(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")


def device_inode(device, inode):
    return type(device) is int and 0 <= device < 2**64 and type(inode) is int and 0 < inode < 2**64


def binding_bytes(binding):
    need(type(binding) is dict and binding.keys() == {
        "instance", "instanceIdentity", "diskIdentity", "configSha256", "imageSha256", "packetSha256",
        "sourceTree", "sourceInventorySha256", "projection",
    } and match(r"[a-z0-9][a-z0-9-]{0,62}", binding["instance"]), "binding")
    for key in ("instanceIdentity", "diskIdentity"):
        pair = binding[key]
        need(type(pair) is list and len(pair) == 2 and device_inode(*pair), "instance_identity")
    need(binding["instanceIdentity"] != binding["diskIdentity"], "instance_identity")
    need(digest(binding["sourceTree"], width=40)
         and all(digest(binding[k]) for k in ("configSha256", "imageSha256", "packetSha256", "sourceInventorySha256")),
         "binding_digest")
    plan = binding["projection"]
    need(type(plan) is dict and plan.keys() == {"run", "unit", "uid", "gid", "runParent"}
         and digest(plan["run"], width=32) and plan["unit"] == f"bluey-jobs-qa-{plan['run']}.service"
         and all(type(plan[k]) is int and 0 < plan[k] < 2**31 for k in ("uid", "gid"))
         and match(r"/tmp/bluey-test-runs-[0-9a-f]{12}", plan["runParent"]), "projection")
    return encode(binding)


def dir_check(path, fd, expected):
    before, named = os.fstat(fd), path.lstat()
    for info in (before, named):
        need(stat.S_ISDIR(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o700 and info.st_uid == os.getuid()
             and (info.st_dev, info.st_ino) == expected, "directory_changed")
    need(path.resolve(strict=True) == path and os.getuid() == os.geteuid(), "directory_path")


@contextmanager
def open_slot(path, expected):
    need(isinstance(path, Path) and path.is_absolute() and path.resolve(strict=True) == path, "directory_path")
    fd = os.open(path, DIR)
    try:
        dir_check(path, fd, expected)
        yield fd
    finally:
        os.close(fd)


def file_stamp(info):
    need(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_nlink == 1
         and stat.S_IMODE(info.st_mode) == 0o600 and 0 < info.st_size <= LIMIT, "record_metadata")
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid, info.st_size,
            info.st_mtime_ns, info.st_ctime_ns)


def read_record(directory, name, raw, *, exact=None):
    with os.fdopen(os.open(name, os.O_RDONLY | FILE, dir_fd=directory), "rb") as reader:
        current = file_stamp(os.fstat(reader.fileno()))
        need((exact is None or exact == current) and reader.read(LIMIT + 1) == raw
             and file_stamp(os.fstat(reader.fileno())) == current
             and file_stamp(os.stat(name, dir_fd=directory, follow_symlinks=False)) == current, "record_changed")
        return current


def write_record(directory, name, raw):
    need(0 < len(raw) <= LIMIT, "record_size")
    try:
        fd = os.open(name, os.O_WRONLY | FILE | os.O_CREAT | os.O_EXCL, 0o600, dir_fd=directory)
    except FileExistsError as error:
        raise ValueError("c2c_host_reservation_refused:already_reserved") from error
    # Any interruption after creation leaves an occupied slot. Even a partial
    # record is retained; no following invocation can reconstruct its history.
    with os.fdopen(fd, "wb") as writer:
        need(writer.write(raw) == len(raw), "short_write")
        writer.flush()
        os.fsync(writer.fileno())
        current = file_stamp(os.fstat(writer.fileno()))
    os.fsync(directory)
    return read_record(directory, name, raw, exact=current)


def reserve_guest(directory, directory_identity, binding, *, stopped, verify_instance):
    """Create an exclusive reservation before the owner starts its guest.

    `stopped` must independently verify the selected, stopped instance and
    exact binding under the owner lease. `verify_instance` independently binds
    the same instance/disk/config and current owner lease without requiring the
    guest to be stopped. The owner must pin one persistent slot per guest across
    invocations. This helper creates neither directories nor reusable handles;
    even the original PID cannot adopt an existing reservation on restart.
    """
    need(type(directory_identity) is tuple and len(directory_identity) == 2
         and device_inode(*directory_identity) and callable(stopped) and callable(verify_instance), "directory_identity")
    frozen = binding_bytes(binding)
    owner_pid = os.getpid()
    raw = encode({"schema": "bluey-jobs-c2c-host-reservation-v1", "state": "reserved", "generation": os.urandom(16).hex(),
                  "ownerPid": owner_pid, "slotIdentity": directory_identity, "binding": json.loads(frozen)})
    sha = hashlib.sha256(raw).hexdigest()

    with open_slot(directory, directory_identity) as slot:
        for name in (RESERVATION, WORKLOAD):
            try:
                os.stat(name, dir_fd=slot, follow_symlinks=False)
            except FileNotFoundError:
                continue
            raise ValueError("c2c_host_reservation_refused:already_reserved")
        need(verify_instance(json.loads(frozen)) is True and stopped(json.loads(frozen)) is True, "stopped_guest")
        dir_check(directory, slot, directory_identity)
        stamp = write_record(slot, RESERVATION, raw)
        need(stopped(json.loads(frozen)) is True and verify_instance(json.loads(frozen)) is True, "stopped_guest_changed")
        dir_check(directory, slot, directory_identity)
        read_record(slot, RESERVATION, raw, exact=stamp)

    def verify(slot):
        need(os.getpid() == owner_pid and verify_instance(json.loads(frozen)) is True, "owner_or_instance")
        read_record(slot, RESERVATION, raw, exact=stamp)
        dir_check(directory, slot, directory_identity)

    def held():
        try:
            with open_slot(directory, directory_identity) as slot:
                verify(slot)
        except (OSError, ValueError):
            return False
        return True

    def note_workload(proof):
        current = json.loads(frozen)
        need(type(proof) is dict and proof.keys() == {"reservationSha256", "sourceTree", "sourceInventorySha256",
                                                     "resultSha256", "teardown"}
             and proof["reservationSha256"] == sha and proof["sourceTree"] == current["sourceTree"]
             and proof["sourceInventorySha256"] == current["sourceInventorySha256"] and digest(proof["resultSha256"]),
             "workload_proof_binding")
        zero = proof["teardown"]
        need(type(zero) is dict and zero.keys() == ZERO.keys() and zero["unitStopped"] is True
             and all(type(zero[k]) is int and zero[k] == 0 for k in ("actorProcesses", "namespaceProcesses")),
             "workload_zero_proof")
        with open_slot(directory, directory_identity) as slot:
            verify(slot)
            info = encode({"schema": "bluey-jobs-c2c-host-workload-note-v1", "guestReusable": False, "proof": proof})
            write_record(slot, WORKLOAD, info)
            verify(slot)
        return True

    return {"reservationSha256": sha, "held": held, "note_workload": note_workload}
