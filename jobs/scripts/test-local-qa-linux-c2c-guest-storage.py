"""Portable controls for read-only C2C guest storage admission."""
import hashlib
import importlib.util
import os
from pathlib import Path
import stat
import unittest


HERE = Path(__file__).resolve().parent
SOURCE = HERE / "local-qa-linux-c2c-guest-storage.py"
SPEC = importlib.util.spec_from_file_location("guest_storage", SOURCE)
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)
FIXTURE_SHA = "7db5f84f3423aabba40c68721d02d679d024b31d1db63d18ba70887463dba1f7"


def line(mid, root, point, *, device="253:1", source="/dev/vda1", optional="", options="rw,nosuid,nodev"):
    extra = (" " + optional) if optional else ""
    return f"{mid} 32 {device} {root} {point} {options}{extra} - ext4 {source} rw\n"


def positive_mountinfo():
    return (line(32, "/", "/", options="rw,relatime")
            + line(101, "/var/lib/bluey-jobs-c2c-runtime/browser", "/run/bluey-jobs-c2c-browser")
            + line(102, "/var/lib/bluey-jobs-c2c-runtime/application", "/run/bluey-jobs-c2c-application")
            + line(103, "/var/lib/bluey-jobs-c2c-runtime/postgres", "/run/bluey-jobs-c2c-postgres")
            + line(104, "/", "/var/lib/bluey-jobs-c2c-intake")).encode("ascii")


def anchors():
    result = {}
    for index, path in enumerate(M.ALL, 101):
        result[path] = {"path": path, "st_dev": os.makedev(253, 1), "st_ino": index,
                        "mode": stat.S_IFDIR | 0o755, "uid": 0, "gid": 0, "nlink": 2,
                        "mnt_id": index, "bavail": (M.REQUIRED_BYTES + 4096) // 4096, "frsize": 4096}
    return result


def namespaces():
    return {name: f"{name}:[402653{index}]" for index, name in enumerate(M.NAMESPACES, 1000)}


class GuestStorage(unittest.TestCase):
    def refuse(self, callback, reason):
        with self.assertRaisesRegex(ValueError, "c2c_guest_storage_refused:" + reason):
            callback()

    def test_retained_run_tmpfs_is_default_negative(self):
        path = Path(os.environ["C2C_GUEST_STORAGE_MOUNTINFO"])
        raw = path.read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(), FIXTURE_SHA)
        rows = M.parse_mountinfo(raw)
        self.assertEqual(M.mounted_at(rows, "/run/bluey-jobs-c2c-browser")["mountpoint"], "/run")
        self.refuse(lambda: M.verify_guest_storage_projection(raw, anchors(), namespaces()), "payload_mount")

    def test_explicit_private_ext4_bind_roots_pass_with_one_capacity_observation(self):
        evidence = M.verify_guest_storage_projection(positive_mountinfo(), anchors(), namespaces())
        self.assertEqual(evidence["schema"], "bluey-jobs-c2c-guest-storage-v1")
        self.assertEqual(evidence["requiredBytes"], M.REQUIRED_BYTES)
        self.assertEqual(evidence["mountIds"]["/run/bluey-jobs-c2c-browser"], 101)
        self.assertRegex(evidence["topology"]["sha256"], r"^[0-9a-f]{64}$")
        self.assertFalse(evidence["runtimeQualified"])

    def test_refuses_noexec_shared_tmpfs_split_device_and_capacity(self):
        original = positive_mountinfo()
        self.refuse(lambda: M.verify_guest_storage_projection(
            original.replace(b"rw,nosuid,nodev", b"rw,nosuid,nodev,noexec", 1), anchors(), namespaces()), "payload_mount")
        self.refuse(lambda: M.verify_guest_storage_projection(
            original.replace(b"/dev/vda1 rw", b"/dev/vda1 rw", 1).replace(
                b"rw,nosuid,nodev - ext4 /dev/vda1", b"rw,nosuid,nodev shared:1 - ext4 /dev/vda1", 1),
            anchors(), namespaces()), "payload_mount")
        self.refuse(lambda: M.verify_guest_storage_projection(
            original.replace(b"103 32 253:1", b"103 32 253:2"), anchors(), namespaces()), "anchor_device")
        weak = anchors()
        for value in weak.values():
            value["bavail"] = 1
        self.refuse(lambda: M.verify_guest_storage_projection(original, weak, namespaces()), "capacity")

    def test_refuses_parent_mount_adoption_and_namespace_or_anchor_drift(self):
        raw = positive_mountinfo().replace(
            b"101 32 253:1 /var/lib/bluey-jobs-c2c-runtime/browser /run/bluey-jobs-c2c-browser",
            b"101 32 253:1 / /run")
        self.refuse(lambda: M.verify_guest_storage_projection(raw, anchors(), namespaces()), "payload_mount")
        bad_ns = namespaces()
        bad_ns.pop("net")
        self.refuse(lambda: M.verify_guest_storage_projection(positive_mountinfo(), anchors(), bad_ns), "projection_shape")
        bad = anchors()
        bad[M.INTAKE]["mode"] = stat.S_IFDIR | 0o700
        self.refuse(lambda: M.verify_guest_storage_projection(positive_mountinfo(), bad, namespaces()), "anchor_shape")

    def test_post_stage_reuses_topology_with_reserve_only_capacity(self):
        first = M.verify_guest_storage_projection(positive_mountinfo(), anchors(), namespaces())
        after = anchors()
        for value in after.values():
            value["bavail"] = M.RESERVE_BYTES // 4096
        later = M.verify_guest_storage_projection(
            positive_mountinfo(), after, namespaces(), expected_topology=first["topology"],
            minimum_available_bytes=M.RESERVE_BYTES)
        self.assertEqual(later["topology"], first["topology"])
        changed_anchors = anchors()
        for value in changed_anchors.values():
            value["bavail"] = M.RESERVE_BYTES // 4096
        changed_anchors["/run/bluey-jobs-c2c-browser"]["st_ino"] += 1
        self.refuse(lambda: M.verify_guest_storage_projection(
            positive_mountinfo(), changed_anchors, namespaces(), expected_topology=first["topology"],
            minimum_available_bytes=M.RESERVE_BYTES), "topology_changed")

    def test_refuses_non_executable_launcher_or_tmp_root(self):
        self.refuse(lambda: M.verify_guest_storage_projection(
            positive_mountinfo().replace(b"32 32 253:1 / / rw,relatime",
                                         b"32 32 253:1 / / rw,relatime,noexec"),
            anchors(), namespaces()), "exec_support_mount")


if __name__ == "__main__":
    unittest.main()
