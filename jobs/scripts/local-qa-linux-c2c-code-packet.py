"""Seal and verify the exact C2C actor and privileged manager source packet.

Only data is read or returned. The receiver must stage actor members separately
from manager-only members; this module does not extract or execute the tar.
"""
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import tarfile


SOURCE_TREE = "8e971541d3ab8d1b8f5c4c9eebafa8fec81406d5"
SOURCE_INVENTORY = "a7dd3071bd03f53718ceb9db8b4de9571fe0aa1a6ef6e29f840a2b20e6a89ea5"
BASE_INVENTORY_SHA = "f1bbf6fff7f1230f52efc5d51e83c1170e9f299f3958907c17105bebfb8c3f7a"
BASE_MEMBER = "prior-actor-inventory.json"
PROTOCOL = "local-qa-linux-c2c-release-protocol.py"
BASE_ACTOR_COUNT = 29
FILE_LIMIT = 128 << 10
PACKET_LIMIT = 1 << 20
NAME = re.compile(r"local-qa-[a-z0-9-]{1,96}\.(?:py|mjs)")
SHA = re.compile(r"[0-9a-f]{64}")
MANAGER = {
    "interview-prep-linux-closure.py": "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4",
    "local-qa-linux-c2c-guest-manager.py": "61e026cf2e5216e44a0a3a06893f9de45824581996a05f51768824b4b3d94256",
    "local-qa-linux-c2c-guest-teardown.py": "4cc2c01f50f4d83f4ec702036645ed8769052a24f78f6af9bb84cd1c1a9987cd",
    "local-qa-linux-c2c-launch-custody.py": "49c887c278408e057005940d9b97ce9579279938c81170bf4780bb9feb22cfca",
    "local-qa-linux-c2c-manager-observer.py": "57d60374c15596bfe0594b707892f7a847c3c5a459da6564778525729c886530",
    PROTOCOL: "850b48cd7e36778d7c1be4d4eaf8f5ead5f00eadd90f0ea18a42db366c5a57ec",
    "local-qa-linux-c2c-release-writer.py": "4a9a08727453f2b8e658b1c15fd5a7544f3e691c4d9db37e0ad3c4fdfaf09cda",
    "local-qa-linux-c2c-stage-actor-inputs.py": "73719c3608fd1b50a35503e828ce9a5f7adc48ce1a9690af92817f716ae30c13",
    "receipt-browser-manager-stop.py": "b41eeb361e5511885995b1318c08beec22fc7699d41f67a915e4a8012ba4a1e9",
    "receipt-browser-post-stop.py": "26a34e0cb65ad3d34d2d72b3cfb9ae58f280fcb99fa3ff968ae8bce5f3d46a48",
    "receipt-browser-release-writer.py": "8aeeba6bb06e37976201ea4b88c8a532ab6dbb8ab4bc56615fb7e8c3ad1d6014",
}
PROTOCOL_DELTA = (6120, "dd271a0f23712c34650d6e311f57e11fe3baf2e296e659efa784f9afb1766114", 6310)
PG_REPLACEMENTS = {
    "local-qa-linux-c2c-native.mjs": (
        (10137, "6f9004d49ae084e9b02d6c38a8ba98a4517362a8cfa58847c65f753ba49c968b"),
        (10130, "e296a99ca9f127434d9bf870c56693c0a167658fa549f6013db977d32a98bbf9"),
    ),
    "local-qa-linux-owned-api-session.mjs": (
        (18172, "fd4e22b298a6633d65972a5fe9e443c51e1ca41b11050c9c17ac0f7da2c2e860"),
        (18172, "3830b623326add12d9da1e7b968c861f96cd2aed09a3489c418e3fb37fd3b798"),
    ),
    "local-qa-linux-pg-runtime.mjs": (
        (16265, "2d7436392dc730a0e507088e9d606fc5f16b5273a9123d70ddfb889f1b6272a9"),
        (16573, "f8538cb5bc3490eaafd4d60ef247ba7717791a37bd3eb15322638d21c9ec28e7"),
    ),
    "local-qa-linux-pg-tools.mjs": (
        (14455, "c7ddf965f31890f9d9aed7a99d696f33e4981935f81730012026de0d0b38c67b"),
        (19828, "2ccead8c358568b630f3060c9c6a0c75cf037fae8f985c4163fe654c3e7c6bf2"),
    ),
    "local-qa-linux-postmaster-pidfd.py": (
        (15860, "9353dad2256c666f91f7e990e2ad36b830ea8589e82f5ad6b485545dbe0d3fa7"),
        (16572, "f15df1fa0777510d0f1d022dd35c64520cbbcb1c02d5fb1146c98fa413ae96a8"),
    ),
}
PG_ADDITIONS = {
    "local-qa-linux-c2c-pg-capsule.mjs": (
        8551, "4dc620c50817158c66983dfb811f85a33f911e4c265267bca05c91af3730eeae",
    ),
    "local-qa-linux-c2c-pg-command.mjs": (
        3826, "6541e5e6d3af88d2e5e4cfb2e66ea8ddaf7ebaa6e4094cf90746ffa15cae0ace",
    ),
}
ACTOR_COUNT = BASE_ACTOR_COUNT + len(PG_ADDITIONS)
DIR = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_DIRECTORY
FILE = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK


