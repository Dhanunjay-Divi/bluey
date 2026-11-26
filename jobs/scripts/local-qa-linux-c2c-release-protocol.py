#!/usr/bin/env python3
"""Pure wire protocol for the Linux C2C QA actor release barrier.

This module intentionally has no filesystem, process, clock, or network calls.
The waiter and root-owned manager supply all observed values explicitly.
"""
from __future__ import annotations

import hashlib
import json
import re
from typing import Any


_SHA = re.compile(r"^[0-9a-f]{64}$")
_HEX32 = re.compile(r"^[0-9a-f]{32}$")
_NETNS = re.compile(r"^net:\[[1-9][0-9]{0,19}\]$")
_ROOT = re.compile(
    r"^/tmp/bluey-test-runs-[0-9a-f]{12}/run\.[1-9][0-9]{0,19}\.([1-9][0-9]{0,9})\.[0-9]{3,12}$"
)
_MAX_I63 = (1 << 63) - 1
_MAX_UID = (1 << 31) - 1
_FRESH_NS = 90_000_000_000
_WAITING_KEYS = frozenset((
    "schema", "run", "unitName", "bootId", "invocationId", "unitCgroup", "networkNamespace",
    "root", "rootDevice", "rootInode", "markerSha256", "actorUid", "mainPid", "mainStartTime",
    "actorPid", "actorStartTime", "nonce", "waitingNs",
))
_OBSERVED_KEYS = _WAITING_KEYS - {"schema", "nonce", "waitingNs"}
_RELEASE_KEYS = frozenset(("schema", "waitingSha256", "specSha256", "issuedNs"))


def _fail() -> None:
    raise ValueError("local_qa_linux_c2c_release_protocol_refused")


def _json_value(value: Any) -> None:
    if value is None or type(value) in (bool, str):
        return
    if type(value) is int:
        return
    if type(value) is float:
        _fail()
    if type(value) is list:
        for item in value:
            _json_value(item)
        return
    if type(value) is dict and all(type(key) is str for key in value):
        for item in value.values():
            _json_value(item)
        return
    _fail()


def encode(obj: Any) -> bytes:
    """Return the sole accepted JSON wire form: sorted, ASCII, compact, LF."""
    _json_value(obj)
    return (json.dumps(obj, sort_keys=True, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")


def _no_duplicate_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            _fail()
        value[key] = item
    return value


def _decode(raw: bytes) -> dict[str, Any]:
    if type(raw) is not bytes or not raw or len(raw) > 16_384:
        _fail()
    try:
        text = raw.decode("ascii")
        value = json.loads(text, object_pairs_hook=_no_duplicate_object, parse_constant=lambda _: _fail())
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError):
        _fail()
    if type(value) is not dict or encode(value) != raw:
        _fail()
    return value


def _positive(value: Any, maximum: int = _MAX_I63) -> bool:
    return type(value) is int and 0 < value <= maximum


def _waiting(value: dict[str, Any]) -> dict[str, Any]:
    if set(value) != _WAITING_KEYS:
        _fail()
    run = value.get("run")
    unit = value.get("unitName")
    root = value.get("root")
    root_match = _ROOT.fullmatch(root) if type(root) is str else None
    if not (
        value.get("schema") == "bluey-jobs-c2c-waiting-v1"
        and type(run) is str and _HEX32.fullmatch(run) and run != "0" * 32
        and unit == f"bluey-jobs-qa-{run}.service"
        and type(value.get("bootId")) is str and _HEX32.fullmatch(value["bootId"]) and value["bootId"] != "0" * 32
        and type(value.get("invocationId")) is str and _HEX32.fullmatch(value["invocationId"]) and value["invocationId"] != "0" * 32
        and value.get("unitCgroup") == f"/system.slice/{unit}"
        and type(value.get("networkNamespace")) is str and _NETNS.fullmatch(value["networkNamespace"])
        and root_match is not None
        and type(value.get("rootDevice")) is int and 0 <= value["rootDevice"] <= _MAX_I63
        and _positive(value.get("rootInode"))
        and type(value.get("markerSha256")) is str and _SHA.fullmatch(value["markerSha256"])
        and _positive(value.get("actorUid"), _MAX_UID)
        and _positive(value.get("mainPid"), _MAX_UID) and _positive(value.get("mainStartTime"))
        and _positive(value.get("actorPid"), _MAX_UID) and _positive(value.get("actorStartTime"))
        and int(root_match[1]) == value["mainPid"] > 1
        and value["actorPid"] > 1 and value["actorPid"] != value["mainPid"]
        and type(value.get("nonce")) is str and _HEX32.fullmatch(value["nonce"]) and value["nonce"] != "0" * 32
        and _positive(value.get("waitingNs"))
    ):
        _fail()
    return value


def parse_waiting(raw: bytes) -> dict[str, Any]:
    """Parse exactly one canonical waiting record without observing its claims."""
    return dict(_waiting(_decode(raw)))


def _observed(waiting: dict[str, Any], observed: Any) -> None:
    if type(observed) is not dict or set(observed) != _OBSERVED_KEYS:
        _fail()
    for key in _OBSERVED_KEYS:
        if observed[key] != waiting[key] or type(observed[key]) is not type(waiting[key]):
            _fail()


def make_release(waiting_raw: bytes, observed: dict[str, Any], spec_sha256: str, issued_ns: int) -> bytes:
    """Make a canonical release only after the root owner supplies exact observations."""
    waiting = parse_waiting(waiting_raw)
    _observed(waiting, observed)
    if type(spec_sha256) is not str or not _SHA.fullmatch(spec_sha256) or not _positive(issued_ns):
        _fail()
    if issued_ns < waiting["waitingNs"] or issued_ns - waiting["waitingNs"] >= _FRESH_NS:
        _fail()
    return encode({"schema": "bluey-jobs-c2c-release-v1", "waitingSha256": hashlib.sha256(waiting_raw).hexdigest(),
                   "specSha256": spec_sha256, "issuedNs": issued_ns})


def accept_release(raw: bytes, waiting_raw: bytes, now_ns: int) -> dict[str, Any]:
    """Accept one release if it is canonical, bound to waiting bytes, and fresh."""
    waiting = parse_waiting(waiting_raw)
    value = _decode(raw)
    if set(value) != _RELEASE_KEYS or value.get("schema") != "bluey-jobs-c2c-release-v1":
        _fail()
    if not (
        type(value.get("waitingSha256")) is str and value["waitingSha256"] == hashlib.sha256(waiting_raw).hexdigest()
        and type(value.get("specSha256")) is str and _SHA.fullmatch(value["specSha256"])
        and _positive(value.get("issuedNs")) and _positive(now_ns)
        and waiting["waitingNs"] <= value["issuedNs"] <= now_ns < waiting["waitingNs"] + _FRESH_NS
    ):
        _fail()
    return dict(value)
