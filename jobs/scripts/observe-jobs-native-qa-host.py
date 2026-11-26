"""Read-only prerequisite observations for a separately authorized Linux Jobs QA host.

This probe never admits a workload.  In particular, space sufficient for the
retained wire packet is not a measurement of browser, package, database, or
runtime capacity.
"""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import select
import stat
import sys


BOOT = Path("/proc/sys/kernel/random/boot_id")
BOOT_ID = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")
MOUNTINFO = Path("/proc/self/mountinfo")
NAMESPACES = ("mnt", "pid", "user", "net", "cgroup")
SYSTEMD = "/usr/lib/systemd/systemd"
# Current native pair and changed portal, measured through EOF including framing.
WIRE_BYTES = 490_699_941  # 4 + 271498 + 44659086 + 4 + 2797 + 445766552
SOURCE_BUNDLE_SHA256 = "2e9c7ddae3290852fe061c90d3dddc7018a1097367abe697dc3af8f2d16d926b"
JOBS_MANIFEST_WIRE_SHA256 = "ece652d59b69d8909fb019f9a599b6865453d1de6abeea4e618598c351f9571a"
REQUIRED = {
    "architecture", "distribution", "rootIds", "pidOneExecutable", "bootId",
    "sameAsVisiblePidOneNamespaces", "effectiveCgroup2", "varLibFreeBytes",
}
DIRECTORY_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC


class SupervisorRefused(RuntimeError):
    """The expected live root parent could not be established or retained."""


def bounded(path, maximum, *, proc=False):
    """Read one bounded immutable/proc file without following a symlink."""
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC), "rb") as stream:
        before = os.fstat(stream.fileno())
        assert stat.S_ISREG(before.st_mode)
        assert proc or (before.st_uid == 0 and not before.st_mode & 0o022)
        data = stream.read(maximum + 1)
        after = os.fstat(stream.fileno())
        assert len(data) <= maximum and os.path.samestat(before, after)
        assert proc or (before.st_size == len(data) and before.st_ctime_ns == after.st_ctime_ns)
    return data


def mount_id(raw_fdinfo: bytes) -> int:
    """Return exactly one bounded numeric mount identity from proc fdinfo bytes."""
    assert type(raw_fdinfo) is bytes and 0 < len(raw_fdinfo) <= 8192
    matches = []
    for line in raw_fdinfo.splitlines():
        if line.startswith(b"mnt_id:"):
            matched = re.fullmatch(rb"mnt_id:\t([1-9][0-9]{0,9})", line)
            assert matched is not None
            matches.append(matched.group(1))
    assert len(matches) == 1
    value = int(matches[0])
    assert value <= 2_147_483_647
    return value


def effective_cgroup2(mountinfo: bytes, effective_id: int) -> bool:
    """Check the held cgroup descriptor's exact mountinfo row, not a lookalike."""
    assert type(mountinfo) is bytes and 0 < len(mountinfo) <= 1 << 20
    assert type(effective_id) is int and 0 < effective_id <= 2_147_483_647
    matches = []
    for line in mountinfo.splitlines():
        left, separator, right = line.partition(b" - ")
        fields = left.split()
        right_fields = right.split()
        if not separator or len(fields) < 6 or not right_fields:
            continue
        if not re.fullmatch(rb"[1-9][0-9]{0,9}", fields[0]):
            continue
        if int(fields[0]) != effective_id:
            continue
        matches.append((fields, right_fields))
    if len(matches) != 1:
        return False
    fields, right_fields = matches[0]
    return fields[3] == b"/" and fields[4] == b"/sys/fs/cgroup" and right_fields[0] == b"cgroup2"


def ubuntu_version(raw):
    keys = {}
    for line in raw.decode("ascii", "strict").splitlines():
        key, separator, value = line.partition("=")
        if key in ("ID", "VERSION_ID"):
            assert separator and key not in keys
            if value.startswith('"') and value.endswith('"'):
                value = value[1:-1]
            assert re.fullmatch(r"[a-z0-9.]+", value)
            keys[key] = value
    assert set(keys) == {"ID", "VERSION_ID"}
    return keys["ID"] + ":" + keys["VERSION_ID"]


def nonzero_boot(value):
    return (type(value) is str and BOOT_ID.fullmatch(value) is not None
            and value != "00000000-0000-0000-0000-000000000000")


def read_boot():
    raw = bounded(BOOT, 40, proc=True)
    value = raw.removesuffix(b"\n").decode("ascii", "strict")
    if raw != (value + "\n").encode("ascii") or not nonzero_boot(value):
        raise SupervisorRefused()
    return value


