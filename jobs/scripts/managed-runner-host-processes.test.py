"""Pure mocked tests: importing the producer performs no native probe or capture."""

import importlib.util
import io
import json
import pathlib
import stat
import sys
import unittest
from dataclasses import replace
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "managed_runner_host_processes",
    pathlib.Path(__file__).with_name("managed-runner-host-processes.py"),
)
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)
M = MODULE
PYTHON = "/synthetic/root/python3"
CHILD = "/opt/homebrew/Cellar/lima/2.2.1/bin/limactl"


class Provider:
    def __init__(self):
        self.pids = [20, 10]
        self.identities = {
            10: M.BsdIdentity(10, 1, 501, 501, 100, 123),
            20: M.BsdIdentity(20, 10, 501, 501, 101, 456),
        }
        self.paths = {10: PYTHON, 20: CHILD}

    def list_pids(self, uid):
        if uid != 501:
            raise AssertionError("wrong real UID selector")
        return self.pids

    def bsd_identity(self, pid):
        value = self.identities[pid]
        return value.pop(0) if isinstance(value, list) else value

    def executable_path(self, pid):
        value = self.paths[pid]
        return value.pop(0) if isinstance(value, list) else value


class Filesystem:
    def __init__(self):
        self.entries = {
            PYTHON: M.ExecutableObservation(PYTHON, 1, 1000, stat.S_IFREG | 0o755, 0),
            CHILD: M.ExecutableObservation(CHILD, 1, 2000, stat.S_IFREG | 0o755, 501),
        }

    def executable(self, path):
        value = self.entries[path]
        if isinstance(value, Exception):
            raise value
        return value.pop(0) if isinstance(value, list) else value


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        self.provider = Provider()
        self.filesystem = Filesystem()

    def capture(self):
        return M.capture_snapshot(self.provider, self.filesystem, 10, 501, PYTHON,
                                  lambda: 200000000)

    def denied(self, code=None):
        with self.assertRaises(M.ClosedError) as raised:
            self.capture()
        if code is not None:
            self.assertEqual(raised.exception.code, code)

    def test_exact_stable_identity_is_closed_sorted_and_has_no_path(self):
        encoded = self.capture()
        result = json.loads(encoded)
        self.assertEqual(set(result), {"version", "records"})
        self.assertEqual([row["pid"] for row in result["records"]], ["10", "20"])
        child = result["records"][1]
        self.assertEqual(set(child), {"pid", "ppid", "euid", "ruid", "birthSec",
                                     "birthUsec", "executableIdentitySha256", "helperClass"})
        self.assertEqual(child["birthUsec"], "456")
        self.assertEqual(child["helperClass"], "lima")
        self.assertEqual(child["executableIdentitySha256"],
                         M.executable_identity(self.filesystem.entries[CHILD]))
        self.assertNotIn(CHILD.encode(), encoded)
        self.assertNotIn(PYTHON.encode(), encoded)
        self.assertLessEqual(len(encoded), M.MAX_OUTPUT_BYTES)

    def test_birth_parent_and_pid_reuse_changes_deny_whole_snapshot(self):
        original = self.provider.identities[20]
        for changed in (replace(original, birth_sec=102), replace(original, birth_usec=457),
                        replace(original, ppid=11), replace(original, pid=21)):
            with self.subTest(changed=changed):
                self.provider.identities[20] = [original, changed]
                self.denied()

    def test_real_or_effective_uid_mismatch_denies(self):
        original = self.provider.identities[20]
        for changed in (replace(original, euid=0), replace(original, ruid=0)):
            self.provider.identities[20] = changed
            self.denied("host_process_uid")

    def test_invalid_parent_birth_or_integer_shape_denies(self):
        original = self.provider.identities[20]
        for changed in (replace(original, ppid=-1), replace(original, ppid=M.MAX_PID + 1),
                        replace(original, birth_sec=0), replace(original, birth_usec=1000000),
                        replace(original, birth_sec=201), replace(original, pid=True)):
            self.provider.identities[20] = changed
            self.denied()

    def test_path_canonical_device_inode_and_mode_changes_deny(self):
        original = self.filesystem.entries[CHILD]
        for changed in (replace(original, canonical_path="/synthetic/other"),
                        replace(original, device=2), replace(original, inode=2001),
                        replace(original, mode=stat.S_IFREG | 0o700)):
            self.filesystem.entries[CHILD] = [original, changed]
            self.denied("host_process_changed")
        self.filesystem.entries[CHILD] = original
        self.provider.paths[20] = [CHILD, "/synthetic/other"]
        self.filesystem.entries["/synthetic/other"] = original
        self.denied("host_process_changed")

    def test_permission_and_disappeared_process_are_not_silently_dropped(self):
        for error in (PermissionError("PRIVATE-path"), FileNotFoundError("PRIVATE-path")):
            self.filesystem.entries[CHILD] = error
            self.denied("host_process_uninspectable")

    def test_uninspectable_native_process_denies(self):
        with patch.object(self.provider, "bsd_identity", side_effect=M.ClosedError(
                "host_process_uninspectable")):
            self.denied("host_process_uninspectable")

    def test_executable_special_file_and_invalid_path_deny(self):
        original = self.filesystem.entries[CHILD]
        for changed in (replace(original, mode=stat.S_IFLNK | 0o777),
                        replace(original, mode=stat.S_IFDIR | 0o700),
                        replace(original, inode=0), replace(original, canonical_path="relative")):
            self.filesystem.entries[CHILD] = changed
            self.denied("host_process_file")

    def test_duplicate_overflow_invalid_or_missing_self_pids_deny(self):
        for values in ([10, 10], [0, 10], [True, 10], [10, M.MAX_PID + 1], [],
                       list(range(1, M.MAX_PIDS + 2)), [20]):
            self.provider.pids = values
            self.denied()

    def test_native_buffer_nonmultiple_truncated_negative_and_overcap_deny(self):
        for returned, capacity, values in ((3, 8, [10, 20]), (8, 8, [10, 20]),
                                           (-1, 8, [10]), (0, 8, [10]),
                                           (12, 16, [10, 20]),
                                           ((M.MAX_PIDS + 1) * 4, (M.MAX_PIDS + 2) * 4,
                                            list(range(1, M.MAX_PIDS + 2)))):
            with self.assertRaises(M.ClosedError):
                M.decode_pid_buffer(returned, capacity, values)
        self.assertEqual(M.decode_pid_buffer(8, 12, [10, 20, 0]), [10, 20])

    def test_native_bsd_layout_and_short_or_wrong_return_sizes(self):
        self.assertEqual(M.ctypes.sizeof(M.ProcBsdInfo), 136)
        buffer = M.ProcBsdInfo()
        buffer.pid, buffer.ppid, buffer.uid, buffer.ruid = 20, 10, 501, 501
        buffer.start_sec, buffer.start_usec = 101, 456
        self.assertEqual(M.decode_bsd_buffer(136, buffer), self.provider.identities[20])
        for returned in (-1, 0, 135, 137, True):
            with self.assertRaises(M.ClosedError):
                M.decode_bsd_buffer(returned, buffer)

    def test_native_path_lengths_nul_utf8_and_overflow_are_exact(self):
        path = CHILD.encode("utf-8")
        raw = path + bytes(4096 - len(path))
        self.assertEqual(M.decode_path_buffer(len(path), raw), CHILD)
        for returned, value in ((0, raw), (-1, raw), (4096, raw), (1, raw),
                                (len(path), b"x" * 4096), (2, b"/\xff" + bytes(4094)),
                                (len(path), raw[:-1])):
            with self.assertRaises(M.ClosedError):
                M.decode_path_buffer(returned, value)

    def test_self_probe_requires_matching_root_owned_nonwritable_python(self):
        original = self.filesystem.entries[PYTHON]
        for changed in (replace(original, uid=501), replace(original, mode=stat.S_IFREG | 0o775)):
            self.filesystem.entries[PYTHON] = changed
            self.denied("host_process_self_probe")

    def test_identity_domain_binds_canonical_path_device_and_inode(self):
        original = self.filesystem.entries[CHILD]
        digest = M.executable_identity(original)
        self.assertRegex(digest, r"^[0-9a-f]{64}$")
        for changed in (replace(original, canonical_path="/synthetic/other"),
                        replace(original, device=2), replace(original, inode=2001)):
            self.assertNotEqual(digest, M.executable_identity(changed))
        self.assertEqual(M.helper_class("/synthetic/limactl"), "python-or-other")

    def test_output_cap_denies_instead_of_truncating(self):
        with patch.object(M, "MAX_OUTPUT_BYTES", 10):
            self.denied("host_process_limit")

    def test_main_errors_expose_only_fixed_codes_without_native_details(self):
        for error, expected in ((M.ClosedError("host_process_uninspectable"),
                                 "host_process_uninspectable\n"),
                                (RuntimeError("PRIVATE-path secret native details"),
                                 "host_process_internal\n")):
            output, errors = io.StringIO(), io.StringIO()
            with patch.object(sys, "argv", ["producer", "--snapshot"]), \
                    patch.object(M, "capture_local", side_effect=error), \
                    patch.object(sys, "stdout", output), patch.object(sys, "stderr", errors):
                self.assertEqual(M.main(), 1)
            self.assertEqual(output.getvalue(), "")
            self.assertEqual(errors.getvalue(), expected)

    def test_unknown_cli_arguments_never_probe_native_state(self):
        with patch.object(sys, "argv", ["producer", "--snapshot", "--uid", "0"]), \
                patch.object(M, "capture_local") as capture, \
                patch.object(sys, "stderr", io.StringIO()) as errors:
            self.assertEqual(M.main(), 1)
            capture.assert_not_called()
            self.assertEqual(errors.getvalue(), "host_process_arguments\n")


