"""Portable controls. No systemd unit, privileged release, or workload starts."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
ACTOR = HERE / "local-qa-linux-c2c-wait-actor.py"
PROTOCOL = HERE / "local-qa-linux-c2c-release-protocol.py"
ROOT = Path("/tmp/bluey-test-runs-123456789abc/run.1795250653.1001.102030")


def load(path):
    scope = {"__name__": "_portable_c2c_release_test", "__file__": str(path)}
    exec(compile(path.read_bytes(), str(path), "exec"), scope)
    return scope


def observed():
    run = "1" * 32
    unit = f"bluey-jobs-qa-{run}.service"
    return {"run": run, "unitName": unit, "bootId": "2" * 32, "invocationId": "3" * 32,
            "unitCgroup": f"/system.slice/{unit}", "networkNamespace": "net:[4026532555]",
            "root": str(ROOT), "rootDevice": 1, "rootInode": 99, "markerSha256": "4" * 64,
            "actorUid": 1000, "mainPid": 1001, "mainStartTime": 123, "actorPid": 1002, "actorStartTime": 234}


class WaitActor(unittest.TestCase):
    def setUp(self):
        self.actor = load(ACTOR)
        self.protocol = load(PROTOCOL)

    def test_real_marker_grammar_binds_separate_actor_group_and_main(self):
        marker = (f"magic=bluey-test-run-v2\nrun_root={ROOT}\nrun_name={ROOT.name}\n"
                  "owner_uid=1000\nowner_pid=1001\nprocess_group_id=1002\n"
                  "launcher_group_id=1001\ncreated_epoch=1795250653\nstate=running\n").encode()
        check = self.actor["marker_binding"]
        check(marker, ROOT, 1001, 1000, 1001, 1002)
        for changes in ((1003, 1000, 1001, 1002), (1001, 1001, 1001, 1002),
                        (1001, 1000, 1002, 1002), (1001, 1000, 1001, 1003)):
            with self.assertRaises(ValueError):
                check(marker, ROOT, *changes)
        with self.assertRaisesRegex(ValueError, "running_launcher_marker"):
            check(marker.replace(b"state=running", b"state=starting"), ROOT, 1001, 1000, 1001, 1002)

    def test_process_birth_parser_refuses_wrong_pid_and_zombie(self):
        raw = b"1002 (proc ) fake) S " + b"0 " * 18 + b"234 9\n"
        parse = self.actor["proc_start"]
        self.assertEqual(parse(raw, 1002), 234)
        for data, pid in ((raw, 1001), (raw.replace(b") S ", b") Z "), 1002),
                          (raw.replace(b"234 9", b"0234 9"), 1002)):
            with self.assertRaises(ValueError):
                parse(data, pid)

    def test_held_file_refuses_symlink_and_hardlink(self):
        with tempfile.TemporaryDirectory(prefix="bluey-c2c-wait-test-") as directory:
            where = Path(directory)
            regular = where / "spec"
            regular.write_bytes(b"synthetic")
            regular.chmod(0o600)
            fd = os.open(where, self.actor["DIR"])
            try:
                read = self.actor["named_bytes"]
                self.assertEqual(read(fd, "spec", 128, uid=os.getuid(), mode=0o600)[0], b"synthetic")
                (where / "sym").symlink_to(regular)
                with self.assertRaises(OSError):
                    read(fd, "sym", 128, uid=os.getuid(), mode=0o600)
                os.link(regular, where / "alias")
                with self.assertRaisesRegex(ValueError, "file_identity"):
                    read(fd, "spec", 128, uid=os.getuid(), mode=0o600)
            finally:
                os.close(fd)

    def test_waiter_refuses_mismatch_and_correctly_accepts_verified_wire(self):
        actor, release = self.actor, self.protocol
        observed_fields = observed()
        waiting = release["encode"](dict(observed_fields, schema="bluey-jobs-c2c-waiting-v1",
                                         nonce="5" * 32, waitingNs=100))
        frame = release["make_release"](waiting, observed_fields, "6" * 64, 110)
        with tempfile.TemporaryDirectory(prefix="bluey-c2c-gate-test-") as folder:
            fd = os.open(folder, actor["DIR"])
            try:
                self.assertTrue(actor["absent_release"](fd))
                ticks = iter((100, 90_000_000_099, 90_000_000_100))
                with self.assertRaisesRegex(ValueError, "release_timeout"):
                    actor["wait"](fd, waiting, release["accept_release"], clock=lambda: next(ticks), sleep=lambda _: None)
                # Portable control injects only the held-read result, not the
                # platform guard or production execution.
                actor["named_bytes"] = lambda *args, **kwargs: (frame, ("fixture",))
                found, _, value = actor["wait"](fd, waiting, release["accept_release"], clock=lambda: 112, sleep=None)
                self.assertEqual(found, frame)
                self.assertEqual(value["specSha256"], "6" * 64)
                other = release["encode"](dict(json.loads(waiting), nonce="7" * 32))
                with self.assertRaises(ValueError):
                    actor["wait"](fd, other, release["accept_release"], clock=lambda: 112, sleep=None)
            finally:
                os.close(fd)

    def test_spec_must_repeat_observed_invocation_and_the_sealed_source(self):
        fields = observed()
        plan = {"sourceTree": "8" * 40, "sourceInventorySha256": "9" * 64}
        unit = {k: fields[k] for k in ("mainPid", "bootId", "invocationId", "unitName")}
        binding = {k: fields[k] for k in ("root", "unitCgroup", "networkNamespace")}
        spec = dict(plan, unit=unit, binding=binding)
        self.actor["bind_spec"](json.dumps(spec).encode(), plan, fields)
        bad = dict(spec, unit=dict(unit, mainPid=fields["actorPid"]))
        with self.assertRaisesRegex(ValueError, "spec_observation"):
            self.actor["bind_spec"](json.dumps(bad).encode(), plan, fields)
        bad = dict(spec, sourceTree="0" * 40)
        with self.assertRaisesRegex(ValueError, "spec_observation"):
            self.actor["bind_spec"](json.dumps(bad).encode(), plan, fields)

    @unittest.skipIf(sys.platform == "linux", "Mac/Linux separation control")
    def test_cli_refuses_before_root_probe_on_non_linux(self):
        result = subprocess.run([sys.executable, "-I", "-S", "-B", str(ACTOR), "--run", "1" * 32,
                                 "--plan-sha256", "2" * 64], capture_output=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(result.stdout)
        self.assertEqual(result.stderr, b"local_qa_linux_c2c_wait_actor_refused\n")


if __name__ == "__main__":
    unittest.main()