def root_process():
    return os.getresuid() == os.getresgid() == (0, 0, 0)


def proc_field(raw, name, expected):
    """Match exactly one canonical field in a bounded Linux proc snapshot."""
    return [line for line in raw.split(b"\n") if line.startswith(name)] == [name + expected]


def root_parent(proc):
    first = proc.stat()
    status = bounded(proc / "status", 64 << 10, proc=True)
    if (not stat.S_ISDIR(first.st_mode) or first.st_uid != 0 or first.st_gid != 0
            or not proc_field(status, b"Uid:", b"\t0\t0\t0\t0")
            or not proc_field(status, b"Gid:", b"\t0\t0\t0\t0")
            or not os.path.samestat(first, proc.stat())):
        raise SupervisorRefused()
    return first


@contextmanager
def hold_root_parent(expected_boot, expected_parent):
    """Hold only the direct Linux parent; false is permanent for this lease.

    An enabled caller must obtain the expected values independently of any
    input packet. This detects parent/boot loss, not workload authorization.
    """
    if (sys.platform != "linux" or not hasattr(os, "pidfd_open") or not nonzero_boot(expected_boot)
            or type(expected_parent) is not int or not 1 < expected_parent < 2**31
            or not root_process() or os.getppid() != expected_parent or read_boot() != expected_boot):
        raise SupervisorRefused()
    proc = Path("/proc") / str(expected_parent)
    original = root_parent(proc)
    fd = os.pidfd_open(expected_parent, 0)
    active = True
    try:
        if not proc_field(bounded(Path(f"/proc/self/fdinfo/{fd}"), 8192, proc=True),
                          b"Pid:", f"\t{expected_parent}".encode("ascii")):
            raise SupervisorRefused()
        poller = select.poll()
        poller.register(fd, select.POLLIN | select.POLLHUP | select.POLLERR)

        def held():
            nonlocal active
            if active:
                try:
                    active = (not poller.poll(0) and root_process() and os.getppid() == expected_parent
                              and read_boot() == expected_boot and os.path.samestat(original, root_parent(proc))
                              and os.getppid() == expected_parent and not poller.poll(0))
                except (OSError, ValueError, AssertionError, SupervisorRefused):
                    active = False
            return active

        if not held():
            raise SupervisorRefused()
        yield held
    finally:
        active = False
        os.close(fd)


def collect_with_parent():
    # Local read-only observation, not an owner-supplied execution challenge.
    boot, parent = read_boot(), os.getppid()
    with hold_root_parent(boot, parent) as held:
        evidence = collect()
        if not held() or evidence["bootDigest"] != hashlib.sha256(boot.encode("ascii")).hexdigest():
            raise SupervisorRefused()
        return {**evidence, "schema": "bluey-jobs-native-qa-parent-facts-v1", "rootDirectParentHeld": True}


def assess(facts):
    """Evaluate host facts only; a positive result never authorizes execution."""
    assert type(facts) is dict and set(facts) == REQUIRED
    boot = facts["bootId"]
    assert nonzero_boot(boot)
    namespaces = facts["sameAsVisiblePidOneNamespaces"]
    assert type(namespaces) is dict and set(namespaces) == set(NAMESPACES)
    assert all(type(value) is bool for value in namespaces.values())
    capacity = facts["varLibFreeBytes"]
    assert type(capacity) is int and 0 <= capacity < 2**63
    checks = {
        "ubuntuNoble": facts["distribution"] == "ubuntu:24.04",
        "x86_64": facts["architecture"] == "x86_64",
        "rootIdentities": (type(facts["rootIds"]) is tuple and len(facts["rootIds"]) == 4 and
                           all(type(value) is int and value == 0 for value in facts["rootIds"])),
        "systemdPidOne": facts["pidOneExecutable"] == SYSTEMD,
        "namespacesMatchVisiblePidOne": all(namespaces.values()),
        "effectiveCgroup2": facts["effectiveCgroup2"] is True,
        "roomForWireBytesOnly": capacity >= WIRE_BYTES,
    }
    return {
        "schema": "bluey-jobs-native-qa-host-facts-v2",
        "packet": {
            "wireBytes": WIRE_BYTES,
            "sourceBundleSha256": SOURCE_BUNDLE_SHA256,
            "jobsManifestWireSha256": JOBS_MANIFEST_WIRE_SHA256,
        },
        "checks": checks,
        "preconditionsObserved": all(checks.values()),
        "varLibFreeBytes": capacity,
        "bootDigest": hashlib.sha256(boot.encode("ascii")).hexdigest(),
        "executionAdmitted": False,
        "productionQualified": False,
    }


