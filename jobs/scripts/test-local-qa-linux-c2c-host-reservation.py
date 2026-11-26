#!/usr/bin/env python3
"""Portable host-reservation controls; never start a guest or open the network."""

from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
RESERVATION = HERE / "local-qa-linux-c2c-host-reservation.py"
REFUSED = r"c2c_host_reservation_refused:"
RUN, TREE, INVENTORY = "1" * 32, "a" * 40, "b" * 64


def load_module():
    scope = {"__name__": "_portable_c2c_host_reservation_test", "__file__": str(RESERVATION)}
    exec(compile(RESERVATION.read_bytes(), str(RESERVATION), "exec"), scope)
    return scope


def binding(**changes):
    value = {
        "instance": "bluey-c2c-guest",
        "instanceIdentity": [101, 102],
        "diskIdentity": [103, 104],
        "configSha256": "c" * 64,
        "imageSha256": "d" * 64,
        "packetSha256": "e" * 64,
        "sourceTree": TREE,
        "sourceInventorySha256": INVENTORY,
        "projection": {
            "run": RUN,
            "unit": f"bluey-jobs-qa-{RUN}.service",
            "uid": 1000,
            "gid": 1000,
            "runParent": "/tmp/bluey-test-runs-123456789abc",
        },
    }
    value.update(changes)
    return value


def completion(reservation_sha256, **changes):
    value = {
        "reservationSha256": reservation_sha256,
        "sourceTree": TREE,
        "sourceInventorySha256": INVENTORY,
        "resultSha256": "f" * 64,
        "teardown": {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0},
    }
    value.update(changes)
    return value


def path_identity(path):
    info = path.lstat()
    return (info.st_dev, info.st_ino)


def file_pin(path):
    info = path.lstat()
    return (
        info.st_dev, info.st_ino, info.st_uid, info.st_gid, stat.S_IMODE(info.st_mode),
        info.st_nlink, info.st_size, hashlib.sha256(path.read_bytes()).hexdigest(),
    )


CHILD = r"""
import json
import os
from pathlib import Path
import stat
import sys

module_path, root = Path(sys.argv[1]), Path(sys.argv[2])
expected, mode = json.loads(sys.argv[3]), sys.argv[4]
scope = {"__name__": "_c2c_host_reservation_crash_child", "__file__": str(module_path)}
exec(compile(module_path.read_bytes(), str(module_path), "exec"), scope)
root_info = root.lstat()
stopped_calls = 0

def verify_instance(value):
    if value != expected or value is expected or value["projection"] is expected["projection"]:
        os._exit(40)
    return True

def stopped(value):
    global stopped_calls
    stopped_calls += 1
    if not verify_instance(value):
        os._exit(41)
    if mode == "second-check" and stopped_calls == 2:
        path = root / "reservation.json"
        info, raw = path.lstat(), path.read_bytes()
        record = json.loads(raw)
        if (not stat.S_ISREG(info.st_mode) or stat.S_IMODE(info.st_mode) != 0o600
                or info.st_nlink != 1 or record["binding"] != expected):
            os._exit(42)
        os._exit(29)
    return True

hold = scope["reserve_guest"](
    root, (root_info.st_dev, root_info.st_ino), expected,
    stopped=stopped, verify_instance=verify_instance,
)
if stopped_calls != 2 or set(hold) != {"reservationSha256", "held", "note_workload"}:
    os._exit(43)
if hold["held"]() is not True:
    os._exit(44)
os._exit(23)
"""


