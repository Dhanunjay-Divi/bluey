"""Stage the exact current mock-signup QA roles; never launch or publish.

The owner supplies a previously admitted guest, independently live
reservation, bounded packet-only transport and protected helper bundle.
The legacy C2C preparation profile is unchanged.
"""
import hashlib
import os
from pathlib import Path
import re
import stat
import sys
from types import MappingProxyType, ModuleType, SimpleNamespace

LEGACY_PREP = ("local-qa-linux-c2c-prepare-guest.py",
               "a9716028761c3ac53aee746c5a9484a05b1162871ebd8f0a54cd1a51426cecba")
APPLICATION = ("local-qa-linux-signup-mock-receipt-application.py",
               "56d53fb84fdbd72c4a7889d8eea360e221a55850f946d7f414866ac0018c454f")
ACTOR_PLAN = ("local-qa-linux-signup-mock-receipt-actor-plan.py",
              "86a64d324f5430f498f720b6629c71e9675a40a0883aa511b3278674af988349")
TRANSPORT = ("local-qa-linux-signup-mock-receipt-transport.py",
             "01be6d7faf4cce8599844effbc2300d7f96cc249f5aee264d870420bd7bf2ef6")
INVENTORY = ("local-qa-linux-c2c-release-inventory.py",
             "20f4b15c90305cd248ad8068cb32fff1996dd51e463fefb76f9e02f43d6caa1b")
READBACK = ("local-qa-linux-c2c-release-readback.py",
            "009f2000ff484fd18e5c841cee66631a349898616339f66b99237c5c34d3121a")
APPLICATION_INPUTS = MappingProxyType({
    "native-receipt.json": (1274, "8e923b0ec14fccc9ddb1ad13153fd4fd7831825dd73281155fe81f6123f361e3"),
    "bluey-server": (108_786_264, "29a50252ff22e740aea9e90a513a00eb82892849d0abfa52b76b2c27588f8884"),
    "jobs-nonprod-browser-fixture": (87_021_888, "8b10ffebe32a4488964533ac0af5d25911ddafbb389985d01d7c14629cb8fd0c"),
    "portal.tar": (5_509_120, "db31a9a96ca40be18d882516c42c930cdf8f6be02e5add04dee28d289336e0f0"),
})
DERIVED = MappingProxyType({
    "resolver.json": (429, "21a0a4459f07381c5ee6ea65cc4399294d8d8a56c0667da8274955ea9d302e32"),
    "portal-receipt.json": (324, "cbcf82c5cd7468f21945cb042cf8b8298b32c82bb04a3bb92b0cc34acaa1a18a"),
    "portal-manifest.json": (4532, "ba874296d043ceb15ddae0893981b92ed8fd651c46abdb55a46f8b2637c43aa6"),
})
BASE_INTAKE = ("launcher.sh", "browser-inventory.json", "playwright.tgz", "chrome.zip",
               "node.tar.xz", "postgres.tar.gz", "postgres-files.json")
# Exact sizes in bytewise route order for the SHA-pinned 43-file portal.
PORTAL_LENGTHS = (6179, 2142, 192969, 231663, 2038, 46406, 96066, 244075, 62356, 3161, 10652,
                  13299, 23857, 41658, 17452, 45007, 731, 667, 1680, 303, 1030, 443, 119992, 430,
                  202363, 459, 134306, 255206, 346205, 384036, 159673, 390277, 476, 432990,
                  1262398, 73, 449, 307, 30469, 5698, 1377, 15620, 680805)
RD = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC


def need(value, label):
    if not value:
        raise ValueError("signup_mock_prepare_guest_refused:" + label)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def stamp(item):
    return (item.st_dev, item.st_ino, item.st_mode, item.st_uid, item.st_gid, item.st_nlink,
            item.st_size, item.st_mtime_ns, item.st_ctime_ns)


def load_pinned(pin):
    """Only a root-private fixed sibling may supply executable helper bytes."""
    here = Path(__file__).resolve(strict=True).parent
    for parent in (here, *here.parents):
        row = parent.lstat()
        need(stat.S_ISDIR(row.st_mode) and row.st_uid == row.st_gid == 0
             and not row.st_mode & 0o7022, "helper_ancestry")
    leaf, digest = pin
    need(re.fullmatch(r"[a-z0-9-]+\.py", leaf) is not None
         and re.fullmatch(r"[a-f0-9]{64}", digest) is not None, "helper_role")
    path = here / leaf
    need(path.resolve(strict=True) == path, "helper_path")
    with os.fdopen(os.open(path, RD), "rb") as source:
        before = os.fstat(source.fileno())
        need(stat.S_ISREG(before.st_mode) and before.st_uid == before.st_gid == 0
             and stat.S_IMODE(before.st_mode) == 0o600 and before.st_nlink == 1
             and 0 < before.st_size <= 128 << 10, "helper_metadata")
        raw = source.read(before.st_size + 1)
        need(len(raw) == before.st_size and sha(raw) == digest
             and stamp(before) == stamp(os.fstat(source.fileno())) == stamp(path.lstat()), "helper_pin")
    module = ModuleType("_verified_signup_mock_guest_" + leaf.removesuffix(".py").replace("-", "_"))
    module.__file__ = str(path)
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


