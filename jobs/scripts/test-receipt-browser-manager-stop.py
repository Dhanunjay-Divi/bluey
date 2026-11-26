"""Synthetic manager/reference controls; never opens a real system bus."""
from contextlib import contextmanager
import ctypes
from pathlib import Path
import runpy
import sys
from types import SimpleNamespace
import unittest

M = runpy.run_path(str(Path(__file__).with_name("receipt-browser-manager-stop.py")))
RUN, INV = "1" * 32, "2" * 32
UNIT = f"bluey-jobs-receipt-{RUN}.service"
LIVE = {"Id": UNIT, "InvocationID": INV, "ControlGroup": f"/system.slice/{UNIT}",
        "LoadState": "loaded", "ActiveState": "active", "SubState": "running", "MainPID": "55", "Job": ""}
AFTER = {**LIVE, "ControlGroup": "", "SubState": "exited", "MainPID": "0"}
STOPPED = {**AFTER, "ActiveState": "inactive", "SubState": "dead"}
TIMED = {**AFTER, "ActiveState": "failed", "SubState": "failed"}


@contextmanager
def simulated(trace, data, *, mode="case", stopped="42", ref_fails=False, unref_fails=False):
    deadline = M["clock"]() + 3_000_000_000
    states = iter(data)

    def reference(method, unit, end):
        assert unit == UNIT and type(end) is int
        trace.append(method)
        if (method == "RefUnit" and ref_fails) or (method == "UnrefUnit" and unref_fails):
            raise RuntimeError(method + " failed")

    def read(end):
        assert end == deadline
        trace.append("read")
        return next(states)

    def stop(invocation, end):
        assert invocation == INV and end == deadline
        trace.append("stop")
        return stopped(invocation) if callable(stopped) else stopped

    with M["scoped_reference"](UNIT, INV, mode, deadline, reference, read, stop, lambda: None,
                               pause=lambda _: None) as action:
        yield action, deadline


