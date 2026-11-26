"""Pure expected-file projection for one sealed C2C Jobs QA release.

This module reads no paths and starts no processes.  It turns the already
pinned manifests used by the existing installers into the five closed trees
that are immutable before launch.  The gate plan is checked separately because
the release publisher owns additional leaves in that directory.
"""
import hashlib
import json
from pathlib import PurePosixPath
import re


RUN = re.compile(r"[0-9a-f]{32}")
SHA = re.compile(r"[0-9a-f]{64}")
SAFE = re.compile(r"[A-Za-z0-9_.+@%/-]{1,512}")
ACTOR_NAME = re.compile(r"local-qa-[a-z0-9-]{1,96}\.(?:py|mjs)")

BROWSER_INVENTORY_SHA = "9ecdff4561fe0924e499325c40a6b61ffbe7ea047044d3f2cc0a89f82e728dce"
BROWSER_INVENTORY_BYTES = 70_130
BROWSER_FILES = 410
BROWSER_PAYLOAD_BYTES = 533_939_096
NODE = "receipt-journey-node/toolchain/bin/node"
CHROME = "receipt-prep-browser/chromium/chrome"
CRASHPAD = "receipt-prep-browser/chromium/chrome_crashpad_handler"
BROWSER_EXECUTABLES = frozenset({NODE, CHROME, CRASHPAD})
NODE_SHA = "94efd062ed066c1a0b332e18249859c905e920eeada5df55d24c99b919629a90"

POSTGRES_MANIFEST_SHA = "e0f68a27c049e69aff5d789051ad731ddce5f41d120e46bdd21ba994bb46dc92"
POSTGRES_MANIFEST_BYTES = 240_820
POSTGRES_MEMBERS = 1_640
POSTGRES_REGULAR_FILES = 1_639
POSTGRES_DIRECTORIES = 150
POSTGRES_PAYLOAD_BYTES = 47_718_773
POSTGRES_LINK = "usr/lib/x86_64-linux-gnu/libpq.so.5"
POSTGRES_LINK_TARGET = "libpq.so.5.18"

PORTAL_MANIFEST_SHA = "1441b6884155c4d78d0ecfeeff7709ba020a70f584ed29cbd2c07f248cf20469"
PORTAL_MANIFEST_BYTES = 4_568
PORTAL_FILES = 44
NATIVE_RECEIPT = (2_047, "7995aef600a191f0b02d3f11012a4f503adf4c32219a0fbd7c9cd40473930e3e")
RESOLVER = (507, "f6572c944641495b8933bbffd463ad1e773bd38c426cf3f73549945b3685d541")
PORTAL_RECEIPT = (1_608, "3d88967ea03dfac2e7cc704d2857f985247087c6819071144f270d5f47fc3586")
SERVER = (111_198_656, "8adff1b0bf73d451bc58ad7e7050658693a3dbae2b4f379ea0c73779b906351f")
FIXTURE = (29_931_688, "0c4f7908d8f3fece7369921385a44a7f7023796ccfc19409e0042c324710fa94")

ACTOR_INVENTORY_SHA = "a33491cd0e3a60b971450534e5dc9eab8895d61e81fb2bff26cd013a79628e3f"
ACTOR_FILES = 31
SOURCE_TREE = "8e971541d3ab8d1b8f5c4c9eebafa8fec81406d5"
SOURCE_INVENTORY_SHA = "a7dd3071bd03f53718ceb9db8b4de9571fe0aa1a6ef6e29f840a2b20e6a89ea5"

LAUNCHER_SHA = "8ad1d3f2d3976b88da64a55b1de9111997603115c7a56ae2d5d0a0afdcbb55c8"
LAUNCHER_BYTES = 94_631
GIT_HEAD = b"ref: refs/heads/bluey-c2c-qa\n"
GIT_CONFIG = b"[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tlogallrefupdates = false\n"


