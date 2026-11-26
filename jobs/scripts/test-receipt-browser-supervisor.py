"""Real observer-pipe children; no systemd, root operations or Jobs runtime."""
from contextlib import contextmanager
import os
from pathlib import Path
import runpy
import sys
import tempfile
import unittest

M = runpy.run_path(str(Path(__file__).with_name("receipt-browser-supervisor.py")))
SCOPE = {"run": "1" * 32, "boot": "e49df459-43b1-4b26-a11f-d0b210879cb5",
         "invocation": "2" * 32}
BOUND = M["observer_frame"]("jobs_browser_closure_bound",
                            {**SCOPE, "mountId": 17, "eventsInode": 22})
ZERO = M["observer_frame"]("jobs_browser_closure_observed",
                           {**SCOPE, "populated": 0, "sameEventsDescriptor": True})
PROGRAM = """
import pathlib, sys, time
print(sys.argv[2], end="", flush=True)
deadline = time.monotonic() + 2
while not pathlib.Path(sys.argv[1]).exists():
    if time.monotonic() >= deadline:
        raise SystemExit(70)
    time.sleep(0.01)
if sys.argv[5] == "1":
    print("synthetic error", file=sys.stderr, flush=True)
print(sys.argv[3], end="", flush=True)
raise SystemExit(int(sys.argv[4]))
"""


