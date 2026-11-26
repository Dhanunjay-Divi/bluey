"""Build the Jobs QA manager's compose and installed-artifact predicates.

Capture is done before the waiting-unit deadline. The publication callback
only rechecks identity in the independently measured live actor view. It
does not stage the spec or fixture, send any signal, or publish release.
"""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import stat
import sys
from types import MappingProxyType, ModuleType


SUPPORT = MappingProxyType({
    "prepare": ("local-qa-linux-c2c-prepare-guest.py",
                "a9716028761c3ac53aee746c5a9484a05b1162871ebd8f0a54cd1a51426cecba"),
    "readback": ("local-qa-linux-c2c-artifact-readback.py",
                 "96b78118f0b6f254698c4ab4fa06f87bb7918d85d60e6d1b630d91c92f6d307f"),
    "inventory": ("local-qa-linux-c2c-release-inventory.py",
                  "20f4b15c90305cd248ad8068cb32fff1996dd51e463fefb76f9e02f43d6caa1b"),
})
SOURCE = 128 << 10
READ = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
DIR = READ | os.O_DIRECTORY
DIGEST = re.compile(r"[a-f0-9]{64}")
RUN = re.compile(r"[a-f0-9]{32}")
SERVICE_ROOT = re.compile(r"/tmp/bluey-test-runs-[a-f0-9]{12}/run\.[1-9][0-9]{0,19}\.[1-9][0-9]{0,9}\.[0-9]{3,12}")
PIDFD = "local-qa-linux-postmaster-pidfd.py"


def need(value, label):
    if not value:
        raise ValueError("c2c_release_readback_refused:" + label)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def stamp(row):
    return (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_gid,
            row.st_nlink, row.st_size, row.st_mtime_ns, row.st_ctime_ns)


def initial():
    return (sys.platform == "linux" and platform.machine() == "x86_64"
            and os.getuid() == os.geteuid() == 0
            and all(os.path.samestat(os.stat(f"/proc/self/ns/{name}"), os.stat(f"/proc/1/ns/{name}"))
                    for name in ("mnt", "pid", "user", "net", "cgroup")))


def load(role):
    name, expected = SUPPORT[role]
    path = Path(__file__).resolve(strict=True).with_name(name)
    need(type(expected) is str and path.resolve(strict=True) == path, "support_path")
    for parent in path.parents:
        row = parent.lstat()
        need(stat.S_ISDIR(row.st_mode) and row.st_uid == 0 and not row.st_mode & 0o7022, "support_parent")
    with os.fdopen(os.open(path, READ), "rb") as file:
        row = os.fstat(file.fileno())
        need(stat.S_ISREG(row.st_mode) and row.st_uid == row.st_gid == 0 and row.st_nlink == 1
             and stat.S_IMODE(row.st_mode) == 0o600 and 0 < row.st_size <= SOURCE, "support_metadata")
        raw = file.read(row.st_size + 1)
        need(len(raw) == row.st_size and sha(raw) == expected
             and stamp(row) == stamp(os.fstat(file.fileno())) == stamp(path.lstat()), "support_bytes")
    module = ModuleType("_bluey_c2c_release_" + role)
    module.__file__ = str(path)
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


@contextmanager
def view_at(path, *, proc_root=False):
    # /proc/<verified-pid>/root is the only intentional magic link.
    flags = DIR & ~os.O_NOFOLLOW if proc_root else DIR
    fd = os.open(path, flags)
    try:
        yield fd
    finally:
        os.close(fd)


def read_manifest(files, view, parent, name, limit=1 << 20):
    with files.directory_at(view, parent, owner=0) as fd:
        with os.fdopen(os.open(name, READ, dir_fd=fd), "rb") as file:
            row = os.fstat(file.fileno())
            need(stat.S_ISREG(row.st_mode) and row.st_uid == 0 and row.st_nlink == 1
                 and stat.S_IMODE(row.st_mode) == 0o444 and 0 < row.st_size <= limit, "manifest_metadata")
            raw = file.read(row.st_size + 1)
            named = os.stat(name, dir_fd=fd, follow_symlinks=False)
            need(len(raw) == row.st_size and stamp(row) == stamp(os.fstat(file.fileno())) == stamp(named),
                 "manifest_custody")
            return raw


