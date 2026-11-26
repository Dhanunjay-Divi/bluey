"""Bind, but never run, the staged signup/mock-receipt manager tree."""
from contextlib import contextmanager
import hashlib
import importlib.util
import os
from pathlib import Path
import platform
import re
import stat
import sys
import types


MANAGER_BASE = Path("/run/bluey-jobs-c2c-manager")
STAGER = Path(__file__).resolve().with_name("local-qa-linux-signup-mock-receipt-code-stage.py")
STAGER_SHA256 = "f67b2d888da5ca0bc81fb65eef434ee308aec04b935d478f0365782dbaa0dec5"
PACKET_SHA256 = "ff7df61110b4521faadef1a10f6f58a5396657d8f8ad537a7ab65648f1bebd29"
PACKET_BYTES = 686_080
MANAGER_NAME = "local-qa-linux-signup-mock-receipt-guest-manager.py"
MANAGER = {
    "interview-prep-linux-closure.py": (9199, "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4"),
    "local-qa-linux-c2c-guest-teardown.py": (15641, "4cc2c01f50f4d83f4ec702036645ed8769052a24f78f6af9bb84cd1c1a9987cd"),
    "local-qa-linux-c2c-launch-custody.py": (13220, "49c887c278408e057005940d9b97ce9579279938c81170bf4780bb9feb22cfca"),
    "local-qa-linux-c2c-manager-observer.py": (16859, "57d60374c15596bfe0594b707892f7a847c3c5a459da6564778525729c886530"),
    "local-qa-linux-c2c-release-protocol.py": (6310, "850b48cd7e36778d7c1be4d4eaf8f5ead5f00eadd90f0ea18a42db366c5a57ec"),
    "local-qa-linux-c2c-release-writer.py": (16373, "4a9a08727453f2b8e658b1c15fd5a7544f3e691c4d9db37e0ad3c4fdfaf09cda"),
    "local-qa-linux-c2c-stage-actor-inputs.py": (13027, "73719c3608fd1b50a35503e828ce9a5f7adc48ce1a9690af92817f716ae30c13"),
    MANAGER_NAME: (26387, "1ccd14a1551b11b93ec9a22f95c6d45c692a9d5b3a67cfea4cc83623586bd9b1"),
    "local-qa-linux-signup-mock-receipt-terminal.py": (2001, "b44511c40dafed18f7676c1cb25a6c4a4aefba618cc87527b09a0fac924c49ec"),
    "receipt-browser-manager-stop.py": (11994, "b41eeb361e5511885995b1318c08beec22fc7699d41f67a915e4a8012ba4a1e9"),
    "receipt-browser-post-stop.py": (10730, "26a34e0cb65ad3d34d2d72b3cfb9ae58f280fcb99fa3ff968ae8bce5f3d46a48"),
    "receipt-browser-release-writer.py": (11491, "8aeeba6bb06e37976201ea4b88c8a532ab6dbb8ab4bc56615fb7e8c3ad1d6014"),
}
RUN = re.compile(r"^[0-9a-f]{32}$")
RD = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK
DIR = RD | os.O_DIRECTORY


def _need(value, label):
    if not value:
        raise RuntimeError("local_qa_linux_signup_mock_receipt_manager_loader_refused:" + label)


def _sha(raw):
    return hashlib.sha256(raw).hexdigest()


def _stamp(row):
    return (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_gid, row.st_nlink,
            row.st_size, row.st_mtime_ns, row.st_ctime_ns)


def _read_exact(parent, leaf, size, digest):
    fd = os.open(leaf, RD, dir_fd=parent)
    try:
        before = os.fstat(fd)
        _need(stat.S_ISREG(before.st_mode) and before.st_uid == before.st_gid == 0
              and stat.S_IMODE(before.st_mode) == 0o600 and before.st_nlink == 1 and before.st_size == size,
              "manager_leaf_metadata")
        parts, remaining = [], size + 1
        while remaining:
            chunk = os.read(fd, min(65536, remaining))
            if not chunk:
                break
            parts.append(chunk)
            remaining -= len(chunk)
        raw = b"".join(parts)
        named = os.stat(leaf, dir_fd=parent, follow_symlinks=False)
        _need(len(raw) == size and _sha(raw) == digest and _stamp(before) == _stamp(os.fstat(fd)) == _stamp(named),
              "manager_leaf_pin")
        return raw
    finally:
        os.close(fd)


