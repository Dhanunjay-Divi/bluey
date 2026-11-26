"""Build the inert signup/sample-resume source candidate. Never installs or runs it."""
import ast
import hashlib
import os
from pathlib import Path
import stat
import sys


BASE_PACKET = "3dc5fe64624b64d57939d3c525cd00a1e3f6c027fcb505eea03f0390e5a7ef25"
BASE_PACKET_BYTES = 552_960
BASE_VERIFIER = "local-qa-linux-c2c-code-packet.py"
BASE_VERIFIER_SHA = "694adfec5310035d94858d626fa5f84c2e093347f0f5fec6c020b03fff0a9870"
BASE_WAITER = "local-qa-linux-c2c-wait-actor.py"
BASE_MANAGER = "local-qa-linux-c2c-guest-manager.py"
BASE_WAITER_SHA = "f7e4ef89cafd917ec5312eca615883e68fd1ae6a7da61fa8a19aca2010696d4a"
WAITER = "local-qa-linux-signup-sample-wait-actor.py"
MANAGER = "local-qa-linux-signup-sample-guest-manager.py"
ACTOR = "local-qa-linux-signup-sample-native.mjs"
TERMINAL = "local-qa-linux-signup-sample-terminal.py"
TERMINAL_SHA = "3c6fd08a9d91dc6a9963c8b7b02977e1ca0b7c7cfa26102eaeb34f411f120b7b"
REPLACEMENTS = {
    "local-qa-gateway.mjs": (22628, "6bbfe2a227b2e3e32f82f5389c68cffa73c47e931480e3d4b63a4c71bd6061a7"),
    "local-qa-linux-api-auth.mjs": (9967, "658a0e2aecae53ca8a18d51359746105934303e37a9c27a8aad3bdb3af740393"),
    "local-qa-linux-c2c-native.mjs": (10897, "b52a576d23881a629a67b4257a366edc35dd94db3b07b9d628a1a20716aefaa4"),
    "local-qa-linux-c2c-restart-gateway.mjs": (5643, "350d7cebb438719f2f7783e4853750b870a4937cd88212cb78240eb0a41325b6"),
    "local-qa-linux-owned-api-session.mjs": (21475, "d148cd47e8101f7b2d98fe13fda1fa0b8e14033a904d02ee86c1feed7a0880bf"),
}
ADDITIONS = {
    "local-qa-browser-signup.mjs": (7417, "cb8e41be86cb0ea326a33ec782bf4984aa85af89cad7f36f8fd662a2787c37d7"),
    "local-qa-browser-sample-resume.mjs": (6575, "71b5b6cc54d4eb51fb79b0f479dcc5c311f1785b14c1de454fb538cde1f59a59"),
    "local-qa-linux-signup-gateway.mjs": (14986, "37eb098e821e467a54cf947b1c88e1dbc332b1b1d4fbb0201c06b9c3fe39677e"),
}
ACTOR_PIN = (3806, "3602977f19f964cd6aeae1a74ff5cb5036a4791eed4a468211daebde25559a50")
DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
FILE = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC


