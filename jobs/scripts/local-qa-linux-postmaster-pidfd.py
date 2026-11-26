#!/usr/bin/env python3
"""Closed pidfd lifecycle helper for one owned Linux local-QA postmaster.

Invoke only as: pinned-python -I -B /absolute/path/to/this/file, with no
arguments and anonymous pipe/Unix-stream stdin/stdout.  The first frame is one
strict JSON line; thereafter the only frames are stop and dispose.  Diagnostics
are intentionally never written to the protocol stream.
"""
import errno
import hashlib
import json
import os
import re
import select
import signal
import socket
import stat
import struct
import sys
import time

MAX_FRAME = 4096
MAX_PROC = 65536
MAX_BINARY = 256 * 1024 * 1024
IDLE_SECONDS = 30 * 60
ROOT_RE = re.compile(r"^/tmp/bluey-test-runs-[0-9a-f]{12}/run\.[1-9][0-9]{0,19}\.[1-9][0-9]{0,9}\.[A-Za-z0-9]{1,64}$")
U64_RE = re.compile(r"^[1-9][0-9]{0,19}$")
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
NET_RE = re.compile(r"^net:\[[1-9][0-9]{0,19}\]$")
CAPSULE_POSTGRES = re.compile(
    r"/run/bluey-jobs-c2c-postgres/([0-9a-f]{32})/capsule/"
    r"usr/lib/postgresql/17/bin/postgres"
)


class Refusal(Exception):
    pass


def need(value):
    if not value:
        raise Refusal()


def u64(value):
    need(isinstance(value, str) and U64_RE.fullmatch(value) is not None and int(value) <= 18446744073709551615)
    return value


def pid_value(value):
    need(type(value) is int and 1 < value <= 2147483647)
    return value


def port_value(value):
    need(type(value) is int and 1024 < value <= 65535 and value != 5432)
    return value


def safe_stream(fd):
    value = os.fstat(fd).st_mode
    target = os.readlink("/proc/self/fd/%d" % fd)
    if stat.S_ISFIFO(value):
        need(re.fullmatch(r"pipe:\[[1-9][0-9]*\]", target) is not None)
        return
    need(stat.S_ISSOCK(value) and re.fullmatch(r"socket:\[[1-9][0-9]*\]", target) is not None)
    probe = socket.socket(fileno=os.dup(fd))
    try:
        need(probe.family == socket.AF_UNIX and probe.getsockopt(socket.SOL_SOCKET, socket.SO_TYPE) == socket.SOCK_STREAM
             and probe.getsockname() == "")
        peer = probe.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("3i"))
        peer_pid, peer_uid, _ = struct.unpack("3i", peer)
        need(peer_pid == os.getppid() and peer_uid == os.getuid())
    finally:
        probe.close()


def bounded_read(path, maximum=MAX_PROC):
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | getattr(os, "O_NOFOLLOW", 0))
    try:
        parts = []
        total = 0
        while True:
            chunk = os.read(fd, min(4096, maximum + 1 - total))
            if not chunk:
                break
            total += len(chunk)
            need(total <= maximum)
            parts.append(chunk)
        return b"".join(parts)
    finally:
        os.close(fd)


def private_directory(path):
    need(isinstance(path, str) and os.path.isabs(path) and os.path.realpath(path) == path)
    info = os.lstat(path)
    need(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700)
    return (info.st_dev, info.st_ino, info.st_uid, info.st_mode)


def root_marker(root):
    path = os.path.join(root, ".bluey-test-run-v2")
    before = os.lstat(path)
    need(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid() and before.st_nlink == 1 and stat.S_IMODE(before.st_mode) == 0o600)
    raw = bounded_read(path, 4096)
    need(raw.endswith(b"\n") and b"\x00" not in raw)
    lines = raw.decode("utf-8", "strict").split("\n")
    need(len(lines) == 10 and lines[-1] == "" and lines[0] == "magic=bluey-test-run-v2")
    fields = {}
    for line in lines[1:-1]:
        pair = line.split("=", 1)
        need(len(pair) == 2 and pair[0] not in fields and pair[0] != "" and "=" not in pair[1])
        fields[pair[0]] = pair[1]
    expected = ("run_root", "run_name", "owner_uid", "owner_pid", "process_group_id", "launcher_group_id", "created_epoch", "state")
    need(tuple(fields.keys()) == expected and fields["run_root"] == root and fields["run_name"] == os.path.basename(root)
         and fields["owner_uid"] == str(os.getuid()) and fields["state"] == "running")
    for name in ("owner_pid", "process_group_id", "launcher_group_id", "created_epoch"):
        u64(fields[name])
    after = os.lstat(path)
    identity = (before.st_dev, before.st_ino, before.st_uid, before.st_mode, before.st_size, before.st_mtime_ns, before.st_ctime_ns)
    need(identity == (after.st_dev, after.st_ino, after.st_uid, after.st_mode, after.st_size, after.st_mtime_ns, after.st_ctime_ns))
    return identity + (hashlib.sha256(raw).hexdigest(),)