def validate_spec(raw, waiting, cap, receipts, keys):
    """Check that the composed spec consumes the paths and pins just measured."""
    need(type(raw) is bytes and 0 < len(raw) <= 65_536, "spec_size")
    value = json.loads(raw.decode("ascii"))
    need(type(value) is dict and set(value) == {"unit", "binding", "fixture", "browser", "sourceTree",
                                               "sourceInventorySha256", "version"}
         and raw == (json.dumps(value, separators=(",", ":"), ensure_ascii=True) + "\n").encode("ascii")
         and type(value["version"]) is int and value["version"] == 1
         and value["sourceTree"] == cap["sourceTree"]
         and value["sourceInventorySha256"] == cap["sourceInventorySha256"], "spec_identity")
    root, run = waiting["root"], waiting["run"]
    unit, binding, browser = value["unit"], value["binding"], value["browser"]
    fixture = f"{root}/fixtures/jobs-nonprod-browser-fixture"
    app, chrome, pg = (receipts[name] for name in ("application", "browser", "postgres"))
    need(type(unit) is dict and unit == {**{name: waiting[name] for name in
                                           ("unitName", "bootId", "invocationId", "mainPid")},
                                        "systemctl": keys["systemctl"]}
         and type(binding) is dict and all(binding.get(name) == waiting[name] for name in
                                           ("root", "unitCgroup", "networkNamespace"))
         and binding.get("buildBinding") == {"qualificationOnly": True, "version": 1,
               "artifactPath": app["apiPath"], "artifactSha256": keys["api"],
               "manifestFile": app["portalManifestPath"], "manifestSha256": app["portalManifestSha256"],
               "sourceSha256": cap["sourceInventorySha256"]}
         and binding.get("pidfdToolchain") == {"python": keys["python"], "helper": keys["pidfd"]}
         and value["fixture"] == {"artifactPath": fixture, "artifactSha256": keys["fixture"]},
         "spec_application")
    pg_plan = binding.get("pgCapsule")
    need(type(browser) is dict and browser.keys() == {"port", "webDirectory", "closureRoot", "inventoryFile", "inventorySha256"}
         and browser["webDirectory"] == app["webDirectory"] and browser["closureRoot"] == chrome["browserRoot"]
         and browser["inventoryFile"] == chrome["inventoryFile"]
         and browser["inventorySha256"] == chrome["browserInventorySha256"]
         and type(pg_plan) is dict and pg_plan.get("run") == run and pg_plan.get("capsuleRoot") == pg["capsuleRoot"]
         and pg_plan.get("archiveSha256") == pg["archiveSha256"] and pg_plan.get("manifestSha256") == pg["manifestSha256"],
         "spec_tools")
    return sha(raw)


def executable_view(storage, pid, paths, baseline):
    """Mount IDs may be cloned by systemd, but base device and exec must agree."""
    rows = storage.parse_mountinfo(storage.read_bounded_file(f"/proc/{pid}/mountinfo", storage.MOUNTINFO_LIMIT))
    for path, owner_path in paths.items():
        row = storage.mounted_at(rows, path)
        expected = baseline["value"]["paths"][owner_path]
        need(row["device"] == expected["device"] and row["source"] == expected["source"]
             and row["fstype"] == expected["fstype"] and "noexec" not in (row["options"] | row["superoptions"]),
             "actor_exec_mount")


def waiting_in_run(waiting, actor):
    need(type(waiting) is dict and waiting.get("run") == actor["run"]
         and waiting.get("unitName") == actor["unit"] and waiting.get("actorUid") == actor["uid"]
         and waiting.get("unitCgroup") == "/system.slice/" + actor["unit"]
         and type(waiting.get("actorPid")) is int and 1 < waiting["actorPid"] < 2**31
         and type(waiting.get("root")) is str and SERVICE_ROOT.fullmatch(waiting["root"]) is not None
         and str(Path(waiting["root"]).parent) == actor["runParent"], "actor_scope")


