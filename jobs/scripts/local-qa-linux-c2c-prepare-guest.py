"""Stage the exact Jobs QA guest inputs on a single owner reservation.

This does not provision mounts, accept an unbounded transport, start a unit,
issue release, or remove a partial. The caller provides an independently
live owner reservation and a bounded authenticated input stream.
"""
import hashlib
import io
import os
from pathlib import Path
import platform
import re
import stat
import sys
from types import MappingProxyType, ModuleType


MODULES = MappingProxyType({
    "guest-inputs": ("local-qa-linux-c2c-guest-inputs.py",
                     "9ca4cfde9e34973a32bdf93c3a49d2e9112f034d84ff4a74f2664d6842ed970b"),
    "guest-storage": ("local-qa-linux-c2c-guest-storage.py",
                      "52be4d22a19a07eecaa862ce645ec036e6a4c5d163c52c8cce22308833cea386"),
    "launcher": ("local-qa-linux-c2c-install-launcher.py",
                 "77665a020bacfeaa7ff7f1f6fcb5c6cff48fc2672c4c9eb3c393d9f44fdf2f76"),
    "application": ("local-qa-linux-c2c-install-application.py",
                    "070c53877083969708998fbc18a44acbf461e7550f8e313b3bcbe406f0d3c546"),
    "postgres": ("local-qa-linux-c2c-install-postgres.py",
                 "e0230c011e70e0156dfeee0f27f509911a6ba46c1f5ebf5a2a2107fb6537b2ab"),
    "browser": ("local-qa-linux-c2c-install-browser.py",
                "4ba15742f83d06beb7a699eac4b3c666a6091e2e4b7488e9394b2351f9c7c7da"),
    "browser-reader": ("stage-browser-receipt-runtime.py",
                       "793273419b521bcdecaf4a649fa43d100a8c089940c073b781d522511b4a6936"),
    "transport": ("local-qa-linux-c2c-preparation-transport.py",
                  "9c2fa11ea5b65f55432c1ff3883ac7780eb2d581e7168d27229ec04209605635"),
    "receiver": ("local-qa-linux-c2c-receive-preparation.py",
                 "0eca7bec34b6ba14499468a0f05ebd9297ddc97082cc5c11442e62a8f3d9ae99"),
})
BROWSER = Path("/run/bluey-jobs-c2c-browser")
APPLICATION = Path("/run/bluey-jobs-c2c-application")
POSTGRES = Path("/run/bluey-jobs-c2c-postgres")
LAUNCHER = Path("/opt/bluey-jobs-c2c-launcher")
BOOTSTRAP = Path("/run/bluey-jobs-c2c-bootstrap")
CODE = Path("/run/bluey-jobs-c2c-code")
MANAGER = Path("/run/bluey-jobs-c2c-manager")
GATES = Path("/run/bluey-jobs-c2c-gates")
RD = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
SOURCE_LIMIT = 128 << 10


def need(value, label):
    if not value:
        raise ValueError("c2c_prepare_guest_refused:" + label)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def stamp(row):
    return (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_gid, row.st_nlink,
            row.st_size, row.st_mtime_ns, row.st_ctime_ns)


def initial_guest():
    return (sys.platform == "linux" and platform.machine() == "x86_64"
            and os.getuid() == os.geteuid() == 0
            and all(os.path.samestat(os.stat(f"/proc/self/ns/{kind}"), os.stat(f"/proc/1/ns/{kind}"))
                    for kind in ("mnt", "user", "pid", "net", "cgroup")))


def protected(path, mode=None):
    need(path.is_absolute() and path.resolve(strict=True) == path, "fixed_directory")
    for directory in (path, *path.parents):
        info = directory.lstat()
        need(stat.S_ISDIR(info.st_mode) and info.st_uid == info.st_gid == 0 and not info.st_mode & 0o7022,
             "directory_authority")
        if directory == path and mode is not None:
            need(stat.S_IMODE(info.st_mode) == mode, "directory_mode")


