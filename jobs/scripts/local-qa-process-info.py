#!/usr/bin/python3
"""One same-user Darwin PID identity; no process enumeration or argv inspection."""
import importlib.util
import json
import os
import platform
import sys
import time


def main():
    if (platform.system() != "Darwin" or platform.machine() != "arm64"
            or sys.version_info[:3] != (3, 9, 6) or not sys.flags.isolated
            or not sys.flags.no_site or not sys.flags.dont_write_bytecode
            or not sys.flags.ignore_environment or os.getuid() == 0
            or os.getuid() != os.geteuid() or len(sys.argv) != 2):
        raise ValueError("refused")
    raw = sys.argv[1]
    if not raw.isascii() or not raw.isdecimal() or raw.startswith("0") or len(raw) > 10:
        raise ValueError("refused")
    pid = int(raw)
    if pid <= 1 or pid > 2147483647:
        raise ValueError("refused")
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "managed-runner-host-processes.py")
    spec = importlib.util.spec_from_file_location("local_qa_native_process_observer", path)
    if spec is None or spec.loader is None:
        raise ValueError("refused")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    provider, filesystem = module.NativeProvider(), module.FilesystemObserver()
    launcher = "/Library/Developer/CommandLineTools/usr/bin/python3"
    if sys.executable != launcher:
        raise ValueError("refused")
    before_pair = module.reviewed_python_pair(filesystem, launcher)
    _, own_executable = module.observe_stable_process(
        provider, filesystem, os.getpid(), os.getuid(), lambda: time.time_ns() // 1000)
    if own_executable != before_pair[1]:
        raise ValueError("refused")
    first, first_file = module.observe_stable_process(
        provider, filesystem, pid, os.getuid(), lambda: time.time_ns() // 1000)
    first_group = os.getpgid(pid)
    second, second_file = module.observe_stable_process(
        provider, filesystem, pid, os.getuid(), lambda: time.time_ns() // 1000)
    second_group = os.getpgid(pid)
    if (first != second or first_file != second_file or first_group != second_group
            or first_group <= 1 or module.reviewed_python_pair(filesystem, launcher) != before_pair):
        raise ValueError("refused")
    result = {"pid": first.pid, "ppid": first.ppid, "uid": first.euid, "ruid": first.ruid,
              "pgid": first_group, "birthSec": str(first.birth_sec), "birthUsec": first.birth_usec,
              "executable": first_file.canonical_path, "executableDevice": str(first_file.device),
              "executableInode": str(first_file.inode)}
    print(json.dumps(result, separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("local_qa_process_info_refused", file=sys.stderr)
        sys.exit(1)
