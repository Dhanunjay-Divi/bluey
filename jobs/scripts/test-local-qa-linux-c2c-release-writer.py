"""Portable exercise of publication with synthetic systemd observations.

The real one-shot filesystem primitive runs against a fresh test directory.
No root Linux check is bypassed by production code and no unit is started.
"""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import types
import unittest


HERE = Path(__file__).resolve().parent
WRITER = HERE / "local-qa-linux-c2c-release-writer.py"
PROTOCOL = HERE / "local-qa-linux-c2c-release-protocol.py"
ATOMIC = HERE / "receipt-browser-release-writer.py"
DIGEST = "f" * 64


def load(path):
    scope = types.ModuleType("_portable_release_fixture")
    scope.__file__ = str(path)
    exec(compile(path.read_bytes(), str(path), "exec"), scope.__dict__)
    return scope


def observed():
    run = "1" * 32
    unit = f"bluey-jobs-qa-{run}.service"
    return {"run": run, "unitName": unit, "bootId": "2" * 32, "invocationId": "3" * 32,
            "unitCgroup": f"/system.slice/{unit}", "networkNamespace": "net:[4026532555]",
            "root": "/tmp/bluey-test-runs-123456789abc/run.1795250653.1001.102030",
            "rootDevice": 0, "rootInode": 99, "markerSha256": "4" * 64,
            "actorUid": 1000, "mainPid": 1001, "mainStartTime": 123, "actorPid": 1002,
            "actorStartTime": 234}


class TestOS:
    def __getattr__(self, name):
        return getattr(os, name)

    def uname(self):
        return types.SimpleNamespace(machine="x86_64")

    def getuid(self):
        return 0

    def geteuid(self):
        return 0


