"""Real framed transport and exact installer-contract controls; no guest writes."""
from contextlib import contextmanager
import hashlib
import io
import os
from pathlib import Path
import runpy
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
RUN = "a" * 32
ACTOR = {"run": RUN, "unit": f"bluey-jobs-qa-{RUN}.service", "uid": 1001, "gid": 1002,
         "runParent": "/tmp/bluey-test-runs-abcdef012345"}


class GuestPreparationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.prepare = staticmethod(runpy.run_path(
            str(HERE / "local-qa-linux-c2c-prepare-guest.py"))["prepare_guest"])
        cls.scope = cls.prepare.__globals__
        cls.transport = SimpleNamespace(**runpy.run_path(str(HERE / "local-qa-linux-c2c-preparation-transport.py")))
        cls.inputs = runpy.run_path(str(HERE / "local-qa-linux-c2c-guest-inputs.py"))
        cls.cap = {**cls.transport.PINNED, "run": RUN, "reservationSha256": "b" * 64}
        cls.frame = cls.transport.encode_request(
            cls.cap, Path(os.environ["C2C_CODE_PACKET"]).read_bytes(),
            {name: (HERE / name).read_bytes() for name in cls.transport.BOOTSTRAP})

    def setUp(self):
        self.writes, self.field_checks = [], []
        self.boot_ok, self.reserved = True, True
        self.changed_after, self.bad_receipt = None, None
        self.lose_reservation_after = None
        self.mount_events = 0
        self.scopes, self.receipts = self.scope["stage_expectations"](self.cap, ACTOR, self.inputs["PINS"])
        self.payload = {name: name.encode() for name in self.inputs["PINS"] if name not in self.inputs["ARCHIVES"]}
        self.archives = {name: Path("/inert") / name for name in self.inputs["ARCHIVES"]}

    def verify_storage(self, **kwargs):
        self.mount_events += 1
        if self.mount_events > 1:
            self.assertEqual(kwargs["expected_topology"], {"sha256": "c" * 64, "value": {}})
            self.assertEqual(kwargs["minimum_available_bytes"], 2 if self.mount_events == 2 else 1)
        return {"topology": {"sha256": ("c" if self.changed_after != self.mount_events else "d") * 64,
                             "value": {}}}

    @contextmanager
    def intake(self, run):
        self.assertEqual(run, RUN)
        yield self.archives, self.payload, lambda: True

    def install(self, name, admitted):
        self.assertTrue(admitted(dict(self.scopes[name])))
        self.assertFalse(admitted({**self.scopes[name], "run": "f" * 32}))
        self.field_checks.append(name)
        self.writes.append(name)
        if self.lose_reservation_after == name:
            self.reserved = False
        value = dict(self.receipts[name])
        if self.bad_receipt == name:
            value["runtimeQualified"] = True
        return value

    def launch_install(self, run, reservation, uid, gid, run_parent, raw, *, admitted):
        self.assertEqual((run, reservation, uid, gid, run_parent, raw),
                         (RUN, self.cap["reservationSha256"], ACTOR["uid"], ACTOR["gid"],
                          ACTOR["runParent"], self.payload["launcher.sh"]))
        return self.install("launcher", admitted)

    def browser_install(self, run, reservation, paths, raw, *, admitted):
        self.assertEqual((run, reservation, paths, raw), (RUN, self.cap["reservationSha256"],
            {"chrome": self.archives["chrome.zip"], "node": self.archives["node.tar.xz"],
             "playwright": self.archives["playwright.tgz"]}, self.payload["browser-inventory.json"]))
        return self.install("browser", admitted)

    def pg_install(self, run, reservation, path, raw, *, admitted):
        self.assertEqual((run, reservation, path, raw), (RUN, self.cap["reservationSha256"],
            self.archives["postgres.tar.gz"], self.payload["postgres-files.json"]))
        return self.install("postgres", admitted)

    def app_install(self, **kw):
        self.assertEqual(kw["run"], RUN)
        self.assertEqual(kw["reservation_sha256"], self.cap["reservationSha256"])
        self.assertEqual(kw["server"], self.payload["bluey-server"])
        self.assertEqual(kw["fixture"], self.payload["jobs-nonprod-browser-fixture"])
        self.assertEqual(kw["portal_manifest"], self.payload["portal-manifest.json"])
        return self.install("application", kw["admitted"])

    def receive(self, stream, cap, *, admitted):
        self.assertTrue(admitted(dict(cap)))
        self.assertEqual(stream.getvalue(), self.frame)
        self.writes.append("packet")
        _, _, digest = self.transport.read_request(stream, cap)
        return self.transport.encode_response(self.transport.expected_result(cap), cap, digest)

    def stubs(self):
        return {
            "guest-storage": SimpleNamespace(verify_guest_storage=self.verify_storage, REQUIRED_BYTES=2, RESERVE_BYTES=1),
            "guest-inputs": SimpleNamespace(PINS=self.inputs["PINS"], open_intake=self.intake),
            "transport": self.transport,
            "launcher": SimpleNamespace(install_launcher=self.launch_install),
            "browser": SimpleNamespace(install_browser=self.browser_install),
            "application": SimpleNamespace(install_application=self.app_install),
            "postgres": SimpleNamespace(install_postgres=self.pg_install),
            "receiver": SimpleNamespace(receive_preparation=self.receive),
            "browser-reader": None,
        }

    def run_stage(self, body=None, *, actor=ACTOR):
        modules = self.stubs()
        with patch.dict(self.scope, {"initial_guest": lambda: self.boot_ok,
                                     "load": lambda role, **_: modules[role], "fresh": lambda *_: None}):
            return self.prepare(io.BytesIO(self.frame if body is None else body), self.cap, actor,
                                independently_admitted=lambda scope: scope == {**self.cap, **ACTOR} and self.reserved)

    def test_real_frame_composes_same_run_roles_and_stages_browser_before_actor(self):
        value = self.run_stage()
        self.assertEqual(self.writes, ["browser", "postgres", "application", "launcher", "packet"])
        self.assertEqual(self.field_checks, self.writes[:-1])
        self.assertEqual(value["receipts"]["packet"], self.transport.expected_result(self.cap))
        self.assertEqual(value["receipts"]["application"]["fixturePath"],
                         f"/run/bluey-jobs-c2c-application/{RUN}/fixture/jobs-nonprod-browser-fixture")
        for name in ("launchAdmitted", "releasePublished", "runtimeQualified"):
            self.assertIs(value[name], False)

    def test_wrong_platform_reservation_projection_or_suffix_never_stages(self):
        self.boot_ok = False
        with self.assertRaisesRegex(ValueError, "platform"):
            self.run_stage()
        self.boot_ok, self.reserved = True, False
        with self.assertRaisesRegex(ValueError, "reservation_before_frame"):
            self.run_stage()
        self.reserved = True
        with self.assertRaisesRegex(ValueError, "actor_projection"):
            self.run_stage(actor={**ACTOR, "runParent": "/tmp"})
        with self.assertRaisesRegex(ValueError, "request_extra_bytes"):
            self.run_stage(self.frame + b"x")
        self.assertEqual(self.writes, [])
        self.assertEqual(self.mount_events, 0)

    def test_layout_drift_before_first_write_never_stages(self):
        self.changed_after = 2
        with self.assertRaisesRegex(ValueError, "storage_before"):
            self.run_stage()
        self.assertEqual(self.writes, [])

    def test_retained_noexec_run_is_refused_before_staging_the_real_frame(self):
        storage = runpy.run_path(str(HERE / "local-qa-linux-c2c-guest-storage.py"))
        raw = Path(os.environ["C2C_GUEST_STORAGE_MOUNTINFO"]).read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(),
                         "7db5f84f3423aabba40c68721d02d679d024b31d1db63d18ba70887463dba1f7")
        nss = {name: f"{name}:[4026531000]" for name in storage["NAMESPACES"]}
        stub_anchors = dict.fromkeys(storage["ALL"])
        with patch.object(self, "verify_storage", side_effect=lambda **_: storage["verify_guest_storage_projection"](
                raw, stub_anchors, nss)):
            with self.assertRaisesRegex(ValueError, "c2c_guest_storage_refused:payload_mount"):
                self.run_stage()
        self.assertEqual(self.writes, [])

    def test_a_bad_installer_receipt_stops_without_returning_the_packet(self):
        self.bad_receipt = "postgres"
        with self.assertRaisesRegex(ValueError, "postgres_receipt"):
            self.run_stage()
        self.assertEqual(self.writes, ["browser", "postgres"])

    def test_final_layout_drift_does_not_grant_qualification(self):
        self.changed_after = 3
        with self.assertRaisesRegex(ValueError, "storage_after"):
            self.run_stage()
        self.assertEqual(self.writes, ["browser", "postgres", "application", "launcher", "packet"])

    def test_lost_reservation_after_any_install_stops_before_packet(self):
        self.lose_reservation_after = "application"
        with self.assertRaisesRegex(ValueError, "application_receipt"):
            self.run_stage()
        self.assertEqual(self.writes, ["browser", "postgres", "application"])

    def test_retained_partial_name_including_dangling_link_refuses(self):
        with TemporaryDirectory() as temporary:
            target = Path(temporary) / RUN
            self.scope["absent"](target)
            target.symlink_to("missing")
            with self.assertRaisesRegex(ValueError, "previous_attempt"):
                self.scope["absent"](target)

    def test_every_pinned_helper_matches_the_current_source_bytes(self):
        for role, (name, expected) in self.scope["MODULES"].items():
            with self.subTest(role=role):
                self.assertEqual(hashlib.sha256((HERE / name).read_bytes()).hexdigest(), expected)


if __name__ == "__main__":
    unittest.main()
