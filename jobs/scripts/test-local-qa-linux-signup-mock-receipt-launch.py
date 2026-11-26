"""Portable tests of the protected launch join; process and release are doubles."""
import base64
from contextlib import contextmanager
import io
import json
import os
from pathlib import Path
import runpy
import subprocess
import tarfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ENTRY = "local-qa-linux-signup-mock-receipt-compose-entry.mjs"
SPEC = "local-qa-linux-signup-mock-receipt-spec.mjs"
L = runpy.run_path(str(HERE / "local-qa-linux-signup-mock-receipt-launch.py"))
RUN = "a" * 32
UNIT = f"bluey-jobs-qa-{RUN}.service"
PARENT = "/tmp/bluey-test-runs-012345abc123"
PROJECTION = dict(run=RUN, unit=UNIT, uid=1001, gid=1001, runParent=PARENT)
WAIT = dict(run=RUN, root=PARENT + "/run.100.1233.038", bootId="b"*32, invocationId="d"*32, mainPid=1233,
            unitName=UNIT, unitCgroup=f"/system.slice/{UNIT}", networkNamespace="net:[81]", actorPid=1234)
PORTS = dict(apiPort=43107, postgresPort=43108, smtpPort=43109, objectPort=43110, browserPort=43111)
TOOLS = dict(systemdRunSha256="2"*64, systemctlSha256="3"*64, pythonPath="/usr/bin/python3.12",
             pythonSha256="4"*64, libraryPath="/usr/lib/x86_64-linux-gnu/libsystemd.so.0.38.0", librarySha256="5"*64)
ZERO = dict(unitStopped=True, actorProcesses=0, namespaceProcesses=0)


def loaded(name):
    return SimpleNamespace(**runpy.run_path(str(HERE / name)))


