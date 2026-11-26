"""Pure and local-FD controls; does not open live kernel or systemd state."""
from pathlib import Path
import os
import runpy
import stat
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest

SCRIPT = Path(__file__).with_name("interview-prep-linux-closure.py")
M = runpy.run_path(str(SCRIPT))
Refused = M["Refused"]
UNIT = "bluey-jobs-receipt-" + "1" * 32 + ".service"
CG = "/system.slice/" + UNIT
INV = "2" * 32
LIVE = {"Id": UNIT, "InvocationID": INV, "ControlGroup": CG,
        "LoadState": "loaded", "ActiveState": "active", "SubState": "running", "MainPID": "4100"}
AFTER = LIVE | {"ControlGroup": "", "SubState": "exited", "MainPID": "0"}


class ClosureTests(unittest.TestCase):
    def test_only_complete_kernel_events_are_observations(self):
        parse = M["parse_events"]
        self.assertEqual(parse(b"populated 1\nfrozen 0\n"), 1)
        self.assertEqual(parse(b"populated 0\nfrozen 0\n"), 0)
        for raw, code in [
            (b"", "events_size"), (b"populated 0\nfrozen 0", "events_incomplete"),
            (b"populated 0\n", "events_missing_fields"),
            (b"populated 0\npopulated 1\nfrozen 0\n", "events_malformed"),
            (b"populated 0\nfrozen 0\nother 0\n", "events_malformed"),
            (b"populated 2\nfrozen 0\n", "events_malformed"),
        ]:
            with self.subTest(raw=raw), self.assertRaisesRegex(Refused, code):
                parse(raw)

    def test_mount_id_binds_full_cgroup2_mount(self):
        row = "27 22 0:26 / /sys/fs/cgroup rw - cgroup2 cgroup rw\n"
        self.assertEqual(M["mount_id"](row), 27)
        self.assertEqual(M["fd_mount_id"]("pos:\t0\nmnt_id:\t27\n"), 27)
        for fixture in [row + row, row.replace("cgroup2", "tmpfs"), row.replace("26 / ", "26 /scoped ")]:
            with self.assertRaises(Refused):
                M["mount_id"](fixture)
        with self.assertRaisesRegex(Refused, "fd_mount_id"):
            M["fd_mount_id"]("mnt_id: 27\nmnt_id: 28\n")

    def test_malformed_manager_fields_refuse(self):
        raw = "".join(f"{key}={value}\n" for key, value in LIVE.items())
        self.assertEqual(M["unit_properties"](raw), LIVE)
        with self.assertRaisesRegex(Refused, "manager_field"):
            M["unit_properties"](raw + f"Id={UNIT}\n")
        with self.assertRaisesRegex(Refused, "manager_file_shape"):
            M["unit_properties"](raw.rstrip("\n"))

    def test_manager_pipe_collects_bounded_output_and_reaps_its_child(self):
        def query(program, timeout=2):
            return M["bounded_manager_query"]([sys.executable, "-IB", "-c", program], timeout)
        self.assertEqual(query("print('ok')"), b"ok\n")
        for program, code in [
            ("print('X' * 4096)", "manager_output_bound"),
            ("import sys; sys.stderr.write('X')", "manager_stderr"),
            ("raise SystemExit(7)", "manager_read"),
        ]:
            with self.assertRaisesRegex(Refused, code):
                query(program)
        with self.assertRaisesRegex(Refused, "manager_deadline"):
            query("import time; time.sleep(10)", timeout=0.1)

    def test_terminal_empty_path_is_not_a_populated_observation(self):
        self.assertFalse(M["check_manager"](LIVE, UNIT, INV, True))
        self.assertTrue(M["check_manager"](AFTER, UNIT, INV, False))
        for props in [AFTER | {"InvocationID": "3" * 32}, AFTER | {"ControlGroup": "/different"}]:
            with self.assertRaises(Refused):
                M["check_manager"](props, UNIT, INV, False)
        with self.assertRaisesRegex(Refused, "manager_not_live"):
            M["check_manager"](AFTER, UNIT, INV, True)
        with self.assertRaises(FileNotFoundError):
            M["require_zero"](lambda: (_ for _ in ()).throw(FileNotFoundError()), lambda: True,
                              lambda: None, 1, now=lambda: 0, sleep=lambda _: None)

    def test_zero_is_bracketed_by_matching_terminal_and_boot(self):
        trace = []
        M["require_zero"](lambda: trace.append("zero") or 0, lambda: trace.append("same") or True,
                          lambda: trace.append("boot"), 1, now=lambda: 0)
        self.assertEqual(trace, ["boot", "same", "zero", "boot", "same"])
        checks = iter([True, False])
        clock = iter([0, 0.9, 1])
        with self.assertRaisesRegex(Refused, "closure_deadline"):
            M["require_zero"](lambda: 0, lambda: next(checks), lambda: None, 1,
                              now=lambda: next(clock), sleep=lambda _: None)

    def test_nonzero_and_late_zero_do_not_pass(self):
        for values, clock in [([1], [0, 0.5, 1]), ([0], [0, 1])]:
            v, t = iter(values), iter(clock)
            with self.assertRaisesRegex(Refused, "closure_deadline"):
                M["require_zero"](lambda: next(v), lambda: True, lambda: None, 1,
                                  now=lambda: next(t), sleep=lambda _: None)

    def test_exact_fd_is_revalidated_before_and_after_full_read(self):
        stat_mock = lambda ino: SimpleNamespace(st_mode=stat.S_IFREG | 0o444, st_dev=11, st_ino=ino)
        for first, second, raw, tail, expected in [
            (41, 41, b"populated 0\nfrozen 0\n", b"", None),
            (41, 42, b"populated 0\nfrozen 0\n", b"", "events_fd_changed"),
            (41, 41, b"populated 0\nfrozen 0\n", b"X", "events_not_eof"),
        ]:
            stats, bytes_read = iter([stat_mock(first), stat_mock(second)]), iter([raw, tail])
            def seek(fd, offset, whence):
                self.assertEqual((fd, offset, whence), (7, 0, os.SEEK_SET))
            ops = SimpleNamespace(fstat=lambda _fd: next(stats), lseek=seek,
                                  read=lambda _fd, _n: next(bytes_read))
            if expected is None:
                self.assertEqual(M["sample_events"](7, (11, 41), ops), 0)
            else:
                with self.assertRaisesRegex(Refused, expected):
                    M["sample_events"](7, (11, 41), ops)

    def test_replaced_path_cannot_supply_zero_for_open_descriptor(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            live = root / "events.fixture"
            live.write_bytes(b"populated 1\nfrozen 0\n")
            with live.open("rb") as original:
                info = os.fstat(original.fileno())
                identity = (info.st_dev, info.st_ino)
                live.rename(root / "previous.fixture")
                live.write_bytes(b"populated 0\nfrozen 0\n")
                self.assertEqual(M["sample_events"](original.fileno(), identity), 1)

    @unittest.skipIf(sys.platform == "linux", "Darwin refusal is a platform control only")
    def test_direct_entry_refuses_this_host_without_reading_kernel_state(self):
        result = subprocess.run([sys.executable, "-IB", SCRIPT, "1" * 32,
                                 "00000000-0000-0000-0000-000000000001", "2" * 32, "10"],
                                capture_output=True, timeout=3, check=False)
        self.assertEqual((result.returncode, result.stdout), (1, b""))
        self.assertEqual(result.stderr, b"Jobs browser closure unverified: linux_required\n")


if __name__ == "__main__":
    unittest.main()