class ReviewedPythonPairTests(unittest.TestCase):
    def setUp(self):
        self.provider = Provider()
        self.filesystem = Filesystem()
        self.launcher = M.REVIEWED_PYTHON_LAUNCHER
        self.native = M.REVIEWED_PYTHON_NATIVE
        self.launcher_file = M.ExecutableObservation(
            M.REVIEWED_PYTHON_LAUNCHER_CANONICAL, 1, 3000, stat.S_IFREG | 0o755, 0,
        )
        self.native_file = M.ExecutableObservation(self.native, 1, 4000, stat.S_IFREG | 0o755, 0)
        self.filesystem.entries[self.launcher] = self.launcher_file
        self.filesystem.entries[self.native] = self.native_file
        self.provider.paths[10] = self.native

    def capture(self):
        return M.capture_reviewed_python_snapshot(
            self.provider, self.filesystem, 10, 501, self.launcher, lambda: 200000000,
        )

    def test_exact_reviewed_launcher_native_pair_passes_actual_self_comparison(self):
        result = json.loads(self.capture())
        self.assertEqual(result["records"][0]["executableIdentitySha256"],
                         M.executable_identity(self.native_file))
        self.assertNotEqual(M.executable_identity(self.launcher_file),
                            result["records"][0]["executableIdentitySha256"])

    def test_unknown_launcher_canonical_native_or_swapped_pair_denies(self):
        for launcher in (self.native, "/usr/bin/python3", "/Library/Developer/CommandLineTools/usr/bin/python3"):
            with self.assertRaises(M.ClosedError):
                M.reviewed_python_pair(self.filesystem, launcher)
        for path, changed in (
            (self.launcher, replace(self.launcher_file, canonical_path=self.native)),
            (self.native, replace(self.native_file,
                                  canonical_path=M.REVIEWED_PYTHON_LAUNCHER_CANONICAL)),
        ):
            original = self.filesystem.entries[path]
            self.filesystem.entries[path] = changed
            with self.assertRaises(M.ClosedError):
                M.reviewed_python_pair(self.filesystem, self.launcher)
            self.filesystem.entries[path] = original

    def test_both_pair_images_must_be_root_owned_and_not_group_or_world_writable(self):
        for path in (self.launcher, self.native):
            original = self.filesystem.entries[path]
            for changed in (replace(original, uid=501),
                            replace(original, mode=stat.S_IFREG | 0o775),
                            replace(original, mode=stat.S_IFREG | 0o757)):
                self.filesystem.entries[path] = changed
                with self.assertRaises(M.ClosedError):
                    M.reviewed_python_pair(self.filesystem, self.launcher)
            self.filesystem.entries[path] = original

    def test_symlink_or_canonical_substitution_in_either_pair_image_denies(self):
        for path in (self.launcher, self.native):
            original = self.filesystem.entries[path]
            for changed in (replace(original, mode=stat.S_IFLNK | 0o777),
                            replace(original, canonical_path="/synthetic/substitution")):
                self.filesystem.entries[path] = changed
                with self.assertRaises(M.ClosedError):
                    M.reviewed_python_pair(self.filesystem, self.launcher)
            self.filesystem.entries[path] = original

    def test_pair_device_or_inode_change_after_native_capture_denies(self):
        for path in (self.launcher, self.native):
            original = self.filesystem.entries[path]
            for changed in (replace(original, device=2), replace(original, inode=original.inode + 1)):
                self.filesystem.entries[path] = [original, changed]
                with patch.object(M, "capture_snapshot", return_value=b"{}\n") as capture:
                    with self.assertRaises(M.ClosedError) as raised:
                        self.capture()
                    self.assertEqual(raised.exception.code, "host_process_changed")
                    self.assertEqual(capture.call_args[0][4], self.native)
                self.filesystem.entries[path] = original

    def test_a_different_root_owned_native_self_image_is_not_accepted(self):
        self.provider.paths[10] = PYTHON
        with self.assertRaises(M.ClosedError) as raised:
            self.capture()
        self.assertEqual(raised.exception.code, "host_process_self_probe")


