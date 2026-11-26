"""Append-only owner hold for one synthetic mock-receipt QA unit launch.

This is a local durable fence only.  It neither provisions nor authenticates a
guest, starts a unit, opens a network connection, or releases the underlying
guest reservation.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat


ARMED = "launch-armed.json"
COMPLETE = "launch-complete.json"
LIMIT = 4096
RUN = re.compile(r"^[0-9a-f]{32}$")
SHA = re.compile(r"^[0-9a-f]{64}$")
PARENT = re.compile(r"^/tmp/bluey-test-runs-[0-9a-f]{12}$")
ZERO_KEYS = {"unitStopped", "actorProcesses", "namespaceProcesses"}
RD = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK
DIR = RD | os.O_DIRECTORY


def _need(value, label):
    if not value:
        raise ValueError("signup_mock_launch_hold_refused:" + label)


def _wire(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")


def _sha(raw):
    return hashlib.sha256(raw).hexdigest()


def _stamp(item):
    return (item.st_dev, item.st_ino, item.st_mode, item.st_uid, item.st_gid, item.st_nlink,
            item.st_size, item.st_mtime_ns, item.st_ctime_ns)


def _directory_identity(item):
    return (item.st_dev, item.st_ino, item.st_mode, item.st_uid, item.st_gid)


def _read(fd, limit):
    before = os.fstat(fd)
    _need(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid() and before.st_nlink == 1
          and stat.S_IMODE(before.st_mode) == 0o600 and 0 < before.st_size <= limit, "record_metadata")
    parts, remaining = [], before.st_size + 1
    while remaining:
        chunk = os.read(fd, min(65536, remaining))
        if not chunk:
            break
        parts.append(chunk)
        remaining -= len(chunk)
    raw = b"".join(parts)
    _need(len(raw) == before.st_size and _stamp(before) == _stamp(os.fstat(fd)), "record_changed")
    return raw, before


def _read_named(parent, name, raw, expected_stamp):
    fd = os.open(name, RD, dir_fd=parent)
    try:
        found, info = _read(fd, LIMIT)
        named = os.stat(name, dir_fd=parent, follow_symlinks=False)
        _need(found == raw and _stamp(info) == _stamp(named) == expected_stamp, "record_changed")
    finally:
        os.close(fd)


def _write_once(parent, name, raw):
    _need(0 < len(raw) <= LIMIT, "record_size")
    try:
        fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600, dir_fd=parent)
    except FileExistsError as error:
        raise ValueError("signup_mock_launch_hold_refused:already_recorded") from error
    try:
        view = memoryview(raw)
        while view:
            written = os.write(fd, view)
            _need(written > 0, "record_write")
            view = view[written:]
        os.fchmod(fd, 0o600)
        os.fsync(fd)
        info = os.fstat(fd)
        _need(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_nlink == 1
              and stat.S_IMODE(info.st_mode) == 0o600 and info.st_size == len(raw), "record_writeback")
    finally:
        os.close(fd)
    os.fsync(parent)
    _read_named(parent, name, raw, _stamp(info))
    return _stamp(info)


def _actor(value):
    _need(type(value) is dict and set(value) == {"run", "unit", "uid", "gid", "runParent"}
          and type(value["run"]) is str and RUN.fullmatch(value["run"]) is not None and value["run"] != "0" * 32
          and value["unit"] == "bluey-jobs-qa-" + value["run"] + ".service"
          and type(value["uid"]) is int and 0 < value["uid"] < 2**31
          and type(value["gid"]) is int and 0 < value["gid"] < 2**31
          and type(value["runParent"]) is str and PARENT.fullmatch(value["runParent"]) is not None, "actor")
    return dict(value)


def _reservation(value):
    _need(type(value) is dict and set(value) == {"reservationSha256", "held"}
          and type(value["reservationSha256"]) is str and SHA.fullmatch(value["reservationSha256"]) is not None
          and value["reservationSha256"] != "0" * 64 and callable(value["held"]), "reservation")
    return {"reservationSha256": value["reservationSha256"], "held": value["held"]}


def _zero(proof):
    _need(type(proof) is dict and set(proof) == ZERO_KEYS and proof["unitStopped"] is True
          and all(type(proof[key]) is int and proof[key] == 0 for key in ("actorProcesses", "namespaceProcesses")),
          "zero_proof")
    return dict(proof)


def make_reserve_launch(directory, directory_identity, reservation, actor, *, observe_zero):
    """Return the exact ``reserve_launch(actor)`` callback expected by launch.py.

    ``directory`` must already exist and be private to the owner.  Its identity
    is held across both records; failed or interrupted attempts intentionally
    retain the armed leaf and cannot be retried in that directory.
    """
    _need(isinstance(directory, Path) and directory.is_absolute() and directory.resolve(strict=True) == directory
          and type(directory_identity) is tuple and len(directory_identity) == 2
          and all(type(value) is int and value >= 0 for value in directory_identity)
          and callable(observe_zero), "directory")
    reservation = _reservation(reservation)
    actor = _actor(actor)
    owner = os.getuid()

    def reserve_launch(projected):
        _need(_actor(projected) == actor, "actor_binding")
        _need(reservation["held"]() is True, "reservation_before")
        parent = os.open(directory, DIR)
        try:
            initial = os.fstat(parent)
            _need(stat.S_ISDIR(initial.st_mode) and initial.st_uid == owner and stat.S_IMODE(initial.st_mode) == 0o700
                  and (initial.st_dev, initial.st_ino) == directory_identity
                  and _directory_identity(initial) == _directory_identity(directory.lstat()),
                  "directory_identity")
            for name in (ARMED, COMPLETE):
                try:
                    os.stat(name, dir_fd=parent, follow_symlinks=False)
                except FileNotFoundError:
                    continue
                raise ValueError("signup_mock_launch_hold_refused:already_recorded")
            armed = {"schema": "bluey-jobs-signup-mock-receipt-launch-hold-v1", "state": "armed",
                     "reservationSha256": reservation["reservationSha256"], "actor": actor}
            armed_raw = _wire(armed)
            armed_stamp = _write_once(parent, ARMED, armed_raw)
        finally:
            os.close(parent)

        def current():
            _need(reservation["held"]() is True, "reservation_lost")
            fd = os.open(directory, DIR)
            try:
                _need(_directory_identity(initial) == _directory_identity(os.fstat(fd))
                      == _directory_identity(directory.lstat()), "directory_changed")
                _read_named(fd, ARMED, armed_raw, armed_stamp)
            finally:
                os.close(fd)
            return True

        current()
        completion_started = False

        def held():
            try:
                return current() is True
            except (OSError, ValueError):
                return False

        def complete(proof):
            nonlocal completion_started
            _need(completion_started is False, "completion_replayed")
            completion_started = True
            zero = _zero(proof)
            _need(current() is True and observe_zero(dict(zero)) is True and current() is True, "completion_observation")
            completed = {"schema": "bluey-jobs-signup-mock-receipt-launch-complete-v1", "state": "complete",
                         "armedSha256": _sha(armed_raw), "teardown": zero}
            complete_raw = _wire(completed)
            parent = os.open(directory, DIR)
            try:
                _need(_directory_identity(initial) == _directory_identity(os.fstat(parent))
                      == _directory_identity(directory.lstat()), "directory_changed")
                complete_stamp = _write_once(parent, COMPLETE, complete_raw)
                _read_named(parent, COMPLETE, complete_raw, complete_stamp)
            finally:
                os.close(parent)
            current()
            return True

        return {"reservationSha256": reservation["reservationSha256"], "held": held, "complete": complete}

    return reserve_launch
