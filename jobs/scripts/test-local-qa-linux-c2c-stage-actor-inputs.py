#!/usr/bin/env python3
"""Portable controls for the pre-release C2C actor input stager."""
from __future__ import annotations

import hashlib
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest


PATH = Path(__file__).with_name("local-qa-linux-c2c-stage-actor-inputs.py")
SPEC = importlib.util.spec_from_file_location("stage_actor_inputs", PATH)
assert SPEC and SPEC.loader
stage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stage)


def waiting() -> dict:
    run = "a" * 32
    return {"schema": "bluey-jobs-c2c-waiting-v1", "run": run, "unitName": f"bluey-jobs-qa-{run}.service",
            "bootId": "b" * 32, "invocationId": "c" * 32,
            "unitCgroup": f"/system.slice/bluey-jobs-qa-{run}.service", "networkNamespace": "net:[4026532555]",
            "root": "/tmp/bluey-test-runs-abcdef012345/run.1795250653.12345.32615190360",
            "rootDevice": 0, "rootInode": 44, "markerSha256": "d" * 64, "actorUid": 999,
            "mainPid": 12345, "mainStartTime": 1, "actorPid": 12346, "actorStartTime": 2,
            "nonce": "e" * 32, "waitingNs": 3}


class StageShapeTests(unittest.TestCase):
    def test_waiting_shape_accepts_zero_device_and_rejects_ambiguous_fields(self) -> None:
        self.assertEqual(stage._validate_waiting(waiting())["rootDevice"], 0)
        with self.assertRaises(RuntimeError):
            stage._validate_waiting({**waiting(), "rootInode": True})
        with self.assertRaises(RuntimeError):
            stage._validate_waiting({key: value for key, value in waiting().items() if key != "nonce"})

    def test_stage_refuses_before_any_staging_on_nonprivileged_platform_guard(self) -> None:
        original_system, original_machine, original_uid, original_euid = (stage.platform.system,
            stage.platform.machine, stage.os.getuid, stage.os.geteuid)
        try:
            stage.platform.system = lambda: "Darwin"
            stage.platform.machine = lambda: "arm64"
            stage.os.getuid = stage.os.geteuid = lambda: 501
            with self.assertRaises(RuntimeError):
                stage.stage_actor_inputs(waiting(), spec_bytes=b"{}", spec_sha256=hashlib.sha256(b"{}").hexdigest(),
                                         fixture_path="/missing", fixture_sha256="0" * 64,
                                         independently_admitted=lambda _: (_ for _ in ()).throw(AssertionError()))
        finally:
            stage.platform.system, stage.platform.machine = original_system, original_machine
            stage.os.getuid, stage.os.geteuid = original_uid, original_euid

    def test_descriptor_rooted_source_rejects_symlink_before_source_custody(self) -> None:
        with tempfile.TemporaryDirectory(prefix="bluey-c2c-source-") as temporary:
            root = Path(temporary)
            real = root / "jobs-nonprod-browser-fixture"
            real.write_bytes(b"fixture")
            real.chmod(0o755)
            link = root / "linked" / "jobs-nonprod-browser-fixture"
            link.parent.mkdir()
            link.symlink_to(real)
            with self.assertRaises((OSError, RuntimeError)):
                stage._read_source(link, hashlib.sha256(b"fixture").hexdigest())

    def test_unadmitted_root_plan_refuses_before_source_or_actor_open(self) -> None:
        original_system, original_machine, original_uid, original_euid = (stage.platform.system,
            stage.platform.machine, stage.os.getuid, stage.os.geteuid)
        try:
            stage.platform.system = lambda: "Linux"
            stage.platform.machine = lambda: "x86_64"
            stage.os.getuid = stage.os.geteuid = lambda: 0
            with self.assertRaises(RuntimeError):
                stage.stage_actor_inputs(waiting(), spec_bytes=b"{}", spec_sha256=hashlib.sha256(b"{}").hexdigest(),
                                         fixture_path="/no/such/jobs-nonprod-browser-fixture", fixture_sha256="0" * 64,
                                         independently_admitted=lambda _: False)
        finally:
            stage.platform.system, stage.platform.machine = original_system, original_machine
            stage.os.getuid, stage.os.geteuid = original_uid, original_euid


if __name__ == "__main__":
    unittest.main()
