"""Verify and stage one fixed signup/mock-receipt code packet.

This successor intentionally shares no verifier or mutable packet contract with
the protected C2C path.  It only accepts the sealed candidate below, writes no
files unless it is executing as the isolated Linux root manager, and never
executes actor or manager code.
"""
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import tarfile
import types


ACTOR_BASE = Path("/run/bluey-jobs-c2c-code")
MANAGER_BASE = Path("/run/bluey-jobs-c2c-manager")
STAGER = Path(__file__).resolve().with_name("local-qa-linux-c2c-stage-code-packet.py")
STAGER_SHA256 = "1941e8ae352b1d3e0dd4cbf676e2b3d4f9c76a66d54c59a59d15afc1ab08a661"
PACKET_SHA256 = "ff7df61110b4521faadef1a10f6f58a5396657d8f8ad537a7ab65648f1bebd29"
PACKET_BYTES = 686_080
SOURCE_TREE = "6fe9eab0180160bc70079f8b9020fa21abf0db62"
SOURCE_INVENTORY = "08459c30b037e839c798b4c5859dcbd2bb271905da36bcc3cdd7aaa4241eb7cc"
CODE_INVENTORY = "99035f76562ad58570f6f79e71907c02e53a0a14d69fae5b2d4a19e439930c56"
SCHEMA = "bluey-jobs-signup-mock-receipt-code-candidate-v1"
ACTOR_COUNT = 39
MANAGER_COUNT = 12
MAX_MEMBER = 128 << 10
SHA = re.compile(r"^[0-9a-f]{64}$")
TREE = re.compile(r"^[0-9a-f]{40}$")
RUN = re.compile(r"^[0-9a-f]{32}$")
NAME = re.compile(r"^[a-z][a-z0-9-]{1,96}\.(?:py|mjs)$")
RD = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK


def _need(value, label):
    if not value:
        raise RuntimeError("local_qa_linux_signup_mock_receipt_code_stage_refused:" + label)


def _sha(raw):
    return hashlib.sha256(raw).hexdigest()


