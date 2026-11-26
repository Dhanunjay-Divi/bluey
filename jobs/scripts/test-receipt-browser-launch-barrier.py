"""Exercise release ordering without starting a browser, PostgreSQL or systemd."""
import importlib.util
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("receipt-browser-launch-barrier.py").resolve()
SPEC = importlib.util.spec_from_file_location("barrier", SCRIPT)
BARRIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BARRIER)


def waiting_frame():
    run = "ab" * 16
    return {
        "run": run, "unit": f"bluey-jobs-receipt-{run}.service",
        "boot": "12345678-abcd-1234-abcd-123456789abc", "invocation": "cd" * 16,
        "root": "/tmp/bluey-test-runs-123456789abc/run.1234.5678.999",
        "rootDevice": 1, "rootInode": 2, "capsuleDevice": 1, "capsuleInode": 3,
        "actorPid": 22, "actorPgid": 20, "actorUid": 1234, "challenge": "ef" * 16,
        "waitingNs": BARRIER.kernel_monotonic_ns(), "workloadNetns": 6,
        "capsuleInventory": BARRIER.CAPSULE, "journeyClosure": "ce" * 32,
    }


def supervisor(waiting):
    return {
        **{k: v for k, v in waiting.items() if k not in {"challenge", "waitingNs"}},
        "supervisorNetns": 5, "interfaces": ["lo"], "defaultRoutes": 0,
    }


def observer(waiting):
    return {"event": "jobs_browser_closure_bound", "run": waiting["run"],
            "boot": waiting["boot"], "invocation": waiting["invocation"],
            "mountId": 418, "eventsInode": 912}


class ReleaseBarrier(unittest.TestCase):
    def setUp(self):
        self.waiting = waiting_frame()
        self.supervised = supervisor(self.waiting)
        self.bound = observer(self.waiting)

    def release(self):
        return BARRIER.render_release(self.waiting, self.supervised, self.bound, BARRIER.kernel_monotonic_ns())

    def test_requires_observer_bound_not_terminal(self):
        self.bound["event"] = "jobs_browser_closure_observed"
        with self.assertRaisesRegex(BARRIER.Refused, "^supervisor_observer_not_bound$"):
            self.release()

    def test_rejects_observer_from_another_invocation(self):
        self.bound["invocation"] = "ae" * 16
        with self.assertRaisesRegex(BARRIER.Refused, "^supervisor_observer_not_bound$"):
            self.release()

    def test_supervisor_must_independently_agree_on_leaf(self):
        self.supervised["capsuleInode"] += 1
        with self.assertRaisesRegex(BARRIER.Refused, "^supervisor_binding$"):
            self.release()

    def test_rejects_non_isolated_network(self):
        self.supervised["supervisorNetns"] = self.waiting["workloadNetns"]
        with self.assertRaisesRegex(BARRIER.Refused, "^release_network$"):
            self.release()

    def test_stale_time_challenge_and_serialization_refuse(self):
        baseline = self.release()
        for change, reason in (
            ({"issuedNs": self.waiting["waitingNs"] - 1}, "release_time"),
            ({"challenge": "ee" * 16}, "release_binding"),
            ({"rootInode": self.waiting["rootInode"] + 1}, "release_binding"),
        ):
            data = BARRIER.wire({**json.loads(baseline), **change})
            with self.subTest(change=change), self.assertRaisesRegex(BARRIER.Refused, f"^{reason}$"):
                BARRIER.validate_release(data, self.waiting, BARRIER.kernel_monotonic_ns())
        with self.assertRaisesRegex(BARRIER.Refused, "^release_canonical$"):
            BARRIER.validate_release(baseline + b"\n", self.waiting, BARRIER.kernel_monotonic_ns())

    def test_deadline_does_not_treat_eof_as_release(self):
        deadline = self.waiting["waitingNs"] + 1_000_000_000
        clock = iter((self.waiting["waitingNs"], self.waiting["waitingNs"] + 1, deadline))
        with self.assertRaisesRegex(BARRIER.Refused, "^release_timeout$"):
            BARRIER.wait_for_release(lambda: None, self.waiting, 1,
                                     clock=lambda: next(clock), pause=lambda _: None)

    def test_release_permissions_links_and_identity(self):
        with tempfile.TemporaryDirectory() as scratch:
            path = Path(scratch) / BARRIER.GATE_FILE
            fd = os.open(scratch, BARRIER.DIR)
            try:
                self.assertIsNone(BARRIER.read_release(fd, uid=os.getuid()))
                path.write_bytes(self.release())
                path.chmod(0o644)
                original = BARRIER.read_release(fd, uid=os.getuid())
                path.rename(path.with_name("old"))
                path.write_bytes(original[0])
                path.chmod(0o644)
                self.assertNotEqual(BARRIER.read_release(fd, uid=os.getuid()), original)
                path.chmod(0o664)
                with self.assertRaisesRegex(BARRIER.Refused, "^release_file_identity$"):
                    BARRIER.read_release(fd, uid=os.getuid())
                path.unlink()
                path.symlink_to("old")
                with self.assertRaises(OSError):
                    BARRIER.read_release(fd, uid=os.getuid())
            finally:
                os.close(fd)

    def test_real_child_holds_then_accepts_atomic_record(self):
        code = """
import importlib.util, json, os, sys
spec=importlib.util.spec_from_file_location('barrier', sys.argv[1])
barrier=importlib.util.module_from_spec(spec)
spec.loader.exec_module(barrier)
waiting=json.loads(sys.argv[3])
fd=os.open(sys.argv[2],barrier.DIR)
try:
    print('waiting',flush=True)
    record, obj=barrier.wait_for_release(lambda:barrier.read_release(fd,uid=os.getuid()),waiting,5)
    print('released', obj['challenge'])
finally:
    os.close(fd)
"""
        with tempfile.TemporaryDirectory() as scratch, subprocess.Popen(
            [sys.executable, "-IB", "-c", code, str(SCRIPT), scratch, json.dumps(self.waiting)],
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL,
        ) as child:
            try:
                ready, _, _ = select.select([child.stdout], [], [], 3)
                self.assertTrue(ready)
                self.assertEqual(child.stdout.readline(), "waiting\n")
                self.assertEqual(select.select([child.stdout], [], [], 0.15)[0], [])
                pending = Path(scratch) / "pending"
                pending.write_bytes(self.release())
                pending.chmod(0o644)
                pending.rename(Path(scratch) / BARRIER.GATE_FILE)
                out, err = child.communicate(timeout=4)
                self.assertEqual(child.returncode, 0, err)
                self.assertEqual(err, "")
                self.assertEqual(out, "released " + self.waiting["challenge"] + "\n")
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=3)

    def test_private_home_uses_fresh_directory(self):
        with tempfile.TemporaryDirectory() as scratch:
            fd = os.open(scratch, BARRIER.DIR)
            try:
                home = BARRIER.ensure_home(fd)
                self.assertEqual(home.st_mode & 0o777, 0o700)
                with self.assertRaises(FileExistsError):
                    BARRIER.ensure_home(fd)
            finally:
                os.close(fd)


if __name__ == "__main__":
    unittest.main()