def parse_stat(raw, expected_pid):
    expected_pid = pid_value(expected_pid)
    text = raw.decode("utf-8", "strict")
    need(text.endswith("\n") and text.count("\n") == 1 and "\x00" not in text and "\r" not in text)
    text = text[:-1]
    close = text.rfind(") ")
    prefix = "%d (" % expected_pid
    need(close > len(prefix) and text.startswith(prefix))
    fields = text[close + 2:].split(" ")
    need(len(fields) >= 20 and re.fullmatch(r"[A-Z]", fields[0]) is not None)
    ppid = pid_value(int(fields[1])) if re.fullmatch(r"[1-9][0-9]{0,9}", fields[1]) else (_ for _ in ()).throw(Refusal())
    pgrp = pid_value(int(fields[2])) if re.fullmatch(r"[1-9][0-9]{0,9}", fields[2]) else (_ for _ in ()).throw(Refusal())
    return (expected_pid, ppid, pgrp, u64(fields[19]))


def parse_status_uid(raw, uid):
    text = raw.decode("utf-8", "strict")
    need("\x00" not in text and len(text) <= MAX_PROC)
    matches = re.findall(r"^Uid:\s*([0-9]+)\s+([0-9]+)\s+([0-9]+)\s+([0-9]+)\s*$", text, re.MULTILINE)
    need(len(matches) == 1 and all(value == str(uid) for value in matches[0]))


def exact_cgroup(proc, expected):
    need(isinstance(expected, str) and re.fullmatch(r"/(?:[^\x00\r\n]+)?", expected) is not None)
    need(bounded_read(os.path.join(proc, "cgroup")).decode("utf-8", "strict") == "0::%s\n" % expected)


def exact_netns(proc, expected):
    need(isinstance(expected, str) and NET_RE.fullmatch(expected) is not None)
    need(os.readlink(os.path.join(proc, "ns", "net")) == expected)


def executable_digest(proc_exe, expected_path, expected_sha):
    """Bind the proc executable link to a stable, pinned physical file."""
    need(os.readlink(proc_exe) == expected_path)
    fd = os.open(expected_path, os.O_RDONLY | os.O_NONBLOCK | getattr(os, "O_NOFOLLOW", 0))
    try:
        before = os.fstat(fd)
        need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and 0 < before.st_size <= MAX_BINARY
             and before.st_uid in (0, os.getuid()) and (stat.S_IMODE(before.st_mode) & 0o111) != 0
             and stat.S_IMODE(before.st_mode) & 0o022 == 0 and stat.S_IMODE(before.st_mode) & 0o7000 == 0)
        digest = hashlib.sha256()
        total = 0
        while True:
            chunk = os.read(fd, 65536)
            if not chunk:
                break
            total += len(chunk)
            need(total <= MAX_BINARY)
            digest.update(chunk)
        after = os.fstat(fd)
        named = os.lstat(expected_path)
        identity = (before.st_dev, before.st_ino, before.st_size, before.st_ctime_ns, before.st_mode)
        proc_info = os.stat(proc_exe)
        need(total == before.st_size and identity == (after.st_dev, after.st_ino, after.st_size, after.st_ctime_ns, after.st_mode)
             and identity == (named.st_dev, named.st_ino, named.st_size, named.st_ctime_ns, named.st_mode)
             and identity == (proc_info.st_dev, proc_info.st_ino, proc_info.st_size, proc_info.st_ctime_ns, proc_info.st_mode)
             and digest.hexdigest() == expected_sha)
        return identity
    finally:
        os.close(fd)


