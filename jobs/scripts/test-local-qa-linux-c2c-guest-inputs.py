#!/usr/bin/env python3
"""Portable custody controls for the inert C2C guest-input reader.

The optional ``C2C_GUEST_INPUTS_EVIDENCE`` case streams the current local
retained roles in place, without copying or retaining the payloads.
"""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import importlib.util
import os
from pathlib import Path
import stat
import tempfile
import unittest
from types import MappingProxyType
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
PATH = HERE / "local-qa-linux-c2c-guest-inputs.py"
SPEC = importlib.util.spec_from_file_location("c2c_guest_inputs", PATH)
assert SPEC and SPEC.loader
inputs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(inputs)


def pin(body: bytes) -> tuple[int, str]:
    return len(body), hashlib.sha256(body).hexdigest()


def root_owned(row: os.stat_result) -> os.stat_result:
    """Project the test's sticky temp ancestor to the protected guest ancestry."""
    values = list(row)
    values[stat.ST_UID] = values[stat.ST_GID] = 0
    if stat.S_ISDIR(row.st_mode) and stat.S_IMODE(row.st_mode) == 0o1777:
        values[stat.ST_MODE] = stat.S_IFDIR | 0o755
    return os.stat_result(values)


@contextmanager
def temporary_pins(entries: dict[str, tuple[int, str]], archives: set[str] | None = None):
    previous_pins, previous_archives = inputs.PINS, inputs.ARCHIVES
    inputs.PINS = MappingProxyType(dict(entries))
    inputs.ARCHIVES = frozenset(archives or ())
    try:
        yield
    finally:
        inputs.PINS, inputs.ARCHIVES = previous_pins, previous_archives


@contextmanager
def rooted_stat_projection():
    """Run the privileged constructor over a local tiny tree, without root."""
    actual_fstat, actual_stat = inputs.os.fstat, inputs.os.stat
    actual_lstat = inputs.Path.lstat

    def projected_fstat(fd: int):
        return root_owned(actual_fstat(fd))

    def projected_stat(path, *args, **kwargs):
        return root_owned(actual_stat(path, *args, **kwargs))

    def projected_lstat(path):
        return root_owned(actual_lstat(path))

    with patch.object(inputs.os, "fstat", side_effect=projected_fstat), \
            patch.object(inputs.os, "stat", side_effect=projected_stat), \
            patch.object(inputs.Path, "lstat", new=projected_lstat), \
            patch.object(inputs.os, "getuid", return_value=0), \
            patch.object(inputs.os, "geteuid", return_value=0):
        yield


class MeasureFdTests(unittest.TestCase):
    def test_streaming_and_retaining_are_explicit_and_preserve_fd_custody(self) -> None:
        body = b"small-role\n"
        with temporary_pins({"role": pin(body)}):
            with tempfile.NamedTemporaryFile() as stream:
                stream.write(body)
                stream.flush()
                stream.seek(0)
                before, raw = inputs.measure_fd(stream.fileno(), "role", keep=False)
                self.assertIsNone(raw)
                self.assertEqual(before, inputs.stamp(os.fstat(stream.fileno())))
                retained_before, retained = inputs.measure_fd(stream.fileno(), "role", keep=True)
                self.assertEqual(retained_before, before)
                self.assertEqual(retained, body)

    def test_same_size_digest_drift_is_refused(self) -> None:
        expected, changed = b"same-size", b"same-Xize"
        self.assertEqual(len(expected), len(changed))
        with temporary_pins({"role": pin(expected)}):
            with tempfile.NamedTemporaryFile() as stream:
                stream.write(changed)
                stream.flush()
                with self.assertRaisesRegex(ValueError, r"c2c_guest_inputs_refused:file_pin"):
                    inputs.measure_fd(stream.fileno(), "role")

    def test_non_regular_and_multi_link_descriptors_are_refused(self) -> None:
        body = b"linked\n"
        with temporary_pins({"role": pin(body)}):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                directory = os.open(root, inputs.DIR)
                try:
                    with self.assertRaisesRegex(ValueError, r"c2c_guest_inputs_refused:file_metadata"):
                        inputs.measure_fd(directory, "role")
                finally:
                    os.close(directory)
                source, alias = root / "source", root / "alias"
                source.write_bytes(body)
                os.link(source, alias)
                fd = os.open(source, inputs.RD)
                try:
                    with self.assertRaisesRegex(ValueError, r"c2c_guest_inputs_refused:file_metadata"):
                        inputs.measure_fd(fd, "role")
                finally:
                    os.close(fd)