class ManagerTests(unittest.TestCase):
    def test_reference_stays_held_through_normal_and_timeout_census(self):
        for mode, terminal in [("case", AFTER), ("timeout-control", TIMED)]:
            calls = []
            with self.subTest(mode=mode), simulated(calls, [LIVE, LIVE, terminal, STOPPED], mode=mode) as (stop, end):
                self.assertTrue(stop(UNIT, INV, end))
                self.assertNotIn("UnrefUnit", calls)
                calls.append("census")
                with self.assertRaisesRegex(RuntimeError, "manager_stop_scope"):
                    stop(UNIT, INV, end)
            self.assertEqual(calls, ["read", "RefUnit", "read", "read", "stop", "read", "census", "UnrefUnit"])

    def test_replaced_invocation_or_queued_job_never_receives_stop(self):
        for drift in [{"InvocationID": "9" * 32}, {"Job": "38"}, {"LoadState": "not-found"},
                      {"Id": "foreign.service"}]:
            calls = []
            with self.subTest(drift=drift), self.assertRaisesRegex(RuntimeError, "identity_or_pending"):
                with simulated(calls, [LIVE, LIVE, dict(AFTER, **drift)]) as (stop, end):
                    stop(UNIT, INV, end)
            self.assertEqual(calls, ["read", "RefUnit", "read", "read", "UnrefUnit"])

    def test_no_rebind_or_stop_after_platform_state_drift(self):
        calls = []
        with self.assertRaisesRegex(RuntimeError, "bind_changed"):
            with simulated(calls, [LIVE, dict(LIVE, MainPID="56")]):
                self.fail("accepted rebound unit")
        self.assertEqual(calls, ["read", "RefUnit", "read", "UnrefUnit"])
        calls = []
        with self.assertRaisesRegex(RuntimeError, "unexpected_state"):
            with simulated(calls, [LIVE, LIVE, AFTER], mode="timeout-control") as (stop, end):
                stop(UNIT, INV, end)
        self.assertNotIn("stop", calls)

    def test_stop_requires_exact_final_state_and_transcript(self):
        for after, reply, reason in [(dict(STOPPED, InvocationID=""), "42", "identity_or_pending"),
                                     (TIMED, "42", "unexpected_state"),
                                     (dict(AFTER, Job="43"), "42", "changed_job"),
                                     (STOPPED, "unexpected", "manager_stop_job")]:
            trace = []
            with self.subTest(reason=reason), self.assertRaisesRegex(RuntimeError, reason):
                with simulated(trace, [LIVE, LIVE, AFTER, after], stopped=reply) as (stop, end):
                    stop(UNIT, INV, end)
            self.assertEqual(trace.count("stop"), 1)
            self.assertEqual(trace.count("UnrefUnit"), 1)

    def test_reference_failure_and_unreference_failure_are_distinct(self):
        trace = []
        with self.assertRaisesRegex(RuntimeError, "RefUnit failed"):
            with simulated(trace, [LIVE], ref_fails=True):
                self.fail("unexpected lease")
        self.assertEqual(trace, ["read", "RefUnit", "UnrefUnit"])
        trace = []
        with self.assertRaisesRegex(RuntimeError, "UnrefUnit failed"):
            with simulated(trace, [LIVE, LIVE], unref_fails=True):
                pass
        self.assertEqual(trace[-1], "UnrefUnit")

    def test_only_enqueued_stop_job_may_be_pending_during_readback(self):
        trace = []
        with simulated(trace, [LIVE, LIVE, AFTER, dict(AFTER, Job="42"), STOPPED]) as (stop, end):
            self.assertTrue(stop(UNIT, INV, end))
        self.assertEqual(trace, ["read", "RefUnit", "read", "read", "stop", "read", "read", "UnrefUnit"])

    def test_replacement_at_delivery_never_gets_the_old_invocations_stop(self):
        trace = []
        effects = []
        replacement = "9" * 32

        def manager_stop(addressed_invocation):
            if addressed_invocation != replacement:
                raise RuntimeError("fixture_invocation_no_longer_registered")
            effects.append(replacement)
            return "42"

        with self.assertRaisesRegex(RuntimeError, "no_longer_registered"):
            with simulated(trace, [LIVE, LIVE, AFTER], stopped=manager_stop) as (stop, end):
                stop(UNIT, INV, end)
        self.assertEqual(effects, [])
        self.assertEqual(trace[-2:], ["stop", "UnrefUnit"])

    def test_stop_callback_never_outlives_reference_scope(self):
        trace = []
        with simulated(trace, [LIVE, LIVE]) as (stop, end):
            pass
        with self.assertRaisesRegex(RuntimeError, "manager_stop_scope"):
            stop(UNIT, INV, end)
        self.assertEqual(trace, ["read", "RefUnit", "read", "UnrefUnit"])

    def test_manager_projection_rejects_duplicate_and_extra_fields(self):
        raw = "".join(f"{k}={v}\n" for k, v in LIVE.items()).encode()
        self.assertEqual(M["parse_show"](raw), LIVE)
        for body in (raw + b"Job=1\n", raw.rstrip(), raw + b"Path=unexpected\n"):
            with self.subTest(body=body), self.assertRaisesRegex(RuntimeError, "show_"):
                M["parse_show"](body)

    def test_sdbus_fixed_address_string_argument_and_one_connection(self):
        calls = []
        p = ctypes.c_void_p

        def assign(ptr, value):
            ctypes.cast(ptr, ctypes.POINTER(p))[0] = value
            return 0

        def request(bus, out, service, path, interface, method):
            self.assertEqual(bus.value, 11)
            self.assertEqual(service, M["SERVICE"])
            if method == b"Stop":
                self.assertEqual(path, b"/org/freedesktop/systemd1/unit/_32" + b"2" * 31)
                self.assertEqual(interface, M["UNIT"])
            else:
                self.assertEqual((path, interface), (M["MANAGER_PATH"], M["MANAGER"]))
            calls.append(method)
            return assign(out, 22)

        def append(message, kind, data):
            self.assertEqual(message.value, 22)
            self.assertEqual(kind, b"s")
            expected = b"fail" if calls[-1] == b"Stop" else UNIT.encode()
            self.assertEqual(ctypes.cast(data, ctypes.c_char_p).value, expected)
            return 0

        def call(bus, message, usec, error, out):
            self.assertEqual((bus.value, message.value, error), (11, 22, None))
            self.assertTrue(0 < usec <= 2_000_000)
            return assign(out, 33)

        job = ctypes.c_char_p(b"/org/freedesktop/systemd1/job/42")

        def read_job(reply, kind, out):
            self.assertEqual((reply.value, kind), (33, b"o"))
            ctypes.cast(out, ctypes.POINTER(p))[0] = ctypes.cast(job, p)
            return 1

        lib = SimpleNamespace(sd_bus_new=lambda out: assign(out, 11),
                              sd_bus_set_address=lambda bus, address: calls.append(address) or 0,
                              sd_bus_set_bus_client=lambda bus, enabled: 0,
                              sd_bus_start=lambda bus: 0, sd_bus_message_new_method_call=request,
                              sd_bus_message_append_basic=append, sd_bus_call=call,
                              sd_bus_message_read_basic=read_job,
                              sd_bus_message_at_end=lambda reply, full: int(reply.value == 33 and full == 1),
                              sd_bus_message_unref=lambda message: calls.append(message.value),
                              sd_bus_close_unref=lambda bus: calls.append(bus.value))
        with M["bus_manager"](lib) as (ref, stop):
            ref("RefUnit", UNIT, M["clock"]() + 1_000_000_000)
            self.assertEqual(stop(INV, M["clock"]() + 1_000_000_000), "42")
            ref("UnrefUnit", UNIT, M["clock"]() + 1_000_000_000)
        self.assertEqual(calls, [b"unix:path=/run/dbus/system_bus_socket", b"RefUnit", 33, 22,
                                 b"Stop", 33, 22,
                                 b"UnrefUnit", 33, 22, 11])

    @unittest.skipIf(sys.platform == "linux", "non-Linux entry refusal only")
    def test_public_entry_refuses_darwin_before_inputs(self):
        with self.assertRaisesRegex(RuntimeError, "linux_root"):
            with M["bind_manager_stop"]({}, "case", library_path="", library_sha256="",
                                        independently_admitted=self.fail, deadline_ns=0):
                self.fail("opened an unexpected system bus")


if __name__ == "__main__":
    unittest.main()