def need(ok, label):
    if not ok:
        raise ValueError("c2c_code_packet_refused:" + label)


def sha(body):
    return hashlib.sha256(body).hexdigest()


def wire(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")


def actor_rows(base):
    need(type(base) is bytes and 0 < len(base) <= 8192 and sha(base) == BASE_INVENTORY_SHA, "base_inventory_pin")
    value = json.loads(base)
    need(type(value) is dict and value.keys() == {"version", "files"} and type(value["version"]) is int
         and value["version"] == 1 and type(value["files"]) is list and len(value["files"]) == BASE_ACTOR_COUNT
         and wire(value) == base, "base_inventory")
    rows = {}
    for row in value["files"]:
        need(type(row) is dict and row.keys() == {"name", "bytes", "sha256"}
             and type(row["name"]) is str and NAME.fullmatch(row["name"]) and row["name"] not in rows
             and type(row["bytes"]) is int and 0 < row["bytes"] <= FILE_LIMIT
             and type(row["sha256"]) is str and SHA.fullmatch(row["sha256"]), "actor_file")
        rows[row["name"]] = (row["bytes"], row["sha256"])
    need(list(rows) == sorted(rows) and rows.keys() & MANAGER.keys() == {PROTOCOL}
         and rows[PROTOCOL] == PROTOCOL_DELTA[:2], "actor_protocol_preimage")
    rows[PROTOCOL] = (PROTOCOL_DELTA[2], MANAGER[PROTOCOL])
    for name, (prior, successor) in PG_REPLACEMENTS.items():
        need(rows.get(name) == prior, "pg_actor_preimage")
        rows[name] = successor
    need(not rows.keys() & PG_ADDITIONS.keys() and not PG_ADDITIONS.keys() & MANAGER.keys(),
         "pg_actor_additions")
    rows.update(PG_ADDITIONS)
    return dict(sorted(rows.items()))


def actor_inventory(rows):
    return wire({"version": 1, "files": [{"name": name, "bytes": size, "sha256": digest}
                                           for name, (size, digest) in rows.items()]})


def archive_bytes(members):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, (mode, body) in members.items():
            entry = tarfile.TarInfo(name)
            entry.mode, entry.size = mode, len(body)
            archive.addfile(entry, io.BytesIO(body))
    return output.getvalue()


def file_stamp(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def read_code(root, file, digest, size=None):
    with os.fdopen(os.open(file, FILE, dir_fd=root), "rb") as stream:
        before = os.fstat(stream.fileno())
        need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and 0 < before.st_size <= FILE_LIMIT
             and (size is None or before.st_size == size), "source_file_metadata")
        raw = stream.read(before.st_size + 1)
        need(len(raw) == before.st_size and sha(raw) == digest
             and file_stamp(before) == file_stamp(os.fstat(stream.fileno()))
             == file_stamp(os.stat(file, dir_fd=root, follow_symlinks=False)), "source_file_pin")
        return raw


def build_packet(code_dir, base_raw):
    """Produce a deterministic USTAR packet from the admitted source paths."""
    expect = actor_rows(base_raw)
    need(isinstance(code_dir, Path) and code_dir.is_absolute() and code_dir.resolve(strict=True) == code_dir,
         "source_directory")
    root = os.open(code_dir, DIR)
    try:
        need(os.path.samestat(os.fstat(root), code_dir.lstat()), "source_directory_changed")
        members = {BASE_MEMBER: (0o600, base_raw), "actor/inventory.json": (0o444, actor_inventory(expect))}
        actor = {name: read_code(root, name, digest, size) for name, (size, digest) in expect.items()}
        members.update(("actor/" + name, (0o444, raw)) for name, raw in actor.items())
        for name, digest in sorted(MANAGER.items()):
            raw = actor[PROTOCOL] if name == PROTOCOL else read_code(root, name, digest)
            members["manager/" + name] = (0o600, raw)
        need(os.path.samestat(os.fstat(root), code_dir.lstat()), "source_directory_changed")
    finally:
        os.close(root)
    raw = archive_bytes(members)
    verify_packet(raw, sha(raw))
    return raw


def verify_packet(raw, expected_sha256):
    """Verify an independently bound packet before returning exact role bytes."""
    need(type(raw) is bytes and 0 < len(raw) <= PACKET_LIMIT and type(expected_sha256) is str
         and SHA.fullmatch(expected_sha256) and sha(raw) == expected_sha256, "packet_pin")
    members = {}

    def member(archive, name, mode, expected_hash, size=None):
        entry = archive.next()
        need(entry is not None and entry.name == name and entry.type == tarfile.REGTYPE
             and entry.mode == mode and not entry.pax_headers and not entry.linkname
             and (entry.uid, entry.gid, entry.mtime) == (0, 0, 0) and 0 < entry.size <= FILE_LIMIT
             and (size is None or entry.size == size), "archive_member")
        with archive.extractfile(entry) as stream:
            body = stream.read(entry.size + 1)
        need(len(body) == entry.size and sha(body) == expected_hash, "archive_body")
        members[name] = (mode, body)
        return body

    try:
        with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
            base = member(archive, BASE_MEMBER, 0o600, BASE_INVENTORY_SHA)
            expect = actor_rows(base)
            inventory = actor_inventory(expect)
            actor = {"inventory.json": member(archive, "actor/inventory.json", 0o444, sha(inventory), len(inventory))}
            for name, (size, digest) in expect.items():
                actor[name] = member(archive, "actor/" + name, 0o444, digest, size)
            manager = {name: member(archive, "manager/" + name, 0o600, digest)
                       for name, digest in sorted(MANAGER.items())}
            need(archive.next() is None, "archive_extra_member")
    except (tarfile.TarError, EOFError, OSError) as error:
        raise ValueError("c2c_code_packet_refused:archive_invalid") from error
    need(archive_bytes(members) == raw and actor[PROTOCOL] == manager[PROTOCOL], "archive_canonical")
    receipt = {"schema": "bluey-jobs-c2c-code-packet-v1", "packetSha256": expected_sha256,
               "sourceTree": SOURCE_TREE, "sourceInventorySha256": SOURCE_INVENTORY,
               "priorActorInventorySha256": BASE_INVENTORY_SHA, "codeInventorySha256": sha(inventory),
               "actorFiles": ACTOR_COUNT, "managerFiles": len(manager), "releaseAdmitted": False,
               "runtimeQualified": False}
    return actor, manager, receipt