class IntakeConstructorTests(unittest.TestCase):
    def _open_once(self, mode: int, kind: str = "file") -> None:
        body = b"intake-role\n"
        with tempfile.TemporaryDirectory() as temporary, temporary_pins({"role": pin(body)}):
            base = Path(temporary).resolve(strict=True) / "intake"
            run = "a" * 32
            base.mkdir(mode=0o755)
            target = base / run
            target.mkdir(mode=0o700)
            leaf = target / "role"
            if kind == "file":
                leaf.write_bytes(body)
                leaf.chmod(mode)
            else:
                leaf.mkdir(mode=mode)
            original_base = inputs.BASE
            inputs.BASE = base
            try:
                with rooted_stat_projection():
                    with inputs.open_intake(run) as (archives, payloads, current):
                        self.assertEqual(dict(archives), {})
                        self.assertEqual(payloads, {"role": body})
                        self.assertTrue(current())
            finally:
                inputs.BASE = original_base

    def test_constructor_accepts_exact_flat_root_owned_role(self) -> None:
        self._open_once(0o400)

    def test_constructor_refuses_wrong_mode_and_wrong_type_before_payload_return(self) -> None:
        for label, mode, kind in (("mode", 0o600, "file"), ("type", 0o700, "directory")):
            with self.subTest(label=label):
                with self.assertRaisesRegex(ValueError, r"c2c_guest_inputs_refused:intake_leaf"):
                    self._open_once(mode, kind)


class ExplicitFixtureTests(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("C2C_GUEST_INPUTS_EVIDENCE"), "requires retained intake evidence")
    def test_explicit_fixture_streams_every_measured_role_without_retaining_payloads(self) -> None:
        root = Path(os.environ["C2C_GUEST_INPUTS_EVIDENCE"])
        self.assertTrue(root.is_absolute() and root.resolve(strict=True) == root)
        application = root / "api4c-build-b5065ce51b837250"
        portal = root / "jobs-8e971541-static-85c8691f2da7"
        browser = root / "jobs-linux-x64-runtime-20261116-xuUXoUk7"
        pg = root / "parallel-drafts-phase657/parent-bootstrap/phase660-pgdg-noble-intake-v1/capsule"
        paths = {
            "launcher.sh": HERE.parents[1] / "scripts/run-bluey-tests.sh",
            "browser-inventory.json": root / "jobs-c2c-browser-inert-manifest-Auv0nTdj/inventory.json",
            "playwright.tgz": root / "jobs-f9-playwright-core-1.61.1/playwright-core-1.61.1.tgz",
            "chrome.zip": browser / "chrome-linux64.zip",
            "node.tar.xz": browser / "node-v22.23.4-linux-x64.tar.xz",
            "native-receipt.json": application / "receipt.json",
            "resolver.json": application / "resolver.json",
            "bluey-server": application / "bluey-server",
            "jobs-nonprod-browser-fixture": application / "jobs-nonprod-browser-fixture",
            "portal.tar.gz": portal / "static-roles.tar.gz",
            "portal-manifest.json": portal / "static-manifest.json",
            "portal-receipt.json": portal / "receipt.json",
            "postgres.tar.gz": pg / "noble-pg17-amd64.tar.gz",
            "postgres-files.json": pg / "files.json",
        }
        self.assertEqual(set(paths), set(inputs.PINS))
        for name, path in paths.items():
            with self.subTest(role=name):
                self.assertTrue(path.is_file() and path.resolve(strict=True) == path)
                fd = os.open(path, inputs.RD)
                try:
                    _, retained = inputs.measure_fd(fd, name, keep=False)
                    self.assertIsNone(retained)
                finally:
                    os.close(fd)


if __name__ == "__main__":
    unittest.main()