def _wire(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def _stable(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode,
            info.st_nlink, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def _read_member(archive, expected_name, expected_mode, expected_size=None, expected_sha=None):
    entry = archive.next()
    _need(entry is not None and entry.name == expected_name and entry.isfile()
          and entry.mode == expected_mode and entry.uid == entry.gid == entry.mtime == 0
          and not entry.pax_headers and not entry.linkname and 0 < entry.size <= MAX_MEMBER
          and (expected_size is None or entry.size == expected_size), "packet_member")
    stream = archive.extractfile(entry)
    _need(stream is not None, "packet_member_stream")
    with stream:
        body = stream.read(entry.size + 1)
    _need(len(body) == entry.size and (expected_sha is None or _sha(body) == expected_sha), "packet_member_body")
    return body


def _inventory(raw):
    try:
        value = json.loads(raw.decode("ascii"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("local_qa_linux_signup_mock_receipt_code_stage_refused:inventory_json") from error
    _need(type(value) is dict and set(value) == {"version", "files"} and value["version"] == 1
          and type(value["files"]) is list and len(value["files"]) == ACTOR_COUNT and _wire(value) == raw,
          "inventory_shape")
    rows = []
    seen = set()
    for row in value["files"]:
        _need(type(row) is dict and set(row) == {"name", "bytes", "sha256"}
              and type(row["name"]) is str and NAME.fullmatch(row["name"]) is not None
              and row["name"] not in seen and type(row["bytes"]) is int
              and 0 < row["bytes"] <= MAX_MEMBER and type(row["sha256"]) is str
              and SHA.fullmatch(row["sha256"]) is not None, "inventory_row")
        seen.add(row["name"])
        rows.append((row["name"], row["bytes"], row["sha256"]))
    _need(rows == sorted(rows), "inventory_order")
    return rows


def verify_packet(raw, expected_sha256=PACKET_SHA256):
    """Return exact actor/manager bytes after closed packet validation only."""
    _need(type(raw) is bytes and type(expected_sha256) is str and expected_sha256 == PACKET_SHA256
          and len(raw) == PACKET_BYTES and _sha(raw) == PACKET_SHA256, "packet_pin")
    try:
        with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
            meta_raw = _read_member(archive, "candidate.json", 0o444)
            try:
                meta = json.loads(meta_raw.decode("ascii"))
            except (UnicodeDecodeError, json.JSONDecodeError) as error:
                raise RuntimeError("local_qa_linux_signup_mock_receipt_code_stage_refused:packet_json") from error
            _need(type(meta) is dict and _wire(meta) == meta_raw and meta == {
                "schema": SCHEMA, "basePacketSha256": "fffacd898962bf5d4bc86e09dc6e51879d7088aa79ca6866c647800056fe6af3",
                "baseCodeInventorySha256": "30be415457e3a83607c0d2a49b542b9ebb95937a767227555deffec6c8de9fd1",
                "codeInventorySha256": CODE_INVENTORY, "actorFiles": ACTOR_COUNT,
                "managerFiles": MANAGER_COUNT, "actor": "local-qa-linux-signup-mock-receipt-native.mjs",
                "waiter": "local-qa-linux-signup-mock-receipt-wait-actor.py",
                "manager": "local-qa-linux-signup-mock-receipt-guest-manager.py",
                "physicalSourceAdmitted": False, "releaseAdmitted": False, "runtimeQualified": False,
            }, "packet_meta")
            inventory_raw = _read_member(archive, "actor/inventory.json", 0o444, expected_sha=CODE_INVENTORY)
            rows = _inventory(inventory_raw)
            actor = {"inventory.json": inventory_raw}
            for name, size, digest in rows:
                actor[name] = _read_member(archive, "actor/" + name, 0o444, size, digest)
            manager = {}
            for _ in range(MANAGER_COUNT):
                entry = archive.next()
                _need(entry is not None and entry.name.startswith("manager/")
                      and NAME.fullmatch(entry.name[8:]) is not None and entry.name[8:] not in manager
                      and entry.isfile() and entry.mode == 0o600 and entry.uid == entry.gid == entry.mtime == 0
                      and not entry.pax_headers and not entry.linkname and 0 < entry.size <= MAX_MEMBER,
                      "manager_member")
                stream = archive.extractfile(entry)
                _need(stream is not None, "manager_stream")
                with stream:
                    body = stream.read(entry.size + 1)
                _need(len(body) == entry.size, "manager_body")
                manager[entry.name[8:]] = body
            _need(archive.next() is None and set(actor) & set(manager) == {"local-qa-linux-c2c-release-protocol.py"},
                  "packet_extra")
    except (OSError, tarfile.TarError, EOFError) as error:
        raise RuntimeError("local_qa_linux_signup_mock_receipt_code_stage_refused:packet_archive") from error
    receipt = {
        "schema": SCHEMA, "packetSha256": PACKET_SHA256, "sourceTree": SOURCE_TREE,
        "sourceInventorySha256": SOURCE_INVENTORY, "codeInventorySha256": CODE_INVENTORY,
        "actorFiles": ACTOR_COUNT, "managerFiles": MANAGER_COUNT,
        "releaseAdmitted": False, "runtimeQualified": False,
    }
    return actor, manager, receipt


def _capability(value):
    keys = {"run", "reservationSha256", "packetSha256", "codeInventorySha256", "sourceTree", "sourceInventorySha256"}
    _need(type(value) is dict and set(value) == keys and type(value["run"]) is str
          and RUN.fullmatch(value["run"]) is not None and value["run"] != "0" * 32
          and value["packetSha256"] == PACKET_SHA256 and value["codeInventorySha256"] == CODE_INVENTORY
          and value["sourceTree"] == SOURCE_TREE and value["sourceInventorySha256"] == SOURCE_INVENTORY
          and type(value["reservationSha256"]) is str and SHA.fullmatch(value["reservationSha256"]) is not None
          and value["reservationSha256"] != "0" * 64, "capability")
    return dict(value)


def _load_protected_stager():
    """Load only the immutable protected filesystem primitives.

    This module never invokes the protected packet verifier or its staging
    entrypoint, so the predecessor's old packet contract remains untouched.
    """
    _need(STAGER.is_absolute() and STAGER.resolve(strict=True) == STAGER, "stager_path")
    fd = os.open(STAGER, RD)
    try:
        before = os.fstat(fd)
        _need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and not before.st_mode & 0o022
              and before.st_uid in (0, os.geteuid()) and 0 < before.st_size <= 64 << 10, "stager_metadata")
        raw = os.read(fd, before.st_size + 1)
        _need(len(raw) == before.st_size and _sha(raw) == STAGER_SHA256
              and _stable(before) == _stable(os.fstat(fd)) == _stable(STAGER.lstat()), "stager_pin")
    finally:
        os.close(fd)
    module = types.ModuleType("_pinned_local_qa_linux_c2c_stage")
    module.__file__ = str(STAGER)
    exec(compile(raw, str(STAGER), "exec"), module.__dict__)
    _need(all(callable(getattr(module, name, None)) for name in
              ("_root_linux", "_open_base", "_base_current", "_absent", "_stage", "_verify_stage")),
          "stager_contract")
    return module


def stage_code_packet(raw, expected_sha256, capability, *, admitted):
    """Stage only the fixed candidate after independent root-manager admission."""
    stager = _load_protected_stager()
    _need(stager._root_linux() and callable(admitted), "platform")
    cap = _capability(capability)
    actor, manager, receipt = verify_packet(raw, expected_sha256)
    _need(receipt == {"schema": SCHEMA, "packetSha256": cap["packetSha256"], "sourceTree": cap["sourceTree"],
                      "sourceInventorySha256": cap["sourceInventorySha256"], "codeInventorySha256": cap["codeInventorySha256"],
                      "actorFiles": ACTOR_COUNT, "managerFiles": MANAGER_COUNT, "releaseAdmitted": False,
                      "runtimeQualified": False}, "receipt")
    _need(admitted(dict(cap)) is True and stager._root_linux(), "admission_before")
    actor_base = stager._open_base(ACTOR_BASE, 0o755)
    manager_base = -1
    actor_run = manager_run = -1
    try:
        manager_base = stager._open_base(MANAGER_BASE, 0o700)
        stager._absent(actor_base, cap["run"])
        stager._absent(manager_base, cap["run"])
        stager._base_current(ACTOR_BASE, actor_base, 0o755)
        stager._base_current(MANAGER_BASE, manager_base, 0o700)
        actor_run = stager._stage(actor_base, cap["run"], actor, 0o755)
        manager_run = stager._stage(manager_base, cap["run"], manager, 0o700)
        _need(admitted(dict(cap)) is True and stager._root_linux(), "admission_after")
        stager._base_current(ACTOR_BASE, actor_base, 0o755)
        stager._base_current(MANAGER_BASE, manager_base, 0o700)
        stager._verify_stage(actor_base, cap["run"], actor_run, actor, 0o755)
        stager._verify_stage(manager_base, cap["run"], manager_run, manager, 0o700)
        return {"run": cap["run"], "packetSha256": PACKET_SHA256, "codeInventorySha256": CODE_INVENTORY,
                "actorFiles": ACTOR_COUNT, "managerFiles": MANAGER_COUNT,
                "releasePublished": False, "runtimeQualified": False}
    finally:
        if actor_run >= 0:
            os.close(actor_run)
        if manager_run >= 0:
            os.close(manager_run)
        os.close(actor_base)
        if manager_base >= 0:
            os.close(manager_base)