def _load_stager():
    _need(STAGER.is_absolute() and STAGER.resolve(strict=True) == STAGER, "stager_path")
    fd = os.open(STAGER, RD)
    try:
        before = os.fstat(fd)
        _need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and not before.st_mode & 0o022
              and before.st_uid in (0, os.geteuid()) and 0 < before.st_size <= 64 << 10, "stager_metadata")
        parts, remaining = [], before.st_size + 1
        while remaining:
            chunk = os.read(fd, min(65536, remaining))
            if not chunk:
                break
            parts.append(chunk)
            remaining -= len(chunk)
        raw = b"".join(parts)
        _need(len(raw) == before.st_size and _sha(raw) == STAGER_SHA256
              and _stamp(before) == _stamp(os.fstat(fd)) == _stamp(STAGER.lstat()), "stager_pin")
    finally:
        os.close(fd)
    spec = importlib.util.spec_from_file_location("_pinned_mock_receipt_code_stage", STAGER)
    module = importlib.util.module_from_spec(spec)
    exec(compile(raw, str(STAGER), "exec"), module.__dict__)
    _need(callable(getattr(module, "verify_packet", None)), "stager_contract")
    return module


def verify_sealed_packet(raw):
    """Portable proof that the supplied sealed packet has the expected 12 manager leaves."""
    _need(type(raw) is bytes and len(raw) == PACKET_BYTES and _sha(raw) == PACKET_SHA256, "packet_pin")
    _, manager, receipt = _load_stager().verify_packet(raw, PACKET_SHA256)
    _need(receipt["packetSha256"] == PACKET_SHA256 and receipt["actorFiles"] == 39
          and receipt["managerFiles"] == len(MANAGER) and set(manager) == set(MANAGER), "packet_shape")
    _need(all(len(manager[name]) == size and _sha(manager[name]) == digest
              for name, (size, digest) in MANAGER.items()), "packet_manager_members")
    return {"packetSha256": PACKET_SHA256, "managerFiles": len(MANAGER),
            "managerSha256": MANAGER[MANAGER_NAME][1]}


def _root_linux():
    return (sys.platform == "linux" and platform.machine() == "x86_64" and os.getuid() == os.geteuid() == 0
            and all(os.path.samestat(os.stat("/proc/self/ns/" + kind), os.stat("/proc/1/ns/" + kind))
                    for kind in ("mnt", "user", "pid", "net", "cgroup")))


@contextmanager
def bind_staged_manager(run, reservation_current):
    """Yield ``(private_manager_module, remeasure)`` while held FDs remain valid.

    The caller must execute ``remeasure`` immediately before compose/release.
    Neither this function nor the returned callback calls a manager entrypoint.
    """
    _need(type(run) is str and RUN.fullmatch(run) is not None and run != "0" * 32
          and callable(reservation_current) and _root_linux() and reservation_current() is True, "admission")
    _need(MANAGER_BASE.is_absolute() and MANAGER_BASE.resolve(strict=True) == MANAGER_BASE, "manager_base_path")
    base = os.open(MANAGER_BASE, DIR)
    run_fd = -1
    try:
        base_before = os.fstat(base)
        _need(stat.S_ISDIR(base_before.st_mode) and base_before.st_uid == base_before.st_gid == 0
              and stat.S_IMODE(base_before.st_mode) == 0o700 and base_before.st_nlink >= 2
              and _stamp(base_before) == _stamp(MANAGER_BASE.lstat()), "manager_base_metadata")
        run_fd = os.open(run, DIR, dir_fd=base)
        run_before = os.fstat(run_fd)
        _need(stat.S_ISDIR(run_before.st_mode) and run_before.st_uid == run_before.st_gid == 0
              and stat.S_IMODE(run_before.st_mode) == 0o700 and run_before.st_nlink >= 2
              and _stamp(run_before) == _stamp(os.stat(run, dir_fd=base, follow_symlinks=False)), "manager_run_metadata")

        def measure():
            _need(reservation_current() is True and _root_linux(), "admission_remeasure")
            _need(_stamp(base_before) == _stamp(os.fstat(base)) == _stamp(MANAGER_BASE.lstat()), "manager_base_changed")
            _need(_stamp(run_before) == _stamp(os.fstat(run_fd))
                  == _stamp(os.stat(run, dir_fd=base, follow_symlinks=False)), "manager_run_changed")
            _need(set(os.listdir(run_fd)) == set(MANAGER), "manager_entries")
            rows = {name: _read_exact(run_fd, name, size, digest) for name, (size, digest) in MANAGER.items()}
            _need(_stamp(base_before) == _stamp(os.fstat(base)) == _stamp(MANAGER_BASE.lstat()), "manager_base_changed")
            _need(_stamp(run_before) == _stamp(os.fstat(run_fd))
                  == _stamp(os.stat(run, dir_fd=base, follow_symlinks=False)), "manager_run_changed")
            return rows

        rows = measure()

        def remeasure():
            measure()
            return True

        path = MANAGER_BASE / run / MANAGER_NAME
        module = types.ModuleType("_bound_local_qa_linux_signup_mock_receipt_manager")
        module.__file__ = str(path)
        exec(compile(rows[MANAGER_NAME], str(path), "exec"), module.__dict__)
        _need(callable(getattr(module, "run_admitted_journey", None)), "manager_contract")
        remeasure()
        yield module, remeasure
    finally:
        if run_fd >= 0:
            os.close(run_fd)
        os.close(base)