class HostReservationTests(unittest.TestCase):
    def setUp(self):
        self.module = load_module()
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.new_slot("host-reservation")
        self.binding = binding()

    def new_slot(self, name):
        path = self.base / name
        path.mkdir(mode=0o700)
        path.chmod(0o700)
        return path.resolve(strict=True)

    def observer(self, expected=None, calls=None, result=True):
        expected = self.binding if expected is None else expected
        calls = [] if calls is None else calls

        def observe(value):
            self.assertEqual(value, expected)
            self.assertIsNot(value, expected)
            self.assertIsNot(value["projection"], expected["projection"])
            calls.append(deepcopy(value))
            return result

        return observe

    def reserve(self, root=None, value=None, stopped=None, verify_instance=None):
        root = self.root if root is None else root
        value = self.binding if value is None else value
        return self.module["reserve_guest"](
            root,
            path_identity(root),
            value,
            stopped=self.observer(value) if stopped is None else stopped,
            verify_instance=self.observer(value) if verify_instance is None else verify_instance,
        )

    def refused(self, function, *args, **kwargs):
        with self.assertRaisesRegex(ValueError, REFUSED):
            function(*args, **kwargs)

    def test_reservation_is_private_durable_and_checked_twice(self):
        stopped_calls, instance_calls = [], []
        hold = self.reserve(
            stopped=self.observer(calls=stopped_calls),
            verify_instance=self.observer(calls=instance_calls),
        )
        self.assertEqual(set(hold), {"reservationSha256", "held", "note_workload"})
        self.assertEqual(stopped_calls, [self.binding, self.binding])
        self.assertEqual(instance_calls, [self.binding, self.binding])
        path = self.root / "reservation.json"
        info, raw = path.lstat(), path.read_bytes()
        self.assertTrue(stat.S_ISREG(info.st_mode))
        self.assertEqual((stat.S_IMODE(info.st_mode), info.st_nlink), (0o600, 1))
        self.assertEqual(hashlib.sha256(raw).hexdigest(), hold["reservationSha256"])
        self.assertEqual(json.loads(raw)["binding"], self.binding)
        self.assertIs(hold["held"](), True)

    def test_duplicate_same_or_different_binding_never_replaces_authority(self):
        hold = self.reserve()
        path = self.root / "reservation.json"
        before = file_pin(path)
        for value in (deepcopy(self.binding), binding(packetSha256="9" * 64)):
            with self.subTest(packet=value["packetSha256"]):
                self.refused(self.reserve, value=value)
                self.assertEqual(file_pin(path), before)
                self.assertIs(hold["held"](), True)

    def test_symlink_root_or_record_is_never_followed(self):
        real = self.new_slot("real-root")
        alias = self.base / "root-alias"
        alias.symlink_to(real, target_is_directory=True)
        self.refused(
            self.module["reserve_guest"], alias, path_identity(alias), self.binding,
            stopped=self.observer(), verify_instance=self.observer(),
        )
        self.assertFalse((real / "reservation.json").exists())

        root = self.new_slot("reservation-link-root")
        target = self.base / "reservation-link-target"
        target.write_bytes(b"do-not-follow\n")
        (root / "reservation.json").symlink_to(target)
        target_before = file_pin(target)
        self.refused(
            self.module["reserve_guest"], root, path_identity(root), self.binding,
            stopped=self.observer(), verify_instance=self.observer(),
        )
        self.assertEqual(file_pin(target), target_before)

    def test_bad_binding_directory_and_stop_authority_fail_closed(self):
        bad = [
            dict(self.binding, extra=True),
            {key: value for key, value in self.binding.items() if key != "packetSha256"},
            binding(instance="Bluey_C2C"),
            binding(instanceIdentity=[True, 102]),
            binding(diskIdentity=[103]),
            binding(projection=dict(self.binding["projection"], uid=True)),
            binding(projection=dict(self.binding["projection"], runParent="/tmp")),
        ]
        for index, value in enumerate(bad):
            root = self.new_slot(f"bad-binding-{index}")
            with self.subTest(index=index):
                self.refused(
                    self.module["reserve_guest"], root, path_identity(root), value,
                    stopped=lambda _: self.fail("called"), verify_instance=lambda _: self.fail("called"),
                )
                self.assertFalse((root / "reservation.json").exists())

        root = self.new_slot("wrong-directory-identity")
        wrong = (path_identity(root)[0], path_identity(root)[1] + 1)
        self.refused(
            self.module["reserve_guest"], root, wrong, self.binding,
            stopped=lambda _: self.fail("called"), verify_instance=lambda _: self.fail("called"),
        )
        self.assertFalse((root / "reservation.json").exists())

        for index, answer in enumerate((False, 1, None)):
            root = self.new_slot(f"bad-stopped-{index}")
            self.refused(
                self.module["reserve_guest"], root, path_identity(root), self.binding,
                stopped=lambda _, answer=answer: answer, verify_instance=self.observer(),
            )
            self.assertFalse((root / "reservation.json").exists())

    def test_second_stop_refusal_retains_nonreusable_authority(self):
        root = self.new_slot("second-stop-refusal")
        calls = []

        def stopped(value):
            calls.append(deepcopy(value))
            return len(calls) == 1

        self.refused(
            self.module["reserve_guest"], root, path_identity(root), self.binding,
            stopped=stopped, verify_instance=self.observer(),
        )
        self.assertEqual(calls, [self.binding, self.binding])
        path = root / "reservation.json"
        before = file_pin(path)
        self.refused(
            self.module["reserve_guest"], root, path_identity(root), self.binding,
            stopped=self.observer(), verify_instance=self.observer(),
        )
        self.assertEqual(file_pin(path), before)

    def run_crash_child(self, name, mode, expected_code):
        root = self.new_slot(name)
        process = subprocess.run(
            [sys.executable, "-I", "-S", "-B", "-c", CHILD, str(RESERVATION), str(root),
             json.dumps(self.binding, separators=(",", ":"), sort_keys=True), mode],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
        )
        self.assertEqual(
            process.returncode, expected_code,
            (process.stdout.decode("utf-8", "replace"), process.stderr.decode("utf-8", "replace")),
        )
        self.assertEqual((process.stdout, process.stderr), (b"", b""))
        path = root / "reservation.json"
        self.assertEqual(json.loads(path.read_bytes())["binding"], self.binding)
        before = file_pin(path)
        for value in (self.binding, binding(packetSha256="9" * 64)):
            self.refused(
                self.module["reserve_guest"], root, path_identity(root), value,
                stopped=self.observer(value), verify_instance=self.observer(value),
            )
            self.assertEqual(file_pin(path), before)

    def test_process_exit_after_success_retains_one_shot_reservation(self):
        self.run_crash_child("crash-after-success", "success", 23)

    def test_process_exit_at_second_check_retains_created_reservation(self):
        self.run_crash_child("crash-at-second-check", "second-check", 29)

    def test_held_detects_drift_and_bad_authority_cannot_be_recreated(self):
        mutators = {
            "content": lambda path, raw: path.write_bytes(raw[:-1] + b"!"),
            "mode": lambda path, _raw: path.chmod(0o640),
            "same-bytes-new-inode": self.replace_same_bytes,
        }
        for name, mutate in mutators.items():
            root = self.new_slot(f"held-{name}")
            hold = self.reserve(root=root)
            path = root / "reservation.json"
            mutate(path, path.read_bytes())
            bad = file_pin(path)
            with self.subTest(name=name):
                self.assertIs(hold["held"](), False)
                self.refused(
                    self.module["reserve_guest"], root, path_identity(root), self.binding,
                    stopped=self.observer(), verify_instance=self.observer(),
                )
                self.assertEqual(file_pin(path), bad)

    @staticmethod
    def replace_same_bytes(path, raw):
        path.unlink()
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())

    def test_verify_instance_drift_blocks_held_and_workload_note(self):
        state, calls = {"valid": True}, []

        def verify(value):
            calls.append(deepcopy(value))
            return state["valid"]

        hold = self.reserve(verify_instance=verify)
        self.assertEqual(calls, [self.binding, self.binding])
        state["valid"] = False
        self.assertIs(hold["held"](), False)
        self.refused(hold["note_workload"], completion(hold["reservationSha256"]))
        self.assertFalse((self.root / "workload-closed.json").exists())

    def test_invalid_completion_never_creates_workload_authority(self):
        invalid = [
            lambda good: dict(good, extra=True),
            lambda good: {key: value for key, value in good.items() if key != "resultSha256"},
            lambda good: dict(good, reservationSha256="9" * 64),
            lambda good: dict(good, sourceTree="9" * 40),
            lambda good: dict(good, sourceInventorySha256="9" * 64),
            lambda good: dict(good, resultSha256="not-a-digest"),
            lambda good: dict(good, teardown=dict(good["teardown"], unitStopped=False)),
            lambda good: dict(good, teardown=dict(good["teardown"], actorProcesses=1)),
            lambda good: dict(good, teardown=dict(good["teardown"], namespaceProcesses=1)),
            lambda good: dict(good, teardown=dict(good["teardown"], actorProcesses=False)),
            lambda _good: [],
        ]
        for index, change in enumerate(invalid):
            root = self.new_slot(f"invalid-completion-{index}")
            hold = self.reserve(root=root)
            reservation = file_pin(root / "reservation.json")
            with self.subTest(index=index):
                self.refused(hold["note_workload"], change(completion(hold["reservationSha256"])))
                self.assertFalse((root / "workload-closed.json").exists())
                self.assertEqual(file_pin(root / "reservation.json"), reservation)
                self.assertIs(hold["held"](), True)

    def test_closed_workload_is_private_one_shot_and_never_releases_guest(self):
        hold = self.reserve()
        reservation_path = self.root / "reservation.json"
        reservation = file_pin(reservation_path)
        proof = completion(hold["reservationSha256"])
        self.assertIs(hold["note_workload"](proof), True)
        workload_path = self.root / "workload-closed.json"
        info = workload_path.lstat()
        self.assertEqual((stat.S_IMODE(info.st_mode), info.st_nlink), (0o600, 1))
        self.assertEqual(
            json.loads(workload_path.read_bytes()),
            {"schema": "bluey-jobs-c2c-host-workload-note-v1", "guestReusable": False, "proof": proof},
        )
        workload = file_pin(workload_path)
        self.assertEqual(file_pin(reservation_path), reservation)
        self.assertIs(hold["held"](), True)
        self.refused(hold["note_workload"], proof)
        self.assertEqual(file_pin(workload_path), workload)
        for value in (self.binding, binding(packetSha256="9" * 64)):
            self.refused(self.reserve, value=value)
            self.assertEqual((file_pin(reservation_path), file_pin(workload_path)),
                             (reservation, workload))

    def test_bad_workload_state_cannot_be_recreated_or_overwritten(self):
        hold = self.reserve()
        path = self.root / "workload-closed.json"
        path.write_bytes(b"invalid-state\n")
        path.chmod(0o600)
        before = file_pin(path)
        self.refused(hold["note_workload"], completion(hold["reservationSha256"]))
        self.assertEqual(file_pin(path), before)
        self.refused(self.reserve)
        self.assertEqual(file_pin(path), before)

    def test_relevant_oserror_from_stop_authority_propagates(self):
        error = OSError(5, "synthetic stopped authority I/O failure")

        def stopped(_):
            raise error

        with self.assertRaises(OSError) as raised:
            self.reserve(stopped=stopped)
        self.assertIs(raised.exception, error)
        self.assertFalse((self.root / "reservation.json").exists())


if __name__ == "__main__":
    unittest.main()
