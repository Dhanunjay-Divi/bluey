"""Real private-directory regressions for the mock-receipt launch hold."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "launch_hold", HERE / "local-qa-linux-signup-mock-receipt-launch-hold.py",
)
hold = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(hold)


class LaunchHoldTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name).resolve(strict=True)
        os.chmod(self.directory, 0o700)
        info = self.directory.stat()
        self.identity = (info.st_dev, info.st_ino)
        self.live = True
        self.observed = []
        self.actor = {"run": "a" * 32, "unit": "bluey-jobs-qa-" + "a" * 32 + ".service",
                      "uid": max(1, os.getuid()), "gid": max(1, os.getgid()),
                      "runParent": "/tmp/bluey-test-runs-012345abc123"}

    def reserve(self):
        return hold.make_reserve_launch(
            self.directory, self.identity,
            {"reservationSha256": "b" * 64, "held": lambda: self.live}, self.actor,
            observe_zero=lambda proof: self.observed.append(proof) is None,
        )

    @staticmethod
    def zero():
        return {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0}

    def test_arms_once_completes_once_without_releasing_hold(self):
        lease = self.reserve()(dict(self.actor))
        self.assertEqual(set(lease), {"reservationSha256", "held", "complete"})
        self.assertTrue(lease["held"]())
        self.assertTrue(lease["complete"](self.zero()))
        self.assertTrue(lease["held"]())
        self.assertEqual(self.observed, [self.zero()])
        with self.assertRaisesRegex(ValueError, "completion_replayed"):
            lease["complete"](self.zero())

    def test_wrong_actor_refuses_without_arming(self):
        with self.assertRaisesRegex(ValueError, "actor_binding"):
            self.reserve()({**self.actor, "uid": self.actor["uid"] + 1})
        self.assertFalse((self.directory / hold.ARMED).exists())

    def test_invalid_reservation_digest_refuses_before_arming(self):
        with self.assertRaisesRegex(ValueError, "reservation"):
            hold.make_reserve_launch(
                self.directory, self.identity,
                {"reservationSha256": "0" * 64, "held": lambda: True}, self.actor,
                observe_zero=lambda _: True,
            )
        self.assertFalse((self.directory / hold.ARMED).exists())

    def test_lost_external_reservation_is_fail_closed_and_nonretriable(self):
        lease = self.reserve()(dict(self.actor))
        self.live = False
        self.assertFalse(lease["held"]())
        with self.assertRaisesRegex(ValueError, "reservation_lost"):
            lease["complete"](self.zero())
        self.live = True
        with self.assertRaisesRegex(ValueError, "completion_replayed"):
            lease["complete"](self.zero())
        self.assertFalse((self.directory / hold.COMPLETE).exists())

    def test_parent_mode_drift_invalidates_held_record(self):
        lease = self.reserve()(dict(self.actor))
        os.chmod(self.directory, 0o755)
        self.assertFalse(lease["held"]())
        os.chmod(self.directory, 0o700)

    def test_orphan_completion_and_reuse_refuse(self):
        (self.directory / hold.COMPLETE).write_bytes(b"orphan")
        os.chmod(self.directory / hold.COMPLETE, 0o600)
        with self.assertRaisesRegex(ValueError, "already_recorded"):
            self.reserve()(dict(self.actor))

    def test_fresh_factory_cannot_reuse_after_armed_or_completed(self):
        lease = self.reserve()(dict(self.actor))
        with self.assertRaisesRegex(ValueError, "already_recorded"):
            self.reserve()(dict(self.actor))
        self.assertTrue(lease["complete"](self.zero()))
        with self.assertRaisesRegex(ValueError, "already_recorded"):
            self.reserve()(dict(self.actor))

    def test_same_size_armed_record_mutation_invalidates_hold(self):
        lease = self.reserve()(dict(self.actor))
        arm = self.directory / hold.ARMED
        raw = arm.read_bytes()
        arm.write_bytes(bytes([raw[0] ^ 1]) + raw[1:])
        os.chmod(arm, 0o600)
        self.assertFalse(lease["held"]())


if __name__ == "__main__":
    unittest.main()
