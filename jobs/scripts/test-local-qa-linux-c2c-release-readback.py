"""Concrete spec and actor-view admission controls without a Linux unit."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import runpy
from tempfile import TemporaryDirectory
from types import SimpleNamespace as Module
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
S = runpy.run_path(str(HERE / "local-qa-linux-c2c-release-readback.py"))["bind_prepared_readback"].__globals__
P = runpy.run_path(str(HERE / "local-qa-linux-c2c-prepare-guest.py"))
T = runpy.run_path(str(HERE / "local-qa-linux-c2c-preparation-transport.py"))
I = runpy.run_path(str(HERE / "local-qa-linux-c2c-guest-inputs.py"))
RUN = "a" * 32
CAP = {**T["PINNED"], "run": RUN, "reservationSha256": "b" * 64}
PARENT = "/tmp/bluey-test-runs-012345abc123"
ACTOR = {"run": RUN, "uid": 1001, "gid": 1001, "runParent": PARENT, "unit": f"bluey-jobs-qa-{RUN}.service"}
REQUEST = "e" * 64
SYSTEMCTL = "f" * 64
PYTHON = "a" * 64
HELPER = "b" * 64
CODE = f"/run/bluey-jobs-c2c-code/{RUN}"
API = I["PINS"]["bluey-server"][1]
FIXTURE = I["PINS"]["jobs-nonprod-browser-fixture"][1]
TOPOLOGY = {"sha256": "c" * 64, "value": {}}
WAITING = {"run": RUN, "actorUid": 1001, "actorPid": 1234, "mainPid": 1233, "bootId": "d" * 32,
           "invocationId": "e" * 32, "unitName": ACTOR["unit"], "unitCgroup": "/system.slice/" + ACTOR["unit"],
           "root": PARENT + "/run.100.1233.038", "networkNamespace": "net:[81]"}


def wire(value):
    return (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")


class ReleaseReadbackTests(unittest.TestCase):
    def setUp(self):
        _, receipts = P["stage_expectations"](CAP, ACTOR, I["PINS"])
        receipts["packet"] = T["expected_result"](CAP)
        self.prepared = {"receipts": receipts,
                         "preparationResponse": T["encode_response"](receipts["packet"], CAP, REQUEST),
                         "storageTopologySha256": TOPOLOGY["sha256"], "launchAdmitted": False,
                         "releasePublished": False, "runtimeQualified": False}
        app, chrome, pg = [receipts[key] for key in ("application", "browser", "postgres")]
        self.spec = {
            "version": 1, "sourceTree": CAP["sourceTree"], "sourceInventorySha256": CAP["sourceInventorySha256"],
            "unit": {**{k: WAITING[k] for k in ("unitName", "bootId", "invocationId", "mainPid")},
                     "systemctl": {"path": "/usr/bin/systemctl", "sha256": SYSTEMCTL}},
            "binding": {**{k: WAITING[k] for k in ("root", "unitCgroup", "networkNamespace")},
                "buildBinding": {"qualificationOnly": True, "version": 1, "artifactPath": app["apiPath"],
                                 "artifactSha256": API, "manifestFile": app["portalManifestPath"],
                                 "manifestSha256": app["portalManifestSha256"], "sourceSha256": CAP["sourceInventorySha256"]},
                "pidfdToolchain": {"python": {"path": "/usr/bin/python3.12", "sha256": PYTHON},
                                   "helper": {"path": f"{CODE}/{S['PIDFD']}", "sha256": HELPER}},
                "pgCapsule": {"run": RUN, "capsuleRoot": pg["capsuleRoot"], "archiveSha256": pg["archiveSha256"],
                              "manifestSha256": pg["manifestSha256"]}},
            "fixture": {"artifactPath": WAITING["root"] + "/fixtures/jobs-nonprod-browser-fixture", "artifactSha256": FIXTURE},
            "browser": {"port": 53141, "webDirectory": app["webDirectory"], "closureRoot": chrome["browserRoot"],
                        "inventoryFile": chrome["inventoryFile"], "inventorySha256": chrome["browserInventorySha256"]},
        }
        self.live, self.live_owner = True, True
        self.measurements = []
        self.prior = object()

    @contextmanager
    def view(self, path, *, proc_root=False):
        self.assertEqual(proc_root, path != "/")
        yield path

    def measure(self, view, _, *, baseline=None):
        self.measurements.append((view, baseline))
        return self.prior

    def prepare_stubs(self):
        support = {"transport": Module(**T), "guest-inputs": Module(PINS=I["PINS"]),
                   "guest-storage": Module(RESERVE_BYTES=1, verify_guest_storage=lambda **_: {"topology": TOPOLOGY})}
        files = Module(inventory=lambda value: value, measure=self.measure,
                       plan_leaf=lambda *args, **kwargs: "plan fingerprint")
        rows = {CODE: ({".": 0o755}, {S["PIDFD"]: ("file", 0o444, 1, HELPER)})}
        return {"prepare": Module(**{**P, "load": support.__getitem__}),
                "inventory": Module(expected_trees=lambda *args, **kwargs: rows, expected_plan=lambda _: b"plan"),
                "readback": files}

    @contextmanager
    def binding(self):
        support = self.prepare_stubs()
        with patch.dict(S, {"load": support.__getitem__, "read_manifest": lambda *a, **k: b"manifest",
                            "initial": lambda: True, "view_at": self.view, "executable_view": lambda *a: None}):
            yield S["bind_prepared_readback"](self.prepared, CAP, ACTOR, REQUEST,
                 compose=lambda *_: wire(self.spec), observe=self.observe,
                 independently_admitted=lambda scope: self.live_owner and scope == {**CAP, **ACTOR},
                 systemctl_sha256=SYSTEMCTL, python_path="/usr/bin/python3.12", python_sha256=PYTHON)

    def observe(self, waiting):
        return {"observedRoot": waiting["root"], "sameActor": self.live}

    def test_capture_before_waiting_and_use_the_actor_view_on_every_manager_recheck(self):
        with self.binding() as adapters:
            self.assertEqual(self.measurements, [("/", None)])
            spec = adapters["compose"](WAITING, self.observe(WAITING))
            self.assertEqual(spec, wire(self.spec))
            self.assertEqual(adapters["fixture_path"], self.prepared["receipts"]["application"]["fixturePath"])
            for _ in range(8):
                self.assertTrue(adapters["verify_admitted_artifacts"](WAITING, S["sha"](spec)))
            self.assertEqual(self.measurements[1:], [("/proc/1234/root", self.prior)] * 8)

    def test_existing_manager_accepts_the_adapters_without_loosening_its_staging_gates(self):
        manager = runpy.run_path(str(HERE / "local-qa-linux-c2c-guest-manager.py"))
        waiting_raw = T["wire"](WAITING)
        events = []

        def stage(waiting, *, spec_bytes, spec_sha256, fixture_path, fixture_sha256, independently_admitted):
            self.assertEqual(waiting, WAITING)
            self.assertEqual(spec_sha256, S["sha"](spec_bytes))
            self.assertEqual(fixture_path, self.prepared["receipts"]["application"]["fixturePath"])
            for _ in range(2):
                self.assertTrue(independently_admitted(WAITING))
            events.append("staged")
            return {"specSha256": spec_sha256, "fixtureSha256": fixture_sha256, "releasePublished": False}

        def publisher(raw, digest, *, independently_observe, verify_admitted_artifacts):
            self.assertEqual(raw, waiting_raw)
            self.assertEqual(independently_observe(WAITING), self.observe(WAITING))
            self.assertTrue(verify_admitted_artifacts(WAITING, digest))
            events.append("publisher_guard_only")
            return {"simulated": True}

        with self.binding() as adapters:
            prepare, publish = manager["bind_preparation"](
                **adapters, parse_waiting=lambda raw: json.loads(raw), observe=self.observe, stage=stage,
                verify_staged=lambda waiting, stamp: waiting == WAITING and stamp["releasePublished"] is False,
                issue_release=publisher, source_tree=CAP["sourceTree"], source_inventory=CAP["sourceInventorySha256"])
            digest = prepare(WAITING, self.observe(WAITING))
            self.assertEqual(publish(waiting_raw, digest), {"simulated": True})
            self.assertEqual([path for path, _ in self.measurements], ["/"] + ["/proc/1234/root"] * 5)
            self.assertEqual(events, ["staged", "publisher_guard_only"])

    def test_drift_in_layout_will_not_bind_even_if_waiting_identity_is_correct(self):
        variants = (
            lambda value: value["binding"]["buildBinding"].update(artifactPath="/foreign/bluey-server"),
            lambda value: value["binding"]["pidfdToolchain"]["helper"].update(path="/foreign/pidfd.py"),
            lambda value: value["browser"].update(inventoryFile="/foreign/inventory.json"),
            lambda value: value["binding"]["pgCapsule"].update(run="b" * 32),
        )
        original = wire(self.spec)
        for change in variants:
            with self.subTest(change=change), self.binding() as adapters:
                self.spec = json.loads(original)
                change(self.spec)
                with self.assertRaisesRegex(ValueError, "spec_application|spec_tools"):
                    adapters["compose"](WAITING, self.observe(WAITING))
                with self.assertRaisesRegex(ValueError, "verify_before_compose"):
                    adapters["verify_admitted_artifacts"](WAITING, S["sha"](wire(self.spec)))
        self.assertTrue(all(view == "/" for view, _ in self.measurements))

    def test_changed_spec_or_process_or_reservation_refuses_before_actor_view_open(self):
        with self.binding() as adapters:
            digest = S["sha"](adapters["compose"](WAITING, self.observe(WAITING)))
            for changed, request in (({**WAITING, "actorPid": 1235}, digest), (WAITING, REQUEST)):
                with self.assertRaisesRegex(ValueError, "actor_observation"):
                    adapters["verify_admitted_artifacts"](changed, request)
            self.live = False
            with self.assertRaisesRegex(ValueError, "actor_observation"):
                adapters["verify_admitted_artifacts"](WAITING, digest)
            self.live, self.live_owner = True, False
            with self.assertRaisesRegex(ValueError, "verify_before_compose"):
                adapters["verify_admitted_artifacts"](WAITING, digest)
        self.assertEqual(self.measurements, [("/", None)])

    def test_noexec_or_different_guest_device_refuses_actor_exec_view(self):
        path = "/run/pinned/server"
        row = {"device": "254:1", "source": "/dev/vda1", "fstype": "ext4",
               "options": {"ro", "nosuid", "nodev"}, "superoptions": {"rw"}}
        baseline = {"value": {"paths": {"/run/pinned": dict(row)}}}
        storage = Module(read_bounded_file=lambda *_: b"kernel snapshot", MOUNTINFO_LIMIT=128 << 10,
                         parse_mountinfo=lambda *_: [], mounted_at=lambda *_: row)
        S["executable_view"](storage, WAITING["actorPid"], {path: "/run/pinned"}, baseline)
        for change in ({"options": {"ro", "noexec"}}, {"device": "250:1"}):
            with self.subTest(change=change):
                previous = dict(row)
                row.update(change)
                with self.assertRaisesRegex(ValueError, "actor_exec_mount"):
                    S["executable_view"](storage, WAITING["actorPid"], {path: "/run/pinned"}, baseline)
                row.update(previous)


class RealInventoryTests(unittest.TestCase):
    def test_exact_five_trees_include_the_actual_chromium_and_replay_actual_actor_files(self):
        evidence = Path(os.environ["C2C_GUEST_INPUTS_EVIDENCE"])
        def tools(name):
            return runpy.run_path(str(HERE / f"local-qa-linux-c2c-{name}.py"))
        project = tools("release-inventory")
        readback = tools("artifact-readback")
        packet = tools("code-packet")
        stage = tools("stage-code-packet")
        plan = project["expected_plan"](RUN)
        self.assertEqual(S["sha"](plan), T["plan_digest"](CAP))
        raw_packet = Path(os.environ["C2C_CODE_PACKET"]).read_bytes()
        actor, _, _ = packet["verify_packet"](raw_packet, CAP["packetSha256"])
        pg = evidence / "parallel-drafts-phase657/parent-bootstrap/phase660-pgdg-noble-intake-v1/capsule/files.json"
        raw = {"browser": (evidence / "jobs-c2c-browser-inert-manifest-Auv0nTdj/inventory.json").read_bytes(),
               "postgres": pg.read_bytes(),
               "portal": (evidence / "jobs-8e971541-static-85c8691f2da7/static-manifest.json").read_bytes(),
               "actor": actor["inventory.json"], "plan": plan}
        trees = project["expected_trees"](RUN, **raw)
        self.assertEqual(sum(len(dirs) for dirs, _ in trees.values()), 208)
        self.assertEqual(sum(len(leaves) for _, leaves in trees.values()), 2137)
        browser = f"/run/bluey-jobs-c2c-browser/{RUN}"
        self.assertEqual(trees[browser][1][project["CHROME"]][2], 278_568_152)
        self.assertEqual(len(readback["inventory"](trees)), 5)
        for name, data in raw.items():
            with self.subTest(manifest=name):
                drift = data[:-1] + bytes([data[-1] ^ 1])
                with self.assertRaises(ValueError):
                    project["expected_trees"](RUN, **{**raw, name: drift})
        with TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            roles = base / "run" / "bluey-jobs-c2c-code"
            roles.mkdir(parents=True)
            parent = os.open(roles, readback["DIR"])
            try:
                # Darwin inherits the destination directory group, which need
                # not be the launcher's effective primary group.
                staged = stage["_stage"](parent, RUN, actor, 0o755, os.getuid(), os.fstat(parent).st_gid)
                os.close(staged)
            finally:
                os.close(parent)
            closed = readback["inventory"]({CODE: trees[CODE]})
            view = os.open(base, readback["DIR"])
            try:
                seen = readback["measure"](view, closed, owner=os.getuid())
                self.assertEqual(len(seen[CODE]), 33)
                self.assertEqual(readback["measure"](view, closed, baseline=seen, owner=os.getuid()), seen)
            finally:
                os.close(view)


if __name__ == "__main__":
    unittest.main()
