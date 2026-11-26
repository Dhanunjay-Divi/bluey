"""Portable controls for the private Linux parser; no process is signaled."""
import importlib.util
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch


def load_helper():
    path = Path(__file__).with_name("local-qa-linux-postmaster-pidfd.py")
    spec = importlib.util.spec_from_file_location("local_qa_postmaster_pidfd", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


HELPER = load_helper()
ROOT = "/tmp/bluey-test-runs-a1b2c3d4e5f6/run.1795250653.12345.32615190360"
BINDING = {
    "root": ROOT,
    "pid": 12345,
    "startTime": "9007199254740993",
    "port": 43107,
    "dataDirectory": ROOT + "/bluey-jobs-qa-draft/postgres",
    "postgresPath": "/run/bluey-jobs-c2c-postgres/" + "b" * 32
    + "/capsule/usr/lib/postgresql/17/bin/postgres",
    "postgresSha256": "a" * 64,
    "unitCgroup": "/system.slice/bluey-jobs-qa-" + "b" * 32 + ".service",
    "networkNamespace": "net:[4026532555]",
}
REFUSES = (HELPER.Refusal, ValueError, UnicodeError)


def encode(value):
    return json.dumps(value, separators=(",", ":")).encode("utf-8")


class PostmasterPidfdControls(unittest.TestCase):
    def test_exact_private_binding(self):
        self.assertEqual(HELPER.parse_bind(encode(BINDING)), ("postgres", BINDING))

    def test_api_binding_has_a_distinct_closed_shape(self):
        api = {key: value for key, value in BINDING.items()
               if key not in {"dataDirectory", "postgresPath", "postgresSha256"}}
        api.update(stateDirectory=ROOT + "/bluey-jobs-qa-draft", apiPath="/run/admitted/bluey-api", apiSha256="b" * 64)
        self.assertEqual(HELPER.parse_bind(encode(api)), ("api", api))
        for extra in ({"dataDirectory": BINDING["dataDirectory"]}, {"argv": [api["apiPath"]]}, {"signal": "SIGKILL"}):
            with self.assertRaises(REFUSES):
                HELPER.parse_bind(encode({**api, **extra}))

    def test_binding_rejects_arbitrary_targets_booleans_and_malformed_numbers(self):
        for drift in (
            {"signal": "SIGKILL"},
            {"pid": True},
            {"pid": 1},
            {"pid": 2147483648},
            {"startTime": "09007199254740993"},
            {"startTime": "18446744073709551616"},
            {"port": 5432},
            {"port": "43107"},
            {"postgresSha256": "a" * 64 + "\n"},
            {"unitCgroup": "/system.slice/bluey-jobs-qa-" + "c" * 32 + ".service"},
            {"postgresPath": "/run/bluey-jobs-c2c-postgres/" + "c" * 32
             + "/capsule/usr/lib/postgresql/17/bin/postgres"},
        ):
            with self.subTest(drift=drift), self.assertRaises(REFUSES):
                HELPER.parse_bind(encode({**BINDING, **drift}))

    def test_duplicate_keys_invalid_encoding_and_non_records_refuse(self):
        duplicates = b'{"pid":12346,' + encode(BINDING)[1:]
        for frame in (duplicates, b'[]', b'null', b'\xff', b'{}\x00'):
            with self.subTest(frame=frame), self.assertRaises(REFUSES):
                HELPER.parse_bind(frame)

    def test_stat_binds_pid_parent_group_and_starttime_without_numeric_rounding(self):
        fields = ["0"] * 50
        fields[:4] = ["S", "888", "888", "888"]
        fields[19] = BINDING["startTime"]
        stat = ("12345 (postgres (name)) " + " ".join(fields) + "\n").encode()
        self.assertEqual(HELPER.parse_stat(stat, 12345), (12345, 888, 888, BINDING["startTime"]))
        for corrupt in (stat.replace(b"12345 ", b"12346 ", 1), stat + b"\n",
                        stat.replace(b"9007199254740993", b"18446744073709551616"),
                        stat.replace(b" S 888", b" S 0888", 1)):
            with self.subTest(corrupt=corrupt[:70]), self.assertRaises(REFUSES):
                HELPER.parse_stat(corrupt, 12345)

    def test_postgres_command_line_requires_only_the_pinned_capsule_postfix(self):
        expected = [
            BINDING["postgresPath"], "-D", BINDING["dataDirectory"], "-h", "127.0.0.1",
            "-p", str(BINDING["port"]), "-k", "", "-c", "ssl=off", "-c",
            "max_connections=64", "-c",
            "dynamic_library_path=/run/bluey-jobs-c2c-postgres/" + "b" * 32
            + "/capsule/usr/lib/postgresql/17/lib",
        ]
        with patch.object(HELPER, "bounded_read", return_value=("\0".join(expected) + "\0").encode("utf-8")):
            HELPER.exact_cmdline("/proc/12345", BINDING["postgresPath"], BINDING["dataDirectory"], BINDING["port"])
        for path, argv in (
            ("/usr/lib/postgresql/17/bin/postgres", expected),
            (BINDING["postgresPath"], expected[:-2]),
            (BINDING["postgresPath"], [*expected, "-c", "log_statement=all"]),
            (BINDING["postgresPath"], [*expected[:-1], "dynamic_library_path=/tmp"]),
        ):
            with self.subTest(path=path, argv=argv[-2:]), \
                 patch.object(HELPER, "bounded_read", return_value=("\0".join(argv) + "\0").encode("utf-8")):
                with self.assertRaises(HELPER.Refusal):
                    HELPER.exact_cmdline("/proc/12345", path, BINDING["dataDirectory"], BINDING["port"])

    def test_non_linux_never_reads_or_acts_on_the_protocol(self):
        with patch.object(HELPER.sys, "platform", "darwin"), \
             patch.object(HELPER, "safe_stream", side_effect=AssertionError("protocol reached")):
            with self.assertRaises(HELPER.Refusal):
                HELPER.main()

    def test_graceful_stop_and_dispose_each_use_their_one_owned_operation(self):
        requests = iter((b"stop", b"dispose"))
        events = []
        HELPER.serve_commands(lambda: next(requests), events.append,
                              lambda: events.append("stop"), lambda: events.append("dispose"))
        self.assertEqual(events, ["stop", b"stopped\n", "dispose", b"disposed\n"])

    def test_invalid_duplicate_or_eof_command_cannot_signal_another_child(self):
        for requests, expected in (
            ([b"kill"], []),
            ([b"stop", b"stop"], ["stop", b"stopped\n"]),
            ([b""], []),
        ):
            with self.subTest(requests=requests):
                events = []
                source = iter(requests)
                with self.assertRaises(HELPER.Refusal):
                    HELPER.serve_commands(lambda: next(source), events.append,
                                          lambda: events.append("stop"), lambda: events.append("dispose"))
                self.assertEqual(events, expected)

    def test_dispose_does_not_send_a_signal_and_waits_for_its_owned_check(self):
        events = []
        with self.assertRaises(HELPER.Refusal):
            HELPER.serve_commands(lambda: b"dispose", events.append,
                                  lambda: events.append("stop"),
                                  lambda: HELPER.need(False))
        self.assertEqual(events, [])
        HELPER.serve_commands(lambda: b"dispose", events.append,
                              lambda: events.append("stop"), lambda: events.append("dispose"))
        self.assertEqual(events, ["dispose", b"disposed\n"])

    def test_frame_reader_refuses_extra_truncated_or_oversized_data_on_anonymous_pipe(self):
        for frame, maximum, expected in (
            (b"stop\n", 32, b"stop"),
            (b"stop\ndispose\n", 32, None),
            (b"sto", 32, None),
            (b"a" * 32 + b"\n", 32, None),
        ):
            with self.subTest(frame=frame):
                receiver, writer = os.pipe()
                try:
                    os.write(writer, frame)
                finally:
                    os.close(writer)
                try:
                    reader = HELPER.FrameReader(receiver)
                    if expected is None:
                        with self.assertRaises(HELPER.Refusal):
                            reader.line(maximum, 0.5)
                    else:
                        self.assertEqual(reader.line(maximum, 0.5), expected)
                finally:
                    os.close(receiver)


if __name__ == "__main__":
    unittest.main()