def need(value, label):
    if not value:
        raise ValueError("c2c_release_inventory_refused:" + label)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def wire(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")


def checked_run(run):
    need(type(run) is str and RUN.fullmatch(run) is not None and run != "0" * 32, "run")
    return run


def json_value(raw, expected_size, expected_sha, label, *, sort_keys):
    need(type(raw) is bytes and len(raw) == expected_size and sha(raw) == expected_sha, label + "_pin")
    try:
        value = json.loads(raw.decode("ascii"))
    except (UnicodeError, ValueError, RecursionError) as error:
        raise ValueError("c2c_release_inventory_refused:" + label + "_json") from error
    canonical = (json.dumps(value, sort_keys=sort_keys, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
    need(canonical == raw, label + "_canonical")
    return value


def relative(name):
    need(type(name) is str and SAFE.fullmatch(name) is not None and not name.startswith("/"), "relative_path")
    path = PurePosixPath(name)
    need(path.as_posix() == name and all(part not in ("", ".", "..", ".git") for part in path.parts),
         "relative_path")
    return path


def closed_directories(files, mode, *, fixed=None):
    names = {"."}
    for name in files:
        path = relative(name)
        names.update(parent.as_posix() for parent in path.parents if parent.as_posix() != ".")
    need(not names.intersection(files), "tree_collision")
    modes = {name: mode for name in names}
    if fixed:
        need(set(fixed) <= names, "directory_shape")
        modes.update(fixed)
    return dict(sorted(modes.items()))


def browser_tree(raw):
    value = json_value(raw, BROWSER_INVENTORY_BYTES, BROWSER_INVENTORY_SHA, "browser", sort_keys=False)
    need(type(value) is dict and value.keys() == {"version", "files"} and value["version"] == 1
         and type(value["files"]) is list and len(value["files"]) == BROWSER_FILES, "browser_shape")
    files, prior, total = {}, "", 0
    for row in value["files"]:
        need(type(row) is dict and row.keys() == {"path", "sha256", "size", "mode"}, "browser_row")
        name, digest, size, mode = row["path"], row["sha256"], row["size"], row["mode"]
        path = relative(name)
        need(name > prior and (name == NODE or name.startswith(("receipt-prep-browser/chromium/",
              "receipt-prep-browser/node_modules/playwright-core/")))
             and type(digest) is str and SHA.fullmatch(digest) is not None
             and type(size) is int and 0 <= size <= 320 << 20
             and type(mode) is int and mode == (0o555 if name in BROWSER_EXECUTABLES else 0o444)
             and not any(parent.as_posix() in files for parent in path.parents), "browser_member")
        files[name] = ("file", mode, size, digest)
        prior, total = name, total + size
    need(BROWSER_EXECUTABLES <= files.keys() and total == BROWSER_PAYLOAD_BYTES
         and files[NODE][3] == NODE_SHA, "browser_totals")
    files["inventory.json"] = ("file", 0o444, len(raw), BROWSER_INVENTORY_SHA)
    return closed_directories(files, 0o555), dict(sorted(files.items()))


def postgres_tree(raw):
    value = json_value(raw, POSTGRES_MANIFEST_BYTES, POSTGRES_MANIFEST_SHA, "postgres", sort_keys=False)
    need(type(value) is list and len(value) == POSTGRES_MEMBERS, "postgres_shape")
    files, prior, regular, links, total = {}, "", 0, 0, 0
    for row in value:
        need(type(row) is list and len(row) in (4, 5) and type(row[0]) is str, "postgres_row")
        name = row[0]
        relative(name)
        need(name > prior and name.startswith("usr/") and name not in files, "postgres_path")
        if len(row) == 5:
            kind, mode, size, digest = row[1:]
            need(kind == "file" and type(mode) is int and mode in (0o444, 0o555)
                 and type(size) is int and 0 <= size <= 24 << 20
                 and type(digest) is str and SHA.fullmatch(digest) is not None, "postgres_file")
            files[name] = ("file", mode, size, digest)
            regular, total = regular + 1, total + size
        else:
            need(row == [POSTGRES_LINK, "link", 0o777, POSTGRES_LINK_TARGET], "postgres_link")
            files[name] = ("link", 0o777, None, POSTGRES_LINK_TARGET)
            links += 1
        prior = name
    directories = closed_directories(files, 0o555)
    need(regular == POSTGRES_REGULAR_FILES and links == 1 and total == POSTGRES_PAYLOAD_BYTES
         and len(directories) - 1 == POSTGRES_DIRECTORIES
         and str(PurePosixPath(POSTGRES_LINK).with_name(POSTGRES_LINK_TARGET)) in files,
         "postgres_totals")
    files["files.json"] = ("file", 0o444, len(raw), POSTGRES_MANIFEST_SHA)
    return directories, dict(sorted(files.items()))


def application_tree(raw):
    value = json_value(raw, PORTAL_MANIFEST_BYTES, PORTAL_MANIFEST_SHA, "portal", sort_keys=True)
    need(type(value) is dict and len(value) == PORTAL_FILES, "portal_shape")
    files = {
        "api/bluey-server": ("file", 0o555, SERVER[0], SERVER[1]),
        "fixture/jobs-nonprod-browser-fixture": ("file", 0o500, FIXTURE[0], FIXTURE[1]),
        "evidence/native-receipt.json": ("file", 0o400, NATIVE_RECEIPT[0], NATIVE_RECEIPT[1]),
        "evidence/resolver.json": ("file", 0o400, RESOLVER[0], RESOLVER[1]),
        "evidence/portal-receipt.json": ("file", 0o400, PORTAL_RECEIPT[0], PORTAL_RECEIPT[1]),
        "portal/static-manifest.json": ("file", 0o444, len(raw), PORTAL_MANIFEST_SHA),
    }
    for route, digest in value.items():
        need(type(route) is str and route.startswith("/") and route != "/" and ".." not in route
             and type(digest) is str and SHA.fullmatch(digest) is not None, "portal_row")
        name = "portal/web/" + route[1:]
        relative(name)
        need(name not in files, "portal_path")
        files[name] = ("file", 0o444, None, digest)
    need(len(files) == PORTAL_FILES + 6, "portal_totals")
    directories = closed_directories(files, 0o555, fixed={".": 0o755, "fixture": 0o700, "evidence": 0o700})
    return directories, dict(sorted(files.items()))


def actor_tree(raw):
    need(type(raw) is bytes and 0 < len(raw) <= 128 << 10 and sha(raw) == ACTOR_INVENTORY_SHA,
         "actor_pin")
    try:
        value = json.loads(raw.decode("ascii"))
    except (UnicodeError, ValueError, RecursionError) as error:
        raise ValueError("c2c_release_inventory_refused:actor_json") from error
    need(wire(value) == raw and type(value) is dict and value.keys() == {"version", "files"}
         and value["version"] == 1 and type(value["files"]) is list
         and len(value["files"]) == ACTOR_FILES, "actor_shape")
    files, prior = {}, ""
    for row in value["files"]:
        need(type(row) is dict and row.keys() == {"name", "bytes", "sha256"}, "actor_row")
        name, size, digest = row["name"], row["bytes"], row["sha256"]
        need(type(name) is str and ACTOR_NAME.fullmatch(name) is not None and name > prior
             and type(size) is int and 0 < size <= 128 << 10
             and type(digest) is str and SHA.fullmatch(digest) is not None, "actor_member")
        files[name] = ("file", 0o444, size, digest)
        prior = name
    files["inventory.json"] = ("file", 0o444, len(raw), ACTOR_INVENTORY_SHA)
    return {".": 0o755}, dict(sorted(files.items()))


def launcher_tree():
    files = {
        ".git/HEAD": ("file", 0o444, len(GIT_HEAD), sha(GIT_HEAD)),
        ".git/config": ("file", 0o444, len(GIT_CONFIG), sha(GIT_CONFIG)),
        "scripts/run-bluey-tests.sh": ("file", 0o555, LAUNCHER_BYTES, LAUNCHER_SHA),
    }
    directories = {name: 0o555 for name in (".", ".git", ".git/objects", ".git/refs",
                                                      ".git/refs/heads", "scripts")}
    return directories, files


def expected_plan(run):
    run = checked_run(run)
    return wire({"schema": "bluey-jobs-c2c-actor-plan-v1", "run": run, "sourceTree": SOURCE_TREE,
                 "sourceInventorySha256": SOURCE_INVENTORY_SHA,
                 "codeInventorySha256": ACTOR_INVENTORY_SHA,
                 "node": {"path": f"/run/bluey-jobs-c2c-browser/{run}/{NODE}", "sha256": NODE_SHA}})


def expected_trees(run, *, browser, postgres, portal, actor, plan):
    """Return the five immutable installed trees for one already pinned run."""
    run = checked_run(run)
    need(type(plan) is bytes and plan == expected_plan(run), "plan")
    return {
        f"/opt/bluey-jobs-c2c-launcher/{run}": launcher_tree(),
        f"/run/bluey-jobs-c2c-application/{run}": application_tree(portal),
        f"/run/bluey-jobs-c2c-browser/{run}": browser_tree(browser),
        f"/run/bluey-jobs-c2c-postgres/{run}/capsule": postgres_tree(postgres),
        f"/run/bluey-jobs-c2c-code/{run}": actor_tree(actor),
    }
