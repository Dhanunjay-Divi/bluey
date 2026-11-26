#!/usr/bin/env python3
"""Root-only pre-release staging for the dedicated synthetic Linux C2C actor.

The function never publishes a release.  Its return value is a staging
fingerprint, not evidence that the actor will later see unchanged inputs; the
root release predicate must re-check it immediately before publication.
"""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import platform
import re
import stat
from typing import Any, Callable


_SHA = re.compile(r"^[0-9a-f]{64}$")
_ROOT = re.compile(r"^/tmp/bluey-test-runs-[0-9a-f]{12}/run\.[1-9][0-9]{0,18}\.[1-9][0-9]{0,9}\.[0-9]{1,19}$")
_FIXTURE_DIR = "fixtures"
_FIXTURE_NAME = "jobs-nonprod-browser-fixture"
_SPEC_NAME = ".local-qa-linux-c2c-native.json"
_MARKER = ".bluey-test-run-v2"
_MAX_SPEC = 65_536
_MAX_FIXTURE = 192 << 20
_RD = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
_DIR = _RD | os.O_DIRECTORY


def _fail(code: str = "local_qa_linux_c2c_stage_refused") -> None:
    raise RuntimeError(code)


def _identity(item: os.stat_result) -> tuple[int, ...]:
    return (item.st_dev, item.st_ino, item.st_uid, item.st_gid, item.st_mode, item.st_nlink,
            item.st_size, item.st_mtime_ns, item.st_ctime_ns)


def _need(value: bool) -> None:
    if not value:
        _fail()


def _read_fd(fd: int, maximum: int) -> tuple[bytes, os.stat_result]:
    before = os.fstat(fd)
    _need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and 0 < before.st_size <= maximum)
    parts: list[bytes] = []
    total = 0
    while True:
        part = os.read(fd, min(65536, maximum + 1 - total))
        if not part:
            break
        total += len(part)
        _need(total <= maximum)
        parts.append(part)
    raw = b"".join(parts)
    _need(len(raw) == before.st_size and _identity(before) == _identity(os.fstat(fd)))
    return raw, before


def _open_actor_root(waiting: dict[str, Any]) -> int:
    """Open the target mount view; only the proc root magic link is intentional."""
    # /proc/<pid>/root is the one intentional magic link.  Applying
    # O_NOFOLLOW here would reject every valid Linux actor before we can bind
    # its mount view; every path component below it remains descriptor-relative
    # and O_NOFOLLOW.
    fd = os.open(f"/proc/{waiting['actorPid']}/root", os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NONBLOCK)
    try:
        for component in waiting["root"].lstrip("/").split("/"):
            _need(component not in ("", ".", ".."))
            child = os.open(component, _DIR, dir_fd=fd)
            os.close(fd)
            fd = child
        return fd
    except BaseException:
        os.close(fd)
        raise


def _validate_waiting(waiting: Any) -> dict[str, Any]:
    required = {"root", "rootDevice", "rootInode", "markerSha256", "actorUid", "actorPid",
                "mainPid", "run", "unitName", "bootId", "invocationId", "unitCgroup",
                "networkNamespace", "mainStartTime", "actorStartTime", "nonce", "waitingNs", "schema"}
    _need(type(waiting) is dict and set(waiting) == required
          and waiting["schema"] == "bluey-jobs-c2c-waiting-v1"
          and type(waiting["root"]) is str and _ROOT.fullmatch(waiting["root"])
          and type(waiting["actorUid"]) is int and 0 < waiting["actorUid"] <= (1 << 31) - 1
          and type(waiting["actorPid"]) is int and waiting["actorPid"] > 1
          and type(waiting["rootDevice"]) is int and 0 <= waiting["rootDevice"] < 1 << 63
          and type(waiting["rootInode"]) is int and 0 < waiting["rootInode"] < 1 << 63
          and type(waiting["markerSha256"]) is str and _SHA.fullmatch(waiting["markerSha256"]))
    return waiting


def _write_new(parent: int, name: str, raw: bytes, *, uid: int, mode: int, maximum: int) -> tuple[int, ...]:
    _need(name and "/" not in name and 0 < len(raw) <= maximum)
    fd = os.open(name, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, mode, dir_fd=parent)
    try:
        total = 0
        while total < len(raw):
            amount = os.write(fd, raw[total:])
            _need(amount > 0)
            total += amount
        _need(total == len(raw))
        os.fchmod(fd, mode)
        os.fchown(fd, uid, -1)
        os.fsync(fd)
        before = os.fstat(fd)
        _need(stat.S_ISREG(before.st_mode) and before.st_uid == uid and stat.S_IMODE(before.st_mode) == mode
              and before.st_nlink == 1 and before.st_size == len(raw)
              and hashlib.sha256(raw).hexdigest() == hashlib.sha256(os.pread(fd, len(raw) + 1, 0)).hexdigest())
        named = os.stat(name, dir_fd=parent, follow_symlinks=False)
        _need(_identity(before) == _identity(named))
        return _identity(before)
    finally:
        os.close(fd)


