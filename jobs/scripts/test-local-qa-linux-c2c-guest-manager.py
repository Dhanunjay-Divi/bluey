"""Portable manager controls: never run systemd or an actual guest."""
from contextlib import contextmanager
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest


HERE = Path(__file__).resolve().parent
MANAGER = HERE / "local-qa-linux-c2c-guest-manager.py"
PROTOCOL = HERE / "local-qa-linux-c2c-release-protocol.py"
RUN, TREE, INVENTORY = "1" * 32, "a" * 40, "b" * 64
PARENT = "/tmp/bluey-test-runs-123456789abc"
ARGS = dict(run=RUN, uid=1000, gid=1000, run_parent=PARENT,
            plan_sha256="c" * 64, python_path="/usr/bin/python3.12")


def load(path):
    scope = {"__name__": "_portable_guest_manager_test", "__file__": str(path)}
    exec(compile(path.read_bytes(), str(path), "exec"), scope)
    return scope


class Client:
    def __init__(self):
        self.ended = False
        self.stdout = io.BytesIO()
        self.stderr = io.BytesIO()

    def poll(self):
        return 0 if self.ended else None

    def terminate(self):
        self.ended = True

    def wait(self, timeout):
        return self.poll()


class GuestManager(unittest.TestCase):
    def setUp(self):
        self.manager, self.protocol = load(MANAGER), load(PROTOCOL)

    def test_installed_helper_source_pins_match(self):
        for table in ("HELPERS", "RUNTIME_HELPERS"):
            for leaf, digest in self.manager[table].values():
                with self.subTest(leaf=leaf):
                    self.assertEqual(hashlib.sha256((HERE / leaf).read_bytes()).hexdigest(), digest)

    def test_plan_has_closed_environment_and_one_writable_root(self):
        build = self.manager["build_plan"]
        plan = build(**ARGS)
        argv = plan["argv"]
        self.assertEqual(self.manager["observer_plan"](plan),
                         dict(run=RUN, unit=f"bluey-jobs-qa-{RUN}.service", uid=1000, gid=1000, runParent=PARENT))
        self.assertEqual([item for item in argv if "ReadWritePaths" in item],
                         [f"--property=ReadWritePaths={PARENT}"])
        self.assertIn("--property=PrivateNetwork=yes", argv)
        self.assertIn(f"--setenv=GIT_CONFIG_VALUE_0=/opt/bluey-jobs-c2c-launcher/{RUN}", argv)
        self.assertIn(f"--property=RuntimeMaxSec=600", argv)
        self.assertIn("--property=KillMode=control-group", argv)
        unset = next(item for item in argv if item.startswith("--property=UnsetEnvironment="))
        self.assertTrue({"GIT_DIR", "LD_PRELOAD", "BASH_ENV", "BLUEY_DATABASE_URL"}
                        <= set(unset.partition("UnsetEnvironment=")[2].split()))
        self.assertNotIn("--collect", argv)
        self.assertEqual(argv[argv.index("--") + 1:],
                         (f"/opt/bluey-jobs-c2c-launcher/{RUN}/scripts/run-bluey-tests.sh", "--",
                          "/usr/bin/python3.12", "-I", "-S", "-B",
                          f"/run/bluey-jobs-c2c-code/{RUN}/local-qa-linux-c2c-wait-actor.py",
                          "--run", RUN, "--plan-sha256", "c" * 64))
        for override in ({"run": "0" * 32}, {"uid": 0}, {"gid": True}, {"run_parent": "/tmp"},
                         {"python_path": "/usr/bin/python3.12;sh"}):
            with self.subTest(override=override), self.assertRaisesRegex(ValueError, "plan_scope"):
                build(**dict(ARGS, **override))

    def fixture(self, *, bad_frame=False, bad_stop=False, stop_raises=False, publish_raises=False):
        build = self.manager["build_plan"]
        plan = build(**ARGS)
        observed = {"run": RUN, "unitName": plan["unit"], "bootId": "2" * 32, "invocationId": "3" * 32,
                    "unitCgroup": f"/system.slice/{plan['unit']}", "networkNamespace": "net:[4026532555]",
                    "root": f"{PARENT}/run.1795250653.1001.102030", "rootDevice": 1, "rootInode": 99,
                    "markerSha256": "4" * 64, "actorUid": 1000, "mainPid": 1001, "mainStartTime": 123,
                    "actorPid": 1002, "actorStartTime": 234}
        waiting = self.protocol["encode"](dict(observed, schema="bluey-jobs-c2c-waiting-v1",
                                                nonce="5" * 32, waitingNs=100))
        receipt = {"event": "local_qa_linux_c2c_native_completed", "sourceTree": TREE,
                   "sourceInventorySha256": INVENTORY, "evidence": {"matchReads": 1, "navigationsAborted": 1,
                   "mutations": 1 if bad_frame else 0, "unexpected": 0, "stableCensus": True}}
        terminal = (json.dumps(receipt, separators=(",", ":")) + "\n").encode()
        events = []

        class Pipe:
            def __init__(self, child):
                self.child = child
                self.lines = iter((waiting, terminal))

            def line(self, deadline):
                events.append("frame")
                return next(self.lines)

            def finish(self, deadline):
                events.append("client_exit")
                self.child.ended = True

            def close(self):
                events.append("pipe_close")

        def observe(_):
            events.append("observe")
            return dict(observed)

        def prepare(*_):
            events.append("stage")
            return "5" * 64

        def publish(raw, digest):
            self.assertEqual((raw, digest), (waiting, "5" * 64))
            events.append("publish")
            if publish_raises:
                raise RuntimeError("synthetic_publication")
            return {"releaseSha256": "6" * 64, "runtimeQualified": False, "productionQualified": False}

        def settle(*_):
            events.append("stop")
            if stop_raises:
                raise RuntimeError("synthetic_stop")
            return {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 1 if bad_stop else 0}

        def recover(*_):
            events.append("recover")
            return {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0}

        def unbound_cleanup(launch):
            self.assertEqual(launch, self.manager["observer_plan"](plan))
            events.append("unbound_cleanup")
            return {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0}

        client = Client()
        kwargs = dict(parse_waiting=self.protocol["parse_waiting"], observe=observe, prepare=prepare,
                      publish=publish, settle=settle, recover=recover, unbound_cleanup=unbound_cleanup,
                      source_tree=TREE, source_inventory=INVENTORY, pipe_factory=Pipe)
        return client, plan, kwargs, events

    def test_coordinator_requires_release_terminal_and_zero_in_order(self):
        client, plan, kwargs, events = self.fixture()
        value = self.manager["coordinate"](client, plan, **kwargs)
        self.assertIs(value["syntheticJourneyPassed"], True)
        self.assertIs(value["productionQualified"], False)
        self.assertEqual(events, ["frame", "observe", "stage", "publish", "frame", "stop", "client_exit", "pipe_close"])

    def test_failed_or_mutating_actor_is_recovered_once(self):
        for flags, code in (({"bad_frame": True}, "terminal_census"), ({"publish_raises": True}, "synthetic_publication")):
            with self.subTest(flags=flags):
                client, plan, kwargs, events = self.fixture(**flags)
                with self.assertRaisesRegex((ValueError, RuntimeError), code):
                    self.manager["coordinate"](client, plan, **kwargs)
                self.assertEqual(events.count("recover"), 1)
                self.assertNotIn("stop", events)
                self.assertNotIn("client_exit", events)

    def test_failed_teardown_is_not_retried(self):
        for flags, code in (({"bad_stop": True}, "unit_teardown"), ({"stop_raises": True}, "synthetic_stop")):
            client, plan, kwargs, events = self.fixture(**flags)
            with self.assertRaisesRegex((ValueError, RuntimeError), code):
                self.manager["coordinate"](client, plan, **kwargs)
            self.assertEqual(events.count("stop"), 1)
            self.assertNotIn("recover", events)

    def test_mismatched_observation_cleans_only_through_independent_launch_proof(self):
        client, plan, kwargs, events = self.fixture()
        kwargs["observe"] = lambda _: {}
        with self.assertRaisesRegex(ValueError, "waiting_observation"):
            self.manager["coordinate"](client, plan, **kwargs)
        self.assertEqual(events, ["frame", "unbound_cleanup", "pipe_close"])
        self.assertTrue(client.ended)

    def test_malformed_initial_frame_requires_real_cleanup_proof(self):
        client, plan, kwargs, events = self.fixture()

        def malformed(_):
            raise ValueError("synthetic_malformed")

        kwargs["parse_waiting"] = malformed
        kwargs["unbound_cleanup"] = lambda _: {"unitStopped": True, "actorProcesses": 1, "namespaceProcesses": 0}
        with self.assertRaisesRegex(ValueError, "unit_teardown"):
            self.manager["coordinate"](client, plan, **kwargs)
        self.assertEqual(events, ["frame", "pipe_close"])
        self.assertTrue(client.ended)

    def prepared_fixture(self, *, wrong_source=False):
        client, plan, kwargs, _ = self.fixture()
        pipe = kwargs["pipe_factory"](client)
        raw = pipe.line(0)
        waiting = kwargs["parse_waiting"](raw)
        observed = kwargs["observe"](waiting)
        events = []
        fixture_hash = "7" * 64
        verified = {"staged": True}

        def compose(w, _):
            events.append("compose")
            value = {"version": 1, "sourceTree": "0" * 40 if wrong_source else TREE,
                     "sourceInventorySha256": INVENTORY,
                     "unit": {key: w[key] for key in ("mainPid", "bootId", "invocationId", "unitName")},
                     "binding": {key: w[key] for key in ("root", "unitCgroup", "networkNamespace")}, "browser": {},
                     "fixture": {"artifactPath": f"{w['root']}/fixtures/jobs-nonprod-browser-fixture",
                                 "artifactSha256": fixture_hash}}
            return (json.dumps(value, separators=(",", ":")) + "\n").encode()

        def stage(w, *, spec_bytes, spec_sha256, fixture_path, fixture_sha256, independently_admitted):
            events.append("stage")
            self.assertTrue(independently_admitted(w))
            self.assertEqual(fixture_sha256, fixture_hash)
            return {"releasePublished": False, "specSha256": spec_sha256, "fixtureSha256": fixture_hash}

        def issue(raw, digest, *, independently_observe, verify_admitted_artifacts):
            events.append("issue")
            self.assertEqual(independently_observe(waiting), observed)
            self.assertTrue(verify_admitted_artifacts(waiting, digest))
            return {"releaseSha256": "8" * 64}

        prepare, publish = self.manager["bind_preparation"](
            parse_waiting=kwargs["parse_waiting"], compose=compose, observe=lambda _: dict(observed), stage=stage,
            verify_staged=lambda *_: verified["staged"], issue_release=issue, verify_admitted_artifacts=lambda *_: True,
            fixture_path="/opt/admitted/jobs-nonprod-browser-fixture", fixture_sha256=fixture_hash,
            source_tree=TREE, source_inventory=INVENTORY)
        return prepare, publish, raw, waiting, observed, events, verified

    def test_preparation_binds_exact_spec_to_one_publication(self):
        prep, publish, raw, waiting, observation, events, _ = self.prepared_fixture()
        digest = prep(waiting, observation)
        self.assertEqual(publish(raw, digest), {"releaseSha256": "8" * 64})
        with self.assertRaisesRegex(ValueError, "publication_state"):
            publish(raw, digest)
        with self.assertRaisesRegex(ValueError, "stage_already_attempted"):
            prep(waiting, observation)
        self.assertEqual(events, ["compose", "stage", "issue"])

    def test_stale_source_or_changed_fingerprint_never_releases(self):
        prep, _, _, waiting, observation, events, _ = self.prepared_fixture(wrong_source=True)
        with self.assertRaisesRegex(ValueError, "spec_provenance"):
            prep(waiting, observation)
        self.assertEqual(events, ["compose"])
        prep, publish, raw, waiting, observation, events, verified = self.prepared_fixture()
        digest = prep(waiting, observation)
        verified["staged"] = False
        with self.assertRaisesRegex(ValueError, "release_fingerprint_changed"):
            publish(raw, digest)
        self.assertEqual(events, ["compose", "stage"])

    def test_preframe_custody_and_teardown_are_held_until_transport_closes(self):
        client, plan, kwargs, events = self.fixture()
        claim = {"synthetic": "manager-captured"}
        projected = self.manager["observer_plan"](plan)

        @contextmanager
        def capture(actual):
            self.assertEqual(actual, projected)
            events.append("launch_capture")
            try:
                yield claim, 123
            finally:
                events.append("launch_capture_closed")

        @contextmanager
        def teardown(actual, observed_claim, fd):
            self.assertEqual((actual, observed_claim, fd), (projected, claim, 123))
            events.append("teardown_bound")
            try:
                yield {key: kwargs[key] for key in ("settle", "recover", "unbound_cleanup")}
            finally:
                events.append("teardown_closed")

        release = {key: kwargs[key] for key in ("parse_waiting", "observe", "prepare", "publish", "pipe_factory")}
        result = self.manager["drive_started"](client, plan, capture=capture, teardown=teardown,
                                               release=release, source_tree=TREE, source_inventory=INVENTORY)
        self.assertTrue(result["syntheticJourneyPassed"])
        self.assertEqual(events[:3], ["launch_capture", "teardown_bound", "frame"])
        self.assertEqual(events[-3:], ["pipe_close", "teardown_closed", "launch_capture_closed"])

    def test_failed_preframe_custody_closes_client_without_claiming_stop(self):
        for capture_fails in (True, False):
            with self.subTest(capture_fails=capture_fails):
                client, plan, kwargs, events = self.fixture()

                @contextmanager
                def capture(_):
                    if capture_fails:
                        raise RuntimeError("capture unavailable")
                    yield {}, 123

                def teardown(*_):
                    self.assertFalse(capture_fails)
                    raise RuntimeError("teardown unavailable")

                with self.assertRaisesRegex(RuntimeError, "launch_custody_unproven"):
                    self.manager["drive_started"](client, plan, capture=capture, teardown=teardown,
                                                  release=kwargs, source_tree=TREE, source_inventory=INVENTORY)
                self.assertTrue(client.ended and client.stdout.closed and client.stderr.closed)
                self.assertEqual(events, [])

    def test_external_guest_hold_precedes_dispatch_and_is_retained_on_ambiguous_failure(self):
        plan = self.manager["observer_plan"](self.manager["build_plan"](**ARGS))
        for scenario in ("dispatch_lost", "capture_failed", "bind_failed", "invalid_zero", "clean"):
            with self.subTest(scenario=scenario):
                events = []
                reserved = False

                def reserve(actual):
                    nonlocal reserved
                    self.assertEqual(actual, plan)
                    if reserved:
                        raise RuntimeError("guest already reserved")
                    reserved = True
                    events.append("reserve")
                    return {"held": lambda: reserved, "complete": complete}

                def complete(proof):
                    nonlocal reserved
                    self.assertEqual(proof, self.manager["ZERO"])
                    self.assertEqual(scenario, "clean")
                    reserved = False
                    events.append("complete")
                    return True

                def dispatch(held):
                    self.assertTrue(held())
                    events.append("dispatch")
                    if scenario == "dispatch_lost":
                        raise RuntimeError(scenario)
                    return object(), plan

                def drive(_, actual, held):
                    self.assertTrue(held())
                    self.assertEqual(actual, plan)
                    events.append("drive")
                    if scenario in {"capture_failed", "bind_failed"}:
                        raise RuntimeError(scenario)
                    proof = dict(self.manager["ZERO"])
                    if scenario == "invalid_zero":
                        proof["namespaceProcesses"] = 1
                    return {"syntheticJourneyPassed": True, "teardown": proof}

                run = lambda: self.manager["run_in_guest_custody"](plan, reserve=reserve, dispatch=dispatch, drive=drive)
                if scenario == "clean":
                    self.assertTrue(run()["syntheticJourneyPassed"])
                    self.assertFalse(reserved)
                    self.assertEqual(events, ["reserve", "dispatch", "drive", "complete"])
                else:
                    expected = "unit_teardown" if scenario == "invalid_zero" else scenario
                    with self.assertRaisesRegex((RuntimeError, ValueError), expected):
                        run()
                    self.assertTrue(reserved)
                    with self.assertRaisesRegex(RuntimeError, "already reserved"):
                        run()
                    self.assertEqual(events, ["reserve", "dispatch"] + ([] if scenario == "dispatch_lost" else ["drive"]))

    @unittest.skipIf(sys.platform == "linux" and os.getuid() == 0, "privileged Linux caller")
    def test_non_guest_launch_refuses_before_callbacks(self):
        with self.assertRaisesRegex(ValueError, "linux_root"):
            self.manager["start_waiting"](ARGS, {}, independently_admitted=lambda _: self.fail("called"),
                                         unit_absent=lambda _: self.fail("called"))

    def test_frame_reader_checks_real_pipe_and_rejects_extra_frames(self):
        pipe_type = self.manager["UnitPipe"]
        child = subprocess.Popen([sys.executable, "-I", "-S", "-B", "-c", "print('one');print('two')"],
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        pipe = pipe_type(child)
        try:
            # Reap the local test fixture first so the two lines cannot be
            # mistaken for separate time-bounded phases.
            self.assertEqual(child.wait(timeout=5), 0)
            deadline = self.manager["monotonic_ns"]() + 5_000_000_000
            with self.assertRaisesRegex(ValueError, "unit_frame"):
                pipe.line(deadline)
        finally:
            pipe.close()


if __name__ == "__main__":
    unittest.main()