def namespace_identities():
    result = {}
    for name in NAMESPACES:
        result[name] = tuple((item.st_dev, item.st_ino)
                             for item in (os.stat("/proc/self/ns/" + name), os.stat("/proc/1/ns/" + name)))
    return result


def open_directory(path):
    descriptor = os.open(path, DIRECTORY_FLAGS)
    try:
        identity = os.fstat(descriptor)
        assert stat.S_ISDIR(identity.st_mode)
        return descriptor, identity
    except BaseException:
        os.close(descriptor)
        raise


def same_directory(descriptor, identity, path):
    held = os.fstat(descriptor)
    fresh = os.stat(path, follow_symlinks=False)
    assert os.path.samestat(identity, held) and os.path.samestat(identity, fresh)
    assert stat.S_ISDIR(held.st_mode) and stat.S_ISDIR(fresh.st_mode)


def descriptor_mount_id(descriptor):
    return mount_id(bounded(Path("/proc/self/fdinfo/") / str(descriptor), 8192, proc=True))


def checked_directory(path):
    """Hold a readonly directory descriptor and verify a fresh matching view."""
    descriptor, identity = open_directory(path)
    try:
        effective = descriptor_mount_id(descriptor)
        fresh, fresh_identity = open_directory(path)
        try:
            assert os.path.samestat(identity, fresh_identity)
            assert effective == descriptor_mount_id(fresh)
        finally:
            os.close(fresh)
        return descriptor, identity, effective
    except BaseException:
        os.close(descriptor)
        raise


def postflight_directory(descriptor, identity, expected_mount, path):
    """Reopen by path so an identity lookalike cannot hide mount drift."""
    same_directory(descriptor, identity, path)
    fresh, fresh_identity = open_directory(path)
    try:
        assert os.path.samestat(identity, fresh_identity)
        assert expected_mount == descriptor_mount_id(fresh)
    finally:
        os.close(fresh)


def collect():
    cgroup = var_lib = None
    try:
        cgroup, cgroup_identity, cgroup_mount = checked_directory("/sys/fs/cgroup")
        var_lib, var_lib_identity, var_lib_mount = checked_directory("/var/lib")
        boot = read_boot()
        namespaces = namespace_identities()
        pid_one = os.readlink("/proc/1/exe")
        first_mountinfo = bounded(MOUNTINFO, 1 << 20, proc=True)
        space = os.fstatvfs(var_lib)
        result = assess({
            "architecture": os.uname().machine,
            "distribution": ubuntu_version(bounded(Path("/usr/lib/os-release"), 16 << 10)),
            "rootIds": (os.getuid(), os.geteuid(), os.getgid(), os.getegid()),
            "pidOneExecutable": pid_one,
            "bootId": boot,
            "sameAsVisiblePidOneNamespaces": {key: left == right for key, (left, right) in namespaces.items()},
            "effectiveCgroup2": effective_cgroup2(first_mountinfo, cgroup_mount),
            "varLibFreeBytes": space.f_bavail * space.f_frsize,
        })
        assert boot == read_boot()
        assert namespaces == namespace_identities() and pid_one == os.readlink("/proc/1/exe")
        assert first_mountinfo == bounded(MOUNTINFO, 1 << 20, proc=True)
        postflight_directory(cgroup, cgroup_identity, cgroup_mount, "/sys/fs/cgroup")
        postflight_directory(var_lib, var_lib_identity, var_lib_mount, "/var/lib")
        return result
    finally:
        if var_lib is not None:
            os.close(var_lib)
        if cgroup is not None:
            os.close(cgroup)


def main():
    mode = sys.argv[1:]
    if mode not in (["--observe"], ["--observe-parent"]) or not (
            __debug__ and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode):
        return 64
    if sys.platform != "linux":
        print('{"event":"jobs_native_qa_host_observation_refused","reason":"not_linux"}', file=sys.stderr)
        return 69
    if os.getuid() or os.geteuid() or os.getgid() or os.getegid():
        print('{"event":"jobs_native_qa_host_observation_refused","reason":"not_root"}', file=sys.stderr)
        return 69
    try:
        result = collect_with_parent() if mode == ["--observe-parent"] else collect()
    except (OSError, ValueError, AssertionError, SupervisorRefused):
        print('{"event":"jobs_native_qa_host_observation_refused","reason":"host_facts_unavailable"}',
              file=sys.stderr)
        return 69
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0 if result["preconditionsObserved"] else 69


if __name__ == "__main__":
    raise SystemExit(main())