def _read_source(path: Path, expected: str) -> bytes:
    _need(path.is_absolute() and path.name == _FIXTURE_NAME and _SHA.fullmatch(expected)
          and all(part not in ("", ".", "..") for part in path.parts[1:]))
    chain: list[int] = []
    leaf = -1
    try:
        fd = os.open("/", _DIR)
        chain.append(fd)
        info = os.fstat(fd)
        _need(info.st_uid == 0 and stat.S_ISDIR(info.st_mode) and not (info.st_mode & 0o022))
        for part in path.parts[1:-1]:
            fd = os.open(part, _DIR, dir_fd=fd)
            chain.append(fd)
            info = os.fstat(fd)
            _need(info.st_uid == 0 and stat.S_ISDIR(info.st_mode) and not (info.st_mode & 0o022))
        leaf = os.open(path.name, _RD, dir_fd=fd)
    finally:
        for opened in reversed(chain):
            os.close(opened)
    try:
        _need(leaf >= 0)
        raw, info = _read_fd(leaf, _MAX_FIXTURE)
        _need(info.st_uid == 0 and not (info.st_mode & 0o022) and bool(info.st_mode & 0o111)
              and hashlib.sha256(raw).hexdigest() == expected)
        return raw
    finally:
        if leaf >= 0:
            os.close(leaf)


def _held_leaf(parent: int, name: str, maximum: int, uid: int, mode: int,
               expected_sha256: str, expected_identity: tuple[int, ...]) -> None:
    fd = os.open(name, _RD, dir_fd=parent)
    try:
        raw, info = _read_fd(fd, maximum)
        named = os.stat(name, dir_fd=parent, follow_symlinks=False)
        _need(info.st_uid == uid and stat.S_IMODE(info.st_mode) == mode
              and hashlib.sha256(raw).hexdigest() == expected_sha256
              and _identity(info) == expected_identity == _identity(named))
    finally:
        os.close(fd)


def stage_actor_inputs(waiting: dict[str, Any], *, spec_bytes: bytes, spec_sha256: str,
                       fixture_path: str | Path, fixture_sha256: str,
                       independently_admitted: Callable[[dict[str, Any]], bool]) -> dict[str, Any]:
    """Stage canonical spec plus the admitted C2C seeder without granting release.

    The caller must immediately re-verify the returned identities before its
    separate release callback publishes the actor's release record.
    """
    _need(platform.system() == "Linux" and platform.machine() == "x86_64"
          and os.getuid() == os.geteuid() == 0 and callable(independently_admitted))
    waiting = _validate_waiting(waiting)
    _need(independently_admitted(dict(waiting)) is True
          and type(spec_bytes) is bytes and 0 < len(spec_bytes) <= _MAX_SPEC
          and type(spec_sha256) is str and _SHA.fullmatch(spec_sha256)
          and hashlib.sha256(spec_bytes).hexdigest() == spec_sha256
          and type(fixture_sha256) is str and _SHA.fullmatch(fixture_sha256))
    source = _read_source(Path(fixture_path), fixture_sha256)
    root_fd = _open_actor_root(waiting)
    try:
        root = os.fstat(root_fd)
        _need(stat.S_ISDIR(root.st_mode) and root.st_uid == waiting["actorUid"]
              and stat.S_IMODE(root.st_mode) == 0o700 and root.st_dev == waiting["rootDevice"]
              and root.st_ino == waiting["rootInode"])
        marker_fd = os.open(_MARKER, _RD, dir_fd=root_fd)
        try:
            marker, marker_info = _read_fd(marker_fd, 8192)
            _need(marker_info.st_uid == waiting["actorUid"] and stat.S_IMODE(marker_info.st_mode) == 0o600
                  and hashlib.sha256(marker).hexdigest() == waiting["markerSha256"])
        finally:
            os.close(marker_fd)
        for name in (_SPEC_NAME, _FIXTURE_DIR):
            try:
                os.stat(name, dir_fd=root_fd, follow_symlinks=False)
            except FileNotFoundError:
                continue
            _fail()
        # The directory remains root-owned while bytes are written. It becomes
        # actor-owned only after both contents and directory metadata are fsynced.
        os.mkdir(_FIXTURE_DIR, 0o700, dir_fd=root_fd)
        fixture_fd = os.open(_FIXTURE_DIR, _DIR, dir_fd=root_fd)
        try:
            fixtures = os.fstat(fixture_fd)
            _need(stat.S_ISDIR(fixtures.st_mode) and fixtures.st_uid == 0 and stat.S_IMODE(fixtures.st_mode) == 0o700)
            fixture_identity = _write_new(fixture_fd, _FIXTURE_NAME, source, uid=waiting["actorUid"], mode=0o700,
                                          maximum=_MAX_FIXTURE)
            os.fsync(fixture_fd)
            os.fchown(fixture_fd, waiting["actorUid"], -1)
            os.fchmod(fixture_fd, 0o700)
            os.fsync(fixture_fd)
            final_fixtures = os.fstat(fixture_fd)
            _need(final_fixtures.st_uid == waiting["actorUid"] and stat.S_IMODE(final_fixtures.st_mode) == 0o700)
        finally:
            os.close(fixture_fd)
        spec_identity = _write_new(root_fd, _SPEC_NAME, spec_bytes, uid=waiting["actorUid"], mode=0o600,
                                   maximum=_MAX_SPEC)
        os.fsync(root_fd)
        _need(independently_admitted(dict(waiting)) is True)
        final_root = os.fstat(root_fd)
        _need(stat.S_ISDIR(final_root.st_mode) and final_root.st_uid == waiting["actorUid"]
              and stat.S_IMODE(final_root.st_mode) == 0o700)
        return {"fixturePath": f"{waiting['root']}/{_FIXTURE_DIR}/{_FIXTURE_NAME}",
                "fixtureSha256": fixture_sha256, "fixtureIdentity": fixture_identity,
                "specPath": f"{waiting['root']}/{_SPEC_NAME}", "specSha256": spec_sha256,
                "specIdentity": spec_identity, "rootIdentity": _identity(final_root),
                "markerIdentity": _identity(marker_info), "fixtureDirectoryIdentity": _identity(final_fixtures),
                "releasePublished": False}
    finally:
        os.close(root_fd)


