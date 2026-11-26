"""Bound one C2C preparation request and response to a host reservation.

Only accepts the exact reviewed code and bootstrap bytes. Authentication,
deadlines and independent live guest admission belong to the selected owner
transport; a structurally valid frame is not proof of any of those things.
"""
import hashlib
import json
import re


PACKET_SHA = "3dc5fe64624b64d57939d3c525cd00a1e3f6c027fcb505eea03f0390e5a7ef25"
PACKET_BYTES = 552_960
CODE_INVENTORY_SHA = "a33491cd0e3a60b971450534e5dc9eab8895d61e81fb2bff26cd013a79628e3f"
BROWSER_INVENTORY_SHA = "9ecdff4561fe0924e499325c40a6b61ffbe7ea047044d3f2cc0a89f82e728dce"
SOURCE_TREE = "8e971541d3ab8d1b8f5c4c9eebafa8fec81406d5"
SOURCE_INVENTORY_SHA = "a7dd3071bd03f53718ceb9db8b4de9571fe0aa1a6ef6e29f840a2b20e6a89ea5"
NODE_SHA = "94efd062ed066c1a0b332e18249859c905e920eeada5df55d24c99b919629a90"
NODE = "receipt-journey-node/toolchain/bin/node"
BOOTSTRAP = {
    "local-qa-linux-c2c-actor-plan.py": "3c7846a6b6a442229b8fbefedb10d5de89b658fd531e3c0807225689332f7676",
    "local-qa-linux-c2c-stage-code-packet.py": "1941e8ae352b1d3e0dd4cbf676e2b3d4f9c76a66d54c59a59d15afc1ab08a661",
    "local-qa-linux-c2c-code-packet.py": "694adfec5310035d94858d626fa5f84c2e093347f0f5fec6c020b03fff0a9870",
}
PINNED = {"packetSha256": PACKET_SHA, "codeInventorySha256": CODE_INVENTORY_SHA,
          "browserInventorySha256": BROWSER_INVENTORY_SHA, "sourceTree": SOURCE_TREE,
          "sourceInventorySha256": SOURCE_INVENTORY_SHA}
MAX_HEADER = 4096
MAX_BOOTSTRAP_FILE = 64 << 10


def need(ok, label):
    if not ok:
        raise ValueError("c2c_preparation_transport_refused:" + label)


