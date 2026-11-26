"""Compose a fixed-purpose, inert Linux signup/import/Prepare code candidate."""

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


BASE_SHA = "84b9191359ff539524266e8353283679b88ed825e2cc757842755566cf7e6245"
BASE_BYTES = 604_160
BASE_INVENTORY_SHA = "6314f2c801e7c63982360a1076c7970aa29b85763ff0ac0e46823689aa482495"
BASE_PREFIX = "local-qa-linux-signup-sample"
PREFIX = "local-qa-linux-signup-prepare"
ACTOR = PREFIX + "-native.mjs"
WAITER = PREFIX + "-wait-actor.py"
MANAGER = PREFIX + "-guest-manager.py"
TERMINAL = PREFIX + "-terminal.py"
PROTOCOL = "local-qa-linux-c2c-release-protocol.py"
LEAF = re.compile(r"[a-z][a-z0-9-]{1,96}\.(?:json|py|mjs)\Z")
FILE = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
DIRECTORY = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_DIRECTORY
MAX_MEMBER = 128 << 10

REPLACEMENTS = {
    "local-qa-linux-api-auth.mjs": (12942, "64cfd47ee617990117c3e49fe3903626123b08e66071f47d5d3ee8ccf22f4f1a"),
    "local-qa-linux-c2c-native.mjs": (11239, "24dc0e938cd6d985f17b5e4b0dd41e00f12f2cfc54da4ca32061b9eac2270b5b"),
    "local-qa-linux-owned-api-session.mjs": (24966, "a169057b06dbd9410e408ed3ce6b6bb2b3e874b9246642110f8d32ea724be6a8"),
    "local-qa-linux-signup-gateway.mjs": (20083, "ec0282b69bd387e3fc68d85e3fe0503363c145a66a470a0278b24a1a423902ec"),
    "local-qa-durable-posting-seeder.mjs": (16389, "65b0e9dfee8547c17916a8588fca6851f1ae425b3479b39765a3c778c7c20760"),
}
ADDITIONS = {
    ACTOR: (4042, "152a1b2c287e9194edf50772baf48d7c9a9fe07481c6524a874c56e239800f11"),
    "local-qa-browser-sample-prepare.mjs": (6396, "2c38af1aad682fc3ac9d3c640af80c37a0dde1d23b260cf9357dc7f0b29f87fe"),
    "local-qa-imported-resume-prepare.mjs": (7164, "d7b9f08a5102dd4b7a64a1665e08bd5c0a487774ec699d67194f4c5ce7e7db17"),
}
TERMINAL_PIN = (1865, "9f96dc833e680473aff157518f17b280910cf4f8b40a10353fc6e98ff2a4d5d0")


