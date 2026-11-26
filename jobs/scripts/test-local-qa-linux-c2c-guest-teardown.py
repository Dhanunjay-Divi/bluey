"""Portable controls for invocation-bound C2C cleanup. No Linux actuation."""
import os
from pathlib import Path
import sys
import unittest


PATH = Path(__file__).resolve().with_name("local-qa-linux-c2c-guest-teardown.py")
MODULE = {"__name__": "_test_c2c_teardown", "__file__": str(PATH)}
exec(compile(PATH.read_bytes(), str(PATH), "exec"), MODULE)
RUN = "1" * 32
UNIT = f"bluey-jobs-qa-{RUN}.service"
CLAIM = {"run": RUN, "unit": UNIT, "invocation": "2" * 32,
         "boot": "33333333-3333-3333-3333-333333333333", "actorUid": 1000,
         "workloadNetns": 4026532555, "mainPid": 1001, "mainStartTime": 123}
PLAN = {"run": RUN, "unit": UNIT, "uid": 1000, "gid": 1000, "runParent": "/tmp/bluey-test-runs-123456789abc"}
ZERO = {"actorProcesses": 0, "namespaceProcesses": 0}


def row(active="active", sub="exited", **kwargs):
    return dict({"Id": UNIT, "InvocationID": CLAIM["invocation"],
                 "ControlGroup": "" if active == "inactive" else f"/system.slice/{UNIT}",
                 "LoadState": "loaded", "ActiveState": active, "SubState": sub,
                 "MainPID": "1001" if sub == "running" else "0", "Job": "",
                 "Result": "success", "ExecMainCode": "1", "ExecMainStatus": "0"}, **kwargs)


class Runtime:
    def __init__(self, rows, *, populated=(0, 0, 0, 0, 0), counts=(ZERO, ZERO, ZERO)):
        self.reads = iter(rows)
        self.population = iter(populated)
        self.counts = iter(counts)
        self.stops, self.events = [], []

    def read(self):
        self.events.append("read")
        return next(self.reads)

    def stop(self, invocation):
        self.events.append("stop")
        self.stops.append(invocation)
        return "7"

    def populated(self):
        value = next(self.population)
        self.events.append(("population", value))
        return value

    def census(self):
        self.events.append("census")
        return next(self.counts)

    def run(self, normal, stop=None):
        return MODULE["stop_and_census"](CLAIM, normal=normal, read=self.read, stop=stop or self.stop,
                                          populated=self.populated, census=self.census, guard=lambda: None,
                                          pause=lambda: self.events.append("pause"))


