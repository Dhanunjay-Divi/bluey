"""Stage an already verified C2C code packet without publishing a release.

The actor and privileged-manager trees deliberately have different roots and
permissions.  A caller still owns unit launch, release publication, and any
later cleanup; this module has no CLI and never executes staged code.
"""
import hashlib
import os
from pathlib import Path
import platform
import re
import stat
import sys
import types


ACTOR_BASE = Path("/run/bluey-jobs-c2c-code")
MANAGER_BASE = Path("/run/bluey-jobs-c2c-manager")
VERIFIER = Path(__file__).resolve().with_name("local-qa-linux-c2c-code-packet.py")
VERIFIER_SHA256 = "694adfec5310035d94858d626fa5f84c2e093347f0f5fec6c020b03fff0a9870"
SHA = re.compile(r"^[0-9a-f]{64}$")
TREE = re.compile(r"^[0-9a-f]{40}$")
RUN = re.compile(r"^[0-9a-f]{32}$")
MAX_SOURCE = 64 << 10
RD = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
DIR = RD | os.O_DIRECTORY


def _need(value, label):
    if not value:
        raise RuntimeError("local_qa_linux_c2c_code_stage_refused:" + label)


def _stable(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode,
            info.st_nlink, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def _read_fd(fd, maximum):
    before = os.fstat(fd)
    _need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
          and 0 < before.st_size <= maximum, "read_metadata")
    parts, remaining = [], before.st_size + 1
    while remaining:
        chunk = os.read(fd, min(65536, remaining))
        if not chunk:
            break
        parts.append(chunk)
        remaining -= len(chunk)
    raw = b"".join(parts)
    _need(len(raw) == before.st_size and _stable(before) == _stable(os.fstat(fd)), "read_changed")
    return raw, before


def _read_verifier():
    _need(VERIFIER.is_absolute() and VERIFIER.resolve(strict=True) == VERIFIER, "verifier_path")
    fd = os.open(VERIFIER, RD)
    try:
        raw, before = _read_fd(fd, MAX_SOURCE)
        named = VERIFIER.lstat()
        _need(stat.S_ISREG(before.st_mode) and before.st_uid in (0, os.geteuid())
              and stat.S_IMODE(before.st_mode) in (0o600, 0o644)
              and not before.st_mode & 0o022 and _stable(before) == _stable(named)
              and hashlib.sha256(raw).hexdigest() == VERIFIER_SHA256, "verifier_pin")
    finally:
        os.close(fd)
    module = types.ModuleType("_pinned_local_qa_linux_c2c_code_packet")
    module.__file__ = str(VERIFIER)
    exec(compile(raw, str(VERIFIER), "exec"), module.__dict__)
    _need(callable(getattr(module, "verify_packet", None)), "verifier_contract")
    return module


def _namespace_initial():
    return all(os.path.samestat(os.stat("/proc/self/ns/" + kind), os.stat("/proc/1/ns/" + kind))
               for kind in ("mnt", "user", "pid"))


def _root_linux():
    return (platform.system() == "Linux" and platform.machine() == "x86_64"
            and os.getuid() == os.geteuid() == 0 and _namespace_initial())


def _capability(value):
    keys = {"run", "reservationSha256", "packetSha256", "codeInventorySha256", "sourceTree", "sourceInventorySha256"}
    digests = keys - {"run", "sourceTree"}
    _need(type(value) is dict and set(value) == keys and type(value["run"]) is str
          and RUN.fullmatch(value["run"]) is not None and value["run"] != "0" * 32
          and type(value["sourceTree"]) is str and TREE.fullmatch(value["sourceTree"]) is not None
          and value["sourceTree"] != "0" * 40
          and all(type(value[key]) is str and SHA.fullmatch(value[key]) is not None
                  and value[key] != "0" * 64 for key in digests),
          "capability")
    return dict(value)


def _open_base(path, mode):
    _need(path.is_absolute() and path.resolve(strict=True) == path, "base_path")
    fd = os.open(path, DIR)
    try:
        info = os.fstat(fd)
        _need(stat.S_ISDIR(info.st_mode) and info.st_uid == 0 and info.st_gid == 0
              and stat.S_IMODE(info.st_mode) == mode and info.st_nlink >= 2
              and _stable(info) == _stable(path.lstat()), "base_metadata")
        return fd
    except BaseException:
        os.close(fd)
        raise


def _absent(parent, name):
    try:
        os.stat(name, dir_fd=parent, follow_symlinks=False)
    except FileNotFoundError:
        return
    raise RuntimeError("local_qa_linux_c2c_code_stage_refused:run_present")


def _mkdir(parent, name, mode, owner=0, group=0):
    parent_before = os.fstat(parent)
    os.mkdir(name, mode, dir_fd=parent)
    fd = os.open(name, DIR, dir_fd=parent)
    try:
        initial, named_initial = os.fstat(fd), os.stat(name, dir_fd=parent, follow_symlinks=False)
        _need(stat.S_ISDIR(initial.st_mode) and initial.st_uid == owner and initial.st_gid == group
              and _stable(initial) == _stable(named_initial)
              and (parent_before.st_dev, parent_before.st_ino) == (os.fstat(parent).st_dev, os.fstat(parent).st_ino),
              "run_directory_initial")
        os.fchmod(fd, mode)
        os.fsync(fd)
        info = os.fstat(fd)
        _need(stat.S_ISDIR(info.st_mode) and info.st_uid == owner and info.st_gid == group
              and stat.S_IMODE(info.st_mode) == mode and info.st_nlink >= 2
              and _stable(info) == _stable(os.stat(name, dir_fd=parent, follow_symlinks=False)), "run_directory")
        return fd
    except BaseException:
        os.close(fd)
        raise


