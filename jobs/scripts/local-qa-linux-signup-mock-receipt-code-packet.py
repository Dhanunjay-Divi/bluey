"""Seal the current fixed-purpose browser mock-receipt sources without running them."""

import ast
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import sys
import tarfile


BASE = "signup-prepare-source-candidate.tar"
BASE_PIN = (634880, "fffacd898962bf5d4bc86e09dc6e51879d7088aa79ca6866c647800056fe6af3")
BASE_INVENTORY = "30be415457e3a83607c0d2a49b542b9ebb95937a767227555deffec6c8de9fd1"
OLD = "local-qa-linux-signup-prepare"
NEW = "local-qa-linux-signup-mock-receipt"
FORMAT = "bluey-jobs-signup-mock-receipt-code-candidate-v1"
OUT = "signup-mock-receipt-source-candidate.tar"
PROTOCOL = "local-qa-linux-c2c-release-protocol.py"
MAX_MEMBER = 128 << 10
DIRECTORY = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_DIRECTORY
ACTOR_OVERLAY = {
    "local-qa-durable-posting-seeder.mjs": (25675, "002abe9e0a7e17bad93ec55b028352f2294228414a27afbd4dc079f1dfda051f"),
    "local-qa-imported-resume-prepare.mjs": (7992, "745d849a621c73070410d7069b886857d022e951a28dd22ccb4a779339807f7a"),
    "local-qa-linux-api-auth.mjs": (15532, "ab72e0c5084e51ceea6c5475586f3c5ef086ac5412e97a75fa2b7299aa6c2095"),
    "local-qa-linux-c2c-census.mjs": (8991, "18622091da5d299523406658013bcedcb04c8ebd1651173741203ec7b42713ee"),
    "local-qa-linux-c2c-native.mjs": (11622, "aff267360c3dded78ac7b135f0fc46b140b0f841e3bf7b9e29a8b6422c3e5183"),
    "local-qa-linux-owned-api-session.mjs": (32285, "046e701e254403e4f45f04568d970c7c2183c768d791d856631ca2464a8cc0b9"),
    "local-qa-linux-pg-runtime.mjs": (17046, "9274090bf7cac26ec754f086a3a3d487aeddd1274ca51323019cd6b23d3b96ed"),
    "local-qa-linux-pg-tools.mjs": (20455, "814361e62f9b4f58afb8cedf2065e1b4d902fe7e423a001e7171bb6fefeea2de"),
    "local-qa-linux-signup-gateway.mjs": (24437, "7b28e18b6611dd03d7e112c246133465f32e94f1aaca4b25425d2df562a190a2"),
    "local-qa-object-store.mjs": (20624, "9859ebef75dabdffd6d18be58894fc00238130ee342142b3e50a62bb93a77ad8"),
}
ACTOR_ADDITIONS = {
    NEW + "-native.mjs": (4069, "92dd8e2434df778fb757d176945c557d2377e6900cb44442f1671cb8d97def41"),
    "local-qa-browser-mock-receipt.mjs": (7345, "b54c1aa3587058cbd7164a119f645dd3fb79e49a0e7281dd555209ebeacfdda4"),
    "local-qa-mock-receipt-checkpoint.mjs": (7902, "f70214a79183d62fd1e6f899405d30d4165778458656d55f929fec0b9cfc50af"),
}
TERMINAL_PIN = (2001, "b44511c40dafed18f7676c1cb25a6c4a4aefba618cc87527b09a0fac924c49ec")


def need(ok, label):
    if not ok:
        raise ValueError("signup_mock_receipt_code_refused:" + label)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def read(directory, name, size, digest):
    flags = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
    def stamp(row):
        return (row.st_dev, row.st_ino, row.st_mode, row.st_nlink, row.st_uid, row.st_gid,
                row.st_size, row.st_mtime_ns, row.st_ctime_ns)
    with os.fdopen(os.open(name, flags, dir_fd=directory), "rb") as stream:
        before = os.fstat(stream.fileno())
        need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and before.st_size == size, "file_identity")
        raw = stream.read(size + 1)
        need(len(raw) == size and sha(raw) == digest and stamp(before) == stamp(os.fstat(stream.fileno()))
             == stamp(os.stat(name, dir_fd=directory, follow_symlinks=False)), "file_pin")
    return raw


