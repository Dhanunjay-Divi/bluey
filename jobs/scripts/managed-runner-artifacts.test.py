#!/usr/bin/env python3
"""Small synthetic FD/protocol fixtures only; never boot a VM or admit a runtime."""
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import struct
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("artifact_transport", Path(__file__).with_name("managed-runner-artifacts.py"))
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def sha(value):
    return hashlib.sha256(value).hexdigest()


def encoded_frame(value):
    raw = M.canonical(value)
    return struct.pack(">I", len(raw)) + raw


def framed(name, payload, reader, source=None, trailer=None):
    source = source or {"dev": "1", "ino": "2", "uid": os.getuid(), "mode": 0o400,
                        "nlink": 1, "size": len(payload), "mtimeNs": "3", "ctimeNs": "4"}
    header = {"version": 1, "artifact": name, "size": len(payload), "source": source, "reader": reader}
    footer = trailer or {"version": 1, "artifact": name, "size": len(payload),
                         "sha256": sha(payload), "source": source}
    return encoded_frame(header) + payload + encoded_frame(footer)


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bluey-artifact-unit-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(os.path.realpath(self.temporary.name))
        self.parent = self.root / "selected"
        self.parent.mkdir(mode=0o700)
        self.base = {"parent": str(self.parent), "filesystemDev": str(self.parent.stat().st_dev),
                     "excludedRoots": [str(self.root / "launcher")],
                     "producerPath": "/unused-synthetic", "producerSha256": "a" * 64}
        self.runtime = {"synthetic": "unit-only; never CLI admission"}
        self.reader = {"synthetic": "unit-only; never guest runtime admission"}
        self.source = {"sourceCommit": "b" * 40, "sourceArchiveSha256": "c" * 64,
                       "toolSourcesSha256": "d" * 64, "harnessSha256": "e" * 64,
                       "imageToolSourcesSha256": "f" * 64}
        # Disk policy is separately exercised below; tiny synthetic tests reserve no GiBs.
        self.space_mock = patch.object(M, "space")
        self.space_mock.start()
        self.addCleanup(self.space_mock.stop)
        # Only unit fixtures may write below the canonical launcher's TMPDIR.
        # Keep that pure scope guard independently covered; no CLI/runtime bypass exists.
        self.scope_guard = M.assert_persistent_destination
        self.scope_mock = patch.object(M, "assert_persistent_destination")
        self.scope_mock.start()
        self.addCleanup(self.scope_mock.stop)

    def initialize(self):
        checked = M.host_check(self.base)
        self.binding = M.host_init({**self.base, **checked, "source": self.source}, self.runtime)
        self.child = self.parent / self.binding["childName"]
        return self.child

    def operation(self, name, extra=None, stream=None):
        return M.host_bound_operation(name, {**self.base, "binding": self.binding, **(extra or {})},
                                      self.runtime, stream or io.BytesIO())

    def complete(self):
        self.initialize()
        payloads = {"runner.oci.tar": b"opaque-synthetic-archive", "inventory.json": b"{}\n",
                    "measurement.json": b"{\"synthetic\":true}\n"}
        inspection = {"expected": {"sourceCommit": self.source["sourceCommit"]},
                      "oci": {"inventorySha256": sha(payloads["inventory.json"])},
                      "runtime": {"runtimeMeasurementSha256": sha(payloads["measurement.json"])}}
        payloads["report.json"] = M.canonical(inspection) + b"\n"
        for name in M.ORDER:
            result = self.operation("host-receive", {"artifact": name,
                "expected": {"reader": self.reader, "owner": os.getuid(), "sha256": sha(payloads[name])}},
                io.BytesIO(framed(name, payloads[name], self.reader)))
            self.assertEqual(result["sha256"], sha(payloads[name]))
            self.assertEqual((self.child / name).read_bytes(), payloads[name])
        saved = self.operation("host-manifest", {"sha256s": {name: sha(value) for name, value in payloads.items()},
                                                "reader": self.reader, "inspection": inspection})
        return saved

    def proof(self):
        return {"sourceCommit": self.source["sourceCommit"], "imageReportSha256": "1" * 64,
                "harnessSha256": self.source["harnessSha256"],
                "toolSourcesSha256": self.source["imageToolSourcesSha256"],
                "postListenerChecked": True, "sourceUnchanged": True,
                "cleanup": {"privateInventoriesEmpty": True, "ownedVmPidsAbsent": True,
                            "socketAbsent": True, "stateAbsent": True, "usernetSocketsAbsent": True}}

    def test_complete_export_is_unqualified_until_separate_cleanup_proof(self):
        saved = self.complete()
        manifest = (self.child / "export-manifest.json").read_bytes()
        self.assertIn(b'"qualified":false', manifest)
        self.assertFalse((self.child / "qualification.json").exists())
        receipt = self.operation("host-qualify", {"manifestSha256": saved["sha256"], "qualification": self.proof()})
        self.assertEqual(receipt["name"], "qualification.json")
        value = M.decode((self.child / "qualification.json").read_bytes(), 65536)
        self.assertFalse(value["d2Authority"])
        self.assertFalse(value["releaseAuthority"])
        self.assertTrue(value["externalTerminalZeroRequired"])
        with self.assertRaises(M.Refusal):
            self.operation("host-qualify", {"manifestSha256": saved["sha256"], "qualification": self.proof()})

    def test_failed_cleanup_or_source_binding_cannot_finalize(self):
        saved = self.complete()
        for change in ["cleanup", "sourceUnchanged", "postListenerChecked", "harnessSha256", "toolSourcesSha256"]:
            proof = self.proof()
            if change == "cleanup":
                proof["cleanup"]["stateAbsent"] = False
            elif change.endswith("Sha256"):
                proof[change] = "9" * 64
            else:
                proof[change] = False
            with self.subTest(change=change), self.assertRaises(M.Refusal):
                self.operation("host-qualify", {"manifestSha256": saved["sha256"], "qualification": proof})
        self.assertFalse((self.child / "qualification.json").exists())

    def test_retained_directory_fd_stays_on_original_after_parent_rename(self):
        self.initialize()
        directory = os.open(str(self.child), M.DIRECTORY)
        try:
            moved = self.root / "moved"
            self.parent.rename(moved)
            self.parent.mkdir(mode=0o700)
            M.record(directory, "fd-relative-proof.json", {"safe": True})
            self.assertEqual(list(self.parent.iterdir()), [])
            self.assertTrue((moved / self.binding["childName"] / "fd-relative-proof.json").is_file())
            with self.assertRaises(M.Refusal):
                M.bound_directories({**self.base, "binding": self.binding})
        finally:
            os.close(directory)

    def test_parent_scope_mode_filesystem_empty_and_alias_are_enforced(self):
        for path in ["/private/tmp/bluey-test-runs-fixture/run.1.2.x/artifacts",
                     "/tmp/bluey-jobs-linux-p639-abcdef/artifacts"]:
            with self.assertRaises(M.Refusal):
                self.scope_guard(path)
        self.scope_guard("/Users/synthetic/private-artifacts")
        for changes in [{"excludedRoots": [str(self.root)]}, {"filesystemDev": "0"}]:
            with self.assertRaises(M.Refusal):
                M.host_check({**self.base, **changes})
        self.parent.chmod(0o755)
        with self.assertRaises(M.Refusal):
            M.host_check(self.base)
        self.parent.chmod(0o700)
        (self.parent / "existing").write_bytes(b"preserve")
        with self.assertRaises(M.Refusal):
            M.host_check(self.base)
        alias = self.root / "alias"
        alias.symlink_to(self.parent, target_is_directory=True)
        with self.assertRaises(M.Refusal):
            M.host_check({**self.base, "parent": str(alias)})

    def test_binary_frames_are_exact_and_partials_remain_unqualified(self):
        variants = [lambda raw: raw[:-1], lambda raw: raw + b"extra",
                    lambda raw: b"\x00\x00\x10\x01" + raw,
                    lambda raw: raw.replace(sha(b"payload").encode(), b"0" * 64)]
        for index, mutate in enumerate(variants):
            directory_path = self.root / ("frame-%d" % index)
            directory_path.mkdir(mode=0o700)
            directory = os.open(str(directory_path), M.DIRECTORY)
            try:
                with self.assertRaises(M.Refusal):
                    M.receive(directory, "runner.oci.tar", io.BytesIO(mutate(framed("runner.oci.tar", b"payload", self.reader))),
                              {"reader": self.reader, "owner": os.getuid(), "sha256": sha(b"payload")})
                self.assertFalse((directory_path / "qualification.json").exists())
            finally:
                os.close(directory)

    def test_oversize_header_fails_before_file_creation_and_output_cannot_overwrite(self):
        self.initialize()
        directory = os.open(str(self.child), M.DIRECTORY)
        try:
            for name, cap in M.CAPS.items():
                header = {"version": 1, "artifact": name, "size": cap + 1, "reader": self.reader,
                          "source": {}}
                with self.assertRaises(M.Refusal):
                    M.receive(directory, name, io.BytesIO(encoded_frame(header)),
                              {"reader": self.reader, "owner": os.getuid(), "sha256": "a" * 64})
                self.assertFalse((self.child / name).exists())
            M.record(directory, "existing.json", {"first": True})
            with self.assertRaises(FileExistsError):
                M.record(directory, "existing.json", {"second": True})
            target = self.child / "symlink.json"
            target.symlink_to(self.child / "existing.json")
            with self.assertRaises(FileExistsError):
                M.record(directory, "symlink.json", {})
        finally:
            os.close(directory)

    def test_hardlinked_or_tampered_saved_artifact_cannot_qualify(self):
        saved = self.complete()
        os.link(self.child / "runner.oci.tar", self.child / "unexpected-hardlink")
        directory = os.open(str(self.child), M.DIRECTORY)
        try:
            with self.assertRaises(M.Refusal):
                M.readback(directory, "runner.oci.tar", M.CAPS["runner.oci.tar"])
        finally:
            os.close(directory)
        with self.assertRaises(M.Refusal):
            self.operation("host-qualify", {"manifestSha256": saved["sha256"], "qualification": self.proof()})

    def test_readback_failure_keeps_partial_without_manifest(self):
        self.initialize()
        directory = os.open(str(self.child), M.DIRECTORY)
        try:
            with patch.object(M, "readback", side_effect=M.Refusal("artifact_readback")):
                with self.assertRaises(M.Refusal):
                    M.receive(directory, "runner.oci.tar", io.BytesIO(framed("runner.oci.tar", b"payload", self.reader)),
                              {"reader": self.reader, "owner": os.getuid(), "sha256": sha(b"payload")})
            self.assertEqual((self.child / "runner.oci.tar").read_bytes(), b"payload")
            self.assertFalse((self.child / "export-manifest.json").exists())
        finally:
            os.close(directory)

    def test_space_policy_leaves_floor_plus_full_worst_case(self):
        self.space_mock.stop()
        class Space:
            f_frsize = 1
            f_bavail = M.FLOOR + M.RESERVE - 1
        with patch.object(M.os, "fstatvfs", return_value=Space()):
            with self.assertRaises(M.Refusal):
                M.space(123)
            Space.f_bavail += 1
            M.space(123)
        self.space_mock.start()

    def test_guest_reader_hashes_the_one_open_fd_and_rejects_source_drift(self):
        source = self.root / "source"
        source.write_bytes(b"payload")
        source.chmod(0o400)
        output = io.BytesIO()
        with patch.dict(M.SOURCE_PATHS, {"runner.oci.tar": str(source)}), \
                patch.object(M, "guest_runtime", return_value=self.reader):
            M.guest_stream("runner.oci.tar", {}, self.reader, sha(b"payload"), os.getuid(), output)
            stream = io.BytesIO(output.getvalue())
            header = M.read_frame(stream)
            self.assertEqual(stream.read(header["size"]), b"payload")
            self.assertEqual(M.read_frame(stream)["sha256"], sha(b"payload"))
            self.assertEqual(stream.read(), b"")
            alias = self.root / "source-alias"
            alias.symlink_to(source)
            with patch.dict(M.SOURCE_PATHS, {"runner.oci.tar": str(alias)}), self.assertRaises(OSError):
                M.guest_stream("runner.oci.tar", {}, self.reader, sha(b"payload"), os.getuid(), io.BytesIO())
            class ReplacePath(io.BytesIO):
                def __init__(inner):
                    super().__init__()
                    inner.calls = 0

                def write(inner, value):
                    count = super().write(value)
                    inner.calls += 1
                    if inner.calls == 2:
                        source.rename(self.root / "original-fd")
                        source.write_bytes(b"changed")
                        source.chmod(0o400)
                    return count
            with self.assertRaisesRegex(M.Refusal, "artifact_source_changed"):
                M.guest_stream("runner.oci.tar", {}, self.reader, sha(b"payload"), os.getuid(), ReplacePath())

    def test_control_metadata_is_ascii_canonical_closed_and_bounded(self):
        for raw in [b'{"x":1,"x":1}', b'{ "x":1}', b'{}\n', b'\xff', b'x' * 4097]:
            with self.subTest(raw=raw[:30]), self.assertRaises((M.Refusal, ValueError)):
                M.decode(raw, 4096)
        with self.assertRaises(M.Refusal):
            M.keys({"x": 1, "unexpected": 2}, ("x",))

    def failed_initialize(self, payload=b"opaque-synthetic-failed-archive"):
        # Portable metadata fixtures only; never an actual guest approval/probe.
        self.reader = {"runtime": {"canonicalPath": "/usr/bin/python3.12", "version": "3.12.3",
            "platform": "Linux", "architecture": "x86_64", "uid": 0, "mode": 0o755,
            "nlink": 1, "size": 12345, "sha256": "b" * 64, "protocolVersion": 1},
            "continuity": {"dev": "1", "ino": "2"}, "readerSourceSha256": "a" * 64}
        self.frozen_source = {"dev": "1", "ino": "2", "uid": max(1, os.getuid()), "mode": 0o400,
            "nlink": 1, "size": len(payload), "mtimeNs": "3", "ctimeNs": "4"}
        self.descriptor = {"version": 1, "artifact": "runner.oci.tar", "size": len(payload),
            "sha256": sha(payload), "source": self.frozen_source, "reader": self.reader}
        checked = M.host_check(self.base)
        self.binding = M.host_init({**self.base, **checked, "source": self.source,
            "descriptor": self.descriptor}, self.runtime, failed=True)
        self.child = self.parent / self.binding["childName"]
        return payload

    def failed_operation(self, operation, extra=None, stream=None):
        return M.host_failed_operation(operation, {**self.base, "binding": self.binding, **(extra or {})},
            self.runtime, stream or io.BytesIO())

    def failed_receive(self, payload):
        return self.failed_operation("host-failure-receive", stream=io.BytesIO(
            framed("runner.oci.tar", payload, self.reader, self.frozen_source)))

    def failed_checks(self):
        return {"postObservation": True, "sourceUnchanged": True, "normalCleanup": True}

    def test_failed_archive_has_only_three_unqualified_files_and_no_positive_finalizer(self):
        payload = self.failed_initialize()
        received = self.failed_receive(payload)
        saved = self.failed_operation("host-failure-manifest", {"received": received, "checks": self.failed_checks()})
        self.assertEqual(sorted(path.name for path in self.child.iterdir()),
            ["failure-retention-manifest.json", "failure-retention-started.json", "runner.oci.tar"])
        self.assertEqual((self.child / "runner.oci.tar").read_bytes(), payload)
        manifest = M.decode((self.child / saved["name"]).read_bytes(), 65536)
        self.assertEqual(manifest["state"], "failed_archive_retained")
        for field in ("qualified", "d2Authority", "releaseAuthority"):
            self.assertIs(manifest[field], False)
        self.assertEqual(manifest["descriptor"], self.descriptor)
        self.assertEqual(manifest["file"], received)
        for operation in ("host-receive", "host-manifest", "host-qualify"):
            with self.subTest(operation=operation), self.assertRaisesRegex(M.Refusal, "artifact_child_name"):
                self.operation(operation)
        self.assertFalse((self.child / "qualification.json").exists())
        other = self.root / "positive"
        other.mkdir(mode=0o700)
        request = {**self.base, "parent": str(other)}
        positive = M.host_init({**request, **M.host_check(request), "source": self.source}, self.runtime)
        with self.assertRaisesRegex(M.Refusal, "artifact_child_name"):
            M.host_failed_operation("host-failure-receive", {**request, "binding": positive},
                self.runtime, io.BytesIO())

    def test_failed_archive_needs_all_post_source_cleanup_checks_before_manifest(self):
        payload = self.failed_initialize()
        received = self.failed_receive(payload)
        for field in self.failed_checks():
            checks = self.failed_checks()
            checks[field] = False
            with self.subTest(field=field), self.assertRaisesRegex(M.Refusal, "artifact_failed_checks"):
                self.failed_operation("host-failure-manifest", {"received": received, "checks": checks})
            self.assertFalse((self.child / "failure-retention-manifest.json").exists())

    def test_failed_archive_extra_attachment_is_not_a_three_file_retention(self):
        payload = self.failed_initialize()
        received = self.failed_receive(payload)
        (self.child / "inventory.json").write_bytes(b"synthetic-only")
        with self.assertRaisesRegex(M.Refusal, "artifact_child_entries"):
            self.failed_operation("host-failure-manifest", {"received": received, "checks": self.failed_checks()})
        self.assertFalse((self.child / "failure-retention-manifest.json").exists())

    def test_failed_archive_space_refusal_precedes_child_creation(self):
        self.failed_initialize()
        parent = self.root / "space-refusal"
        parent.mkdir(mode=0o700)
        request = {**self.base, "parent": str(parent)}
        checked = M.host_check(request)
        with patch.object(M, "space", side_effect=M.Refusal("artifact_free_space")):
            with self.assertRaisesRegex(M.Refusal, "artifact_free_space"):
                M.host_init({**request, **checked, "source": self.source,
                    "descriptor": self.descriptor}, self.runtime, failed=True)
        self.assertEqual(list(parent.iterdir()), [])

    def test_failed_archive_replacement_even_with_equal_bytes_cannot_finalize(self):
        payload = self.failed_initialize()
        received = self.failed_receive(payload)
        (self.child / "runner.oci.tar").rename(self.root / "preserved-original")
        (self.child / "runner.oci.tar").write_bytes(payload)
        (self.child / "runner.oci.tar").chmod(0o600)
        with self.assertRaisesRegex(M.Refusal, "artifact_output_changed"):
            self.failed_operation("host-failure-manifest", {"received": received, "checks": self.failed_checks()})
        self.assertFalse((self.child / "failure-retention-manifest.json").exists())

    def test_failed_source_mismatch_and_bad_trailer_never_complete_partial_bytes(self):
        payload = self.failed_initialize()
        changed = {**self.frozen_source, "ino": "9"}
        with self.assertRaisesRegex(M.Refusal, "artifact_frozen_source_changed"):
            self.failed_operation("host-failure-receive", stream=io.BytesIO(
                framed("runner.oci.tar", payload, self.reader, changed)))
        self.assertFalse((self.child / "runner.oci.tar").exists())
        trailer = {"version": 1, "artifact": "runner.oci.tar", "size": len(payload),
            "sha256": "f" * 64, "source": self.frozen_source}
        with self.assertRaisesRegex(M.Refusal, "artifact_trailer"):
            self.failed_operation("host-failure-receive", stream=io.BytesIO(
                framed("runner.oci.tar", payload, self.reader, self.frozen_source, trailer)))
        self.assertEqual((self.child / "runner.oci.tar").read_bytes(), payload)
        self.assertFalse((self.child / "failure-retention-manifest.json").exists())

    def test_failed_archive_ambiguous_readback_keeps_bytes_without_manifest(self):
        payload = self.failed_initialize()
        original = M.readback
        def refuse_archive(directory, name, cap, expected_sha=None):
            if name == "runner.oci.tar":
                raise M.Refusal("artifact_readback")
            return original(directory, name, cap, expected_sha)
        with patch.object(M, "readback", side_effect=refuse_archive):
            with self.assertRaises(M.Refusal):
                self.failed_receive(payload)
        self.assertEqual((self.child / "runner.oci.tar").read_bytes(), payload)
        self.assertFalse((self.child / "failure-retention-manifest.json").exists())

    def test_failed_descriptor_rejects_nonarchive_open_metadata_and_unbound_identity(self):
        self.failed_initialize()
        descriptor = self.descriptor
        for patch_value in ({"artifact": "report.json"}, {"size": 0}, {"size": 4 * 1024 ** 3 + 1},
                            {"sha256": "0" * 64}, {"path": "private-value"}):
            with self.subTest(patch=patch_value), self.assertRaises(M.Refusal):
                M.validate_failed_descriptor({**descriptor, **patch_value})
        for patch_value in ({"mode": 0o600}, {"nlink": 2}, {"size": 1}, {"ino": "0"}, {"uid": True}):
            with self.subTest(patch=patch_value), self.assertRaises(M.Refusal):
                M.validate_failed_descriptor({**descriptor, "source": {**self.frozen_source, **patch_value}})

    def test_guest_describe_binds_one_fd_and_failure_stream_refuses_changed_frozen_source(self):
        source = self.root / "descriptor-source"
        source.write_bytes(b"synthetic")
        source.chmod(0o400)
        with patch.dict(M.SOURCE_PATHS, {"runner.oci.tar": str(source)}), \
                patch.object(M, "guest_runtime", return_value=self.reader):
            if os.getuid() == 0:
                with self.assertRaises(M.Refusal):
                    M.guest_archive_descriptor({}, self.reader, 0)
                return
            descriptor = M.guest_archive_descriptor({}, self.reader, os.getuid())
            self.assertEqual(descriptor["sha256"], sha(b"synthetic"))
            self.assertEqual(descriptor["source"], M.identity(source.stat()))
            output = io.BytesIO()
            source.rename(self.root / "original-descriptor-source")
            source.write_bytes(b"synthetic")
            source.chmod(0o400)
            with self.assertRaisesRegex(M.Refusal, "artifact_frozen_source_changed"):
                M.guest_stream("runner.oci.tar", {}, self.reader, descriptor["sha256"], os.getuid(),
                    output, descriptor["source"])
            self.assertEqual(output.getvalue(), b"")
            source.chmod(0o600)
            with self.assertRaises(M.Refusal):
                M.guest_archive_descriptor({}, self.reader, os.getuid())


