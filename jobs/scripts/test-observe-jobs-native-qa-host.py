"""Portable controls for the read-only Linux Jobs QA preflight."""

import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import select
import stat
import subprocess
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


SUT = Path(__file__).resolve().with_name("observe-jobs-native-qa-host.py")
OBSERVE = runpy.run_path(str(SUT))
NAMESPACE_NAMES = ("mnt", "pid", "user", "net", "cgroup")
RELEASE_WIRE_BYTES = 4 + 271_498 + 44_659_086 + 4 + 2_797 + 445_766_552
OLD_RELEASE_WIRE_BYTES = 490_581_101
CGROUP_ROW = b"31 19 0:28 / /sys/fs/cgroup rw,nosuid,nodev,noexec - cgroup2 cgroup rw\n"
COVERING_ROW = b"45 31 0:33 / /sys/fs/cgroup rw - tmpfs tmpfs rw\n"
FRESH_BOOT = "0b4aa9b6-1827-467c-80e6-8cb5c2c49473"
SUPERVISOR = 4310
PIDFD = 204
ROOT_STATUS = b"Name:\tpython3\nUid:\t0\t0\t0\t0\nGid:\t0\t0\t0\t0\n"


def facts(**overrides):
    return {
        "architecture": "x86_64",
        "distribution": "ubuntu:24.04",
        "rootIds": (0, 0, 0, 0),
        "pidOneExecutable": "/usr/lib/systemd/systemd",
        "bootId": FRESH_BOOT,
        "sameAsVisiblePidOneNamespaces": dict.fromkeys(NAMESPACE_NAMES, True),
        "effectiveCgroup2": True,
        "varLibFreeBytes": RELEASE_WIRE_BYTES,
        **overrides,
    }