def need(ok, stage):
    if not ok:
        raise ValueError("signup_sample_code_refused:" + stage)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def stamp(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def patch_once(source, target, replacement):
    need(source.count(target) == 1, "render_anchor")
    return source.replace(target, replacement, 1)


def render_waiter(previous):
    need(sha(previous) == BASE_WAITER_SHA, "waiter_preimage")
    result = patch_once(previous, b'ACTOR = "local-qa-linux-c2c-native.mjs"', f'ACTOR = "{ACTOR}"'.encode("ascii"))
    ast.parse(result, filename=WAITER)
    return result


def render_manager(previous, previous_sha, waiter_sha):
    need(sha(previous) == previous_sha, "manager_preimage")
    output = patch_once(previous, f'WAITER = "{BASE_WAITER}"'.encode("ascii"), f'WAITER = "{WAITER}"'.encode("ascii"))
    output = patch_once(output, f'EXPECTED_WAITER_SHA = "{BASE_WAITER_SHA}"'.encode("ascii"),
                        f'EXPECTED_WAITER_SHA = "{waiter_sha}"'.encode("ascii"))
    output = patch_once(output, b'"bluey-jobs-c2c-guest-qa-v1"', b'"bluey-jobs-signup-sample-guest-qa-v1"')
    prefix = b"\ndef terminal(raw, source_tree, source_inventory):\n"
    following = b"\n\ndef spec_for_launch("
    need(output.count(prefix) == 1 and output.count(following) == 1, "manager_terminal_anchors")
    before, after = output.index(prefix), output.index(following)
    need(before < after, "manager_terminal_order")
    # Keep the previously measured pinned-file reader, bound launch and cleanup
    # logic exactly as sealed. Only the terminal schema belongs to this variant.
    body = f'''\ndef terminal(raw, source_tree, source_inventory):
    path = Path(__file__).resolve(strict=True).with_name("{TERMINAL}")
    _, source = pinned_file(path, "{TERMINAL_SHA}", MAX_FRAME, retain=True)
    scope = {{"__name__": "_admitted_signup_sample_terminal", "__file__": str(path)}}
    exec(compile(source, str(path), "exec"), scope)
    return scope["terminal"](raw, source_tree, source_inventory)
'''.encode("ascii")
    output = output[:before] + body + output[after:]
    ast.parse(output, filename=MANAGER)
    return output


def build_candidate(old_raw, scripts):
    """Return canonical archive bytes and a receipt. No guest or release authority."""
    need(type(old_raw) is bytes and len(old_raw) == BASE_PACKET_BYTES and sha(old_raw) == BASE_PACKET, "base_packet")
    need(isinstance(scripts, Path) and scripts.is_absolute() and scripts.resolve(strict=True) == scripts,
         "source_directory")
    with os.fdopen(os.open(scripts / BASE_VERIFIER, FILE), "rb") as handle:
        before = os.fstat(handle.fileno())
        need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and 0 < before.st_size <= 64 << 10,
             "base_verifier_metadata")
        pinned = handle.read(before.st_size + 1)
        after = os.fstat(handle.fileno())
        need(len(pinned) == before.st_size and sha(pinned) == BASE_VERIFIER_SHA
             and stamp(before) == stamp(after) == stamp((scripts / BASE_VERIFIER).lstat()), "base_verifier_digest")
    scope = {"__name__": "_verified_base_c2c_source_codec"}
    exec(compile(pinned, BASE_VERIFIER, "exec"), scope)
    actor, manager, previous = scope["verify_packet"](old_raw, BASE_PACKET)
    need(previous["actorFiles"] == 31 and previous["managerFiles"] == 11, "base_scope")
    old_inventory = actor.pop("inventory.json")
    need(sha(old_inventory) == previous["codeInventorySha256"], "base_inventory")
    expected_additions = {**ADDITIONS, ACTOR: ACTOR_PIN}
    need(REPLACEMENTS.keys() <= actor.keys() and not expected_additions.keys() & actor.keys()
         and WAITER not in actor and MANAGER not in manager and TERMINAL not in manager, "overlay_scope")
    fd = os.open(scripts, DIR)
    try:
        identity = os.fstat(fd)
        need(os.path.samestat(identity, scripts.lstat()), "source_directory_changed")
        for name, (size, digest) in (REPLACEMENTS | expected_additions).items():
            changed = scope["read_code"](fd, name, digest, size)
            need(actor.get(name) != changed, "unchanged_overlay")
            actor[name] = changed
        manager[TERMINAL] = scope["read_code"](fd, TERMINAL, TERMINAL_SHA)
        need(os.path.samestat(identity, scripts.lstat()), "source_directory_changed")
    finally:
        os.close(fd)
    actor[WAITER] = render_waiter(actor.pop(BASE_WAITER))
    manager[MANAGER] = render_manager(manager.pop(BASE_MANAGER), scope["MANAGER"][BASE_MANAGER], sha(actor[WAITER]))
    need(len(actor) == 35 and len(manager) == 12, "successor_scope")
    inventory = scope["actor_inventory"]({name: (len(body), sha(body)) for name, body in sorted(actor.items())})
    candidate = {
        "schema": "bluey-jobs-signup-sample-code-candidate-v1", "basePacketSha256": BASE_PACKET,
        "baseCodeInventorySha256": sha(old_inventory), "codeInventorySha256": sha(inventory),
        "actorFiles": len(actor), "managerFiles": len(manager), "waiter": WAITER, "actor": ACTOR, "manager": MANAGER,
        "physicalSourceAdmitted": False, "releaseAdmitted": False, "runtimeQualified": False,
    }
    members = {"candidate.json": (0o444, scope["wire"](candidate)), "actor/inventory.json": (0o444, inventory)}
    members.update(("actor/" + name, (0o444, body)) for name, body in sorted(actor.items()))
    members.update(("manager/" + name, (0o600, body)) for name, body in sorted(manager.items()))
    packet = scope["archive_bytes"](members)
    return packet, {**candidate, "candidateSha256": sha(packet), "candidateBytes": len(packet)}