def verify_staged_actor_inputs(waiting: dict[str, Any], fingerprint: dict[str, Any]) -> bool:
    """Read back staged inputs through the actor mount immediately before release.

    This performs no writes and does not publish a release.  The root caller
    still owns live PID/unit re-observation and the release decision.
    """
    waiting = _validate_waiting(waiting)
    required = {"fixturePath", "fixtureSha256", "fixtureIdentity", "specPath", "specSha256",
                "specIdentity", "rootIdentity", "markerIdentity", "fixtureDirectoryIdentity", "releasePublished"}
    _need(type(fingerprint) is dict and set(fingerprint) == required and fingerprint["releasePublished"] is False
          and fingerprint["fixturePath"] == f"{waiting['root']}/{_FIXTURE_DIR}/{_FIXTURE_NAME}"
          and fingerprint["specPath"] == f"{waiting['root']}/{_SPEC_NAME}"
          and type(fingerprint["fixtureSha256"]) is str and _SHA.fullmatch(fingerprint["fixtureSha256"])
          and type(fingerprint["specSha256"]) is str and _SHA.fullmatch(fingerprint["specSha256"])
          and all(type(fingerprint[key]) is tuple for key in
                  ("fixtureIdentity", "specIdentity", "rootIdentity", "markerIdentity", "fixtureDirectoryIdentity")))
    root_fd = _open_actor_root(waiting)
    try:
        root = os.fstat(root_fd)
        _need(_identity(root) == fingerprint["rootIdentity"] and root.st_uid == waiting["actorUid"]
              and stat.S_ISDIR(root.st_mode) and stat.S_IMODE(root.st_mode) == 0o700
              and root.st_dev == waiting["rootDevice"] and root.st_ino == waiting["rootInode"])
        _held_leaf(root_fd, _MARKER, 8192, waiting["actorUid"], 0o600,
                   waiting["markerSha256"], fingerprint["markerIdentity"])
        _held_leaf(root_fd, _SPEC_NAME, _MAX_SPEC, waiting["actorUid"], 0o600,
                   fingerprint["specSha256"], fingerprint["specIdentity"])
        fixtures_fd = os.open(_FIXTURE_DIR, _DIR, dir_fd=root_fd)
        try:
            fixtures = os.fstat(fixtures_fd)
            _need(_identity(fixtures) == fingerprint["fixtureDirectoryIdentity"] and fixtures.st_uid == waiting["actorUid"]
                  and stat.S_ISDIR(fixtures.st_mode) and stat.S_IMODE(fixtures.st_mode) == 0o700)
            _held_leaf(fixtures_fd, _FIXTURE_NAME, _MAX_FIXTURE, waiting["actorUid"], 0o700,
                       fingerprint["fixtureSha256"], fingerprint["fixtureIdentity"])
        finally:
            os.close(fixtures_fd)
        return True
    finally:
        os.close(root_fd)