class ReadOnlyNativeHostTests(unittest.TestCase):
    def test_observer_never_creates_authority_even_if_every_check_succeeds(self):
        evidence = OBSERVE["assess"](facts())
        self.assertTrue(evidence["preconditionsObserved"])
        self.assertTrue(evidence["checks"]["roomForWireBytesOnly"])
        self.assertEqual(evidence["packet"], {
            "wireBytes": RELEASE_WIRE_BYTES,
            "sourceBundleSha256": "2e9c7ddae3290852fe061c90d3dddc7018a1097367abe697dc3af8f2d16d926b",
            "jobsManifestWireSha256": "ece652d59b69d8909fb019f9a599b6865453d1de6abeea4e618598c351f9571a",
        })
        self.assertIs(evidence["executionAdmitted"], False)
        self.assertIs(evidence["productionQualified"], False)
        self.assertNotIn(FRESH_BOOT, json.dumps(evidence))

    def test_wire_bound_counts_entire_current_packet_and_both_length_headers(self):
        self.assertEqual(OBSERVE["WIRE_BYTES"], RELEASE_WIRE_BYTES)
        for available in (445_116_888 + 44_659_086, OLD_RELEASE_WIRE_BYTES,
                          490_059_395, RELEASE_WIRE_BYTES - 1):
            with self.subTest(available=available):
                evidence = OBSERVE["assess"](facts(varLibFreeBytes=available))
                self.assertFalse(evidence["checks"]["roomForWireBytesOnly"])
                self.assertFalse(evidence["preconditionsObserved"])
                self.assertFalse(evidence["executionAdmitted"])

    def test_wrong_platform_identity_and_effective_mount_do_not_pass(self):
        bad = (
            {"architecture": "aarch64"}, {"distribution": "ubuntu:22.04"},
            {"rootIds": (0, 1000, 0, 0)}, {"pidOneExecutable": "/bin/bash"},
            {"sameAsVisiblePidOneNamespaces": {**dict.fromkeys(NAMESPACE_NAMES, True), "net": False}},
            {"effectiveCgroup2": False},
        )
        for overrides in bad:
            with self.subTest(overrides=overrides):
                evidence = OBSERVE["assess"](facts(**overrides))
                self.assertFalse(evidence["preconditionsObserved"])
                self.assertFalse(evidence["productionQualified"])

    def test_held_mount_id_parser_refuses_ambiguous_and_malformed_rows(self):
        parse = OBSERVE["mount_id"]
        valid = b"pos:\t0\nflags:\t02300000\nmnt_id:\t31\nino:\t1\n"
        self.assertEqual(parse(valid), 31)
        for raw in (b"ino:\t31\n", b"mnt_id:\t0\n", b"mnt_id:\t031\n", b"mnt_id:\t-31\n",
                    b"mnt_id:\t31 suffix\n", valid + b"mnt_id:\t31\n", valid + b"mnt_id:\tx\n",
                    b"mnt_id:\t2147483648\n", b"mnt_id:\t" + b"9" * 160 + b"\n"):
            with self.subTest(raw=raw):
                with self.assertRaises((AssertionError, ValueError)):
                    parse(raw)

    def test_a_visible_cgroup2_row_cannot_override_the_held_mount_id(self):
        check = OBSERVE["effective_cgroup2"]
        self.assertTrue(check(CGROUP_ROW, 31))
        self.assertFalse(check(CGROUP_ROW + COVERING_ROW, 45))
        self.assertFalse(check(CGROUP_ROW, 46))
        self.assertFalse(check(CGROUP_ROW + CGROUP_ROW, 31))
        self.assertFalse(check(CGROUP_ROW.replace(b"/ /sys", b"/delegated /sys"), 31))
        self.assertFalse(check(CGROUP_ROW.replace(b"/sys/fs/cgroup", b"/other/cgroup"), 31))

    def test_collect_uses_held_space_and_refuses_mount_identity_drift_with_same_inode(self):
        for fault in (None, "postflight-bind", "var-lib-open"):
            with self.subTest(fault=fault):
                self.collect_with_mocked_kernel(fault)

    def collect_with_mocked_kernel(self, fault):
        cgroup, storage = "/sys/fs/cgroup", "/var/lib"
        stats = {
            cgroup: SimpleNamespace(st_mode=stat.S_IFDIR | 0o755, st_dev=8, st_ino=19),
            storage: SimpleNamespace(st_mode=stat.S_IFDIR | 0o755, st_dev=9, st_ino=22),
        }
        opened, closed, capacity_fd = {}, set(), []
        counts = dict.fromkeys(stats, 0)

        def open_dir(path, flags):
            self.assertTrue(flags & os.O_DIRECTORY and flags & os.O_NOFOLLOW and flags & os.O_CLOEXEC)
            self.assertEqual(flags & os.O_ACCMODE, os.O_RDONLY)
            if fault == "var-lib-open" and path == storage:
                raise OSError("absent storage")
            counts[path] += 1
            fd = 201 + len(opened)
            opened[fd] = (path, counts[path])
            return fd

        def close(fd):
            self.assertIn(fd, opened)
            self.assertNotIn(fd, closed)
            closed.add(fd)

        def bounded(path, _limit, *, proc=False):
            name = str(path)
            if name.startswith("/proc/self/fdinfo/"):
                fd = int(path.name)
                self.assertNotIn(fd, closed)
                dir_path, attempt = opened[fd]
                ident = (45 if fault == "postflight-bind" and attempt >= 3 else 31) if dir_path == cgroup else 36
                return f"mnt_id:\t{ident}\n".encode("ascii")
            if name == "/proc/sys/kernel/random/boot_id":
                return (FRESH_BOOT + "\n").encode("ascii")
            if name == "/proc/self/mountinfo":
                return CGROUP_ROW + COVERING_ROW  # raw snapshot stays unchanged under the induced FD change
            self.assertEqual(name, "/usr/lib/os-release")
            self.assertFalse(proc)
            return b'ID=ubuntu\nVERSION_ID="24.04"\n'

        def space(fd):
            self.assertEqual(opened[fd], (storage, 1))
            self.assertNotIn(fd, closed)
            capacity_fd.append(fd)
            return SimpleNamespace(f_bavail=RELEASE_WIRE_BYTES, f_frsize=1)

        observe = OBSERVE["collect"]
        namespaces = dict.fromkeys(NAMESPACE_NAMES, ((1, 2), (1, 2)))
        with patch.dict(observe.__globals__, {"bounded": bounded, "namespace_identities": lambda: namespaces}), \
                patch.multiple(os, open=open_dir, close=close, fstat=lambda fd: stats[opened[fd][0]],
                               stat=lambda path, **_: stats[path], fstatvfs=space,
                               readlink=lambda _: "/usr/lib/systemd/systemd",
                               uname=lambda: SimpleNamespace(machine="x86_64"),
                               getuid=lambda: 0, geteuid=lambda: 0, getgid=lambda: 0, getegid=lambda: 0):
            if fault is None:
                result = observe()
            else:
                with self.assertRaises((AssertionError, OSError, ValueError)):
                    observe()
        self.assertEqual(closed, set(opened))
        self.assertEqual(len(capacity_fd), 0 if fault == "var-lib-open" else 1)
        if fault is None:
            self.assertTrue(result["preconditionsObserved"])
            self.assertFalse(result["executionAdmitted"])

    def test_cli_is_inert_by_default_and_cannot_fake_linux_from_argv(self):
        for args in ([], ["--observe", "--assume-linux"], ["--admit"]):
            with self.subTest(args=args):
                result = subprocess.run([sys.executable, "-I", "-S", "-B", str(SUT), *args],
                                        capture_output=True, timeout=5, check=False)
                self.assertEqual((result.returncode, result.stdout, result.stderr), (64, b"", b""))
        if sys.platform != "linux":
            for mode in ("--observe", "--observe-parent"):
                result = subprocess.run([sys.executable, "-I", "-S", "-B", str(SUT), mode],
                                        capture_output=True, timeout=5, check=False)
                self.assertEqual((result.returncode, result.stdout), (69, b""))
                self.assertEqual(json.loads(result.stderr)["reason"], "not_linux")

    def test_cli_hides_errors_from_collector(self):
        stdout, stderr = io.StringIO(), io.StringIO()
        main = OBSERVE["main"]
        with patch.object(sys, "argv", [str(SUT), "--observe"]), patch.object(sys, "platform", "linux"), \
                patch.multiple("os", getuid=lambda: 0, geteuid=lambda: 0, getgid=lambda: 0, getegid=lambda: 0), \
                patch.dict(main.__globals__, {"collect": self.fail_collection}), \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            self.assertEqual(main(), 69)
        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(json.loads(stderr.getvalue())["reason"], "host_facts_unavailable")

    @staticmethod
    def fail_collection():
        raise OSError("private diagnostic must not leave collector")

    def test_linux_cli_routes_only_the_selected_readonly_mode(self):
        main = OBSERVE["main"]
        base = OBSERVE["assess"](facts())
        parent = {**base, "schema": "bluey-jobs-native-qa-parent-facts-v1", "rootDirectParentHeld": True}
        for mode, want in (("--observe", base), ("--observe-parent", parent)):
            plain, extra = Mock(return_value=base), Mock(return_value=parent)
            stdout, stderr = io.StringIO(), io.StringIO()
            with self.subTest(mode=mode), patch.object(sys, "argv", [str(SUT), mode]), \
                    patch.object(sys, "platform", "linux"), \
                    patch.multiple("os", getuid=lambda: 0, geteuid=lambda: 0, getgid=lambda: 0, getegid=lambda: 0), \
                    patch.dict(main.__globals__, {"collect": plain, "collect_with_parent": extra}), \
                    contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                self.assertEqual(main(), 0)
            self.assertEqual(json.loads(stdout.getvalue()), want)
            self.assertEqual(stderr.getvalue(), "")
            self.assertEqual((plain.call_count, extra.call_count), (1, 0) if mode == "--observe" else (0, 1))