class ReleaseWriter(unittest.TestCase):
    def initialize(self, folder):
        protocol, writer, atomic = load(PROTOCOL), load(WRITER), load(ATOMIC)
        sample = observed()
        waiting = protocol.encode(dict(sample, schema="bluey-jobs-c2c-waiting-v1",
                                       nonce="5" * 32, waitingNs=time.clock_gettime_ns(time.CLOCK_MONOTONIC)))
        gate = Path(folder) / sample["run"]
        gate.mkdir()
        gate.chmod(0o755)
        writer.sys, writer.os, writer.GATE_PARENT = types.SimpleNamespace(platform="linux"), TestOS(), gate.parent
        writer._protocol = lambda: protocol
        writer._read_actor_inputs = lambda *args: ("fixture-root", "fixture-marker", "fixture-spec")
        writer._root_recheck = lambda *args: None

        def open_gate(run):
            fd = os.open(gate, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            return fd, writer._gate_identity(os.fstat(fd))

        def publish(fd, body, check, *, owner, deadline_ns, clock):
            self.assertEqual(owner, 0)
            return atomic.publish_at(fd, body, check, owner=os.getuid(), deadline_ns=deadline_ns, clock=clock)

        writer._open_gate = open_gate
        writer._read_pinned_source = lambda *args: types.SimpleNamespace(publish_at=publish)
        return writer, protocol, sample, waiting, gate

    def test_atomic_publication_only_once_despite_gate_timestamp_changes(self):
        with tempfile.TemporaryDirectory(prefix="bluey-c2c-release-test-") as folder:
            writer, protocol, sample, waiting, gate = self.initialize(folder)
            report = writer.issue_release(waiting, DIGEST, independently_observe=lambda _: dict(sample),
                                          verify_admitted_artifacts=lambda *_: True)
            self.assertIs(report["runtimeQualified"], False)
            self.assertIs(report["productionQualified"], False)
            original = (gate / "release.json").read_bytes()
            final = protocol.accept_release(original, waiting, time.clock_gettime_ns(time.CLOCK_MONOTONIC))
            self.assertEqual(final["specSha256"], DIGEST)
            with self.assertRaisesRegex(RuntimeError, "release_already_published"):
                writer.issue_release(waiting, DIGEST, independently_observe=lambda _: dict(sample),
                                     verify_admitted_artifacts=lambda *_: True)
            self.assertEqual((gate / "release.json").read_bytes(), original)

    def test_lost_admission_after_claim_does_not_publish(self):
        with tempfile.TemporaryDirectory(prefix="bluey-c2c-release-test-") as folder:
            writer, _, sample, waiting, gate = self.initialize(folder)
            with self.assertRaisesRegex(RuntimeError, "c2c_release_artifacts"):
                writer.issue_release(waiting, DIGEST, independently_observe=lambda _: dict(sample),
                                     verify_admitted_artifacts=lambda *_: not (gate / ".release.claim").exists())
            self.assertTrue((gate / ".release.claim").is_file())
            self.assertFalse((gate / "release.json").exists())

    def test_disagreement_refuses_before_claim(self):
        with tempfile.TemporaryDirectory(prefix="bluey-c2c-release-test-") as folder:
            writer, _, sample, waiting, gate = self.initialize(folder)
            with self.assertRaisesRegex(RuntimeError, "c2c_release_observation_mismatch"):
                writer.issue_release(waiting, DIGEST,
                                     independently_observe=lambda _: dict(sample, actorStartTime=235),
                                     verify_admitted_artifacts=lambda *_: True)
            self.assertEqual(list(gate.iterdir()), [])

    def test_actor_root_uses_actual_running_marker_leaf_and_spec_hash(self):
        writer = load(WRITER)
        with tempfile.TemporaryDirectory(prefix="bluey-c2c-release-leaves-") as folder:
            root = Path(folder)
            root.chmod(0o700)
            dev, ino = root.stat().st_dev, root.stat().st_ino
            wait = dict(observed(), actorUid=os.getuid(), rootDevice=dev, rootInode=ino)
            logical = Path(wait["root"])
            marker = (f"magic=bluey-test-run-v2\nrun_root={logical}\nrun_name={logical.name}\n"
                      f"owner_uid={os.getuid()}\nowner_pid=1001\nprocess_group_id=1002\n"
                      "launcher_group_id=1001\ncreated_epoch=1795250653\nstate=running\n").encode()
            wait["markerSha256"] = hashlib.sha256(marker).hexdigest()
            spec = b"synthetic-spec-bytes\n"
            marker_path = root / ".bluey-test-run-v2"
            spec_path = root / ".local-qa-linux-c2c-native.json"
            for path, body in ((marker_path, marker), (spec_path, spec)):
                path.write_bytes(body)
                path.chmod(0o600)
            writer._open_actor_root = lambda _: os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            writer._proc_stat = lambda pid: (10, pid)
            self.assertEqual(len(writer._read_actor_inputs(wait, hashlib.sha256(spec).hexdigest())), 3)
            with self.assertRaisesRegex(RuntimeError, "c2c_release_spec"):
                writer._read_actor_inputs(wait, DIGEST)
            marker_path.rename(root / ".bluey-local-qa.json")
            with self.assertRaises(FileNotFoundError):
                writer._read_actor_inputs(wait, hashlib.sha256(spec).hexdigest())

    def test_ipv6_loopback_reject_default_allowed_but_real_default_refused(self):
        writer = load(WRITER)
        values = {"dev": "Inter-| Receive\n face | stats\n lo: 0 0\n",
                  "route": "Iface Destination Gateway Flags\n",
                  "ipv6_route": ("0" * 32 + " 00 " + "0" * 32 + " 00 " + "0" * 32
                                 + " ffffffff 00000001 00000000 00200200 lo\n")}
        writer._proc_text = lambda path, maximum=4096: values[path.name]
        self.assertIs(writer._network_is_loopback_only(1002), True)
        values["ipv6_route"] = values["ipv6_route"].replace("00200200", "00200000")
        with self.assertRaisesRegex(RuntimeError, "c2c_release_ipv6_default"):
            writer._network_is_loopback_only(1002)

    @unittest.skipIf(sys.platform == "linux" and os.getuid() == 0, "privileged Linux caller")
    def test_platform_guard_does_not_consult_callbacks(self):
        writer = load(WRITER)
        with self.assertRaisesRegex(RuntimeError, "c2c_release_linux_root"):
            writer.issue_release(json.dumps(observed()).encode(), DIGEST,
                                 independently_observe=lambda _: self.fail("unexpected observation"),
                                 verify_admitted_artifacts=lambda *_: self.fail("unexpected verification"))


if __name__ == "__main__":
    unittest.main()