def load(role, *, inert=False):
    name, expected = MODULES[role]
    protected(Path(__file__).resolve(strict=True).parent)
    path = Path(__file__).resolve(strict=True).with_name(name)
    need(type(expected) is str and path.resolve(strict=True) == path, "module_pin")
    with os.fdopen(os.open(path, RD), "rb") as file:
        info = os.fstat(file.fileno())
        need(stat.S_ISREG(info.st_mode) and info.st_uid == info.st_gid == 0
             and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1
             and 0 < info.st_size <= SOURCE_LIMIT, "module_metadata")
        body = file.read(info.st_size + 1)
        need(len(body) == info.st_size and sha(body) == expected
             and stamp(info) == stamp(os.fstat(file.fileno())) == stamp(path.lstat()), "module_bytes")
    if inert:
        return
    module = ModuleType("_bluey_c2c_guest_" + role.replace("-", "_"))
    module.__file__ = str(path)
    exec(compile(body, str(path), "exec"), module.__dict__)
    return module


def projection(value, run):
    need(type(value) is dict and value.keys() == {"run", "unit", "uid", "gid", "runParent"}
         and value["run"] == run and value["unit"] == f"bluey-jobs-qa-{run}.service"
         and all(type(value[x]) is int and 0 < value[x] < 2**31 for x in ("uid", "gid"))
         and type(value["runParent"]) is str
         and re.fullmatch(r"/tmp/bluey-test-runs-[0-9a-f]{12}", value["runParent"]) is not None,
         "actor_projection")
    return dict(value)


def absent(path):
    try:
        path.lstat()
    except FileNotFoundError:
        return
    raise ValueError("c2c_prepare_guest_refused:previous_attempt")


def fresh(run, run_parent):
    for base in (BROWSER, APPLICATION, POSTGRES, LAUNCHER, BOOTSTRAP, CODE, MANAGER, GATES):
        protected(base, 0o700 if base == MANAGER else 0o755)
        absent(base / run)
    temporary = Path("/tmp")
    info = temporary.lstat()
    need(temporary.resolve(strict=True) == temporary and stat.S_ISDIR(info.st_mode)
         and info.st_uid == info.st_gid == 0 and stat.S_IMODE(info.st_mode) == 0o1777, "tmp_authority")
    absent(Path(run_parent))


def stage_expectations(cap, actor, pins):
    run, reservation = cap["run"], cap["reservationSha256"]
    base = {"run": run, "reservationSha256": reservation}
    paths = {"launcher": LAUNCHER / run, "browser": BROWSER / run, "application": APPLICATION / run,
             "postgres": POSTGRES / run / "capsule"}
    launcher = {**base, "actorUid": actor["uid"], "actorGid": actor["gid"],
                "runParent": actor["runParent"], "launcherSha256": pins["launcher.sh"][1]}
    browser = {**base, "browserRoot": str(paths["browser"]), "browserInventorySha256": cap["browserInventorySha256"]}
    pg = {**base, "capsuleRoot": str(paths["postgres"]), "archiveSha256": pins["postgres.tar.gz"][1],
          "manifestSha256": pins["postgres-files.json"][1]}
    application = {**base, "sourceTree": cap["sourceTree"], "nativeReceiptSha256": pins["native-receipt.json"][1],
                   "resolverSha256": pins["resolver.json"][1], "portalReceiptSha256": pins["portal-receipt.json"][1],
                   "portalManifestSha256": pins["portal-manifest.json"][1]}
    scopes = {"launcher": launcher, "browser": browser, "postgres": pg, "application": application, "packet": cap}
    disabled = {"runtimeQualified": False}
    receipts = {
        "launcher": {**launcher, **disabled, "launchAdmitted": False,
                     "sourceRoot": str(paths["launcher"]),
                     "launcherPath": str(paths["launcher"] / "scripts/run-bluey-tests.sh")},
        "browser": {**browser, **disabled, "installed": True, "files": 410, "payloadBytes": 533_939_096,
                    "inventoryFile": str(paths["browser"] / "inventory.json")},
        "postgres": {**pg, **disabled, "installed": True, "files": 1640, "directories": 150,
                     "payloadBytes": 47_718_773, "manifestPath": str(paths["postgres"] / "files.json")},
        "application": {**{key: value for key, value in application.items() if key != "run"}, **disabled,
                        "releasePublished": False, "root": str(paths["application"]),
                        "apiPath": str(paths["application"] / "api/bluey-server"),
                        "fixturePath": str(paths["application"] / "fixture/jobs-nonprod-browser-fixture"),
                        "webDirectory": str(paths["application"] / "portal/web"),
                        "portalManifestPath": str(paths["application"] / "portal/static-manifest.json")},
    }
    return scopes, receipts