def exact_cmdline(proc, postgres_path, data_directory, port):
    """Bind PostgreSQL to the sole admitted C2C capsule argv shape."""
    match = CAPSULE_POSTGRES.fullmatch(postgres_path)
    need(match is not None and match.group(1) != "0" * 32)
    capsule = "/run/bluey-jobs-c2c-postgres/%s/capsule" % match.group(1)
    raw = bounded_read(os.path.join(proc, "cmdline"), 4096)
    pieces = raw.split(b"\0")
    need(pieces[-1] == b"")
    expected = [postgres_path, "-D", data_directory, "-h", "127.0.0.1", "-p", str(port), "-k", "", "-c", "ssl=off", "-c", "max_connections=64", "-c", "dynamic_library_path=%s/usr/lib/postgresql/17/lib" % capsule]
    need([item.decode("utf-8", "strict") for item in pieces[:-1]] == expected)


def exact_api_cmdline(proc, api_path):
    raw = bounded_read(os.path.join(proc, "cmdline"), 4096)
    pieces = raw.split(b"\0")
    need(pieces[-1] == b"")
    need([item.decode("utf-8", "strict") for item in pieces[:-1]] == [api_path])


def exact_cwd(proc, state_directory):
    need(os.readlink(os.path.join(proc, "cwd")) == state_directory)


def fdinfo_pid(fd, pid):
    text = bounded_read("/proc/self/fdinfo/%d" % fd, 4096).decode("utf-8", "strict")
    matches = re.findall(r"^Pid:\s*([0-9]+)\s*$", text, re.MULTILINE)
    need(matches == [str(pid)])


def alive(pidfd):
    poller = select.poll()
    poller.register(pidfd, select.POLLIN | select.POLLHUP | select.POLLERR)
    need(poller.poll(0) == [])


def terminal(pidfd, timeout_ms):
    poller = select.poll()
    poller.register(pidfd, select.POLLIN | select.POLLHUP | select.POLLERR)
    return bool(poller.poll(timeout_ms))


class FrameReader:
    """Nonblocking, bounded line reader that retains only protocol bytes."""
    def __init__(self, fd):
        self.fd = fd
        self.pending = bytearray()

    def line(self, maximum, timeout):
        deadline = time.monotonic() + timeout
        while True:
            index = self.pending.find(b"\n")
            if index >= 0:
                # The protocol is lock-step: a complete frame must not carry a
                # preloaded next command that could act after a response.
                need(index + 1 <= maximum and len(self.pending) == index + 1)
                raw = bytes(self.pending[:index + 1])
                del self.pending[:index + 1]
                need(raw.endswith(b"\n") and raw.count(b"\n") == 1)
                return raw[:-1]
            need(len(self.pending) <= maximum)
            remaining = deadline - time.monotonic()
            need(remaining > 0)
            readable, _, _ = select.select([self.fd], [], [], remaining)
            need(readable)
            chunk = os.read(self.fd, min(1024, maximum + 1 - len(self.pending)))
            need(chunk)
            self.pending.extend(chunk)


def parse_bind(raw):
    need(isinstance(raw, bytes) and 0 < len(raw) <= MAX_FRAME and b"\x00" not in raw)
    def no_duplicates(pairs):
        result = {}
        for key, item in pairs:
            need(key not in result)
            result[key] = item
        return result
    value = json.loads(raw.decode("utf-8", "strict"), object_pairs_hook=no_duplicates)
    pg_names = {"root", "pid", "startTime", "port", "dataDirectory", "postgresPath", "postgresSha256", "unitCgroup", "networkNamespace"}
    api_names = {"root", "pid", "startTime", "port", "stateDirectory", "apiPath", "apiSha256", "unitCgroup", "networkNamespace"}
    need(type(value) is dict and set(value.keys()) in (pg_names, api_names))
    api = set(value.keys()) == api_names
    path_name, sha_name = ("apiPath", "apiSha256") if api else ("postgresPath", "postgresSha256")
    state_name = "stateDirectory" if api else "dataDirectory"
    need(isinstance(value["root"], str) and isinstance(value[state_name], str) and isinstance(value[path_name], str)
         and isinstance(value["unitCgroup"], str) and isinstance(value["networkNamespace"], str)
         and type(value["pid"]) is int and type(value["port"]) is int and isinstance(value["startTime"], str)
         and isinstance(value[sha_name], str) and SHA_RE.fullmatch(value[sha_name]) is not None)
    pid_value(value["pid"]); port_value(value["port"]); u64(value["startTime"])
    if not api:
        capsule = CAPSULE_POSTGRES.fullmatch(value["postgresPath"])
        need(capsule is not None and capsule.group(1) != "0" * 32
             and value["unitCgroup"] == "/system.slice/bluey-jobs-qa-%s.service" % capsule.group(1))
    return ("api" if api else "postgres", value)


