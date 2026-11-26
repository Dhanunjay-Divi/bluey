"""One-shot, packet-only preparation transport for the synthetic mock receipt.

The stream carries a sealed packet and a capability bound to a host reservation.
It never carries bootstrap sources: reviewed successor modules must already be
provisioned and held-pinned inside the admitted guest.  Transport authentication,
guest admission, and any later workload completion remain caller authority.
"""
import hashlib
import json
import re


PACKET_SHA256 = "ff7df61110b4521faadef1a10f6f58a5396657d8f8ad537a7ab65648f1bebd29"
PACKET_BYTES = 686_080
CODE_INVENTORY_SHA256 = "99035f76562ad58570f6f79e71907c02e53a0a14d69fae5b2d4a19e439930c56"
BROWSER_INVENTORY_SHA256 = "9ecdff4561fe0924e499325c40a6b61ffbe7ea047044d3f2cc0a89f82e728dce"
SOURCE_TREE = "6fe9eab0180160bc70079f8b9020fa21abf0db62"
SOURCE_INVENTORY_SHA256 = "08459c30b037e839c798b4c5859dcbd2bb271905da36bcc3cdd7aaa4241eb7cc"
NODE_SHA256 = "94efd062ed066c1a0b332e18249859c905e920eeada5df55d24c99b919629a90"
NODE_PATH = "receipt-journey-node/toolchain/bin/node"
PINNED = {
    "packetSha256": PACKET_SHA256,
    "codeInventorySha256": CODE_INVENTORY_SHA256,
    "browserInventorySha256": BROWSER_INVENTORY_SHA256,
    "sourceTree": SOURCE_TREE,
    "sourceInventorySha256": SOURCE_INVENTORY_SHA256,
}
REQUEST_SCHEMA = "bluey-jobs-signup-mock-receipt-preparation-request-v1"
RESPONSE_SCHEMA = "bluey-jobs-signup-mock-receipt-preparation-response-v1"
MAX_HEADER = 4096
SHA = re.compile(r"[0-9a-f]{64}")
RUN = re.compile(r"[0-9a-f]{32}")


def _need(value, label):
    if not value:
        raise ValueError("signup_mock_receipt_transport_refused:" + label)


def _sha(raw):
    return hashlib.sha256(raw).hexdigest()