def intake_profile(inputs):
    """Use the old held-file reader with only the current flat role map."""
    inputs.PINS = MappingProxyType({**{name: inputs.PINS[name] for name in BASE_INTAKE}, **APPLICATION_INPUTS})
    return inputs


def storage_profile(storage):
    """Rebase the floor for larger current files and the later private fixture copy."""
    runtime_fixture = APPLICATION_INPUTS["jobs-nonprod-browser-fixture"][0]
    storage.RESERVE_BYTES += runtime_fixture
    # Keep the previous payload allowance, and reserve a full additional
    # current application plus metadata and code. This is conservative.
    storage.REQUIRED_BYTES += (sum(size for size, _ in APPLICATION_INPUTS.values()) + runtime_fixture + (64 << 20))
    return storage


def stage_expectations(predecessor, cap, actor, pins):
    return predecessor.stage_expectations(cap, actor, {**pins, **DERIVED})


def bind_inventory(inventory, app):
    """Project the installed application and actor trees for later physical readback."""
    inventory.PORTAL_MANIFEST_BYTES, inventory.PORTAL_MANIFEST_SHA = DERIVED["portal-manifest.json"]
    inventory.PORTAL_FILES = 43
    inventory.NATIVE_RECEIPT = APPLICATION_INPUTS["native-receipt.json"]
    inventory.RESOLVER, inventory.PORTAL_RECEIPT = DERIVED["resolver.json"], DERIVED["portal-receipt.json"]
    inventory.SERVER = APPLICATION_INPUTS["bluey-server"]
    inventory.FIXTURE = APPLICATION_INPUTS["jobs-nonprod-browser-fixture"]
    inventory.ACTOR_INVENTORY_SHA, inventory.ACTOR_FILES = app.CODE_INVENTORY, 39
    inventory.SOURCE_TREE, inventory.SOURCE_INVENTORY_SHA = app.SOURCE_TREE, app.SOURCE_INVENTORY
    base_application_tree = inventory.application_tree

    def application_tree(raw):
        dirs, files = base_application_tree(raw)
        static = sorted(name for name in files if name.startswith("portal/web/"))
        need(len(static) == len(PORTAL_LENGTHS), "portal_length_count")
        for name, length in zip(static, PORTAL_LENGTHS):
            kind, mode, bound, digest = files[name]
            need(kind == "file" and mode == 0o444 and bound is None, "portal_length_row")
            files[name] = kind, mode, length, digest
        return dirs, files

    # Preserve the legacy reader's aggregate byte ceiling while measuring
    # this larger pair precisely. No broad increase or per-file relaxation.
    inventory.application_tree = application_tree
    return inventory


