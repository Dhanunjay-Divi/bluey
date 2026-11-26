"""Export only the synthetic wrapper's raw streams to a held per-run directory."""
import contextlib
import hashlib
import os
import re
import stat

STDOUT = "wrapper.stdout"
STDERR = "wrapper.stderr"
LIMITS = {STDOUT: 2 * 1024**2, STDERR: 4 * 1024**2}
WRITE = os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
HEX32 = re.compile(r"[a-f0-9]{32}")
BOOT = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")


def check(ok, code):
    if not ok:
        raise RuntimeError(code)


def identity(item):
    return (item.st_dev, item.st_ino, item.st_uid, item.st_mode, item.st_nlink,
            item.st_size, item.st_mtime_ns, item.st_ctime_ns)


def exact_scope(scope):
    return (isinstance(scope, dict) and scope.keys() == {"run", "boot", "invocation", "challenge"}
            and all(isinstance(value, str) for value in scope.values())
            and all(HEX32.fullmatch(scope[key]) for key in ("run", "invocation", "challenge"))
            and scope["invocation"] != "0" * 32 and BOOT.fullmatch(scope["boot"]))


def bind_exporter(directory_fd, expected_scope, expected_device, expected_inode):
    """Return the workload callback; the outer supervisor owns the held FD."""
    check(exact_scope(expected_scope), "stream_export_scope")
    bound_scope = dict(expected_scope)

    def owned():
        item = os.fstat(directory_fd)
        check(stat.S_ISDIR(item.st_mode) and item.st_uid == os.getuid()
              and stat.S_IMODE(item.st_mode) == 0o700
              and (item.st_dev, item.st_ino) == (expected_device, expected_inode), "stream_export_directory")

    owned()
    used = False

    def export(scope, stdout, stderr):
        nonlocal used
        check(not used and scope == bound_scope and exact_scope(scope), "stream_export_binding")
        used = True                    # Partial output is never retry authority.
        entries = {STDOUT: stdout, STDERR: stderr}
        check(all(isinstance(payload, bytes) and len(payload) <= LIMITS[name]
                  for name, payload in entries.items()), "stream_export_bound")
        owned()
        check(os.listdir(directory_fd) == [], "stream_export_preexisting")
        with contextlib.ExitStack() as stack:
            handles = {}
            for name, data in entries.items():
                handle = os.open(name, WRITE, mode=0o600, dir_fd=directory_fd)
                file = stack.enter_context(os.fdopen(handle, "w+b"))
                check(file.write(data) == len(data), "stream_export_short_write")
                file.flush()
                os.fsync(handle)
                handles[name] = file
            os.fsync(directory_fd)
            owned()
            check(set(os.listdir(directory_fd)) == set(entries), "stream_export_inventory")
            for name, file in handles.items():
                info = os.fstat(file.fileno())
                data = entries[name]
                check(info.st_uid == os.getuid() and stat.S_ISREG(info.st_mode) and info.st_nlink == 1
                      and stat.S_IMODE(info.st_mode) == 0o600 and info.st_size == len(data),
                      "stream_export_file")
                file.seek(0)
                observed = file.read(LIMITS[name] + 1)
                check(len(observed) == len(data)
                      and hashlib.sha256(observed).digest() == hashlib.sha256(data).digest(),
                      "stream_export_readback")
                check(identity(info) == identity(os.fstat(file.fileno()))
                      == identity(os.stat(name, dir_fd=directory_fd, follow_symlinks=False)),
                      "stream_export_replaced")
        return True

    return export