class SupervisorTests(unittest.TestCase):
    def invoke(self, trace, *, first=BOUND, last=ZERO, child_exit=0, stderr=False,
               capture_error=False, fail_admission_after_export=False, publish_gate=True,
               final_deadline_ms=2500, wrong_exit_order=False, use_manager_parser=False):
        with tempfile.TemporaryDirectory() as parent:
            release_gate = Path(parent) / "conditional-fixture"
            process_args = [sys.executable, "-IB", "-c", PROGRAM, str(release_gate),
                            first.decode(), last.decode(), str(child_exit), str(int(stderr))]
            reference_fd = None
            publication_ns = 0

            def bind():
                trace.append("bound")
                if use_manager_parser:
                    suite = runpy.run_path(str(Path(__file__).with_name(
                        "test-receipt-browser-unit-terminal.py")))
                    started = str(M["clock"]() // 1000 - 1)

                    def manager():
                        if publication_ns == 0:
                            return suite["raw"](dict(suite["LIVE"], ExecMainStartTimestampMonotonic=started))
                        return suite["raw"](dict(
                            suite["DONE"], ExecMainStartTimestampMonotonic=started,
                            ExecMainExitTimestampMonotonic=str(publication_ns // 1000 + 1)))

                    terminal = suite["M"]["prepare_reader"](
                        suite["SCOPE"], "case", manager, lambda: SCOPE["boot"], M["clock"],
                        lambda: True, now + final_deadline_ms * 1_000_000)

                    def capture_real(ns):
                        trace.append("terminal")
                        return terminal(ns)

                    return capture_real

                def capture(publication_ns):
                    self.assertGreater(publication_ns, 0)
                    trace.append("terminal")
                    if capture_error:
                        raise RuntimeError("fixture_terminal_refusal")
                    observed = M["clock"]()
                    return {"exitNs": observed if wrong_exit_order else publication_ns + 1,
                            "observedNs": observed}

                return capture

            def publish(bound, live):
                nonlocal publication_ns
                self.assertEqual(bound, BOUND)
                self.assertTrue(live())
                trace.append("released")
                publication_ns = M["clock"]()
                if publish_gate:
                    release_gate.touch(exist_ok=False)
                return {"publicationAfterNs": publication_ns}

            def export(observed, published):
                nonlocal reference_fd
                self.assertGreater(observed["observedNs"], published["publicationAfterNs"])
                trace.append("exported")
                reference_fd = os.open(os.devnull, os.O_RDONLY)
                return {"fd": reference_fd}

            now = M["clock"]()
            try:
                with M["open_observer"](process_args) as observer:
                    result = M["join"](SCOPE, observer, bind, publish, export,
                                       lambda: not fail_admission_after_export or "exported" not in trace,
                                       now + 1_500_000_000, now + final_deadline_ms * 1_000_000)
                    self.assertFalse(observer.live())
                    self.assertEqual(observer.child.returncode, 0)
                    return result
            finally:
                if reference_fd is not None:
                    if fail_admission_after_export:
                        with self.assertRaises(OSError):
                            os.fstat(reference_fd)
                    else:
                        os.close(reference_fd)

    def test_bound_real_child_releases_then_zero_then_manager_then_export(self):
        trace = []
        result = self.invoke(trace)
        self.assertEqual(trace, ["bound", "released", "terminal", "exported"])
        self.assertEqual(result["observer"]["stdout"].encode(), BOUND + ZERO)
        self.assertEqual(result["launchObserver"]["eventsInode"], 22)
        self.assertLess(result["zeroAcceptedNs"], result["terminal"]["observedNs"])
        self.assertFalse(result["runtimeAdmitted"])

    def test_real_terminal_parser_feeds_exit_timestamp_to_join(self):
        trace = []
        result = self.invoke(trace, use_manager_parser=True)
        self.assertEqual(trace, ["bound", "released", "terminal", "exported"])
        self.assertLess(result["release"]["publicationAfterNs"], result["terminal"]["exitNs"])
        self.assertLess(result["terminal"]["exitNs"], result["zeroAcceptedNs"])
        self.assertEqual(result["terminal"]["record"]["status"],
                         {"code": 0, "signal": None, "deadline": False})

    def test_observer_nonzero_stderr_or_wrong_final_cannot_capture_or_export(self):
        for args, reason in [
            ({"last": b""}, "observer_zero"),
            ({"last": ZERO + ZERO}, "observer_zero"),
            ({"last": ZERO.replace(b'"populated": 0', b'"populated": 1')}, "observer_zero"),
            ({"last": ZERO.replace(b'"sameEventsDescriptor": true', b'"sameEventsDescriptor": false')},
             "observer_zero"),
            ({"last": ZERO.replace(SCOPE["invocation"].encode(), b"4" * 32)}, "observer_zero"),
            ({"child_exit": 5}, "observer_exit"),
            ({"stderr": True}, "observer_stderr"),
        ]:
            trace = []
            with self.subTest(args=args), self.assertRaisesRegex(RuntimeError, reason):
                self.invoke(trace, **args)
            self.assertEqual(trace, ["bound", "released"])

    def test_wrong_or_duplicate_bound_frame_never_publishes(self):
        for first, reason in [(BOUND.replace(SCOPE["run"].encode(), b"9" * 32), "bound_scope"),
                              (BOUND + ZERO, "observer_not_held")]:
            trace = []
            with self.subTest(reason=reason), self.assertRaisesRegex(RuntimeError, reason):
                self.invoke(trace, first=first)
            self.assertEqual(trace, [])

    def test_missing_closure_times_out_without_export(self):
        trace = []
        with self.assertRaisesRegex(RuntimeError, "observer_deadline"):
            self.invoke(trace, publish_gate=False, final_deadline_ms=400)
        self.assertEqual(trace, ["bound", "released"])

    def test_manager_refusal_after_zero_cannot_export(self):
        trace = []
        with self.assertRaisesRegex(RuntimeError, "fixture_terminal_refusal"):
            self.invoke(trace, capture_error=True)
        self.assertEqual(trace, ["bound", "released", "terminal"])

    def test_late_manager_exit_cannot_be_spliced_to_earlier_zero(self):
        trace = []
        with self.assertRaisesRegex(RuntimeError, "terminal_order"):
            self.invoke(trace, wrong_exit_order=True)
        self.assertEqual(trace, ["bound", "released", "terminal"])

    def test_lost_admission_during_export_closes_returned_descriptor(self):
        with self.assertRaisesRegex(RuntimeError, "export_deadline"):
            self.invoke([], fail_admission_after_export=True)

    def test_stop_then_census_are_required_before_returning_its_artifact(self):
        for failure in (None, "stop", "census"):
            trace = []
            fd = os.open(os.devnull, os.O_RDONLY)
            deadline = M["clock"]() + 1_000_000_000
            terminal = {"record": {"unit": "expected.service", "invocation": "inv"}}
            evidence = {"terminalArtifact": {"fd": fd}, "terminal": terminal}

            def stop(unit, invocation, bound):
                self.assertEqual((unit, invocation, bound), ("expected.service", "inv", deadline))
                trace.append("stop")
                return failure != "stop"

            def census():
                trace.append("census")
                return {"unitStopped": True, "actorProcesses": 0,
                        "namespaceProcesses": int(failure == "census")}

            accept = M["accept_after_stop"]
            if failure is not None:
                with self.subTest(failure=failure), self.assertRaisesRegex(RuntimeError, "supervisor_stop"):
                    accept(evidence, stop, census, lambda: True, deadline)
                with self.assertRaises(OSError):
                    os.fstat(fd)
            else:
                try:
                    result = accept(evidence, stop, census, lambda: True, deadline)
                    self.assertEqual(result["postStop"]["namespaceProcesses"], 0)
                    self.assertLess(result["stoppedNs"], deadline)
                    os.fstat(fd)
                finally:
                    os.close(fd)
            self.assertEqual(trace, ["stop"] if failure == "stop" else ["stop", "census"])

    def test_manager_reference_cleanup_failure_withholds_the_artifact(self):
        fd = os.open(os.devnull, os.O_RDONLY)
        trace = []

        @contextmanager
        def ref():
            trace.append("ref")
            yield "scoped-stop"
            trace.append("unref")
            raise RuntimeError("fixture_unref_failed")

        def supervise(stop):
            self.assertEqual(stop, "scoped-stop")
            trace.append("supervise")
            return {"terminalArtifact": {"fd": fd}}

        with self.assertRaisesRegex(RuntimeError, "fixture_unref_failed"):
            M["under_manager_stop"](ref(), supervise, lambda: True, M["clock"]() + 1_000_000_000)
        self.assertEqual(trace, ["ref", "supervise", "unref"])
        with self.assertRaises(OSError):
            os.fstat(fd)

    @unittest.skipIf(sys.platform == "linux", "pure platform refusal only")
    def test_darwin_root_entry_refuses_before_admission_callback(self):
        with self.assertRaisesRegex(RuntimeError, "linux_root"):
            M["supervise_waiting_unit"]({}, {}, -1, "case", independently_admitted=self.fail,
                                        release_deadline_ns=0, deadline_ns=0)


if __name__ == "__main__":
    unittest.main()