def serve_commands(read_command, reply, stop_owned, dispose_owned):
    """Pure command progression over injected owned operations; no fallback action."""
    stopped = False
    while True:
        raw = read_command()
        if raw == b"stop":
            need(not stopped)
            stop_owned()
            stopped = True
            reply(b"stopped\n")
        elif raw == b"dispose":
            dispose_owned()
            reply(b"disposed\n")
            return
        else:
            raise Refusal()


def main():
    need(sys.platform == "linux" and sys.version_info >= (3, 9) and sys.flags.isolated and sys.flags.no_site
         and sys.dont_write_bytecode and len(sys.argv) == 1 and os.getuid() != 0)
    safe_stream(0); safe_stream(1)
    reader = FrameReader(0)
    role, config = parse_bind(reader.line(MAX_FRAME, 30))
    root = config["root"]
    need(ROOT_RE.fullmatch(root) is not None and root == os.environ.get("BLUEY_TEST_RUN_ROOT") and os.path.realpath(root) == root)
    state_directory = os.path.join(root, "bluey-jobs-qa-draft")
    data_directory = os.path.join(state_directory, "postgres")
    if role == "postgres":
        need(config["dataDirectory"] == data_directory and os.path.isabs(config["postgresPath"])
             and os.path.realpath(config["postgresPath"]) == config["postgresPath"])
    else:
        need(config["stateDirectory"] == state_directory and os.path.isabs(config["apiPath"])
             and os.path.realpath(config["apiPath"]) == config["apiPath"])
    scope = (private_directory(os.path.dirname(root)), private_directory(root), private_directory(state_directory),
             private_directory(data_directory), root_marker(root))
    helper_proc = "/proc/self"
    exact_cgroup(helper_proc, config["unitCgroup"]); exact_netns(helper_proc, config["networkNamespace"])
    parent = os.getppid()

    def observe():
        need((private_directory(os.path.dirname(root)), private_directory(root), private_directory(state_directory),
              private_directory(data_directory), root_marker(root)) == scope)
        proc = "/proc/%d" % config["pid"]
        result = parse_stat(bounded_read(os.path.join(proc, "stat")), config["pid"])
        need(result[1] == parent and result[3] == config["startTime"])
        parse_status_uid(bounded_read(os.path.join(proc, "status")), os.getuid())
        exact_cgroup(proc, config["unitCgroup"]); exact_netns(proc, config["networkNamespace"])
        if role == "postgres":
            executable = executable_digest(os.path.join(proc, "exe"), config["postgresPath"], config["postgresSha256"])
            exact_cmdline(proc, config["postgresPath"], data_directory, config["port"])
        else:
            executable = executable_digest(os.path.join(proc, "exe"), config["apiPath"], config["apiSha256"])
            exact_api_cmdline(proc, config["apiPath"])
            exact_cwd(proc, state_directory)
        return (result, executable)

    before = observe()
    need(hasattr(os, "pidfd_open") and hasattr(signal, "pidfd_send_signal"))
    pidfd = os.pidfd_open(config["pid"], 0)
    try:
        alive(pidfd); fdinfo_pid(pidfd, config["pid"])
        after = observe()
        need(after == before); alive(pidfd); fdinfo_pid(pidfd, config["pid"])
        sys.stdout.buffer.write(b"bound\n"); sys.stdout.buffer.flush()
        deadline = time.monotonic() + IDLE_SECONDS
        def read_command():
            remaining = deadline - time.monotonic()
            need(remaining > 0)
            return reader.line(32, remaining)
        def reply(frame):
            sys.stdout.buffer.write(frame); sys.stdout.buffer.flush()
        def stop_owned():
            need(os.getppid() == parent)
            exact_cgroup(helper_proc, config["unitCgroup"]); exact_netns(helper_proc, config["networkNamespace"])
            current = observe(); need(current == before); alive(pidfd); fdinfo_pid(pidfd, config["pid"])
            signal.pidfd_send_signal(pidfd, signal.SIGINT, None, 0)
        def dispose_and_mark():
            nonlocal pidfd
            need(terminal(pidfd, 0))
            os.close(pidfd); pidfd = -1
        serve_commands(read_command, reply, stop_owned, dispose_and_mark)
        return
    finally:
        if pidfd >= 0:
            os.close(pidfd)


if __name__ == "__main__":
    try:
        main()
    except (Refusal, OSError, UnicodeError, ValueError, json.JSONDecodeError):
        sys.exit(1)
