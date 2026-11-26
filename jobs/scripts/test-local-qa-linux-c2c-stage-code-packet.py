"""Portable write-path controls for ``local-qa-linux-c2c-stage-code-packet``.

The real Linux/root authority is not exercised. Tests use private temporary
bases and the actual verifier, write, and readback logic.
"""
from functools import partial
import hashlib
import importlib.util
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("stage", HERE / "local-qa-linux-c2c-stage-code-packet.py")
stage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stage)


class CodePacketStageTests(unittest.TestCase):
    def _bases(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        actor, manager = root / "actor", root / "manager"
        actor.mkdir(0o755)
        manager.mkdir(0o700)
        os.chmod(actor, 0o755)
        os.chmod(manager, 0o700)
        return actor, manager

    def test_existing_run_refuses_without_replacement(self):
        actor, manager = self._bases()
        actor_fd = os.open(actor, stage.DIR)
        manager_fd = os.open(manager, stage.DIR)
        self.addCleanup(os.close, actor_fd)
        self.addCleanup(os.close, manager_fd)
        run = "a" * 32
        os.mkdir(run, 0o755, dir_fd=actor_fd)
        with self.assertRaisesRegex(RuntimeError, "run_present"):
            stage._absent(actor_fd, run)
        self.assertFalse((manager / run).exists())

    def test_separate_trees_have_exact_modes_and_readback(self):
        actor, manager = self._bases()
        actor_fd = os.open(actor, stage.DIR)
        manager_fd = os.open(manager, stage.DIR)
        self.addCleanup(os.close, actor_fd)
        self.addCleanup(os.close, manager_fd)
        run = "b" * 32
        owner = os.getuid()
        actor_group, manager_group = self._group(actor_fd), self._group(manager_fd)
        actor_run = stage._stage(actor_fd, run, {"one.mjs": b"actor"}, 0o755, owner, actor_group)
        self.addCleanup(os.close, actor_run)
        manager_run = stage._stage(manager_fd, run, {"two.py": b"manager"}, 0o700, owner, manager_group)
        self.addCleanup(os.close, manager_run)
        stage._verify_stage(actor_fd, run, actor_run, {"one.mjs": b"actor"}, 0o755, owner, actor_group)
        stage._verify_stage(manager_fd, run, manager_run, {"two.py": b"manager"}, 0o700, owner, manager_group)
        self.assertEqual(os.stat(actor / run).st_mode & 0o777, 0o755)
        self.assertEqual(os.stat(manager / run).st_mode & 0o777, 0o700)
        self.assertEqual(os.stat(actor / run / "one.mjs").st_mode & 0o777, 0o444)
        self.assertEqual(os.stat(manager / run / "two.py").st_mode & 0o777, 0o600)

    @staticmethod
    def _group(fd):
        # APFS inherits the parent GID; the test launcher's temp root can be
        # group wheel even when the invoking user's primary group is staff.
        return os.fstat(fd).st_gid if sys.platform == "darwin" else os.getegid()

    def test_actual_packet_stages_with_simulated_guest_authority(self):
        source = stage._read_verifier()
        baseline = Path(os.environ["C2C_CODE_BASE_INVENTORY"]).read_bytes()
        packet = source.build_packet(HERE, baseline)
        digest = hashlib.sha256(packet).hexdigest()
        _, _, receipt = source.verify_packet(packet, digest)
        run = "a" * 32
        cap = {key: receipt[key] for key in ("packetSha256", "codeInventorySha256", "sourceTree", "sourceInventorySha256")}
        cap.update(run=run, reservationSha256="b" * 64)
        actual_stage, actual_verify = stage._stage, stage._verify_stage

        def base_current(path, fd, mode):
            held = os.fstat(fd)
            self.assertEqual(path.resolve(), path)
            self.assertTrue(os.path.samestat(held, path.lstat()))
            self.assertEqual((held.st_uid, stat.S_IMODE(held.st_mode)), (os.getuid(), mode))

        def open_base(path, mode):
            fd = os.open(path, stage.DIR)
            try:
                base_current(path, fd, mode)
            except BaseException:
                os.close(fd)
                raise
            return fd

        def portable(function, parent, *args):
            return function(parent, *args, owner=os.getuid(), group=self._group(parent))

        for scenario, error in (("clean", None), ("bad-packet", "packet_pin"),
                                ("occupied-manager", "run_present"), ("late-body-drift", "stage_leaf")):
            with self.subTest(scenario=scenario):
                actor, manager = (path.resolve() for path in self._bases())
                calls = []

                def admitted(projection):
                    self.assertEqual(projection, cap)
                    self.assertIsNot(projection, cap)
                    calls.append(True)
                    if scenario == "late-body-drift" and len(calls) == 2:
                        leaf = actor / run / source.PROTOCOL
                        body = leaf.read_bytes()
                        leaf.chmod(0o600)
                        changed = bytes([body[0] ^ 1]) + body[1:]
                        self.assertNotEqual(changed, body)
                        leaf.write_bytes(changed)
                        leaf.chmod(0o444)
                    return True

                if scenario == "occupied-manager":
                    (manager / run).mkdir()
                with patch.multiple(stage, ACTOR_BASE=actor, MANAGER_BASE=manager, _root_linux=lambda: True,
                                    _open_base=open_base, _base_current=base_current,
                                    _stage=partial(portable, actual_stage), _verify_stage=partial(portable, actual_verify)):
                    raw = packet + b"unexpected" if scenario == "bad-packet" else packet
                    if error:
                        with self.assertRaisesRegex((RuntimeError, ValueError), error):
                            stage.stage_code_packet(raw, digest, cap, admitted=admitted)
                    else:
                        result = stage.stage_code_packet(raw, digest, cap, admitted=admitted)
                        self.assertEqual((result["actorFiles"], result["managerFiles"]), (31, 11))
                        self.assertIs(result["releasePublished"], False)
                        self.assertIs(result["runtimeQualified"], False)
                expected_calls = {"clean": 2, "bad-packet": 0, "occupied-manager": 1, "late-body-drift": 2}
                self.assertEqual(len(calls), expected_calls[scenario])
                self.assertEqual((actor / run).exists(), scenario in {"clean", "late-body-drift"})
                self.assertEqual((manager / run).exists(), scenario != "bad-packet")


if __name__ == "__main__":
    unittest.main()