def prepare_guest(stream, expected_capability, actor_projection, *, independently_admitted):
    """Stage on one independently reserved Linux guest, with no retry or launch."""
    need(sys.platform == "linux" and os.getuid() == os.geteuid() == 0
         and callable(independently_admitted), "platform")
    legacy = load_pinned(LEGACY_PREP)
    need(legacy.initial_guest(), "platform")
    transport = load_pinned(TRANSPORT)
    cap = transport.capability(expected_capability)
    actor = legacy.projection(actor_projection, cap["run"])
    run, reservation = cap["run"], cap["reservationSha256"]
    owner_scope = {**cap, **actor}

    def owner():
        return legacy.initial_guest() and independently_admitted(dict(owner_scope)) is True and legacy.initial_guest()

    need(owner(), "reservation_before_frame")
    packet, request_digest = transport.read_request(stream, cap)
    need(sha(transport.encode_request(cap, packet)) == request_digest and owner(), "reservation_after_frame")
    storage = storage_profile(legacy.load("guest-storage"))
    baseline = storage.verify_guest_storage(minimum_available_bytes=storage.REQUIRED_BYTES)
    topology = baseline["topology"]
    need(type(topology) is dict and topology.keys() == {"sha256", "value"}
         and type(topology["sha256"]) is str and re.fullmatch(r"[a-f0-9]{64}", topology["sha256"]) is not None,
         "storage_topology")

    def storage_current(minimum):
        return storage.verify_guest_storage(expected_topology=topology,
                                            minimum_available_bytes=minimum)["topology"] == topology

    legacy.fresh(run, actor["runParent"])
    inputs = intake_profile(legacy.load("guest-inputs"))
    legacy.load("browser-reader", inert=True)
    installers = {name: legacy.load(name) for name in ("launcher", "browser", "postgres")}
    app, code = load_pinned(APPLICATION), load_pinned(ACTOR_PLAN)
    scopes, expected = stage_expectations(legacy, cap, actor, inputs.PINS)
    actor_cap = {key: value for key, value in cap.items() if key != "browserInventorySha256"}
    plan_scope = {**cap, "browserRoot": expected["browser"]["browserRoot"]}
    with inputs.open_intake(run) as (archives, payload, intake_current):
        def current():
            return owner() and intake_current() is True

        def for_role(scope):
            return lambda incoming: transport.wire(incoming) == transport.wire(scope) and current()

        def checked(name, install):
            need(current(), name + "_admission_before")
            receipt = install(for_role(scopes[name]))
            need(type(receipt) is dict and transport.wire(receipt) == transport.wire(expected[name]) and current(),
                 name + "_receipt")
            return receipt

        need(storage_current(storage.REQUIRED_BYTES) and current(), "storage_before")
        legacy.fresh(run, actor["runParent"])
        receipts = {}
        receipts["browser"] = checked("browser", lambda admit: installers["browser"].install_browser(
            run, reservation, {"playwright": archives["playwright.tgz"], "chrome": archives["chrome.zip"],
                               "node": archives["node.tar.xz"]}, payload["browser-inventory.json"], admitted=admit))
        receipts["postgres"] = checked("postgres", lambda admit: installers["postgres"].install_postgres(
            run, reservation, archives["postgres.tar.gz"], payload["postgres-files.json"], admitted=admit))
        receipts["application"] = checked("application", lambda admit: app.install_application(
            run=run, reservation_sha256=reservation, source_tree=cap["sourceTree"],
            native_receipt=payload["native-receipt.json"], server=payload["bluey-server"],
            fixture=payload["jobs-nonprod-browser-fixture"], portal=payload["portal.tar"], code=packet, admitted=admit))
        receipts["launcher"] = checked("launcher", lambda admit: installers["launcher"].install_launcher(
            run, reservation, actor["uid"], actor["gid"], actor["runParent"], payload["launcher.sh"], admitted=admit))
        need(current(), "plan_admission_before")
        stage = code.stage_code_and_plan(packet, actor_cap, cap["browserInventorySha256"],
                                         admitted=for_role(plan_scope))
        response = transport.encode_response(stage, cap, request_digest)
        receipts["packet"] = transport.read_response(response, cap, request_digest)
        need(current() and storage_current(storage.RESERVE_BYTES) and current(), "storage_after")
        return {"receipts": receipts, "preparationResponse": response, "storageTopologySha256": topology["sha256"],
                "launchAdmitted": False, "releasePublished": False, "runtimeQualified": False}


def bind_prepared_readback(prepared, capability, actor_projection, request_sha256, *,
                           compose, observe, independently_admitted, systemctl_sha256, python_path, python_sha256):
    """Bind the existing live readback logic to the current prepared roles."""
    need(sys.platform == "linux" and os.getuid() == os.geteuid() == 0, "platform")
    legacy = load_pinned(LEGACY_PREP)
    need(legacy.initial_guest(), "platform")
    reader = load_pinned(READBACK)
    original_load = reader.load
    transport, app = load_pinned(TRANSPORT), load_pinned(APPLICATION)
    inventory = bind_inventory(load_pinned(INVENTORY), app)

    def load_profile(role):
        if role == "transport":
            return transport
        if role == "guest-inputs":
            return intake_profile(legacy.load(role))
        if role == "guest-storage":
            return storage_profile(legacy.load(role))
        raise ValueError("signup_mock_prepare_guest_refused:profile_role")

    def load_reader(role):
        if role == "prepare":
            return SimpleNamespace(load=load_profile, stage_expectations=lambda *args: stage_expectations(legacy, *args),
                                   projection=legacy.projection, CODE=legacy.CODE, GATES=legacy.GATES)
        if role == "inventory":
            return inventory
        return original_load(role)

    # Only this newly loaded reader instance is rebound. Its independent
    # manager observations and descriptor-held artifact checks are unchanged.
    reader.load = load_reader
    return reader.bind_prepared_readback(
        prepared, capability, actor_projection, request_sha256, compose=compose, observe=observe,
        independently_admitted=independently_admitted, systemctl_sha256=systemctl_sha256,
        python_path=python_path, python_sha256=python_sha256,
    )