class GuestTeardown(unittest.TestCase):
    def test_show_rejects_duplicate_fields_and_wrong_unit_invocation(self):
        value = row()
        raw = "".join(f"{key}={item}\n" for key, item in value.items()).encode()
        self.assertEqual(MODULE["parse_show"](raw), value)
        with self.assertRaisesRegex(ValueError, "show_field"):
            MODULE["parse_show"](raw + b"Job=\n")
        for change in ({"InvocationID": "3" * 32}, {"Id": "different.service"}):
            with self.assertRaisesRegex(ValueError, "unit_identity"):
                MODULE["phase"](dict(value, **change), CLAIM)

    def test_success_waits_for_clean_launcher_and_pairs_final_cgroup_with_census(self):
        terminal, stopped = row(), row("inactive", "dead")
        pending = row("deactivating", "stop-sigterm", Job="7")
        control = Runtime([row("active", "running"), terminal, terminal, terminal, pending,
                           stopped, stopped, stopped, stopped], populated=(1, 0, 0, 0, 0))
        self.assertEqual(control.run(True), {"unitStopped": True, **ZERO})
        self.assertEqual(control.stops, [CLAIM["invocation"]])
        self.assertLess(control.events.index(("population", 0)), control.events.index("stop"))
        self.assertEqual(control.events.count("census"), 3)
        self.assertEqual(control.events[-3:], [("population", 0), "read", "census"])

    def test_process_appearing_only_in_final_namespace_census_refuses(self):
        stopped = row("inactive", "dead")
        control = Runtime([stopped] * 4, counts=(ZERO, ZERO, dict(ZERO, namespaceProcesses=1)))
        with self.assertRaisesRegex(ValueError, "live_processes"):
            control.run(False)
        self.assertEqual(control.events.count("census"), 3)

    def test_disappearing_main_requires_same_invocation_terminal_readback(self):
        live, terminal = row("active", "running"), row()

        def gone(pid):
            self.assertEqual(pid, CLAIM["mainPid"])
            raise ProcessLookupError()

        result = MODULE["sample_main"](iter((live, terminal)).__next__, gone, CLAIM, {})
        self.assertEqual(result, terminal)
        for bad, code in ((live, "live_main_changed"),
                          (dict(terminal, InvocationID="3" * 32), "unit_identity")):
            with self.subTest(code=code), self.assertRaisesRegex(ValueError, code):
                MODULE["sample_main"](iter((live, bad)).__next__, gone, CLAIM, {})

    def test_unbound_recovery_can_stop_running_captured_invocation(self):
        stopped = row("inactive", "dead")
        control = Runtime([row("active", "running"), stopped, stopped, stopped, stopped])
        self.assertEqual(control.run(False), {"unitStopped": True, **ZERO})
        self.assertEqual(control.stops, [CLAIM["invocation"]])

    def test_failed_launcher_is_cleaned_but_never_qualified(self):
        failed = row("failed", "failed", Result="exit-code", ExecMainStatus="1")
        stopped = row("inactive", "dead", Result="exit-code", ExecMainStatus="1")
        control = Runtime([failed, stopped, stopped, stopped, stopped])
        with self.assertRaisesRegex(ValueError, "launcher_failed_after_verified_cleanup"):
            control.run(True)
        self.assertEqual(control.stops, [CLAIM["invocation"]])
        self.assertEqual(control.events.count("census"), 3)

    def test_manager_job_and_cgroup_or_namespace_drift_refuse(self):
        stopped = row("inactive", "dead")
        cases = [
            ([row("active", "running", Job="5")], (), (), "existing_job", []),
            ([row("active", "running"), row("deactivating", "stop-sigterm", Job="8")],
             (), (), "stop_job_replaced", [CLAIM["invocation"]]),
            ([stopped], (1,), (), "cgroup_not_empty", []),
            ([stopped, stopped], (0,), (dict(ZERO, namespaceProcesses=1),), "live_processes", []),
            ([stopped, stopped], (0,), (dict(ZERO, actorProcesses=False),), "live_processes", []),
        ]
        for rows, populated, counts, error, stops in cases:
            with self.subTest(error=error, rows=rows):
                control = Runtime(rows, populated=populated, counts=counts)
                with self.assertRaisesRegex(ValueError, error):
                    control.run(False)
                self.assertEqual(control.stops, stops)

    def test_routes_use_preframe_claim_even_on_malformed_actor_output(self):
        seen = []
        routes = MODULE["bind_routes"](PLAN, CLAIM, lambda normal: seen.append(normal) or {"unitStopped": True, **ZERO})
        with self.assertRaisesRegex(ValueError, "route_plan_changed"):
            routes["unbound_cleanup"](dict(PLAN, run="3" * 32))
        self.assertEqual(routes["unbound_cleanup"](PLAN), {"unitStopped": True, **ZERO})
        with self.assertRaisesRegex(ValueError, "teardown_consumed"):
            routes["unbound_cleanup"](PLAN)
        self.assertEqual(seen, [False])

    def test_lost_stop_reply_never_retries_via_another_route(self):
        requested = []

        def unknown(normal):
            requested.append(normal)
            raise TimeoutError("lost synthetic Stop acknowledgement")

        routes = MODULE["bind_routes"](PLAN, CLAIM, unknown)
        observed = {"run": RUN, "unitName": UNIT, "invocationId": CLAIM["invocation"],
                    "bootId": CLAIM["boot"].replace("-", ""), "actorUid": 1000,
                    "networkNamespace": "net:[4026532555]", "mainPid": 1001, "mainStartTime": 123}
        with self.assertRaisesRegex(ValueError, "route_claim"):
            routes["settle"](dict(observed, mainStartTime=999), observed)
        with self.assertRaises(TimeoutError):
            routes["recover"](observed, observed)
        with self.assertRaisesRegex(ValueError, "teardown_consumed"):
            routes["unbound_cleanup"](PLAN)
        self.assertEqual(requested, [False])

    @unittest.skipIf(sys.platform == "linux" and os.getuid() == 0, "privileged Linux caller")
    def test_non_guest_does_not_open_system_bus_or_ask_for_admission(self):
        with self.assertRaisesRegex(ValueError, "linux_root"):
            with MODULE["bind_teardown"](PLAN, CLAIM, -1, systemctl_sha256="a" * 64,
                                         library_path="/fake", library_sha256="b" * 64,
                                         independently_admitted=lambda: self.fail("called"), bind_deadline_ns=1):
                self.fail("bound")


if __name__ == "__main__":
    unittest.main()
