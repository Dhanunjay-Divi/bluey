"""Exercise the Darwin verifier on real, launcher-owned Unix descriptors."""

import os
from pathlib import Path
import select
import socket
import subprocess
import sys


PYTHON = sys.executable
HELPER = Path(__file__).with_name("local-qa-darwin-peer.py").resolve(strict=True)
SERVER = """
import os, socket, sys
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
    listener.bind(sys.argv[1])
    os.chmod(sys.argv[1], 0o600)
    listener.listen(1)
    print("bound", flush=True)
    if sys.stdin.buffer.read(1) != b"a":
        raise SystemExit(71)
    conn, _ = listener.accept()
    with conn:
        conn.settimeout(5)
        conn.sendall(b"z")
        print("accepted", flush=True)
        if conn.recv(1) != b"x":
            raise SystemExit(70)
"""


def observe(pid, uid, stdin, expected):
    child = subprocess.run(
        [PYTHON, "-I", "-S", "-B", str(HELPER), "--pid", str(pid), "--uid", str(uid)],
        stdin=stdin,
        capture_output=True,
        timeout=5,
    )
    assert (child.returncode, child.stdout, child.stderr) == (expected, b"", b"")


def ready(server, expected):
    assert select.select([server.stdout], [], [], 5)[0]
    assert server.stdout.readline() == expected


def main():
    assert __debug__ and sys.platform == "darwin"
    root = Path(os.environ["BLUEY_TEST_RUN_ROOT"])
    assert root == root.resolve(strict=True)
    endpoint = root / "peer"
    assert not os.path.lexists(endpoint)
    with subprocess.Popen(
        [PYTHON, "-I", "-S", "-B", "-c", SERVER, str(endpoint)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ) as server:
        try:
            ready(server, b"bound\n")
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as held:
                held.settimeout(5)
                held.connect(str(endpoint))
                # The kernel binds the listener peer even before userspace accept.
                observe(server.pid, os.geteuid(), held, 0)
                server.stdin.write(b"a")
                server.stdin.flush()
                ready(server, b"accepted\n")
                observe(server.pid, os.geteuid(), held, 0)
                observe(os.getpid(), os.geteuid(), held, 66)
                observe(server.pid, os.geteuid() + 1, held, 65)
                observe(server.pid, os.geteuid(), subprocess.DEVNULL, 65)
                # The helper has neither consumed the marker nor written
                # anything to the backend. The original stream stays open.
                assert held.recv(1) == b"z"
                held.sendall(b"x")
            output, error = server.communicate(timeout=5)
            assert (server.returncode, output, error) == (0, b"", b"")
        finally:
            if server.poll() is None:
                server.terminate()
                server.wait(timeout=5)
    print("Darwin Unix peer verified on inherited stream; wrong PID, wrong UID and non-socket refused")


if __name__ == "__main__":
    main()
