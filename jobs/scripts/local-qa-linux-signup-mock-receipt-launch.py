"""Join the prepared mock-signup roles to the sealed guest manager.

No CLI, host VM start, provisioning, or production action. This function
requires an independently admitted disposable Linux guest and a live owner
reservation plus one-shot unit custody. Only the sealed manager launches and
releases the synthetic actor. Any failure leaves the owner hold intact.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
from types import ModuleType


HELPERS = {
    "prepare": ("local-qa-linux-signup-mock-receipt-prepare-guest.py",
                "210c61138a65fd337a289ee6149f1f2f5012143adf397fb270012f2f95a794de"),
    "transport": ("local-qa-linux-signup-mock-receipt-transport.py",
                  "01be6d7faf4cce8599844effbc2300d7f96cc249f5aee264d870420bd7bf2ef6"),
    "manager-loader": ("local-qa-linux-signup-mock-receipt-manager-loader.py",
                       "c41a21a41ef632c014bd6c834a25c7fa85d39c69b0b52262615feea03a249a25"),
    "pipes": ("local-qa-linux-c2c-owner-pipes.py",
              "f0e7a0d292832667924bf5442e1210b861891adf3900f0d5b35549334d4a7a29"),
}
LAUNCHER = "8ad1d3f2d3976b88da64a55b1de9111997603115c7a56ae2d5d0a0afdcbb55c8"
WAITER = "8f1f8228b81016f7cfad89568af0ac87e6de0743763bf029cbff0c9793013e49"
TOOLS = {"systemdRunSha256", "systemctlSha256", "pythonPath", "pythonSha256",
         "libraryPath", "librarySha256"}
PORTS = {"apiPort", "postgresPort", "smtpPort", "objectPort", "browserPort"}
WAITING = ("run", "root", "bootId", "invocationId", "mainPid", "unitName", "unitCgroup", "networkNamespace")
COMPOSE_ENTRY = "local-qa-linux-signup-mock-receipt-compose-entry.mjs"
COMPOSE_INVENTORY = "ea21b4ceca1ffa1ce23cd59e217d0f4b8eeb72db11df1d4ba68014f6980f8002"
NODE_SHA = "94efd062ed066c1a0b332e18249859c905e920eeada5df55d24c99b919629a90"
RD = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC


def need(ok, label):
    if not ok:
        raise ValueError("signup_mock_launch_refused:" + label)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def digest(value):
    return type(value) is str and re.fullmatch(r"[a-f0-9]{64}", value) is not None and value != "0" * 64


def initial_guest():
    return (sys.platform == "linux" and os.uname().machine == "x86_64" and os.getuid() == os.geteuid() == 0
            and all(os.path.samestat(os.stat(f"/proc/self/ns/{kind}"), os.stat(f"/proc/1/ns/{kind}"))
                    for kind in ("mnt", "user", "pid", "net", "cgroup")))


def load(role):
    name, expected = HELPERS[role]
    here = Path(__file__).resolve(strict=True).parent
    for parent in (here, *here.parents):
        item = parent.lstat()
        need(stat.S_ISDIR(item.st_mode) and item.st_uid == item.st_gid == 0
             and not item.st_mode & 0o7022, "helper_parent")
    path = here / name
    need(path.resolve(strict=True) == path, "helper_path")
    with os.fdopen(os.open(path, RD), "rb") as source:
        before = os.fstat(source.fileno())
        need(stat.S_ISREG(before.st_mode) and stat.S_IMODE(before.st_mode) == 0o600
             and before.st_uid == before.st_gid == 0 and before.st_nlink == 1
             and 0 < before.st_size <= 128 << 10, "helper_metadata")
        raw = source.read(before.st_size + 1)

        def identity(row):
            return (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_gid, row.st_nlink,
                    row.st_size, row.st_mtime_ns, row.st_ctime_ns)

        need(len(raw) == before.st_size and sha(raw) == expected
             and identity(before) == identity(os.fstat(source.fileno())) == identity(path.lstat()), "helper_pin")
    module = ModuleType("_verified_signup_mock_launch_" + role)
    module.__file__ = str(path)
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


def runtime_config(tools, ports):
    need(type(tools) is dict and tools.keys() == TOOLS and all(
        digest(tools[name]) for name in TOOLS if name.endswith("Sha256")), "tool_pins")
    need(type(tools["pythonPath"]) is str and re.fullmatch(r"/usr/bin/python3\.[0-9]{1,2}", tools["pythonPath"])
         and type(tools["libraryPath"]) is str and re.fullmatch(
             r"/usr/lib/x86_64-linux-gnu/libsystemd\.so\.0\.[0-9]+\.[0-9]+", tools["libraryPath"]), "tool_path")
    need(type(ports) is dict and ports.keys() == PORTS and all(type(n) is int and 1024 < n <= 65535
         and n not in (5187, 5432) for n in ports.values()) and len(set(ports.values())) == len(PORTS), "ports")
    return dict(tools), dict(ports)


def make_observer(manager, actor, systemctl_sha256):
    name, pin = manager.HELPERS["observer"]
    path = Path(manager.__file__).resolve(strict=True).with_name(name)
    _, raw = manager.pinned_file(path, pin, 256 << 10, retain=True)
    scope = {"__name__": "_admitted_mock_launch_observer", "__file__": str(path)}
    exec(compile(raw, str(path), "exec"), scope)
    return scope["make_independent_observer"](dict(actor), systemctl_sha256)


def composer_members(raw):
    need(type(raw) is bytes and len(raw) == 4119 and sha(raw) == COMPOSE_INVENTORY, "composer_inventory")
    rows = json.loads(raw.decode("ascii"))
    need(type(rows) is dict and len(rows) == 38 and COMPOSE_ENTRY in rows and all(
        re.fullmatch(r"[a-z0-9-]+\.mjs", name) is not None and type(pair) is list and len(pair) == 2
        and type(pair[0]) is int and 0 < pair[0] <= 128 << 10 and digest(pair[1])
        for name, pair in rows.items()), "composer_members")
    return rows


def bind_local_composer(run, manager, current):
    """One no-shell Node compose process using only sealed local code and pipes."""
    need(initial_guest() and current() is True and re.fullmatch(r"[a-f0-9]{32}", run) is not None, "composer_admission")
    root = Path(__file__).resolve(strict=True).parent / "compose"
    inventory_path = root / "inventory.json"
    node = Path(f"/run/bluey-jobs-c2c-browser/{run}/receipt-journey-node/toolchain/bin/node")
    measure = manager.pinned_file
    manifest_stamp, manifest = measure(inventory_path, COMPOSE_INVENTORY, 16 << 10, retain=True)
    members = composer_members(manifest)

    def snapshot():
        item = root.lstat()
        need(root.resolve(strict=True) == root and item.st_uid == item.st_gid == 0 and stat.S_ISDIR(item.st_mode)
             and stat.S_IMODE(item.st_mode) == 0o500
             and {path.name for path in root.iterdir()} == set(members) | {"inventory.json"}, "composer_root")
        stamps = {name: measure(root / name, pin, size) for name, (size, pin) in members.items()}
        stamps["inventory.json"] = measure(inventory_path, COMPOSE_INVENTORY, len(manifest))
        need(stamps["inventory.json"] == manifest_stamp and all(
            identity[3:6] == (0, 0, 1) and stat.S_IMODE(identity[2]) == 0o400 for identity in stamps.values()),
            "composer_modes")
        after = root.lstat()
        identity = lambda row: (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_gid, row.st_ctime_ns, row.st_mtime_ns)
        need(identity(item) == identity(after), "composer_root_changed")
        return identity(item), stamps, measure(node, NODE_SHA, 192 << 20, executable=True)

    baseline = snapshot()
    pipes = load("pipes")
    # Limit just this private instance; the shared preparation relay stays at
    # its original request/reply policy.
    pipes.MAX_REQUEST, pipes.MAX_REPLY = 128 << 10, 65_536
    child = None

    def open_local(_cap, _deadline):
        nonlocal child
        need(current() is True and snapshot() == baseline and current() is True, "composer_custody_before")
        child = subprocess.Popen((str(node), str(root / COMPOSE_ENTRY)), cwd="/",
                                 env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"},
                                 bufsize=0, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 close_fds=True)
        return child

    def same_child(client, _cap, _deadline):
        return client is child and current() is True and snapshot() == baseline and current() is True

    exchange = pipes.make_exchange(open_local, same_child, timeout_seconds=10)

    def compose(wire):
        try:
            return exchange(wire, {})
        finally:
            # This is our exact local child, never the managed systemd unit.
            if child is not None and child.poll() is None:
                child.kill()
                child.wait(timeout=2)

    return compose


def run_prepared(prepared, capability, actor_projection, request_sha256, packet, *,
                 tools, ports, independently_admitted, reserve_launch, unit_absent):
    """Run at most one unit under the existing reservation.

    `reserve_launch(actor)` must durably prevent a second unit launch and
    return a scoped hold with the *same* reservation digest, plus `held`
    and `complete` callbacks. `complete` records observed unit teardown;
    it does not release or recycle the guest. The spec is generated by a
    verified, bounded local RPC, rather than by an input-provided callback.
    """
    need(initial_guest() and digest(request_sha256) and all(callable(fn) for fn in
         (independently_admitted, reserve_launch, unit_absent)), "admission")
    tools, ports = runtime_config(tools, ports)
    prep, transport, manager_loader = (load(role) for role in ("prepare", "transport", "manager-loader"))
    cap = transport.capability(capability)
    actor = prep.load_pinned(prep.LEGACY_PREP).projection(actor_projection, cap["run"])
    run, reservation = actor["run"], cap["reservationSha256"]
    joined = {**cap, **actor}
    need(sha(transport.encode_request(cap, packet)) == request_sha256, "request_binding")

    def owner():
        return initial_guest() and independently_admitted(dict(joined)) is True and initial_guest()

    need(owner() and type(prepared) is dict and prepared.keys() == {
        "receipts", "preparationResponse", "storageTopologySha256", "launchAdmitted", "releasePublished",
        "runtimeQualified"} and all(prepared[key] is False for key in
        ("launchAdmitted", "releasePublished", "runtimeQualified")), "prepared")
    response = transport.read_response(prepared["preparationResponse"], cap, request_sha256)
    receipts = prepared["receipts"]
    need(receipts.get("packet") == response and response["actorPlanSha256"] == transport.plan_digest(cap),
         "prepared_plan")
    library_pin, library_path = tools["librarySha256"], tools["libraryPath"]
    systemctl_pin = tools["systemctlSha256"]
    plan_args = dict(run=run, uid=actor["uid"], gid=actor["gid"], run_parent=actor["runParent"],
                     plan_sha256=response["actorPlanSha256"], python_path=tools["pythonPath"])
    pins = dict(systemdRun=tools["systemdRunSha256"], python=tools["pythonSha256"], launcher=LAUNCHER, waiter=WAITER)

    with manager_loader.bind_staged_manager(run, owner) as (manager, remeasure_manager):
        def current():
            return owner() and remeasure_manager() is True and owner()

        def admitted(scope):
            return scope == joined and current()

        need(current(), "manager_custody")
        observe = make_observer(manager, actor, systemctl_pin)
        compose_spec = bind_local_composer(run, manager, current)
        gate = Path(f"/run/bluey-jobs-c2c-gates/{run}/plan.json")
        chrome = Path(response["browserRoot"]) / "inventory.json"
        plan_identity, plan_raw = manager.pinned_file(gate, response["actorPlanSha256"], 4096, retain=True)
        chrome_identity, chrome_raw = manager.pinned_file(chrome, cap["browserInventorySha256"], 1 << 20,
                                                          retain=True)

        def compose(waiting, observed):
            need(current() and observe(dict(waiting)) == observed, "compose_observation")
            need(manager.pinned_file(gate, response["actorPlanSha256"], 4096) == plan_identity
                 and manager.pinned_file(chrome, cap["browserInventorySha256"], 1 << 20) == chrome_identity,
                 "compose_inputs")
            wire = transport.wire({
                "schema": "bluey-jobs-signup-mock-receipt-compose-request-v1",
                "reservationSha256": reservation, "waiting": {name: waiting[name] for name in WAITING},
                "application": receipts["application"], "browser": receipts["browser"], "postgres": receipts["postgres"],
                "codePlanBase64": base64.b64encode(plan_raw).decode("ascii"),
                "chromiumInventoryBase64": base64.b64encode(chrome_raw).decode("ascii"),
                "ports": ports, "systemctlSha256": systemctl_pin,
                "python": {"path": tools["pythonPath"], "sha256": tools["pythonSha256"]},
            })
            raw = compose_spec(wire)
            need(type(raw) is bytes and 0 < len(raw) <= 65_536 and current(), "compose_result")
            return raw

        adapters = prep.bind_prepared_readback(
            prepared, cap, actor, request_sha256, compose=compose, observe=observe, independently_admitted=admitted,
            systemctl_sha256=systemctl_pin, python_path=tools["pythonPath"], python_sha256=tools["pythonSha256"],
        )

        def reserve(projected):
            need(projected == actor and current(), "launch_scope")
            lease = reserve_launch(dict(projected))
            need(type(lease) is dict and lease.keys() == {"reservationSha256", "held", "complete"}
                 and lease["reservationSha256"] == reservation and callable(lease["held"])
                 and callable(lease["complete"]), "launch_hold")

            def held():
                return current() and lease["held"]() is True and current()

            def complete(proof):
                need(held(), "complete_hold")
                manager.zero_proof(proof)
                return lease["complete"](dict(proof)) is True and held()

            return {"held": held, "complete": complete}

        return manager.run_admitted_journey(
            plan_args, pins, helper_pins={name: pin for name, (_, pin) in manager.HELPERS.items()},
            runtime_pins={name: pin for name, (_, pin) in manager.RUNTIME_HELPERS.items()}, systemctl_sha256=systemctl_pin,
            library_path=library_path, library_sha256=library_pin,
            independently_admitted=lambda plan: manager.observer_plan(plan) == actor and current(),
            unit_absent=unit_absent, reserve_guest=reserve, source_tree=cap["sourceTree"],
            source_inventory=cap["sourceInventorySha256"], **adapters,
        )
