"""Post-stop protocol and FD traversal of an owned synthetic proc tree."""
import os
from pathlib import Path
import runpy
import sys
import tempfile
import unittest

M = runpy.run_path(str(Path(__file__).with_name("receipt-browser-post-stop.py")))
INV = "2" * 32
UNIT = "bluey-jobs-receipt-" + "1" * 32 + ".service"
STOPPED = {"Id": UNIT, "InvocationID": INV, "ControlGroup": "", "LoadState": "loaded",
           "ActiveState": "inactive", "SubState": "dead", "MainPID": "0"}


class CensusTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.proc = self.root / "proc"
        self.proc.mkdir()
        self.net = self.root / "intended-net"
        self.net.touch()
        self.other = self.root / "other-net"
        self.other.touch()
        self.fd = os.open(self.proc, os.O_RDONLY | os.O_DIRECTORY)
        self.addCleanup(os.close, self.fd)

    def task(self, pid, tid, uid=900, *, net=None, state="S", kernel=0):
        path = self.proc / str(pid) / "task" / str(tid)
        (path / "ns").mkdir(parents=True)
        if net is not None:
            (path / "ns" / "net").symlink_to(net)
        data = f"Pid:\t{tid}\nUid:\t" + "\t".join([str(uid)] * 4)
        (path / "status").write_text(data + f"\nKthread:\t{kernel}\nState:\t{state} (fixture)\n")

    def scan(self, cap=32768):
        return M["scan_proc"](self.fd, 1001, M["identity"](self.net.stat()), lambda: None, cap)

    def test_nonleader_network_member_and_actor_are_detected_once_per_process(self):
        self.task(10, 10, net=self.other)
        self.task(10, 11, net=self.net)
        self.task(10, 12, net=self.net)
        self.task(20, 20, 1001, net=self.other)
        self.task(20, 21, 1001, net=self.other)
        self.assertEqual(self.scan(), {"actorProcesses": 1, "namespaceProcesses": 1})

    def test_live_member_with_unreadable_namespace_refuses(self):
        self.task(10, 10)
        with self.assertRaisesRegex(RuntimeError, "netns_unreadable"):
            self.scan()

    def test_kernel_and_nonactor_zombie_not_members_but_actor_zombie_is_counted(self):
        self.task(10, 10, kernel=1)
        self.task(11, 11, state="Z")
        self.assertEqual(self.scan(), {"actorProcesses": 0, "namespaceProcesses": 0})
        self.task(12, 12, 1001, state="Z")
        self.assertEqual(self.scan(), {"actorProcesses": 1, "namespaceProcesses": 0})

    def test_missing_task_directory_with_visible_leader_refuses(self):
        self.task(10, 10, net=self.other)
        leader = self.proc / "11"
        leader.mkdir()
        (leader / "status").write_text("Pid:\t11\nUid:\t900 900 900 900\nKthread:\t0\nState:\tZ (fixture)\n")
        with self.assertRaisesRegex(RuntimeError, "task_group_unobservable"):
            self.scan()

    def test_task_bound_and_duplicate_security_fields_fail(self):
        self.task(10, 10, net=self.other)
        self.task(10, 11, net=self.other)
        with self.assertRaisesRegex(RuntimeError, "entry_limit|task_limit"):
            self.scan(cap=1)
        path = self.proc / "10" / "task" / "10" / "status"
        path.write_text(path.read_text() + "Uid: 0 0 0 0\n")
        with self.assertRaisesRegex(RuntimeError, "status_duplicate"):
            self.scan()

    def test_full_proc_mount_is_required(self):
        good = "31 23 0:89 / /proc rw,nosuid,nodev,noexec - proc proc rw\n"
        self.assertEqual(M["proc_view"](good), 31)
        for raw in [good + good, good.replace(" / /proc", " /1 /proc"),
                    good.rstrip() + ",hidepid=2\n", good.rstrip() + ",subset=pid\n"]:
            with self.subTest(raw=raw), self.assertRaisesRegex(RuntimeError, "proc_mount|proc_visibility"):
                M["proc_view"](raw)

    def test_both_passes_are_bracketed_by_the_same_stopped_unit(self):
        self.task(10, 10, net=self.other)
        trace = []
        def manager():
            trace.append("unit")
            return M["stopped_state"](STOPPED, UNIT, INV)
        def census():
            trace.append("scan")
            return self.scan()
        self.assertEqual(M["post_stop"](manager, census, lambda: trace.append("lease")),
                         {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0})
        self.assertEqual(trace, ["lease", "unit", "scan", "lease", "unit",
                                 "scan", "lease", "unit", "lease"])
        replaced = iter([STOPPED, dict(STOPPED, LoadState="not-found")])
        with self.assertRaisesRegex(RuntimeError, "unit_changed"):
            M["post_stop"](replaced.__next__, census, lambda: None)
        self.task(11, 11, net=self.net)
        with self.assertRaisesRegex(RuntimeError, "remaining_processes"):
            M["post_stop"](manager, census, lambda: None)

    def test_stopped_does_not_mean_failed_or_replaced(self):
        for change in [{"ActiveState": "failed"}, {"MainPID": "3"}, {"InvocationID": "f" * 32},
                       {"LoadState": "not-found"}, {"InvocationID": ""}, {"InvocationID": "0" * 32},
                       {"ControlGroup": "/system.slice/other.service"}]:
            with self.subTest(change=change), self.assertRaisesRegex(RuntimeError, "not_stopped"):
                M["stopped_state"](dict(STOPPED, **change), UNIT, INV)

    def test_directory_iterator_preserves_callers_fd_and_rewinds_each_pass(self):
        self.task(10, 10, net=self.other)
        self.assertEqual(list(M["task_names"](self.fd, lambda: None, 10)), ["10"])
        os.fstat(self.fd)
        self.assertEqual(list(M["task_names"](self.fd, lambda: None, 10)), ["10"])
        os.fstat(self.fd)

    @unittest.skipIf(sys.platform == "linux", "non-Linux refusal check")
    def test_darwin_never_enters_privileged_scope(self):
        with self.assertRaisesRegex(RuntimeError, "linux_root"):
            with M["bind_post_stop"]({}, independently_admitted=self.fail, deadline_ns=0):
                self.fail("unexpected binding")


if __name__ == "__main__":
    unittest.main()