def need(ok, label):
    if not ok:
        raise ValueError("signup_prepare_code_refused:" + label)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def wire(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")


def stamp(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_nlink, info.st_uid, info.st_gid,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def read_pinned(directory, name, size, digest):
    with os.fdopen(os.open(name, FILE, dir_fd=directory), "rb") as stream:
        before = os.fstat(stream.fileno())
        need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and before.st_size == size, "file_identity")
        raw = stream.read(size + 1)
        need(len(raw) == size and sha(raw) == digest and stamp(before) == stamp(os.fstat(stream.fileno()))
             == stamp(os.stat(name, dir_fd=directory, follow_symlinks=False)), "file_pin")
        return raw


def inventory(rows):
    return wire({"version": 1, "files": [{"name": name, "bytes": len(raw), "sha256": sha(raw)}
                                         for name, raw in sorted(rows.items())]})


def unpack_base(raw):
    need(type(raw) is bytes and len(raw) == BASE_BYTES and sha(raw) == BASE_SHA, "base_packet")
    groups = {"actor": {}, "manager": {}}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        header = archive.next()
        need(header is not None and header.name == "candidate.json" and header.isreg()
             and 0 < header.size <= 4096, "base_manifest")
        with archive.extractfile(header) as stream:
            candidate = json.load(stream)
        need(candidate["schema"] == "bluey-jobs-signup-sample-code-candidate-v1"
             and candidate["codeInventorySha256"] == BASE_INVENTORY_SHA
             and all(candidate[key] is False for key in ("physicalSourceAdmitted", "releaseAdmitted", "runtimeQualified")),
             "base_schema")
        while (member := archive.next()) is not None:
            parts = member.name.split("/")
            need(len(parts) == 2 and parts[0] in groups and LEAF.fullmatch(parts[1]) and member.isreg()
                 and member.mode == (0o444 if parts[0] == "actor" else 0o600)
                 and 0 < member.size <= MAX_MEMBER, "base_member")
            role, name = parts
            need(name not in groups[role], "base_duplicate")
            with archive.extractfile(member) as stream:
                groups[role][name] = stream.read()
    actor, manager = groups["actor"], groups["manager"]
    manifest = actor.pop("inventory.json")
    need(len(actor) == 35 and len(manager) == 12 and sha(manifest) == BASE_INVENTORY_SHA
         and manifest == inventory(actor) and actor.keys() & manager.keys() == {PROTOCOL}
         and actor[PROTOCOL] == manager[PROTOCOL], "base_inventory")
    return actor, manager


def replace_once(raw, old, new):
    need(raw.count(old) == 1, "render_anchor")
    return raw.replace(old, new, 1)


def compose(raw, scripts):
    actor, manager = unpack_base(raw)
    need(REPLACEMENTS.keys() <= actor.keys() and not ADDITIONS.keys() & (actor.keys() | manager.keys()), "overlay_scope")
    old_actor = BASE_PREFIX + "-native.mjs"
    old_waiter = BASE_PREFIX + "-wait-actor.py"
    old_manager = BASE_PREFIX + "-guest-manager.py"
    old_terminal = BASE_PREFIX + "-terminal.py"
    actor.pop(old_actor)
    manager.pop(old_terminal)
    waiter_bytes = actor.pop(old_waiter)
    manager_bytes = manager.pop(old_manager)

    fd = os.open(scripts, DIRECTORY)
    try:
        identity = os.fstat(fd)
        need(scripts.is_absolute() and scripts.resolve(strict=True) == scripts
             and os.path.samestat(identity, scripts.lstat()), "source_directory")
        for name, pin in (REPLACEMENTS | ADDITIONS).items():
            updated = read_pinned(fd, name, *pin)
            need(actor.get(name) != updated, "unchanged_overlay")
            actor[name] = updated
        manager[TERMINAL] = read_pinned(fd, TERMINAL, *TERMINAL_PIN)
        need(os.path.samestat(identity, scripts.lstat()), "source_directory_changed")
    finally:
        os.close(fd)

    waiter = replace_once(waiter_bytes, f'ACTOR = "{old_actor}"'.encode(), f'ACTOR = "{ACTOR}"'.encode())
    actor[WAITER] = waiter
    for old, new in ((old_waiter, WAITER), (sha(waiter_bytes), sha(waiter)),
                     (old_terminal, TERMINAL), ("_admitted_signup_sample_terminal", "_admitted_signup_prepare_terminal"),
                     ("bluey-jobs-signup-sample-guest-qa-v1", "bluey-jobs-signup-prepare-guest-qa-v1"),
                     ("3c6fd08a9d91dc6a9963c8b7b02977e1ca0b7c7cfa26102eaeb34f411f120b7b", TERMINAL_PIN[1])):
        manager_bytes = replace_once(manager_bytes, old.encode(), new.encode())
    manager[MANAGER] = manager_bytes
    for name, body in ((WAITER, waiter), (MANAGER, manager_bytes), (TERMINAL, manager[TERMINAL])):
        ast.parse(body, filename=name)

    actor_inventory = inventory(actor)
    need(len(actor) == 37 and len(manager) == 12 and actor.keys() & manager.keys() == {PROTOCOL}, "candidate_inventory")
    manifest = {
        "schema": "bluey-jobs-signup-prepare-code-candidate-v1", "basePacketSha256": BASE_SHA,
        "baseCodeInventorySha256": BASE_INVENTORY_SHA, "codeInventorySha256": sha(actor_inventory),
        "actorFiles": len(actor), "managerFiles": len(manager), "waiter": WAITER, "actor": ACTOR, "manager": MANAGER,
        "physicalSourceAdmitted": False, "releaseAdmitted": False, "runtimeQualified": False,
    }
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        members = [("candidate.json", 0o444, wire(manifest)), ("actor/inventory.json", 0o444, actor_inventory)]
        members += [("actor/" + name, 0o444, body) for name, body in sorted(actor.items())]
        members += [("manager/" + name, 0o600, body) for name, body in sorted(manager.items())]
        for name, mode, body in members:
            row = tarfile.TarInfo(name)
            row.mode, row.size = mode, len(body)
            archive.addfile(row, io.BytesIO(body))
    candidate = buffer.getvalue()
    return candidate, {**manifest, "candidateSha256": sha(candidate), "candidateBytes": len(candidate)}


def export(base, output):
    need(base.is_absolute() and base.resolve(strict=True) == base and base.name == "signup-sample-source-candidate.tar",
         "base_path")
    need(output.is_absolute() and output.resolve(strict=True) == output, "export_path")
    fd = os.open(base.parent, DIRECTORY)
    try:
        previous = read_pinned(fd, base.name, BASE_BYTES, BASE_SHA)
    finally:
        os.close(fd)
    raw, receipt = compose(previous, Path(__file__).resolve(strict=True).parent)
    fd = os.open(output, DIRECTORY)
    try:
        root = os.fstat(fd)
        need(os.path.samestat(root, output.lstat()) and root.st_uid == os.getuid()
             and stat.S_IMODE(root.st_mode) == 0o700 and os.listdir(fd) == [], "export_scope")
        name = "signup-prepare-source-candidate.tar"
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
        with os.fdopen(os.open(name, flags, 0o600, dir_fd=fd), "wb") as stream:
            need(stream.write(raw) == len(raw), "export_write")
            stream.flush()
            os.fsync(stream.fileno())
        entry = os.stat(name, dir_fd=fd, follow_symlinks=False)
        need(stat.S_IMODE(entry.st_mode) == 0o600 and entry.st_uid == os.getuid()
             and raw == read_pinned(fd, name, len(raw), receipt["candidateSha256"])
             and os.listdir(fd) == [name] and os.path.samestat(root, output.lstat()), "export_readback")
        os.fsync(fd)
    finally:
        os.close(fd)
    return receipt


if __name__ == "__main__":
    try:
        need(len(sys.argv) == 5 and sys.argv[1] == "--base-packet" and sys.argv[3] == "--output-directory", "explicit_export")
        print(wire(export(Path(sys.argv[2]), Path(sys.argv[4]))).decode("ascii"), end="")
    except (OSError, ValueError, tarfile.TarError) as error:
        label = str(error)
        print(label if label.startswith("signup_prepare_code_refused:") else "signup_prepare_code_refused:input",
              file=sys.stderr)
        raise SystemExit(1) from None