def export_candidate(base_path, output):
    """Create the one inert archive in a new private caller-owned directory."""
    need(base_path.is_absolute() and base_path.resolve(strict=True) == base_path
         and base_path.name == "code-packet.tar", "base_path")
    need(output.is_absolute() and output.resolve(strict=True) == output, "export_path")
    with os.fdopen(os.open(base_path, FILE), "rb") as stream:
        base_info = os.fstat(stream.fileno())
        need(stat.S_ISREG(base_info.st_mode) and base_info.st_nlink == 1 and base_info.st_size == BASE_PACKET_BYTES,
             "base_file")
        base = stream.read(BASE_PACKET_BYTES + 1)
        need(stamp(base_info) == stamp(os.fstat(stream.fileno())) == stamp(base_path.lstat()), "base_changed")
    candidate, receipt = build_candidate(base, Path(__file__).resolve(strict=True).parent)
    fd = os.open(output, DIR)
    try:
        root = os.fstat(fd)
        need(os.path.samestat(root, output.lstat()) and root.st_uid == os.getuid()
             and stat.S_IMODE(root.st_mode) == 0o700 and not os.listdir(fd), "export_not_private_empty")
        name = "signup-sample-source-candidate.tar"
        with os.fdopen(os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC,
                               0o600, dir_fd=fd), "wb") as stream:
            need(stream.write(candidate) == len(candidate), "export_write")
            stream.flush()
            os.fsync(stream.fileno())
        with os.fdopen(os.open(name, FILE, dir_fd=fd), "rb") as stream:
            readback = stream.read(len(candidate) + 1)
            artifact = os.fstat(stream.fileno())
            need(readback == candidate and stat.S_IMODE(artifact.st_mode) == 0o600
                 and artifact.st_nlink == 1 and artifact.st_uid == os.getuid()
                 and stamp(artifact) == stamp(os.stat(name, dir_fd=fd, follow_symlinks=False)), "export_readback")
        need(os.listdir(fd) == [name] and os.path.samestat(root, output.lstat()), "export_changed")
        os.fsync(fd)
    finally:
        os.close(fd)
    return receipt


if __name__ == "__main__":
    import json

    try:
        need(len(sys.argv) == 5 and sys.argv[1] == "--base-packet" and sys.argv[3] == "--output-directory",
             "explicit_inert_export_required")
        evidence = export_candidate(Path(sys.argv[2]), Path(sys.argv[4]))
        print(json.dumps(evidence, sort_keys=True, separators=(",", ":")))
    except (ValueError, OSError) as error:
        message = str(error)
        print(message if message.startswith("signup_sample_code_refused:") else "signup_sample_code_refused:input",
              file=sys.stderr)
        raise SystemExit(1) from None
