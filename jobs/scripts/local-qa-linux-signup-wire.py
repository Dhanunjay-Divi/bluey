"""Verify the exact synthetic signup wire and hand Linux callers a sealed reader.

Neither observation mode executes or imports anything from the packet. A future
workload owner still has to provide a live authorized lease and the normal
decoder, package, actor, sandbox, and closure checks.
"""

from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
import stat
import sys


# Exact current compiler and portal candidate, verified offline through EOF.
WIRE_BYTES = 490_699_941
WIRE_SHA256 = "ca2ef640ece49304d71cac545d990a4f1e2431691b0e823ba7b41f7984bf5d92"
CHUNK = 128 << 10
PACKET_FD = 3
NEEDED_SEALS = ("F_SEAL_WRITE", "F_SEAL_GROW", "F_SEAL_SHRINK", "F_SEAL_SEAL")


class WireRefused(ValueError):
    """An exact size, digest, lifecycle, or Linux file-seal check failed."""


def require(condition, reason):
    if not condition:
        raise WireRefused(reason)


def _verify_wire(source, size, digest):
    """Read one exact blob through EOF. Input is data, never executable code."""
    left = size
    hasher = hashlib.sha256()
    while left:
        limit = min(left, CHUNK)
        block = source.read(limit)
        require(type(block) is bytes and 0 < len(block) <= limit, "size")
        hasher.update(block)
        left -= len(block)
    require(source.read(1) == b"", "trailing_bytes")
    require(hasher.hexdigest() == digest, "digest")


def verify_wire(source):
    _verify_wire(source, WIRE_BYTES, WIRE_SHA256)


def sealed_input(fd, size):
    require(sys.platform == "linux", "not_linux")
    require(all(hasattr(fcntl, name) for name in (*NEEDED_SEALS, "F_GET_SEALS")), "seals_unavailable")
    required_mask = 0
    for name in NEEDED_SEALS:
        required_mask |= getattr(fcntl, name)
    require(fcntl.fcntl(fd, fcntl.F_GET_SEALS) & required_mask == required_mask, "seals")
    row = os.fstat(fd)
    require(stat.S_ISREG(row.st_mode) and row.st_nlink == 0 and row.st_size == size, "sealed_metadata")


class _Reader:
    """Use positional reads so the sender cannot change the receiver's offset."""

    def __init__(self, fd, size, held):
        self._fd = fd
        self._limit = size
        self._held = held
        self._offset = 0
        self._valid = True
        self._eof = False

    def read(self, size):
        require(self._valid, "reader_closed")
        try:
            require(type(size) is int and 0 < size, "read_size")
            require(self._held() is True, "owner_lost")
            limit = min(size, CHUNK, self._limit - self._offset)
            if not limit:
                self._eof = True
                return b""
            block = os.pread(self._fd, limit, self._offset)
            require(len(block) == limit, "sealed_read")
            require(self._held() is True, "owner_lost")
        except BaseException:
            self._valid = False
            raise
        self._offset += len(block)
        return block

    def revoke(self):
        self._valid = False


def hold_packet(fd, held):
    """Return a fresh sequential reader from the same immutable descriptor.

    The supplied lease checkpoint is only as authoritative as its caller.
    This function borrows the input descriptor; only its duplicate is closed.
    """
    return _hold_packet(fd, held, WIRE_BYTES, WIRE_SHA256)


@contextmanager
def _verified_descriptor(fd, held, size, digest):
    require(type(fd) is int and fd >= 0 and type(size) is int and 0 < size <= WIRE_BYTES, "input")
    require(callable(held) and held() is True and sys.platform == "linux", "host_or_owner")
    owned = os.dup(fd)
    try:
        sealed_input(owned, size)
        checked = _Reader(owned, size, held)
        try:
            _verify_wire(checked, size, digest)
        finally:
            checked.revoke()
        require(held() is True, "owner_lost")
        yield owned
    finally:
        os.close(owned)


@contextmanager
def _hold_packet(fd, held, size, digest):
    with _verified_descriptor(fd, held, size, digest) as owned:
        reader = _Reader(owned, size, held)
        try:
            yield reader
            require(reader._valid and held() is True, "owner_or_read_failed")
            require(reader._eof and reader._offset == size, "consumer_eof")
        finally:
            reader.revoke()


def main():
    mode = sys.argv[1:]
    if mode not in (["--verify-stream"], ["--observe-sealed-fd"]) or not (
            __debug__ and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode):
        return 64
    sealed = mode == ["--observe-sealed-fd"]
    try:
        if sealed:
            # Observation only: this does not check or claim owner authority.
            with _verified_descriptor(PACKET_FD, lambda: True, WIRE_BYTES, WIRE_SHA256):
                pass
        else:
            verify_wire(sys.stdin.buffer)
    except (OSError, WireRefused):
        print('{"event":"jobs_native_qa_wire_refused"}', file=sys.stderr)
        return 69
    print(json.dumps({"schema": "bluey-jobs-native-qa-wire-observation-v1", "bytes": WIRE_BYTES,
                      "sha256": WIRE_SHA256, "osSealedDescriptorObserved": sealed,
                      "ownerObserved": False, "executionAdmitted": False, "productionQualified": False},
                     sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