def prepare_guest(stream, expected_capability, actor_projection, *, independently_admitted):
    """Stage one admitted guest; return the existing frame and inert receipts."""
    need(initial_guest() and callable(independently_admitted), "platform")
    transport = load("transport")
    cap = transport.capability(expected_capability)
    actor = projection(actor_projection, cap["run"])
    run, reservation = cap["run"], cap["reservationSha256"]
    joined = {**cap, **actor}

    def owner():
        return initial_guest() and independently_admitted(dict(joined)) is True and initial_guest()

    need(owner(), "reservation_before_frame")
    packet, bootstrap, request_digest = transport.read_request(stream, cap)
    canonical = transport.encode_request(cap, packet, bootstrap)
    need(sha(canonical) == request_digest and owner(), "reservation_after_frame")
    storage = load("guest-storage")
    baseline = storage.verify_guest_storage()
    topology = baseline["topology"]
    need(type(topology) is dict and topology.keys() == {"sha256", "value"}
         and type(topology["sha256"]) is str and re.fullmatch(r"[0-9a-f]{64}", topology["sha256"]) is not None,
         "storage_topology")

    def storage_current(*, prepared):
        minimum = storage.RESERVE_BYTES if prepared else storage.REQUIRED_BYTES
        now = storage.verify_guest_storage(expected_topology=topology, minimum_available_bytes=minimum)
        return now["topology"] == topology

    fresh(run, actor["runParent"])
    inputs = load("guest-inputs")
    load("browser-reader", inert=True)
    installers = {name: load(name) for name in ("launcher", "browser", "postgres", "application", "receiver")}
    scopes, expected = stage_expectations(cap, actor, inputs.PINS)
    with inputs.open_intake(run) as (archives, payload, intake_current):
        def current():
            return owner() and intake_current() is True

        def for_role(name):
            return lambda scope: transport.wire(scope) == transport.wire(scopes[name]) and current()

        def checked(name, install):
            need(current(), f"{name}_admission_before")
            receipt = install(for_role(name))
            need(type(receipt) is dict and transport.wire(receipt) == transport.wire(expected[name])
                 and current(), f"{name}_receipt")
            return receipt

        need(storage_current(prepared=False) and current(), "storage_before")
        # Verify fresh targets after all bytes, helper pins and leases are held.
        fresh(run, actor["runParent"])
        receipts = {}
        receipts["browser"] = checked("browser", lambda admit: installers["browser"].install_browser(
            run, reservation, {"playwright": archives["playwright.tgz"], "chrome": archives["chrome.zip"],
                               "node": archives["node.tar.xz"]}, payload["browser-inventory.json"], admitted=admit))
        receipts["postgres"] = checked("postgres", lambda admit: installers["postgres"].install_postgres(
            run, reservation, archives["postgres.tar.gz"], payload["postgres-files.json"], admitted=admit))
        receipts["application"] = checked("application", lambda admit: installers["application"].install_application(
            run=run, reservation_sha256=reservation, native_receipt=payload["native-receipt.json"],
            resolver=payload["resolver.json"], server=payload["bluey-server"], fixture=payload["jobs-nonprod-browser-fixture"],
            portal_archive=payload["portal.tar.gz"], portal_manifest=payload["portal-manifest.json"],
            portal_receipt=payload["portal-receipt.json"], admitted=admit))
        receipts["launcher"] = checked("launcher", lambda admit: installers["launcher"].install_launcher(
            run, reservation, actor["uid"], actor["gid"], actor["runParent"], payload["launcher.sh"], admitted=admit))
        # The existing receiver independently checks the frame and actor/browser pins.
        need(current(), "packet_admission_before")
        response = installers["receiver"].receive_preparation(io.BytesIO(canonical), cap, admitted=for_role("packet"))
        receipts["packet"] = transport.read_response(response, cap, request_digest)
        need(current() and storage_current(prepared=True) and current(), "storage_after")
        return {"receipts": receipts, "preparationResponse": response, "storageTopologySha256": topology["sha256"],
                "launchAdmitted": False, "releasePublished": False, "runtimeQualified": False}
