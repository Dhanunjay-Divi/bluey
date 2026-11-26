"""Pure controls: no systemctl, root operations, or actual service."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import tempfile
import unittest

M = runpy.run_path(str(Path(__file__).with_name("receipt-browser-unit-terminal.py")))
SCOPE = {
    "run": "1" * 32, "boot": "e49df459-43b1-4b26-a11f-d0b210879cb5",
    "invocation": "2" * 32, "challenge": "3" * 32,
    "root": "/tmp/bluey-test-runs-deadbeef1234/run.1000.20.123",
    "actorUid": 1001, "unit": "bluey-jobs-receipt-" + "1" * 32 + ".service",
}
LIVE = {
    "Id": SCOPE["unit"], "InvocationID": SCOPE["invocation"],
    "ControlGroup": "/system.slice/" + SCOPE["unit"], "LoadState": "loaded",
    "ActiveState": "active", "SubState": "running", "MainPID": "55",
    "Restart": "no", "NRestarts": "0", "Result": "success",
    "ExecMainCode": "0", "ExecMainStatus": "0", "ExecMainPID": "55",
    "ExecMainStartTimestampMonotonic": "100", "ExecMainExitTimestampMonotonic": "0",
}
DONE = {**LIVE, "ControlGroup": "", "SubState": "exited", "MainPID": "0",
        "ExecMainCode": "1", "ExecMainExitTimestampMonotonic": "260"}
TIMED = {**DONE, "ActiveState": "failed", "SubState": "failed", "Result": "timeout",
         "ExecMainCode": "2", "ExecMainStatus": "15"}


def raw(rows):
    return "".join(f"{k}={v}\n" for k, v in rows.items()).encode("ascii")


def reader(*rows, mode="case", scope=SCOPE, boot=None, now=None, admitted=lambda: True):
    source = iter(raw(row) if isinstance(row, dict) else row for row in rows)
    if now is None:
        now = iter([190000, 200000, 280000, 290000, 300000, 310000, 320000]).__next__
    return M["prepare_reader"](scope, mode, lambda: next(source),
                               boot or (lambda: SCOPE["boot"]), now, admitted, 350000)


class TerminalTests(unittest.TestCase):
    def test_terminal_export_is_bound_to_the_publisher_bytes_and_inode(self):
        writer = runpy.run_path(str(Path(__file__).with_name("receipt-browser-release-writer.py")))
        barrier = writer["helper"]("receipt-browser-launch-barrier.py", writer["BARRIER"])
        waiting = {**SCOPE, "rootDevice": 3, "rootInode": 4, "capsuleDevice": 3,
                   "capsuleInode": 5, "actorPid": 44, "actorPgid": 44, "waitingNs": 190000,
                   "workloadNetns": 7, "capsuleInventory": "a" * 64, "journeyClosure": "b" * 64}
        supervised = {key: value for key, value in waiting.items()
                      if key not in ("challenge", "waitingNs")}
        supervised.update(supervisorNetns=8, interfaces=["lo"], defaultRoutes=0)
        bound = {"event": "jobs_browser_closure_bound", "mountId": 11, "eventsInode": 12,
                 **{key: SCOPE[key] for key in ("run", "boot", "invocation")}}
        body = barrier["render_release"](waiting, supervised, bound, 220000)
        observation = reader(LIVE, DONE, DONE)(240000)
        with tempfile.TemporaryDirectory() as path:
            os.chmod(path, 0o755)
            gate = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
            try:
                receipt = writer["publish_at"](gate, body, lambda: None, owner=os.getuid(),
                                               clock=iter([230000, 240000]).__next__)
                release = barrier["read_release"](gate, uid=os.getuid())
                validate = barrier["validate_release"]
                self.assertEqual(M["release_hash"](observation, receipt, release, waiting, validate),
                                 receipt["releaseSha256"])
                for modified in [dict(receipt, releaseSha256="0" * 64),
                                 dict(receipt, inode=receipt["inode"] + 1)]:
                    with self.assertRaisesRegex(RuntimeError, "published_release_changed"):
                        M["release_hash"](observation, modified, release, waiting, validate)
                with self.assertRaisesRegex(RuntimeError, "publication_time"):
                    M["release_hash"](observation, dict(receipt, publicationAfterNs=240001),
                                      release, waiting, validate)
                with self.assertRaisesRegex(RuntimeError, "release_scope"):
                    M["release_hash"](dict(observation, record=dict(observation["record"],
                                                                  challenge="f" * 32)),
                                      receipt, release, waiting, validate)
            finally:
                os.close(gate)

    def test_export_is_one_use_and_returns_checked_readonly_fd(self):
        observation = reader(LIVE, DONE, DONE)(240000)
        release_hash = "7" * 64
        body = M["render_terminal"](observation, release_hash)
        self.assertEqual(json.loads(body), {**observation["record"], "releaseSha256": release_hash})
        self.assertTrue(body.endswith(b"\n"))
        with tempfile.TemporaryDirectory() as path:
            parent = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
            try:
                ref = M["_write_terminal_at"](parent, body, os.getuid())
                try:
                    info = os.fstat(ref["fd"])
                    self.assertEqual(fcntl.fcntl(ref["fd"], fcntl.F_GETFL) & os.O_ACCMODE, os.O_RDONLY)
                    self.assertEqual((info.st_dev, info.st_ino, info.st_nlink, info.st_mode & 0o7777),
                                     (ref["device"], ref["inode"], 1, 0o600))
                    self.assertEqual(os.pread(ref["fd"], ref["bytes"] + 1, 0), body)
                    self.assertEqual(ref["sha256"], hashlib.sha256(body).hexdigest())
                    with self.assertRaises(FileExistsError):
                        M["_write_terminal_at"](parent, body, os.getuid())
                finally:
                    os.close(ref["fd"])
            finally:
                os.close(parent)

    def test_export_refuses_exposed_directory_and_malformed_observation(self):
        observation = reader(LIVE, DONE, DONE)(240000)
        with self.assertRaisesRegex(RuntimeError, "export_release"):
            M["render_terminal"](observation, "bad")
        observation["record"]["status"]["code"] = False
        with self.assertRaisesRegex(RuntimeError, "export_status"):
            M["render_terminal"](observation, "9" * 64)
        with tempfile.TemporaryDirectory() as path:
            os.chmod(path, 0o755)
            parent = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
            try:
                with self.assertRaisesRegex(RuntimeError, "export_directory"):
                    M["_write_terminal_at"](parent, b"fixture", os.getuid())
                self.assertEqual(os.listdir(path), [])
            finally:
                os.close(parent)

    def test_success_retains_scope_and_original_manager_samples(self):
        scope = SCOPE.copy()
        capture = reader(LIVE, DONE, DONE, scope=scope)
        scope["challenge"] = "a" * 32
        result = capture(240000)
        self.assertEqual(result["record"], dict(SCOPE, status={
            "code": 0, "signal": None, "deadline": False}))
        self.assertEqual(result["managerSamples"], tuple(map(raw, [LIVE, DONE, DONE])))
        self.assertEqual(result["exitNs"], 260000)
        self.assertLess(result["observedNs"], result["confirmedNs"])

    def test_actual_timeout_result_and_signal_encoding_are_required(self):
        for status, signal in [("15", "SIGTERM"), ("9", "SIGKILL")]:
            timed = dict(TIMED, ExecMainStatus=status)
            result = reader(LIVE, timed, timed, mode="timeout-control")(240000)
            self.assertEqual(result["record"]["status"], {
                "code": None, "signal": signal, "deadline": True})
        for invalid in [DONE, dict(TIMED, Result="signal"), dict(TIMED, ExecMainCode="3"),
                        dict(TIMED, ExecMainStatus="11")]:
            with self.subTest(invalid=invalid), self.assertRaisesRegex(RuntimeError, "not_timeout"):
                reader(LIVE, invalid, mode="timeout-control")(240000)
        with self.assertRaisesRegex(RuntimeError, "not_success"):
            reader(LIVE, TIMED)(240000)

    def test_other_invocation_restart_pid_generation_and_changed_snapshot(self):
        for key, value, reason in [
            ("InvocationID", "f" * 32, "manager_identity"),
            ("NRestarts", "1", "manager_identity"),
            ("ExecMainPID", "56", "exec_changed"),
            ("ExecMainStartTimestampMonotonic", "101", "exec_changed"),
            ("ControlGroup", "/foreign", "manager_not_terminal"),
            ("ExecMainExitTimestampMonotonic", "239", "exit_time"),
        ]:
            with self.subTest(key=key), self.assertRaisesRegex(RuntimeError, reason):
                reader(LIVE, dict(DONE, **{key: value}))(240000)
        with self.assertRaisesRegex(RuntimeError, "snapshot_changed"):
            reader(LIVE, DONE, dict(DONE, SubState="dead"))(240000)

    def test_no_terminal_without_live_admission_and_publication(self):
        with self.assertRaisesRegex(RuntimeError, "not_live"):
            reader(DONE)
        with self.assertRaisesRegex(RuntimeError, "release_order"):
            reader(LIVE)(199999)
        flags = iter([True, True, True, True, False])
        with self.assertRaisesRegex(RuntimeError, "no_supervision"):
            reader(LIVE, DONE, admitted=flags.__next__)(240000)
        times = iter([190000, 200000, 280000, 290000, 350000])
        with self.assertRaisesRegex(RuntimeError, "no_supervision"):
            reader(LIVE, DONE, now=times.__next__)(240000)

    def test_boot_is_checked_on_both_sides_of_each_query(self):
        boots = iter([SCOPE["boot"]] * 4 + ["changed"])
        with self.assertRaisesRegex(RuntimeError, "boot_changed"):
            reader(LIVE, DONE, boot=boots.__next__)(240000)

    def test_closed_scope_and_manager_fields(self):
        for malformed in [raw(LIVE) + b"Id=another\n", raw(LIVE).replace(b"\n", b"\r\n"),
                          raw(LIVE).rstrip(b"\n"), raw(LIVE) + b"Extra=x\n"]:
            with self.subTest(), self.assertRaisesRegex(RuntimeError, "manager_"):
                reader(malformed)
        with self.assertRaisesRegex(RuntimeError, "unit_terminal_nonce"):
            reader(LIVE, scope=dict(SCOPE, challenge="0" * 32))
        with self.assertRaisesRegex(RuntimeError, "unit_terminal_scope"):
            reader(LIVE, scope=dict(SCOPE, actorUid=True))

    @unittest.skipIf(sys.platform == "linux", "only proves non-Linux refusal")
    def test_actual_entry_refuses_darwin_before_admission(self):
        with self.assertRaisesRegex(RuntimeError, "linux_root"):
            M["bind_unit_terminal"](SCOPE, "case", independently_admitted=lambda: self.fail(),
                                    deadline_ns=350000)
        with self.assertRaisesRegex(RuntimeError, "linux_root"):
            M["export_unit_terminal"](-1, -1, {}, {}, {})


if __name__ == "__main__":
    unittest.main()