class TestLaunch(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        e = Path(os.environ["MOCK_RECEIPT_EVIDENCE_ROOT"])
        cls.packet = (e / "jobs-signup-mock-receipt-source.Lu4NPZR1/"
                         "signup-mock-receipt-source-candidate.tar").read_bytes()
        cls.inventory = (e / "jobs-c2c-browser-inert-manifest-Auv0nTdj/inventory.json").read_bytes()
        cls.transport = loaded("local-qa-linux-signup-mock-receipt-transport.py")
        assert L["sha"](cls.packet) == cls.transport.PINNED["packetSha256"]
        cls.cap = {**cls.transport.PINNED, "run": RUN, "reservationSha256": "c"*64}
        cls.request = L["sha"](cls.transport.encode_request(cls.cap, cls.packet))
        cls.prep = loaded("local-qa-linux-signup-mock-receipt-prepare-guest.py")
        cls.old = loaded("local-qa-linux-c2c-prepare-guest.py")
        pins = cls.prep.intake_profile(loaded("local-qa-linux-c2c-guest-inputs.py")).PINS
        _, cls.roles = cls.prep.stage_expectations(cls.old, cls.cap, PROJECTION, pins)
        stage = cls.transport.expected_result(cls.cap)
        cls.roles["packet"] = stage
        cls.response = cls.transport.encode_response(stage, cls.cap, cls.request)
        cls.plan = cls.transport.wire({
            "schema": "bluey-jobs-c2c-actor-plan-v1", "run": RUN, "sourceTree": cls.cap["sourceTree"],
            "sourceInventorySha256": cls.cap["sourceInventorySha256"],
            "codeInventorySha256": cls.cap["codeInventorySha256"],
            "node": {"path": stage["browserRoot"] + "/" + cls.transport.NODE_PATH,
                     "sha256": cls.transport.NODE_SHA256},
        })
        assert L["sha"](cls.plan) == stage["actorPlanSha256"]
        cls.rpc = Path(os.environ["BLUEY_TEST_RUN_ROOT"]) / "local-compose"
        cls.rpc.mkdir(mode=0o700)
        with tarfile.open(fileobj=io.BytesIO(cls.packet), mode="r:") as sealed:
            for m in sealed:
                if m.name.startswith("actor/") and m.name.endswith(".mjs"):
                    assert m.isfile() and m.name.count("/") == 1
                    (cls.rpc / Path(m.name).name).write_bytes(sealed.extractfile(m).read())
                elif m.name == "manager/local-qa-linux-signup-mock-receipt-guest-manager.py":
                    scope = {"__name__": "_sealed_guest_manager"}
                    exec(compile(sealed.extractfile(m).read(), m.name, "exec"), scope)
                    cls.sealed = SimpleNamespace(**scope)
        for leaf in (ENTRY, SPEC):
            (cls.rpc / leaf).write_bytes((HERE / leaf).read_bytes())
        cls.composer_archive = (e / "jobs-signup-mock-receipt-compose-closure-20261122.tar").read_bytes()

    def setUp(self):
        self.events = []
        self.lease = self.cap["reservationSha256"]
        self.owner_live = True
        self.drift_manager = False
        self.drift_plan = False
        self.proof = dict(ZERO)
        self.plan_reads = 0

    def read(self, path, checksum, limit, *, retain=False):
        raw = self.plan if path.name == "plan.json" else self.inventory
        self.assertEqual(L["sha"](raw), checksum)
        self.assertLessEqual(len(raw), limit)
        identity = (checksum,)
        if path.name == "plan.json":
            self.plan_reads += 1
            if self.drift_plan and self.plan_reads > 1:
                identity = ("drift",)
        return (identity, raw) if retain else identity

    def compose(self, wire):
        self.events.append("compose")
        self.assertEqual(base64.b64decode(json.loads(wire)["codePlanBase64"]), self.plan)
        pipes = runpy.run_path(str(HERE / "local-qa-linux-c2c-owner-pipes.py"))
        pipe_scope = pipes["make_exchange"].__globals__
        pipe_scope.update(MAX_REQUEST=128 << 10, MAX_REPLY=65_536)
        child = None

        def opener(*_):
            nonlocal child
            child = subprocess.Popen(("/opt/homebrew/bin/node", str(self.rpc / ENTRY)), cwd=self.rpc,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     bufsize=0)
            return child

        exchange = pipes["make_exchange"](opener, lambda client, *_: client is child, timeout_seconds=8)
        try:
            return exchange(wire, {})
        finally:
            if child is not None and child.poll() is None:
                child.kill()
                child.wait(timeout=2)

    def runner(self, plan, pins, **adapters):
        self.assertEqual(plan["plan_sha256"], self.roles["packet"]["actorPlanSha256"])
        self.assertEqual(pins["waiter"], self.sealed.EXPECTED_WAITER_SHA)
        self.assertEqual(pins["launcher"], self.sealed.EXPECTED_LAUNCHER_SHA)
        self.assertEqual(adapters["source_tree"], self.cap["sourceTree"])
        lease = adapters["reserve_guest"](PROJECTION)
        self.assertTrue(lease["held"]())
        self.events.append("unit-double")
        raw = adapters["compose"](WAIT, WAIT)
        self.assertEqual(self.sealed.spec_for_launch(raw, WAIT, self.cap["sourceTree"],
            self.cap["sourceInventorySha256"], adapters["fixture_sha256"]), L["sha"](raw))
        self.drift_manager = getattr(self, "drift_after_compose", False)
        self.assertTrue(adapters["verify_admitted_artifacts"](WAIT, L["sha"](raw)))
        self.events.append("release-double")
        self.assertTrue(lease["complete"](self.proof))
        return {"syntheticJourneyPassed": False, "teardown": self.proof}

    def bind(self, prepared, cap, projection, digest, *, compose, independently_admitted, **_):
        self.assertTrue(independently_admitted({**cap, **projection}))
        self.assertEqual(digest, self.request)
        self.assertEqual(prepared["receipts"], self.roles)
        self.events.append("readback-double")
        return dict(compose=compose, verify_admitted_artifacts=lambda *_: independently_admitted({**cap, **projection}),
                    fixture_path=self.roles["application"]["fixturePath"],
                    fixture_sha256=self.prep.APPLICATION_INPUTS["jobs-nonprod-browser-fixture"][1])

    def reserve(self, projected):
        self.assertEqual(projected, PROJECTION)
        self.events.append("reserve")
        def complete(_):
            self.events.append("complete")
            return True
        return {"reservationSha256": self.lease, "held": lambda: self.owner_live, "complete": complete}

    def execute(self):
        mgr = SimpleNamespace(HELPERS=self.sealed.HELPERS, RUNTIME_HELPERS=self.sealed.RUNTIME_HELPERS,
                              pinned_file=self.read, run_admitted_journey=self.runner,
                              zero_proof=self.sealed.zero_proof)

        @contextmanager
        def loader(run, owner):
            self.assertEqual(run, RUN)
            self.assertTrue(owner())
            yield mgr, lambda: not self.drift_manager

        stages = {"prepare": SimpleNamespace(load_pinned=lambda _: self.old, LEGACY_PREP=None,
                                               bind_prepared_readback=self.bind),
                  "transport": self.transport, "manager-loader": SimpleNamespace(bind_staged_manager=loader)}
        prepared = dict(receipts=self.roles, preparationResponse=self.response, storageTopologySha256="1"*64,
                        launchAdmitted=False, releasePublished=False, runtimeQualified=False)
        with patch.dict(L["run_prepared"].__globals__, {"initial_guest": lambda: True, "load": stages.__getitem__,
                         "make_observer": lambda *_: lambda waiting: dict(waiting),
                         "bind_local_composer": lambda *_: self.compose}):
            return L["run_prepared"](prepared, self.cap, PROJECTION, self.request, self.packet,
                tools=TOOLS, ports=PORTS, independently_admitted=lambda scope: self.owner_live and scope == {
                    **self.cap, **PROJECTION}, reserve_launch=self.reserve, unit_absent=lambda _: True)

    def test_current_composer_and_manager_spec_parser_join_under_one_hold(self):
        self.assertFalse(self.execute()["syntheticJourneyPassed"])
        self.assertEqual(self.events, ["readback-double", "reserve", "unit-double", "compose",
                                       "release-double", "complete"])

    def test_different_reservation_prevents_dispatch(self):
        self.lease = "9"*64
        with self.assertRaisesRegex(ValueError, "launch_hold"):
            self.execute()
        self.assertEqual(self.events, ["readback-double", "reserve"])

    def test_changed_held_plan_refuses_before_composer(self):
        self.drift_plan = True
        with self.assertRaisesRegex(ValueError, "compose_inputs"):
            self.execute()
        self.assertNotIn("compose", self.events)

    def test_manager_change_refuses_release(self):
        self.drift_after_compose = True
        with self.assertRaises(AssertionError):
            self.execute()
        self.assertNotIn("release-double", self.events)

    def test_nonzero_teardown_cannot_complete_hold(self):
        self.proof["actorProcesses"] = 1
        with self.assertRaisesRegex(ValueError, "unit_teardown"):
            self.execute()
        self.assertNotIn("complete", self.events)

    def test_exact_composer_closure_inventory_and_changed_pin(self):
        with tarfile.open(fileobj=io.BytesIO(self.composer_archive), mode="r:") as complete:
            raw = complete.extractfile("inventory.json").read()
            inventory = L["composer_members"](raw)
            self.assertEqual(len(inventory), 38)
            for leaf, (size, checksum) in inventory.items():
                content = complete.extractfile(leaf).read()
                self.assertEqual(content, (self.rpc / leaf).read_bytes())
                self.assertEqual((len(content), L["sha"](content)), (size, checksum))
            with self.assertRaisesRegex(ValueError, "composer_inventory"):
                L["composer_members"](raw[:-1] + b"x")

    def test_off_platform_refuses_before_protected_load(self):
        with patch.dict(L["run_prepared"].__globals__, {"initial_guest": lambda: False, "load": None}):
            with self.assertRaisesRegex(ValueError, "admission"):
                L["run_prepared"](None, None, None, self.request, self.packet, tools=TOOLS, ports=PORTS,
                                  independently_admitted=self.fail, reserve_launch=self.fail, unit_absent=self.fail)


if __name__ == "__main__":
    unittest.main()
