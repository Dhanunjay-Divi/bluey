"""Portable integration of the exact current intake, transport and plan contracts."""
from contextlib import contextmanager
import io
from pathlib import Path
import os
import runpy
import tarfile
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
MOD = runpy.run_path(str(HERE / "local-qa-linux-signup-mock-receipt-prepare-guest.py"))
PREPARE = MOD["prepare_guest"]
PROFILE = PREPARE.__globals__
RUN = "a" * 32
ACTOR = {"run": RUN, "uid": 1001, "gid": 1001, "unit": f"bluey-jobs-qa-{RUN}.service",
         "runParent": "/tmp/bluey-test-runs-012345abc123"}
TOPOLOGY = {"sha256": "d" * 64, "value": {}}


def module(pin):
    name, digest = pin
    path = HERE / name
    body = path.read_bytes()
    assert MOD["sha"](body) == digest, name
    provider = ModuleType("_candidate_test_" + name)
    provider.__file__ = str(path)
    exec(compile(body, str(path), "exec"), provider.__dict__)
    return provider


class PrepareGuest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        e = Path(os.environ["MOCK_RECEIPT_EVIDENCE_ROOT"])
        q = e / "jobs-release-restored-91f6f868"
        n = q / "current-linux-mock-receipt-qa-pair-20261122"
        cls.browser = (e / "jobs-c2c-browser-inert-manifest-Auv0nTdj/inventory.json").read_bytes()
        cls.pg = (e / "parallel-drafts-phase657/parent-bootstrap/phase660-pgdg-noble-intake-v1/"
                    "capsule/files.json").read_bytes()
        cls.packet = (e / "jobs-signup-mock-receipt-source.Lu4NPZR1/"
                          "signup-mock-receipt-source-candidate.tar").read_bytes()
        cls.app_payload = {
            name: (n / ("receipt.json" if name == "native-receipt.json" else name)).read_bytes()
            for name in MOD["APPLICATION_INPUTS"] if name != "portal.tar"
        }
        cls.app_payload["portal.tar"] = (q / "signup-quiet-start-with-current-portal-20261122.tar").read_bytes()
        for name, payload in cls.app_payload.items():
            assert (len(payload), MOD["sha"](payload)) == MOD["APPLICATION_INPUTS"][name], name
        cls.application = module(MOD["APPLICATION"])
        cls.native = cls.app_payload["native-receipt.json"]
        cls.api = cls.app_payload["bluey-server"]
        cls.fixture = cls.app_payload["jobs-nonprod-browser-fixture"]
        resolver, receipt, cls.manifest, _, cls.static = cls.application.prepare_application(
            cls.native, cls.api, cls.fixture, cls.app_payload["portal.tar"], cls.packet)
        for name, raw in (("resolver.json", resolver), ("portal-receipt.json", receipt),
                          ("portal-manifest.json", cls.manifest)):
            assert (len(raw), MOD["sha"](raw)) == MOD["DERIVED"][name], name
        cls.transport = module(MOD["TRANSPORT"])
        cls.plan = module(MOD["ACTOR_PLAN"])
        cls.cap = {**cls.transport.PINNED, "run": RUN, "reservationSha256": "b" * 64}
        cls.frame = cls.transport.encode_request(cls.cap, cls.packet)
        cls.predecessor = module(MOD["LEGACY_PREP"])

    def setUp(self):
        self.pins = MOD["intake_profile"](module(self.predecessor.MODULES["guest-inputs"])).PINS
        self.scopes, self.receipts = MOD["stage_expectations"](self.predecessor, self.cap, ACTOR, self.pins)
        self.writes, self.storage_observations = [], []
        self.lost_after, self.bad_receipt, self.drift_at = None, None, None
        self.available = True
        self.archive_paths = {name: Path("/inert") / name for name in ("playwright.tgz", "chrome.zip",
                                                                        "node.tar.xz", "postgres.tar.gz")}

    @contextmanager
    def intake(self, run):
        self.assertEqual(run, RUN)
        yield self.archive_paths, {**self.app_payload, "launcher.sh": b"launcher-double",
                                   "browser-inventory.json": self.browser, "postgres-files.json": self.pg}, \
            lambda: self.available

    def storage(self, **scope):
        self.storage_observations.append(scope)
        if self.drift_at == len(self.storage_observations):
            return {"topology": {**TOPOLOGY, "sha256": "e" * 64}}
        return {"topology": TOPOLOGY}

    def stage(self, name, admitted):
        current = self.scopes[name]
        self.assertFalse(admitted({**current, "reservationSha256": "e" * 64}))
        self.assertTrue(admitted(dict(current)))
        self.writes.append(name)
        if self.lost_after == name:
            self.available = False
        result = dict(self.receipts[name])
        if self.bad_receipt == name:
            result["runtimeQualified"] = True
        return result

    def stage_app(self, **kw):
        supplied = tuple(kw[key] for key in ("native_receipt", "server", "fixture", "portal", "code"))
        self.assertEqual(supplied, (self.native, self.api, self.fixture, self.app_payload["portal.tar"], self.packet))
        self.assertEqual(kw["source_tree"], self.cap["sourceTree"])
        return self.stage("application", kw["admitted"])

    def code(self, packet, cap, browser_pin, *, admitted):
        scope = {**cap, "browserInventorySha256": browser_pin, "browserRoot": self.receipts["browser"]["browserRoot"]}
        self.assertEqual(packet, self.packet)
        self.assertTrue(admitted(scope))
        expected = self.transport.expected_result(self.cap)
        code_receipt = {name: expected[name] for name in ("run", "packetSha256", "codeInventorySha256",
                         "actorFiles", "managerFiles", "runtimeQualified", "releasePublished")}
        plan = self.plan.make_plan(cap, code_receipt, scope["browserRoot"], self.browser, browser_pin)
        self.assertEqual(MOD["sha"](plan), expected["actorPlanSha256"])
        self.writes.append("plan")
        return {**code_receipt, "reservationSha256": cap["reservationSha256"], "actorPlanSha256": MOD["sha"](plan),
                "browserRoot": scope["browserRoot"], "browserInventorySha256": browser_pin, "launchAdmitted": False}

    def prepare(self, frame=None):
        storage = SimpleNamespace(REQUIRED_BYTES=2 << 30, RESERVE_BYTES=1 << 30, verify_guest_storage=self.storage)
        guarded = {role: SimpleNamespace(**{fn: lambda *_, role=role, admitted: self.stage(role, admitted)})
                   for role, fn in (("browser", "install_browser"), ("postgres", "install_postgres"),
                                    ("launcher", "install_launcher"))}
        guarded.update({"guest-storage": storage, "guest-inputs": SimpleNamespace(PINS=self.pins, open_intake=self.intake),
                        "browser-reader": None})
        legacy = SimpleNamespace(initial_guest=lambda: True, projection=self.predecessor.projection,
                                 fresh=lambda *_: None, stage_expectations=self.predecessor.stage_expectations,
                                 load=lambda role, **_: guarded[role])
        roles = {MOD["LEGACY_PREP"]: legacy, MOD["TRANSPORT"]: self.transport,
                 MOD["APPLICATION"]: SimpleNamespace(install_application=self.stage_app),
                 MOD["ACTOR_PLAN"]: SimpleNamespace(stage_code_and_plan=self.code)}
        with patch.dict(PROFILE, {"sys": SimpleNamespace(platform="linux"),
                                  "os": SimpleNamespace(getuid=lambda: 0, geteuid=lambda: 0),
                                  "load_pinned": roles.__getitem__}):
            result = PREPARE(io.BytesIO(self.frame if frame is None else frame), self.cap, ACTOR,
                             independently_admitted=lambda scope: scope == {**self.cap, **ACTOR})
        return result

    def test_real_frame_composes_current_intake_plan_and_response(self):
        holds = []

        def reserve(binding):
            self.assertEqual(binding, {**self.transport.PINNED, "projection": ACTOR})
            holds.append("reserve")

            def held():
                holds.append("held")
                return True

            def note(_):
                self.fail("a staging response must not complete the workload")

            return {"reservationSha256": self.cap["reservationSha256"], "held": held, "note_workload": note}

        def exchange(frame, expected):
            holds.append("exchange")
            self.assertEqual(expected, self.cap)
            self.assertEqual(frame, self.frame)
            result = self.prepare(frame)
            self.assertEqual(result["receipts"]["packet"], self.transport.expected_result(self.cap))
            self.assertTrue(all(result[x] is False for x in
                                ("launchAdmitted", "releasePublished", "runtimeQualified")))
            return result["preparationResponse"]

        response = self.transport.prepare_at_reserved_guest(
            {**self.transport.PINNED, "projection": ACTOR}, self.packet, reserve=reserve, exchange=exchange)
        self.assertEqual(response, self.transport.expected_result(self.cap))
        self.assertEqual(holds, ["reserve", "held", "exchange", "held"])
        self.assertEqual(self.writes, ["browser", "postgres", "application", "launcher", "plan"])
        self.assertEqual(self.storage_observations[-1]["minimum_available_bytes"], (1 << 30) + len(self.fixture))

    def test_non_linux_refuses_before_loading_protected_helpers(self):
        with patch.dict(PROFILE, {"sys": SimpleNamespace(platform="darwin"), "load_pinned": None}):
            with self.assertRaisesRegex(ValueError, "platform"):
                PREPARE(io.BytesIO(self.frame), self.cap, ACTOR, independently_admitted=lambda _: self.fail())

    def test_changed_frame_refuses_without_installer(self):
        with self.assertRaisesRegex(ValueError, "request_extra_bytes"):
            self.prepare(self.frame + b"x")
        self.assertEqual(self.writes, [])

    def test_mount_drift_refuses_before_installer(self):
        self.drift_at = 2
        with self.assertRaisesRegex(ValueError, "storage_before"):
            self.prepare()
        self.assertEqual(self.writes, [])

    def test_unqualified_installer_report_never_reaches_actor_plan(self):
        self.bad_receipt = "application"
        with self.assertRaisesRegex(ValueError, "application_receipt"):
            self.prepare()
        self.assertEqual(self.writes, ["browser", "postgres", "application"])

    def test_lost_intake_lease_stops_next_installer(self):
        self.lost_after = "postgres"
        with self.assertRaisesRegex(ValueError, "postgres_receipt"):
            self.prepare()
        self.assertEqual(self.writes, ["browser", "postgres"])

    def test_terminal_mount_drift_refuses_response(self):
        self.drift_at = 3
        with self.assertRaisesRegex(ValueError, "storage_after"):
            self.prepare()
        self.assertEqual(self.writes[-1], "plan")

    def test_exact_five_tree_readback_fits_the_unmodified_shared_ceiling(self):
        inventory = MOD["bind_inventory"](module(MOD["INVENTORY"]), self.application)
        with tarfile.open(fileobj=io.BytesIO(self.packet), mode="r:") as arc:
            actor_inventory = arc.extractfile("actor/inventory.json").read()
        trees = inventory.expected_trees(RUN, browser=self.browser, postgres=self.pg, portal=self.manifest,
                                         actor=actor_inventory, plan=inventory.expected_plan(RUN))
        scopes = self.predecessor.MODULES
        reader = module(MOD["READBACK"])
        readback = module(reader.SUPPORT["readback"])
        self.assertEqual(len(readback.inventory(trees)), 5)
        for member, raw in self.static.items():
            expected = trees[self.receipts["application"]["root"]][1]["portal/" + member]
            self.assertEqual(expected[2:], (len(raw), MOD["sha"](raw)))
        self.assertEqual(scopes["application"][0], "local-qa-linux-c2c-install-application.py")


if __name__ == "__main__":
    unittest.main()