class SelectedSnapshotTests(unittest.TestCase):
    def setUp(self):
        self.provider = Provider()
        self.filesystem = Filesystem()

    def capture(self, selected):
        return M.capture_snapshot(self.provider, self.filesystem, 10, 501, PYTHON,
                                  lambda: 200000000, selected)

    def test_closed_cli_accepts_only_canonical_sorted_integer_selection(self):
        self.assertIsNone(M.snapshot_arguments(["--snapshot"]))
        self.assertEqual(M.snapshot_arguments(["--snapshot-pids", "[2,20]"]), [2, 20])
        for arguments in ([], ["--snapshot-pids"], ["--snapshot", "[20]"],
                          ["--snapshot-pids", "[20]", "extra"], ["--uid", "501"]):
            with self.assertRaises(M.ClosedError):
                M.snapshot_arguments(arguments)

    def test_selection_rejects_whitespace_duplicates_unsorted_nonintegers_and_constants(self):
        for raw in ("", "[]", "[1]", "[0]", "[-2]", "[20,20]", "[20,2]",
                    " [20]", "[20] ", "[2, 20]", "[20.0]", "[2e1]", "[true]",
                    "[false]", "[null]", "[NaN]", "[Infinity]", "[02]", "[+2]",
                    "{}", '"[20]"', '[[20]]', '["20"]', "[2147483648]",
                    "[２０]", "[20]\n"):
            with self.subTest(raw=raw), self.assertRaises(M.ClosedError) as raised:
                M.parse_selection(raw)
            self.assertEqual(raised.exception.code, "host_process_arguments")

    def test_selection_count_and_argument_byte_bounds(self):
        values = list(range(2, M.MAX_PIDS + 2))
        raw = json.dumps(values, separators=(",", ":"))
        self.assertEqual(M.parse_selection(raw), values)
        for invalid in (json.dumps(values + [M.MAX_PIDS + 2], separators=(",", ":")),
                        " " * (M.MAX_SELECTION_BYTES + 1)):
            with self.assertRaises(M.ClosedError):
                M.parse_selection(invalid)

    def test_targeted_capture_adds_self_without_enumerating_unrelated_processes(self):
        original = self.provider.bsd_identity
        queried = []
        def inspect(pid):
            queried.append(pid)
            if pid == 30:
                raise M.ClosedError("host_process_uninspectable")
            return original(pid)
        with patch.object(self.provider, "list_pids", side_effect=AssertionError("must not enumerate")), \
                patch.object(self.provider, "bsd_identity", side_effect=inspect):
            result = json.loads(self.capture([20]))
        self.assertEqual([record["pid"] for record in result["records"]], ["10", "20"])
        self.assertEqual(set(queried), {10, 20})

    def test_self_already_selected_is_returned_once(self):
        result = json.loads(self.capture([10, 20]))
        self.assertEqual([record["pid"] for record in result["records"]], ["10", "20"])

    def test_selected_uninspectable_exited_changed_or_foreign_uid_denies_whole_capture(self):
        for failure in (M.ClosedError("host_process_uninspectable"),
                        ProcessLookupError("PRIVATE-process"), PermissionError("PRIVATE-process")):
            original = self.provider.bsd_identity
            def inspect(pid):
                if pid == 20:
                    raise failure
                return original(pid)
            with patch.object(self.provider, "bsd_identity", side_effect=inspect):
                with self.assertRaises(M.ClosedError):
                    self.capture([20])
        original = self.provider.identities[20]
        for changed in (replace(original, birth_usec=457), replace(original, euid=0)):
            self.provider.identities[20] = [original, changed]
            with self.assertRaises(M.ClosedError):
                self.capture([20])

    def test_whole_uid_mode_remains_strict_about_unrelated_uninspectable_processes(self):
        self.provider.pids = [10, 20, 30]
        original = self.provider.bsd_identity
        def inspect(pid):
            if pid == 30:
                raise M.ClosedError("host_process_uninspectable")
            return original(pid)
        with patch.object(self.provider, "bsd_identity", side_effect=inspect):
            with self.assertRaises(M.ClosedError):
                self.capture(None)

    def test_invalid_selection_fails_before_native_capture_and_errors_stay_private(self):
        for raw in ("[20,20]", "[20,PRIVATE]", '["PRIVATE-process"]'):
            output, errors = io.StringIO(), io.StringIO()
            with patch.object(sys, "argv", ["producer", "--snapshot-pids", raw]), \
                    patch.object(M, "capture_local") as capture, \
                    patch.object(sys, "stdout", output), patch.object(sys, "stderr", errors):
                self.assertEqual(M.main(), 1)
                capture.assert_not_called()
            self.assertEqual(output.getvalue(), "")
            self.assertEqual(errors.getvalue(), "host_process_arguments\n")


if __name__ == "__main__":
    unittest.main()