def wire(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def inventory(actor):
    return wire({"version": 1, "files": [{"name": name, "bytes": len(raw), "sha256": sha(raw)}
                                        for name, raw in sorted(actor.items())]})


def replace(raw, old, new):
    before = old.encode()
    need(raw.count(before) == 1, "transform_anchor")
    return raw.replace(before, new.encode(), 1)


def unpack(raw):
    need(type(raw) is bytes and (len(raw), sha(raw)) == BASE_PIN, "base_pin")
    groups = {"actor": {}, "manager": {}}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        head = archive.next()
        need(head is not None and head.name == "candidate.json" and head.isfile() and 0 < head.size <= 4096,
             "base_manifest")
        with archive.extractfile(head) as data:
            meta = json.load(data)
        need(meta["schema"] == "bluey-jobs-signup-prepare-code-candidate-v1"
             and meta["codeInventorySha256"] == BASE_INVENTORY
             and all(meta[key] is False for key in ("physicalSourceAdmitted", "releaseAdmitted", "runtimeQualified")),
             "base_schema")
        while (entry := archive.next()) is not None:
            parts = entry.name.split("/")
            need(len(parts) == 2 and parts[0] in groups and re.fullmatch(r"[a-z][a-z0-9-]{1,96}\.(?:json|py|mjs)", parts[1])
                 and entry.isfile() and not entry.pax_headers and 0 < entry.size <= MAX_MEMBER, "base_entry")
            role, name = parts
            need(name not in groups[role] and entry.mode == (0o444 if role == "actor" else 0o600), "base_role")
            with archive.extractfile(entry) as body:
                groups[role][name] = body.read()
    actor, manager = groups["actor"], groups["manager"]
    stored = actor.pop("inventory.json")
    need(len(actor) == 37 and len(manager) == 12 and actor.keys() & manager.keys() == {PROTOCOL}
         and stored == inventory(actor) and sha(stored) == BASE_INVENTORY and actor[PROTOCOL] == manager[PROTOCOL],
         "base_inventory")
    return actor, manager


def compose(raw, scripts):
    actor, manager = unpack(raw)
    need(ACTOR_OVERLAY.keys() <= actor.keys() and not ACTOR_ADDITIONS.keys() & (actor.keys() | manager.keys()),
         "overlay_scope")
    del actor[OLD + "-native.mjs"]
    old_waiter = actor.pop(OLD + "-wait-actor.py")
    old_manager = manager.pop(OLD + "-guest-manager.py")
    old_terminal = manager.pop(OLD + "-terminal.py")
    waiter = replace(old_waiter, OLD + "-native.mjs", NEW + "-native.mjs")
    actor[NEW + "-wait-actor.py"] = waiter
    fd = os.open(scripts, DIRECTORY)
    try:
        start = os.fstat(fd)
        need(scripts.resolve(strict=True) == scripts and os.path.samestat(start, scripts.lstat()), "scripts_scope")
        for name, pin in (ACTOR_OVERLAY | ACTOR_ADDITIONS).items():
            body = read(fd, name, *pin)
            need(actor.get(name) != body, "unchanged_overlay")
            actor[name] = body
        terminal = read(fd, NEW + "-terminal.py", *TERMINAL_PIN)
        need(os.path.samestat(start, scripts.lstat()), "scripts_changed")
    finally:
        os.close(fd)
    for before, after in (
        (OLD + "-wait-actor.py", NEW + "-wait-actor.py"), (sha(old_waiter), sha(waiter)),
        (OLD + "-terminal.py", NEW + "-terminal.py"), (sha(old_terminal), sha(terminal)),
        ("_admitted_signup_prepare_terminal", "_admitted_signup_mock_receipt_terminal"),
        ("bluey-jobs-signup-prepare-guest-qa-v1", "bluey-jobs-signup-mock-receipt-guest-qa-v1"),
    ):
        old_manager = replace(old_manager, before, after)
    manager.update({NEW + "-guest-manager.py": old_manager, NEW + "-terminal.py": terminal})
    for body in (waiter, old_manager, terminal):
        ast.parse(body)
    need(len(actor) == 39 and len(manager) == 12 and actor.keys() & manager.keys() == {PROTOCOL}, "inventory_shape")
    for name, body in actor.items():
        if name.endswith(".mjs"):
            deps = re.findall(rb"""(?:\bfrom\s+|\bimport\s*(?:\(\s*)?)["']\./([a-z0-9.-]+\.mjs)["']""", body)
            need(all(dep.decode() in actor for dep in deps), "missing_actor_import")
    manifest = inventory(actor)
    meta = dict(schema=FORMAT, basePacketSha256=BASE_PIN[1], baseCodeInventorySha256=BASE_INVENTORY,
                codeInventorySha256=sha(manifest), actorFiles=len(actor), managerFiles=len(manager),
                actor=NEW + "-native.mjs", waiter=NEW + "-wait-actor.py", manager=NEW + "-guest-manager.py",
                physicalSourceAdmitted=False, releaseAdmitted=False, runtimeQualified=False)
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        entries = [("candidate.json", 0o444, wire(meta)), ("actor/inventory.json", 0o444, manifest)]
        entries += [("actor/" + name, 0o444, content) for name, content in sorted(actor.items())]
        entries += [("manager/" + name, 0o600, content) for name, content in sorted(manager.items())]
        for name, mode, body in entries:
            row = tarfile.TarInfo(name)
            row.size, row.mode = len(body), mode
            archive.addfile(row, io.BytesIO(body))
    result = buffer.getvalue()
    return result, {**meta, "candidateSha256": sha(result), "candidateBytes": len(result)}


def export(base, output):
    need(base.name == BASE and base.resolve(strict=True) == base and output.resolve(strict=True) == output, "path")
    fd = os.open(base.parent, DIRECTORY)
    try:
        previous = read(fd, base.name, *BASE_PIN)
    finally:
        os.close(fd)
    raw, receipt = compose(previous, Path(__file__).resolve(strict=True).parent)
    fd = os.open(output, DIRECTORY)
    try:
        root = os.fstat(fd)
        need(os.path.samestat(root, output.lstat()) and root.st_uid == os.getuid()
             and stat.S_IMODE(root.st_mode) == 0o700 and os.listdir(fd) == [], "export_scope")
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
        with os.fdopen(os.open(OUT, flags, 0o600, dir_fd=fd), "wb") as stream:
            need(stream.write(raw) == len(raw), "export_write")
            stream.flush()
            os.fsync(stream.fileno())
        leaf = os.stat(OUT, dir_fd=fd, follow_symlinks=False)
        need(stat.S_IMODE(leaf.st_mode) == 0o600 and leaf.st_uid == os.getuid()
             and read(fd, OUT, len(raw), sha(raw)) == raw and os.listdir(fd) == [OUT]
             and os.path.samestat(root, output.lstat()), "export_readback")
        os.fsync(fd)
    finally:
        os.close(fd)
    return receipt


if __name__ == "__main__":
    try:
        need(len(sys.argv) == 5 and sys.argv[1] == "--base-packet" and sys.argv[3] == "--output-directory",
             "explicit_export")
        sys.stdout.buffer.write(wire(export(Path(sys.argv[2]), Path(sys.argv[4]))))
    except (OSError, ValueError, tarfile.TarError):
        print("signup_mock_receipt_code_refused", file=sys.stderr)
        raise SystemExit(1) from None