def bind_prepared_readback(prepared, capability, actor_projection, request_sha256, *,
                           compose, observe, independently_admitted, systemctl_sha256, python_path, python_sha256):
    """Return keyword adapters for the existing guest manager without launching it."""
    need(initial() and all(callable(fn) for fn in (compose, observe, independently_admitted))
         and all(type(value) is str and DIGEST.fullmatch(value) for value in (systemctl_sha256, python_sha256))
         and type(python_path) is str and re.fullmatch(r"/usr/bin/python3\.[0-9]{1,2}", python_path),
         "admission")
    prep, files, expected = (load(name) for name in ("prepare", "readback", "inventory"))
    transport, inputs, storage = (prep.load(name) for name in ("transport", "guest-inputs", "guest-storage"))
    cap = transport.capability(capability)
    run = cap["run"]
    actor = prep.projection(actor_projection, run)
    joined = {**cap, **actor}

    def owner():
        return initial() and independently_admitted(dict(joined)) is True and initial()

    need(owner() and type(prepared) is dict and prepared.keys() == {
        "receipts", "preparationResponse", "storageTopologySha256", "launchAdmitted", "releasePublished", "runtimeQualified",
    } and all(prepared[key] is False for key in ("launchAdmitted", "releasePublished", "runtimeQualified")),
         "prepared_scope")
    _, receipts = prep.stage_expectations(cap, actor, inputs.PINS)
    receipts["packet"] = transport.read_response(prepared["preparationResponse"], cap, request_sha256)
    need(transport.wire(receipts) == transport.wire(prepared["receipts"]), "prepared_receipts")
    disk = storage.verify_guest_storage(minimum_available_bytes=storage.RESERVE_BYTES)["topology"]
    need(disk["sha256"] == prepared["storageTopologySha256"] and owner(), "prepared_storage")
    app = receipts["application"]
    chrome = receipts["browser"]
    pg = receipts["postgres"]
    code = str(prep.CODE / run)
    gate = str(prep.GATES / run)
    plan = expected.expected_plan(run)
    with view_at("/") as view:
        role_trees = expected.expected_trees(run,
            browser=read_manifest(files, view, chrome["browserRoot"], "inventory.json"),
            postgres=read_manifest(files, view, pg["capsuleRoot"], "files.json"),
            portal=read_manifest(files, view, str(Path(app["portalManifestPath"]).parent), "static-manifest.json"),
            actor=read_manifest(files, view, code, "inventory.json"),
            plan=read_manifest(files, view, gate, "plan.json", limit=4096))
        layout = files.inventory(role_trees)
        baseline = files.measure(view, layout)
        gate_baseline = files.plan_leaf(view, gate, plan)
    storage.verify_guest_storage(expected_topology=disk, minimum_available_bytes=storage.RESERVE_BYTES)
    need(owner(), "prepared_after_hash")
    keys = {"systemctl": {"path": "/usr/bin/systemctl", "sha256": systemctl_sha256},
            "python": {"path": python_path, "sha256": python_sha256},
            "pidfd": {"path": f"{code}/{PIDFD}", "sha256": role_trees[code][1][PIDFD][3]},
            "api": inputs.PINS["bluey-server"][1], "fixture": inputs.PINS["jobs-nonprod-browser-fixture"][1]}
    executables = {
        path + "/" + rel: str(Path(path).parent) if path != pg["capsuleRoot"] else str(Path(path).parents[1])
        for path, (_, leaves) in role_trees.items()
        for rel, (category, mode, _, _) in leaves.items()
        if category == "file" and mode == 0o555
    }
    selected = None
    attempted = False

    def checked_compose(waiting, observation):
        nonlocal attempted, selected
        need(not attempted and owner(), "compose_once")
        attempted = True
        waiting_in_run(waiting, actor)
        need(observe(dict(waiting)) == observation, "compose_observation")
        raw = compose(dict(waiting), dict(observation))
        digest = validate_spec(raw, waiting, cap, receipts, keys)
        need(owner() and observe(dict(waiting)) == observation, "compose_changed")
        selected = (transport.wire(waiting), transport.wire(observation), digest)
        return raw

    def verify_admitted_artifacts(waiting, spec_digest):
        need(selected is not None and owner(), "verify_before_compose")
        waiting_in_run(waiting, actor)
        identity, observation, digest = selected
        def current():
            return (owner() and transport.wire(waiting) == identity and spec_digest == digest
                    and transport.wire(observe(dict(waiting))) == observation)
        need(current(), "actor_observation")
        pid = waiting["actorPid"]
        with view_at(f"/proc/{pid}/root", proc_root=True) as view:
            files.measure(view, layout, baseline=baseline)
            files.plan_leaf(view, gate, plan, baseline=gate_baseline)
            paths = {**executables, waiting["root"] + "/fixtures/jobs-nonprod-browser-fixture": "/tmp"}
            executable_view(storage, pid, paths, disk)
            need(current(), "actor_changed")
        return True

    return {"compose": checked_compose, "verify_admitted_artifacts": verify_admitted_artifacts,
            "fixture_path": app["fixturePath"], "fixture_sha256": keys["fixture"]}
