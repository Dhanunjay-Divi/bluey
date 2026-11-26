#!/usr/bin/env python3
"""Portable C2C source-packet controls; never extract or execute packet members."""

import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import stat
import tarfile
import unittest


HERE = Path(__file__).resolve().parent
PACKET_MODULE = HERE / "local-qa-linux-c2c-code-packet.py"
REFUSED = r"c2c_code_packet_refused:"
PROTOCOL = "local-qa-linux-c2c-release-protocol.py"
PROTOCOL_SHA256 = "850b48cd7e36778d7c1be4d4eaf8f5ead5f00eadd90f0ea18a42db366c5a57ec"


def required_base_inventory():
    value = os.environ.get("C2C_CODE_BASE_INVENTORY")
    if not value:
        raise RuntimeError("C2C_CODE_BASE_INVENTORY must name the explicit base inventory")
    path = Path(value)
    if not path.is_absolute():
        raise RuntimeError("C2C_CODE_BASE_INVENTORY must be absolute")
    resolved = path.resolve(strict=True)
    info = resolved.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        raise RuntimeError("C2C_CODE_BASE_INVENTORY must be one regular file")
    return resolved


BASE_INVENTORY = required_base_inventory()
MODULE = runpy.run_path(str(PACKET_MODULE), run_name="_portable_c2c_code_packet_test")
BASE_RAW = BASE_INVENTORY.read_bytes()


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def rewritten_archive(packet, *, first_name=None, first_type=None, linkname="", extra=None):
    output = io.BytesIO()
    with tarfile.open(fileobj=io.BytesIO(packet), mode="r:") as source:
        with tarfile.open(fileobj=output, mode="w", format=tarfile.USTAR_FORMAT) as target:
            for index, entry in enumerate(source):
                body = source.extractfile(entry).read() if entry.isfile() else b""
                clone = tarfile.TarInfo(first_name if index == 0 and first_name else entry.name)
                clone.mode = entry.mode
                clone.uid, clone.gid, clone.mtime = entry.uid, entry.gid, entry.mtime
                clone.type = first_type if index == 0 and first_type else entry.type
                clone.linkname = linkname if index == 0 and first_type else entry.linkname
                clone.size = len(body) if clone.type == tarfile.REGTYPE else 0
                target.addfile(clone, io.BytesIO(body) if clone.type == tarfile.REGTYPE else None)
            if extra is not None:
                entry = tarfile.TarInfo(extra)
                entry.mode, entry.size = 0o444, 1
                target.addfile(entry, io.BytesIO(b"x"))
    return output.getvalue()


class CodePacketTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.first = MODULE["build_packet"](HERE, BASE_RAW)
        cls.second = MODULE["build_packet"](HERE, BASE_RAW)
        cls.packet_sha256 = sha(cls.first)

    def refused(self, function, *args):
        with self.assertRaisesRegex(ValueError, REFUSED):
            function(*args)

    def test_two_builds_are_exact_deterministic_ustar_packets(self):
        self.assertEqual(self.first, self.second)
        self.assertLessEqual(len(self.first), 1 << 20)
        self.assertEqual(sha(self.first), self.packet_sha256)
        with tarfile.open(fileobj=io.BytesIO(self.first), mode="r:") as archive:
            rows = archive.getmembers()
        self.assertTrue(rows)
        self.assertTrue(all(row.uid == row.gid == row.mtime == 0 for row in rows))
        self.assertTrue(all(row.isfile() and not row.linkname and not row.pax_headers for row in rows))

    def test_verified_roles_have_exact_source_bytes_and_one_protocol_overlap(self):
        actor, manager, receipt = MODULE["verify_packet"](self.first, self.packet_sha256)
        base_names = [row["name"] for row in json.loads(BASE_RAW)["files"]]
        self.assertEqual(set(actor), set(base_names) | {
            "inventory.json", "local-qa-linux-c2c-pg-capsule.mjs", "local-qa-linux-c2c-pg-command.mjs",
        })
        self.assertEqual((len(actor), receipt["actorFiles"]), (32, 31))
        self.assertEqual((len(manager), receipt["managerFiles"]), (11, 11))
        self.assertTrue(all(name.endswith(".py") for name in manager))
        self.assertEqual(set(actor) & set(manager), {PROTOCOL})
        self.assertEqual(sha(actor[PROTOCOL]), PROTOCOL_SHA256)
        self.assertEqual(actor[PROTOCOL], manager[PROTOCOL])

        for name, raw in actor.items():
            if name != "inventory.json":
                with self.subTest(role="actor", name=name):
                    self.assertEqual(raw, (HERE / name).read_bytes())
        for name, raw in manager.items():
            with self.subTest(role="manager", name=name):
                self.assertEqual(raw, (HERE / name).read_bytes())
                self.assertEqual(sha(raw), MODULE["MANAGER"][name])

        inventory = json.loads(actor["inventory.json"])
        self.assertEqual(inventory["version"], 1)
        self.assertEqual(len(inventory["files"]), 31)
        protocol = next(row for row in inventory["files"] if row["name"] == PROTOCOL)
        self.assertEqual((protocol["bytes"], protocol["sha256"]),
                         (len(actor[PROTOCOL]), PROTOCOL_SHA256))
        self.assertEqual(receipt, {
            "schema": "bluey-jobs-c2c-code-packet-v1",
            "packetSha256": self.packet_sha256,
            "sourceTree": MODULE["SOURCE_TREE"],
            "sourceInventorySha256": MODULE["SOURCE_INVENTORY"],
            "priorActorInventorySha256": MODULE["BASE_INVENTORY_SHA"],
            "codeInventorySha256": sha(actor["inventory.json"]),
            "actorFiles": 31,
            "managerFiles": 11,
            "releaseAdmitted": False,
            "runtimeQualified": False,
        })

    def test_changed_base_or_outer_digest_is_refused(self):
        changed = bytearray(BASE_RAW)
        marker = changed.index(b'"sha256":"') + len(b'"sha256":"')
        changed[marker] = ord("9") if changed[marker] != ord("9") else ord("8")
        self.assertEqual(len(changed), len(BASE_RAW))
        self.refused(MODULE["build_packet"], HERE, bytes(changed))
        self.refused(MODULE["verify_packet"], self.first, "9" * 64)

    def test_actor_delta_is_restricted_to_protocol_and_pg_consumer_closure(self):
        source = {row["name"]: (row["bytes"], row["sha256"])
                  for row in json.loads(BASE_RAW)["files"]}
        bound = MODULE["actor_rows"](BASE_RAW)
        self.assertEqual({name for name in source if source[name] != bound[name]}, {
            PROTOCOL, "local-qa-linux-c2c-native.mjs", "local-qa-linux-owned-api-session.mjs",
            "local-qa-linux-pg-runtime.mjs", "local-qa-linux-pg-tools.mjs", "local-qa-linux-postmaster-pidfd.py",
        })
        self.assertEqual(list(bound), sorted(bound))

    def test_extra_or_dangerous_names_and_links_are_refused_without_extraction(self):
        variants = {
            "extra": rewritten_archive(self.first, extra="extra.py"),
            "parent-traversal": rewritten_archive(self.first, first_name="../escape"),
            "absolute": rewritten_archive(self.first, first_name="/absolute"),
            "symlink": rewritten_archive(
                self.first, first_type=tarfile.SYMTYPE, linkname="../../outside",
            ),
            "hardlink": rewritten_archive(
                self.first, first_type=tarfile.LNKTYPE, linkname="actor/inventory.json",
            ),
        }
        for name, packet in variants.items():
            with self.subTest(name=name):
                self.refused(MODULE["verify_packet"], packet, sha(packet))

    def test_trailing_bytes_and_same_size_member_drift_are_refused(self):
        trailing = self.first + b"unexpected-trailing-bytes"
        self.refused(MODULE["verify_packet"], trailing, sha(trailing))

        with tarfile.open(fileobj=io.BytesIO(self.first), mode="r:") as archive:
            entry = archive.getmember("actor/" + PROTOCOL)
        drift = bytearray(self.first)
        drift[entry.offset_data] ^= 1
        self.assertEqual(len(drift), len(self.first))
        self.refused(MODULE["verify_packet"], bytes(drift), sha(drift))

    def test_bad_packet_shapes_never_return_role_bytes(self):
        for raw, digest in (
            (bytearray(self.first), self.packet_sha256),
            (b"", sha(b"")),
            (self.first, 9),
            (self.first, "0" * 64),
        ):
            with self.subTest(raw_type=type(raw).__name__, digest=digest):
                self.refused(MODULE["verify_packet"], raw, digest)


if __name__ == "__main__":
    unittest.main()