class DiscoveryTests(unittest.TestCase):
    """Synthetic bootstrap observations; no Linux or approval claim follows."""

    def setUp(self):
        self.source = "a" * 64
        self.runtime = {"canonicalPath": "/usr/bin/python3.12", "version": "3.12.3",
                        "platform": "Linux", "architecture": "x86_64", "uid": 0,
                        "mode": 0o755, "nlink": 1, "size": 12345, "sha256": "b" * 64,
                        "protocolVersion": 1}
        self.system = SimpleNamespace(executable="/usr/bin/python3", flags=SimpleNamespace(
            isolated=1, no_site=1, dont_write_bytecode=1, ignore_environment=1))
        self.stat = SimpleNamespace(st_mode=M.stat.S_IFREG | 0o500, st_uid=0,
                                    st_nlink=1, st_size=123)
        patches = [patch.object(M, "sys", self.system),
                   patch.object(M.os, "getuid", return_value=0),
                   patch.object(M.os, "geteuid", return_value=0),
                   patch.object(M.os.path, "realpath", side_effect=lambda path:
                       M.READER_PATH if path == M.__file__ else
                       self.runtime["canonicalPath"] if path == "/usr/bin/python3" else path),
                   patch.object(M.os, "lstat", return_value=self.stat),
                   patch.object(M, "runtime_file", return_value=(self.runtime, {"dev": "1", "ino": "2"})),
                   patch.object(M, "source_digest", return_value=self.source)]
        self.mocks = [item.start() for item in patches]
        for item in patches:
            self.addCleanup(item.stop)

    def test_discovery_returns_portable_unqualified_candidate_only(self):
        value = M.guest_discover({"readerSourceSha256": self.source})
        self.assertEqual(value, {"version": 1, "qualified": False,
                         "candidate": {"runtime": self.runtime, "readerSourceSha256": self.source},
                         "claims": {"image": False, "runnerStartup": False, "release": False}})
        self.assertNotIn("continuity", value["candidate"])
        self.mocks[5].assert_called_once_with("/usr/bin/python3.12")
        self.mocks[6].assert_called_once_with(M.READER_PATH)

    def test_discovery_refuses_nonfixed_bootstrap_nonroot_and_missing_isolation(self):
        for executable in ["/tmp/python3", "/usr/bin/python3.12", "python3"]:
            self.system.executable = executable
            with self.subTest(executable=executable), self.assertRaises(M.Refusal):
                M.guest_discover({"readerSourceSha256": self.source})
        self.system.executable = "/usr/bin/python3"
        for name in ["isolated", "no_site", "dont_write_bytecode", "ignore_environment"]:
            setattr(self.system.flags, name, 0)
            with self.subTest(flag=name), self.assertRaises(M.Refusal):
                M.guest_discover({"readerSourceSha256": self.source})
            setattr(self.system.flags, name, 1)
        for getter in ["getuid", "geteuid"]:
            with patch.object(M.os, getter, return_value=501), self.assertRaises(M.Refusal):
                M.guest_discover({"readerSourceSha256": self.source})
        with patch.object(M.os.path, "realpath", return_value="/tmp/python3.12"), self.assertRaises(M.Refusal):
            M.guest_discover({"readerSourceSha256": self.source})

    def test_discovery_refuses_unbound_source_and_nonportable_runtime(self):
        for request in [{}, {"readerSourceSha256": "x"},
                        {"readerSourceSha256": self.source, "approve": True}]:
            with self.subTest(request=request), self.assertRaises(M.Refusal):
                M.guest_discover(request)
        with patch.object(M, "source_digest", return_value="c" * 64), self.assertRaises(M.Refusal):
            M.guest_discover({"readerSourceSha256": self.source})
        for change in [{"platform": "Darwin"}, {"architecture": "aarch64"}, {"uid": 501},
                       {"mode": 0o777}, {"nlink": 2}, {"size": 0}, {"sha256": "bad"},
                       {"protocolVersion": 2}, {"canonicalPath": "/tmp/python3.12"}]:
            with patch.object(M, "runtime_file", return_value=({**self.runtime, **change}, {})), \
                    self.subTest(change=change), self.assertRaises(M.Refusal):
                M.guest_discover({"readerSourceSha256": self.source})

    def test_discovery_refuses_reader_owner_mode_links_type_and_size_drift(self):
        for change in [{"st_uid": 501}, {"st_mode": M.stat.S_IFREG | 0o755},
                       {"st_mode": M.stat.S_IFREG | 0o777}, {"st_nlink": 2},
                       {"st_mode": M.stat.S_IFDIR | 0o500}, {"st_size": 0},
                       {"st_size": 128 * 1024 + 1}]:
            observed = SimpleNamespace(**{**vars(self.stat), **change})
            with patch.object(M.os, "lstat", return_value=observed), \
                    self.subTest(change=change), self.assertRaises(M.Refusal):
                M.guest_discover({"readerSourceSha256": self.source})


if __name__ == "__main__":
    unittest.main()