def _write_new(parent, name, body, mode, owner=0, group=0):
    _need("/" not in name and isinstance(body, bytes) and body, "leaf_shape")
    fd = os.open(name, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, mode, dir_fd=parent)
    try:
        view = memoryview(body)
        while view:
            count = os.write(fd, view)
            _need(count > 0, "write_progress")
            view = view[count:]
        os.fchmod(fd, mode)
        os.fsync(fd)
        info = os.fstat(fd)
        _need(stat.S_ISREG(info.st_mode) and info.st_uid == owner and info.st_gid == group
              and stat.S_IMODE(info.st_mode) == mode and info.st_nlink == 1 and info.st_size == len(body)
              and hashlib.sha256(os.pread(fd, len(body) + 1, 0)).digest() == hashlib.sha256(body).digest()
              and _stable(info) == _stable(os.stat(name, dir_fd=parent, follow_symlinks=False)), "leaf_readback")
    finally:
        os.close(fd)


def _stage(parent, name, members, mode, owner=0, group=0):
    run = _mkdir(parent, name, mode, owner, group)
    try:
        for leaf, body in members.items():
            _write_new(run, leaf, body, 0o444 if mode == 0o755 else 0o600, owner, group)
        os.fsync(run)
        _need(set(os.listdir(run)) == set(members), "run_entries")
        os.fsync(parent)
        return run
    except BaseException:
        os.close(run)
        raise


def _base_current(path, fd, mode):
    held, named = os.fstat(fd), path.lstat()
    _need(path.resolve(strict=True) == path and stat.S_ISDIR(held.st_mode)
          and (held.st_dev, held.st_ino, held.st_uid, held.st_gid,
          stat.S_IMODE(held.st_mode)) == (named.st_dev, named.st_ino, named.st_uid, named.st_gid,
          stat.S_IMODE(named.st_mode)) == (held.st_dev, held.st_ino, 0, 0, mode), "base_changed")


def _verify_stage(parent, name, run, members, mode, owner=0, group=0):
    current, named = os.fstat(run), os.stat(name, dir_fd=parent, follow_symlinks=False)
    _need(_stable(current) == _stable(named) and stat.S_ISDIR(current.st_mode)
          and current.st_uid == owner and current.st_gid == group and stat.S_IMODE(current.st_mode) == mode
          and set(os.listdir(run)) == set(members), "stage_changed")
    leaf_mode = 0o444 if mode == 0o755 else 0o600
    for leaf, expected in members.items():
        fd = os.open(leaf, RD, dir_fd=run)
        try:
            body, info = _read_fd(fd, len(expected))
            named_leaf = os.stat(leaf, dir_fd=run, follow_symlinks=False)
            _need(body == expected and info.st_uid == owner and info.st_gid == group
                  and stat.S_IMODE(info.st_mode) == leaf_mode
                  and _stable(info) == _stable(named_leaf), "stage_leaf")
        finally:
            os.close(fd)
    final, final_named = os.fstat(run), os.stat(name, dir_fd=parent, follow_symlinks=False)
    _need(_stable(final) == _stable(current) == _stable(final_named)
          and set(os.listdir(run)) == set(members), "stage_changed_after_read")


def stage_code_packet(raw, expected_sha256, capability, *, admitted):
    """Write verified actor and manager trees once; never launches or releases.

    ``admitted`` is an independent root-manager predicate and receives an
    isolated capability copy before and after every write phase.
    """
    _need(_root_linux() and callable(admitted), "platform")
    cap = _capability(capability)
    verifier = _read_verifier()
    actor, manager, receipt = verifier.verify_packet(raw, expected_sha256)
    _need(receipt == {
        "schema": "bluey-jobs-c2c-code-packet-v1", "packetSha256": cap["packetSha256"],
        "sourceTree": cap["sourceTree"], "sourceInventorySha256": cap["sourceInventorySha256"],
        "priorActorInventorySha256": verifier.BASE_INVENTORY_SHA,
        "codeInventorySha256": cap["codeInventorySha256"], "actorFiles": 31,
        "managerFiles": 11, "releaseAdmitted": False, "runtimeQualified": False,
    }, "receipt")
    _need(expected_sha256 == cap["packetSha256"] and admitted(dict(cap)) is True, "admission_before")
    _need(_root_linux(), "namespace_before_write")
    actor_base = _open_base(ACTOR_BASE, 0o755)
    manager_base = -1
    actor_run = manager_run = -1
    try:
        manager_base = _open_base(MANAGER_BASE, 0o700)
        _absent(actor_base, cap["run"])
        _absent(manager_base, cap["run"])
        actor_run = _stage(actor_base, cap["run"], actor, 0o755)
        manager_run = _stage(manager_base, cap["run"], manager, 0o700)
        _need(admitted(dict(cap)) is True, "admission_after")
        _base_current(ACTOR_BASE, actor_base, 0o755)
        _base_current(MANAGER_BASE, manager_base, 0o700)
        _verify_stage(actor_base, cap["run"], actor_run, actor, 0o755)
        _verify_stage(manager_base, cap["run"], manager_run, manager, 0o700)
        return {"run": cap["run"], "packetSha256": cap["packetSha256"],
                "codeInventorySha256": cap["codeInventorySha256"], "actorFiles": len(actor) - 1,
                "managerFiles": len(manager), "releasePublished": False, "runtimeQualified": False}
    finally:
        if actor_run >= 0:
            os.close(actor_run)
        if manager_run >= 0:
            os.close(manager_run)
        os.close(actor_base)
        if manager_base >= 0:
            os.close(manager_base)