class RootSupervisorTests(unittest.TestCase):
    def setUp(self):
        self.hold = OBSERVE["hold_root_parent"]
        self.refused = OBSERVE["SupervisorRefused"]
        self.proc = Path("/proc") / str(SUPERVISOR)
        self.identity = SimpleNamespace(st_dev=4, st_ino=72, st_uid=0, st_gid=0,
                                        st_mode=stat.S_IFDIR | 0o555)
        self.boot_raw = (FRESH_BOOT + "\n").encode("ascii")
        self.status = ROOT_STATUS
        self.fdinfo = b"Pid:\t4310\nNSpid:\t4310\n"
        self.ppid = Mock(return_value=SUPERVISOR)
        self.credentials = Mock(return_value=(0, 0, 0))
        self.pidfd = Mock(return_value=PIDFD)
        self.close = Mock()
        self.poller = Mock()
        self.poller.poll.return_value = []
        stack = contextlib.ExitStack()
        self.addCleanup(stack.close)
        stack.enter_context(patch.object(sys, "platform", "linux"))
        stack.enter_context(patch.object(os, "pidfd_open", self.pidfd, create=True))
        for name, function in (("getppid", self.ppid), ("close", self.close),
                               ("getresuid", self.credentials), ("getresgid", self.credentials)):
            stack.enter_context(patch.object(os, name, function, create=True))
        stack.enter_context(patch.object(select, "poll", return_value=self.poller))
        stack.enter_context(patch.object(Path, "stat", self.proc_stat))
        stack.enter_context(patch.dict(self.hold.__wrapped__.__globals__, {"bounded": self.bounded}))

    def proc_stat(self, *args, **kwargs):
        return self.identity

    def bounded(self, path, limit, *, proc):
        self.assertTrue(proc)
        if path == OBSERVE["BOOT"]:
            return self.boot_raw
        if path == self.proc / "status":
            return self.status
        self.assertEqual(path, Path("/proc/self/fdinfo/204"))
        return self.fdinfo

    def test_direct_parent_is_opened_once_and_closing_revokes_callable(self):
        with self.hold(FRESH_BOOT, SUPERVISOR) as held:
            self.assertTrue(held())
            self.pidfd.assert_called_once_with(SUPERVISOR, 0)
            self.poller.register.assert_called_once_with(PIDFD, select.POLLIN | select.POLLHUP | select.POLLERR)
            self.close.assert_not_called()
        self.close.assert_called_once_with(PIDFD)
        self.poller.poll.reset_mock()
        self.assertFalse(held())
        self.poller.poll.assert_not_called()

    def test_bad_parent_boot_and_credentials_refuse_before_open(self):
        for boot, pid in ((FRESH_BOOT, 1), (FRESH_BOOT, True), (FRESH_BOOT, SUPERVISOR + 1),
                          (FRESH_BOOT, 2**31), ("0" * 36, SUPERVISOR),
                          (FRESH_BOOT.upper(), SUPERVISOR), ("1" + FRESH_BOOT[1:], SUPERVISOR)):
            with self.subTest(boot=boot, pid=pid), self.assertRaises(self.refused):
                with self.hold(boot, pid):
                    self.fail("invalid inputs acquired a lease")
        self.credentials.return_value = (0, 0, 10)
        with self.assertRaises(self.refused), self.hold(FRESH_BOOT, SUPERVISOR):
            self.fail("saved non-root credential acquired a lease")
        self.pidfd.assert_not_called()
        self.close.assert_not_called()

    def test_noncanonical_boot_or_parent_credentials_refuse(self):
        for raw in (self.boot_raw[:-1], self.boot_raw + b"\n", b" " + self.boot_raw):
            with self.subTest(raw=raw), self.assertRaises(self.refused):
                self.boot_raw = raw
                with self.hold(FRESH_BOOT, SUPERVISOR):
                    self.fail("invalid boot acquired a lease")
        self.boot_raw = (FRESH_BOOT + "\n").encode("ascii")
        for status in (ROOT_STATUS + ROOT_STATUS, ROOT_STATUS.replace(b"Gid:\t0", b"Gid:\t1000"),
                       ROOT_STATUS.replace(b"Uid:\t0", b"Uid: \t0"), b"Name:\ta\n"):
            with self.subTest(status=status), self.assertRaises(self.refused):
                self.status = status
                with self.hold(FRESH_BOOT, SUPERVISOR):
                    self.fail("invalid credentials acquired a lease")
        self.pidfd.assert_not_called()

    def test_wrong_exited_or_ambiguous_pidfd_is_closed(self):
        for fdinfo in (b"Pid:\t-1\n", b"Pid:\t1430\n", b"Pid:\t04310\n", self.fdinfo + self.fdinfo):
            with self.subTest(fdinfo=fdinfo), self.assertRaises(self.refused):
                self.fdinfo = fdinfo
                with self.hold(FRESH_BOOT, SUPERVISOR):
                    self.fail("invalid pidfd acquired a lease")
        self.assertEqual(self.close.call_count, 4)

    def test_poll_registration_failure_closes_pidfd(self):
        self.poller.register.side_effect = OSError("registration refused")
        with self.assertRaises(OSError), self.hold(FRESH_BOOT, SUPERVISOR):
            self.fail("untracked pidfd acquired a lease")
        self.close.assert_called_once_with(PIDFD)

    def test_live_check_fails_permanently_after_parent_boot_or_kernel_drift(self):
        for fault in ("exit", "reparent", "replacement", "boot", "creds", "proc-error"):
            with self.subTest(fault=fault), self.hold(FRESH_BOOT, SUPERVISOR) as held:
                self.assertTrue(held())
                original = self.identity
                if fault == "exit":
                    self.poller.poll.return_value = [(PIDFD, select.POLLIN)]
                elif fault == "reparent":
                    self.ppid.return_value = 1
                elif fault == "replacement":
                    self.identity = SimpleNamespace(**{**vars(original), "st_ino": 73})
                elif fault == "boot":
                    self.boot_raw = b"1" + self.boot_raw[1:]
                elif fault == "creds":
                    self.status = ROOT_STATUS.replace(b"Gid:\t0", b"Gid:\t1000")
                else:
                    self.poller.poll.side_effect = OSError("poll failed")
                self.assertFalse(held())
                self.poller.poll.side_effect, self.poller.poll.return_value = None, []
                self.ppid.return_value = SUPERVISOR
                self.identity = original
                self.boot_raw = (FRESH_BOOT + "\n").encode("ascii")
                self.status = ROOT_STATUS
                self.assertFalse(held())
        self.assertEqual(self.close.call_count, 6)

    def test_parent_exit_during_initial_check_and_body_failure_close_descriptor(self):
        self.poller.poll.side_effect = [[], [(PIDFD, select.POLLIN)]]
        with self.assertRaises(self.refused), self.hold(FRESH_BOOT, SUPERVISOR):
            self.fail("exit at initial postflight acquired a lease")
        self.close.assert_called_once_with(PIDFD)
        self.poller.poll.side_effect = None
        with self.assertRaisesRegex(RuntimeError, "body failed"), self.hold(FRESH_BOOT, SUPERVISOR) as held:
            self.assertTrue(held())
            raise RuntimeError("body failed")
        self.assertEqual(self.close.call_count, 2)
        self.assertFalse(held())

    def test_supervised_observation_is_non_admitting_and_rechecks_boot(self):
        collect = OBSERVE["collect_with_parent"]
        evidence = OBSERVE["assess"](facts())
        self.assertEqual(evidence["bootDigest"], hashlib.sha256(FRESH_BOOT.encode("ascii")).hexdigest())
        with patch.dict(collect.__globals__, {"collect": lambda: evidence}):
            result = collect()
        self.assertTrue(result["rootDirectParentHeld"])
        self.assertTrue(result["preconditionsObserved"])
        self.assertIs(result["executionAdmitted"], False)
        self.assertIs(result["productionQualified"], False)
        self.assertNotIn(FRESH_BOOT, json.dumps(result))
        with patch.dict(collect.__globals__, {"collect": lambda: {**evidence, "bootDigest": "bad"}}):
            with self.assertRaises(self.refused):
                collect()
        self.assertEqual(self.close.call_count, 2)


if __name__ == "__main__":
    unittest.main()
