"""Check the kernel identity of an already-connected Unix stream.

The only descriptor this verifier inspects is inherited fd 0. It never
connects, accepts, reads or writes a byte on it. The run owner must bind
the expected PID to its live, exact child throughout the verification;
the PID alone is not an independent process-lifetime witness.
"""

import ctypes
import os
import socket
import stat
import sys


# Darwin <sys/un.h>. Compare both normal and delegated effective PID.
SOL_LOCAL = 0
LOCAL_PEERPID = 0x002
LOCAL_PEEREPID = 0x003


def canonical_decimal(raw, lower, upper):
    if not (raw.isascii() and raw.isdecimal() and 0 < len(raw) <= 10):
        raise ValueError
    value = int(raw)
    if str(value) != raw or not lower <= value <= upper:
        raise ValueError
    return value


def verify(args):
    if sys.platform != "darwin" or args[0::2] != ["--pid", "--uid"] or len(args) != 4:
        return 64
    expected_pid = canonical_decimal(args[1], 2, 2**31 - 1)
    expected_uid = canonical_decimal(args[3], 1, 2**32 - 2)
    if expected_uid != os.geteuid() or not stat.S_ISSOCK(os.fstat(0).st_mode):
        return 65

    # Wrap the inherited descriptor, not a pathname; close only this
    # process's reference at exit. The caller keeps the original open.
    with socket.socket(fileno=0) as peer:
        if peer.family != socket.AF_UNIX or peer.getsockopt(socket.SOL_SOCKET, socket.SO_TYPE) != socket.SOCK_STREAM:
            return 65
        peer.getpeername()
        pid = peer.getsockopt(SOL_LOCAL, LOCAL_PEERPID)
        effective_pid = peer.getsockopt(SOL_LOCAL, LOCAL_PEEREPID)
        getpeereid = ctypes.CDLL(None).getpeereid
        getpeereid.argtypes = [ctypes.c_int, ctypes.POINTER(ctypes.c_uint32), ctypes.POINTER(ctypes.c_uint32)]
        getpeereid.restype = ctypes.c_int
        uid, gid = ctypes.c_uint32(), ctypes.c_uint32()
        if getpeereid(peer.fileno(), ctypes.byref(uid), ctypes.byref(gid)) != 0:
            return 65
        if pid != expected_pid or effective_pid != expected_pid or uid.value != expected_uid:
            return 66
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(verify(sys.argv[1:]))
    except (OSError, ValueError, AttributeError):
        raise SystemExit(65) from None