def wire(value):
    return (json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def _pin(value, width=64):
    return type(value) is str and re.fullmatch(r"[0-9a-f]{%d}" % width, value) is not None and value != "0" * width


def _parse(raw, maximum=MAX_HEADER):
    _need(type(raw) is bytes and 0 < len(raw) <= maximum, "json_size")
    try:
        value = json.loads(raw.decode("ascii"))
        _need(type(value) is dict and wire(value) == raw, "json_canonical")
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError("signup_mock_receipt_transport_refused:json_canonical") from error
    return value


def capability(value):
    _need(type(value) is dict and value.keys() == PINNED.keys() | {"run", "reservationSha256"}
          and _pin(value["run"], 32) and _pin(value["reservationSha256"])
          and all(value[key] == expected for key, expected in PINNED.items()), "capability")
    return dict(value)


def _packet(raw):
    _need(type(raw) is bytes and len(raw) == PACKET_BYTES and _sha(raw) == PACKET_SHA256, "packet_pin")
    return raw


def encode_request(cap, packet):
    cap = capability(cap)
    packet = _packet(packet)
    header = wire({"schema": REQUEST_SCHEMA, "capability": cap, "packetBytes": len(packet)})
    _need(0 < len(header) <= MAX_HEADER, "header_size")
    return len(header).to_bytes(4, "big") + header + packet


def _read_exact(stream, length):
    data = bytearray()
    while len(data) < length:
        requested = min(length - len(data), 64 << 10)
        part = stream.read(requested)
        _need(type(part) is bytes and 0 < len(part) <= requested, "request_truncated")
        data.extend(part)
    return bytes(data)


def read_request(stream, expected_capability):
    """Decode one exact request; no staging or guest actuation occurs here."""
    expected = capability(expected_capability)
    prefix = _read_exact(stream, 4)
    header_size = int.from_bytes(prefix, "big")
    _need(0 < header_size <= MAX_HEADER, "header_size")
    header = _read_exact(stream, header_size)
    value = _parse(header)
    _need(value.keys() == {"schema", "capability", "packetBytes"}
          and value["schema"] == REQUEST_SCHEMA and capability(value["capability"]) == expected
          and type(value["packetBytes"]) is int and value["packetBytes"] == PACKET_BYTES,
          "request_binding")
    packet = _packet(_read_exact(stream, PACKET_BYTES))
    _need(stream.read(1) == b"", "request_extra_bytes")
    frame = encode_request(expected, packet)
    _need(frame == prefix + header + packet, "request_canonical")
    return packet, _sha(frame)


def expected_result(cap):
    cap = capability(cap)
    return {
        "run": cap["run"], "packetSha256": PACKET_SHA256,
        "codeInventorySha256": CODE_INVENTORY_SHA256,
        "actorFiles": 39, "managerFiles": 12,
        "releasePublished": False, "runtimeQualified": False, "launchAdmitted": False,
        "reservationSha256": cap["reservationSha256"], "actorPlanSha256": plan_digest(cap),
        "browserRoot": browser_root(cap), "browserInventorySha256": BROWSER_INVENTORY_SHA256,
    }


def browser_root(cap):
    cap = capability(cap)
    return "/run/bluey-jobs-c2c-browser/" + cap["run"]


def plan_digest(cap):
    cap = capability(cap)
    return _sha(wire({
        "schema": "bluey-jobs-c2c-actor-plan-v1", "run": cap["run"],
        "sourceTree": SOURCE_TREE, "sourceInventorySha256": SOURCE_INVENTORY_SHA256,
        "codeInventorySha256": CODE_INVENTORY_SHA256,
        "node": {"path": browser_root(cap) + "/" + NODE_PATH, "sha256": NODE_SHA256},
    }))


def encode_response(stage, cap, request_sha256):
    expected = expected_result(cap)
    _need(type(stage) is dict and wire(stage) == wire(expected) and _pin(request_sha256), "stage_result")
    return wire({"schema": RESPONSE_SCHEMA, "requestSha256": request_sha256, "stage": expected})


def read_response(raw, cap, request_sha256):
    value = _parse(raw)
    _need(value.keys() == {"schema", "requestSha256", "stage"}
          and value["schema"] == RESPONSE_SCHEMA and value["requestSha256"] == request_sha256
          and encode_response(value["stage"], cap, request_sha256) == raw, "response_binding")
    return dict(value["stage"])


def prepare_at_reserved_guest(binding, packet, *, reserve, exchange):
    """Reserve once and exchange once; ambiguity retains the caller's hold."""
    _need(type(binding) is dict and callable(reserve) and callable(exchange)
          and all(binding.get(key) == expected for key, expected in PINNED.items()), "host_binding")
    projection = binding.get("projection")
    _need(type(projection) is dict and projection.keys() == {"run", "unit", "uid", "gid", "runParent"}
          and _pin(projection["run"], 32)
          and projection["unit"] == "bluey-jobs-qa-" + projection["run"] + ".service"
          and all(type(projection[key]) is int and 0 < projection[key] < 2 ** 31 for key in ("uid", "gid"))
          and type(projection["runParent"]) is str
          and re.fullmatch(r"/tmp/bluey-test-runs-[0-9a-f]{12}", projection["runParent"]) is not None,
          "host_projection")
    packet = _packet(packet)
    hold = reserve(_parse(wire(binding)))
    _need(type(hold) is dict and hold.keys() == {"held", "note_workload", "reservationSha256"}
          and callable(hold["held"]) and callable(hold["note_workload"]) and _pin(hold["reservationSha256"]), "host_hold")
    cap = capability({**PINNED, "run": projection["run"], "reservationSha256": hold["reservationSha256"]})
    frame = encode_request(cap, packet)
    _need(hold["held"]() is True, "hold_before_exchange")
    response = exchange(frame, dict(cap))
    result = read_response(response, cap, _sha(frame))
    _need(hold["held"]() is True, "hold_after_exchange")
    return result