def pin(raw, width=64):
    return type(raw) is str and re.fullmatch(f"[0-9a-f]{{{width}}}", raw) and raw != "0" * width


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def wire(value):
    return (json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def parse(raw, maximum):
    need(type(raw) is bytes and 0 < len(raw) <= maximum, "json_size")
    try:
        value = json.loads(raw.decode("ascii"))
        need(type(value) is dict and wire(value) == raw, "json_canonical")
    except (ValueError, RecursionError) as error:
        raise ValueError("c2c_preparation_transport_refused:json_canonical") from error
    return value


def capability(value):
    need(type(value) is dict and value.keys() == PINNED.keys() | {"run", "reservationSha256"}
         and pin(value["run"], 32) and pin(value["reservationSha256"])
         and all(type(value[k]) is str and value[k] == expected for k, expected in PINNED.items()), "capability")
    return dict(value)


def inputs(packet, bootstrap):
    need(type(packet) is bytes and len(packet) == PACKET_BYTES and sha(packet) == PACKET_SHA, "packet_pin")
    need(type(bootstrap) is dict and bootstrap.keys() == BOOTSTRAP.keys(), "bootstrap_names")
    retained = dict(bootstrap)
    for name, expected in BOOTSTRAP.items():
        raw = retained[name]
        need(type(raw) is bytes and 0 < len(raw) <= MAX_BOOTSTRAP_FILE and sha(raw) == expected, "bootstrap_pin")
    return retained


def encode_request(cap, packet, bootstrap):
    cap = capability(cap)
    bootstrap = inputs(packet, bootstrap)
    header = wire({"schema": "bluey-jobs-c2c-preparation-request-v1", "capability": cap,
                   "packetBytes": len(packet), "bootstrapBytes": [len(bootstrap[name]) for name in BOOTSTRAP]})
    need(0 < len(header) <= MAX_HEADER, "header_size")
    return len(header).to_bytes(4, "big") + header + packet + b"".join(bootstrap[name] for name in BOOTSTRAP)


def read_exact(stream, length):
    data = bytearray()
    while len(data) < length:
        count = min(length - len(data), 64 << 10)
        chunk = stream.read(count)
        need(type(chunk) is bytes and 0 < len(chunk) <= count, "request_truncated")
        data.extend(chunk)
    return bytes(data)


def read_request(stream, expected_capability):
    """Read one request from an already bounded transport. Never actuates."""
    expected = capability(expected_capability)
    prefix = read_exact(stream, 4)
    size = int.from_bytes(prefix, "big")
    need(0 < size <= MAX_HEADER, "header_size")
    header = read_exact(stream, size)
    value = parse(header, MAX_HEADER)
    need(value.keys() == {"schema", "capability", "packetBytes", "bootstrapBytes"}
         and value["schema"] == "bluey-jobs-c2c-preparation-request-v1" and capability(value["capability"]) == expected
         and type(value["packetBytes"]) is int and value["packetBytes"] == PACKET_BYTES,
         "request_binding")
    lengths = value["bootstrapBytes"]
    need(type(lengths) is list and len(lengths) == len(BOOTSTRAP)
         and all(type(count) is int and 0 < count <= MAX_BOOTSTRAP_FILE for count in lengths), "bootstrap_lengths")
    packet = read_exact(stream, PACKET_BYTES)
    bootstrap = {name: read_exact(stream, count) for name, count in zip(BOOTSTRAP, lengths)}
    trailing = stream.read(1)
    need(type(trailing) is bytes and trailing == b"", "request_extra_bytes")
    canonical = encode_request(expected, packet, bootstrap)
    need(canonical[:4 + size] == prefix + header, "request_canonical")
    return packet, bootstrap, sha(canonical)


def browser_root(cap):
    return "/run/bluey-jobs-c2c-browser/" + cap["run"]


def plan_digest(cap):
    cap = capability(cap)
    return sha(wire({"schema": "bluey-jobs-c2c-actor-plan-v1", "run": cap["run"], "sourceTree": SOURCE_TREE,
                     "sourceInventorySha256": SOURCE_INVENTORY_SHA, "codeInventorySha256": CODE_INVENTORY_SHA,
                     "node": {"path": browser_root(cap) + "/" + NODE, "sha256": NODE_SHA}}))


def expected_result(cap):
    cap = capability(cap)
    return {"run": cap["run"], "packetSha256": PACKET_SHA, "codeInventorySha256": CODE_INVENTORY_SHA,
            "actorFiles": 31, "managerFiles": 11, "releasePublished": False, "runtimeQualified": False,
            "reservationSha256": cap["reservationSha256"], "actorPlanSha256": plan_digest(cap),
            "browserRoot": browser_root(cap), "browserInventorySha256": BROWSER_INVENTORY_SHA,
            "launchAdmitted": False}


def encode_response(stage_result, expected_capability, request_sha256):
    expected = expected_result(expected_capability)
    need(type(stage_result) is dict and wire(stage_result) == wire(expected) and pin(request_sha256), "stage_result")
    return wire({"schema": "bluey-jobs-c2c-preparation-response-v1", "requestSha256": request_sha256,
                 "stage": expected})


def read_response(raw, cap, request_sha256):
    value = parse(raw, MAX_HEADER)
    need(value.keys() == {"schema", "requestSha256", "stage"} and value["requestSha256"] == request_sha256
         and encode_response(value["stage"], cap, request_sha256) == raw, "response_binding")
    return dict(value["stage"])


def prepare_at_reserved_guest(binding, packet, bootstrap, *, reserve, exchange):
    """One exchange via the separately authenticated/bounded owner transport.

    ``reserve`` must durably reserve the exact host binding before returning;
    the existing host reservation primitive provides this capability. Neither
    success nor failure here calls ``note_workload`` or releases the guest.
    This function has no retry path, including on ambiguous exchange failure.
    """
    bootstrap = inputs(packet, bootstrap)
    need(type(binding) is dict and all(callable(item) for item in (reserve, exchange))
         and binding.get("packetSha256") == PACKET_SHA and binding.get("sourceTree") == SOURCE_TREE
         and binding.get("sourceInventorySha256") == SOURCE_INVENTORY_SHA, "host_binding")
    scope = parse(wire(binding), MAX_HEADER)
    plan = scope.get("projection")
    need(type(plan) is dict and plan.keys() == {"run", "unit", "uid", "gid", "runParent"}
         and pin(plan["run"], 32) and plan["unit"] == f"bluey-jobs-qa-{plan['run']}.service"
         and all(type(plan[key]) is int and 0 < plan[key] < 2**31 for key in ("uid", "gid"))
         and type(plan["runParent"]) is str and re.fullmatch(r"/tmp/bluey-test-runs-[a-f0-9]{12}", plan["runParent"]),
         "host_projection")
    hold = reserve(parse(wire(scope), MAX_HEADER))
    need(type(hold) is dict and hold.keys() == {"held", "note_workload", "reservationSha256"}
         and all(callable(hold[k]) for k in ("held", "note_workload")), "host_hold")
    cap = capability({**PINNED, "run": plan["run"], "reservationSha256": hold["reservationSha256"]})
    frame = encode_request(cap, packet, bootstrap)
    need(hold["held"]() is True, "hold_before_exchange")
    response = exchange(frame, dict(cap))
    result = read_response(response, cap, sha(frame))
    need(hold["held"]() is True, "hold_after_exchange")
    return result
