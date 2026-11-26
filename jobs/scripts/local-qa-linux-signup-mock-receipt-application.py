"""Pin and stage only the current Linux signup/mock QA application inputs.

Verification is portable. Staging delegates to the measured Linux root
installer, including its independent admission callbacks. Neither can launch
the browser, API, fixture, or a service.
"""
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import tarfile


LEGACY = "local-qa-linux-c2c-install-application.py"
LEGACY_PIN = "070c53877083969708998fbc18a44acbf461e7550f8e313b3bcbe406f0d3c546"
NATIVE_PIN = "8e923b0ec14fccc9ddb1ad13153fd4fd7831825dd73281155fe81f6123f361e3"
SOURCE_INVENTORY = "08459c30b037e839c798b4c5859dcbd2bb271905da36bcc3cdd7aaa4241eb7cc"
SOURCE_TREE = "6fe9eab0180160bc70079f8b9020fa21abf0db62"
BASE_COMMIT = "e1108052e60f19fbd5ef5af534190c52978eb51f"
PORTAL_PIN = "db31a9a96ca40be18d882516c42c930cdf8f6be02e5add04dee28d289336e0f0"
PORTAL_MANIFEST = "ba874296d043ceb15ddae0893981b92ed8fd651c46abdb55a46f8b2637c43aa6"
CODE_PIN = "ff7df61110b4521faadef1a10f6f58a5396657d8f8ad537a7ab65648f1bebd29"
CODE_INVENTORY = "99035f76562ad58570f6f79e71907c02e53a0a14d69fae5b2d4a19e439930c56"
SCRIPT = "/assets/bluey-site-bf339a056cfb3c42b3e8fb8431d6debe8328635c04fce31094b54c90208c00e5.js"
ARTIFACTS = {
    "bluey-server": (108_786_264, "29a50252ff22e740aea9e90a513a00eb82892849d0abfa52b76b2c27588f8884"),
    "jobs-nonprod-browser-fixture": (87_021_888, "8b10ffebe32a4488964533ac0af5d25911ddafbb389985d01d7c14629cb8fd0c"),
}
ROUTE = re.compile(r"/(?:index\.html|(?:assets|jobs)/[A-Za-z0-9._/-]+)")
SHA = re.compile(r"[a-f0-9]{64}")


def need(ok, code):
    if not ok:
        raise ValueError("signup_mock_application_refused:" + code)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def pinned_wire(raw, expected, limit):
    need(type(raw) is bytes and 0 < len(raw) <= limit and sha(raw) == expected, "wire_pin")
    value = json.loads(raw.decode("ascii"))
    need(type(value) is dict and canonical(value) == raw, "wire_canonical")
    return value


def verify_native(raw, server, fixture):
    receipt = pinned_wire(raw, NATIVE_PIN, 16 << 10)
    need(receipt["schema"] == "bluey-jobs-current-linux-mock-receipt-qa-v1" and
         receipt["sourceProjectionSha256"] == SOURCE_INVENTORY and receipt["checkedOutBaseCommit"] == BASE_COMMIT
         and receipt["qaOnly"] is True and receipt["strictClippyScopes"] == 2
         and all(receipt[key] is False for key in ("releaseArtifact", "runtimeQualified", "artifactsExecuted")),
         "native_scope")
    rows = {row["file"]: row for row in receipt["artifacts"]}
    need(len(receipt["artifacts"]) == 2 and rows.keys() == ARTIFACTS.keys(), "native_roles")
    for name, binary in (("bluey-server", server), ("jobs-nonprod-browser-fixture", fixture)):
        row = rows[name]
        is_fixture = name == "jobs-nonprod-browser-fixture"
        need(type(binary) is bytes and (len(binary), sha(binary)) == ARTIFACTS[name]
             and binary.startswith(b"\x7fELF\x02\x01\x01") and binary[18:20] == b"\x3e\0"
             and (row["bytes"], row["sha256"]) == ARTIFACTS[name]
             and row["cargoTarget"] == ("example" if is_fixture else "bin")
             and row["cargoFeatures"] == (["default", "integration-test-support"] if is_fixture else ["default"]),
             "native_artifact")
    return receipt


def verify_portal(raw):
    need(type(raw) is bytes and len(raw) == 5_509_120 and sha(raw) == PORTAL_PIN, "portal_pin")
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        first = archive.next()
        need(first.name == "static-manifest.json" and first.isfile() and 0 < first.size < 64 << 10, "manifest_member")
        with archive.extractfile(first) as stream:
            manifest_raw = stream.read(first.size + 1)
        manifest = pinned_wire(manifest_raw, PORTAL_MANIFEST, 64 << 10)
        need(len(manifest) == 43 and {"/index.html", "/jobs/index.html", SCRIPT} <= manifest.keys(), "portal_routes")
        need(all(ROUTE.fullmatch(route) and ".." not in route and "//" not in route and SHA.fullmatch(digest)
                 for route, digest in manifest.items()), "portal_manifest")
        files = {}
        while (member := archive.next()) is not None:
            path = PurePosixPath(member.name)
            route = member.name[3:]
            need(path.parts and path.as_posix() == member.name and path.parts[0] == "web"
                 and route in manifest and member.name not in files and member.isfile()
                 and member.uid == member.gid == member.mtime == 0
                 and member.mode == 0o600 and not member.pax_headers and 0 < member.size <= 16 << 20, "portal_member")
            with archive.extractfile(member) as stream:
                content = stream.read(member.size + 1)
            need(len(content) == member.size and sha(content) == manifest[route], "portal_member_hash")
            files[member.name] = content
    need(files.keys() == {"web" + route for route in manifest}, "portal_inventory")
    need(SCRIPT.encode() in files["web/index.html"]
         and b'href="/login?mode=signup&amp;next=%2Fjobs"' in files["web/jobs/index.html"], "signup_binding")
    return manifest_raw, manifest, files


def verify_code(raw):
    need(type(raw) is bytes and len(raw) == 686_080 and sha(raw) == CODE_PIN, "code_pin")
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        meta = json.load(archive.extractfile("candidate.json"))
        inventory = archive.extractfile("actor/inventory.json").read()
    need(meta["schema"] == "bluey-jobs-signup-mock-receipt-code-candidate-v1"
         and meta["actor"] == "local-qa-linux-signup-mock-receipt-native.mjs"
         and meta["actorFiles"] == 39 and meta["managerFiles"] == 12
         and meta["codeInventorySha256"] == sha(inventory) == CODE_INVENTORY
         and all(meta[key] is False for key in ("physicalSourceAdmitted", "releaseAdmitted", "runtimeQualified")),
         "code_scope")


def load_installer():
    path = Path(__file__).resolve(strict=True).with_name(LEGACY)
    need(path.resolve(strict=True) == path, "installer_path")
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as stream:
        before = os.fstat(stream.fileno())
        need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and not before.st_mode & 0o7022
             and 0 < before.st_size <= 64 << 10, "installer_stat")
        raw = stream.read(before.st_size + 1)
        def stamp(row):
            return (row.st_dev, row.st_ino, row.st_mode, row.st_uid, row.st_size, row.st_mtime_ns, row.st_ctime_ns)
        need(len(raw) == before.st_size and sha(raw) == LEGACY_PIN
             and stamp(before) == stamp(os.fstat(stream.fileno())) == stamp(path.lstat()), "installer_pin")
    scope = {"__file__": str(path), "__name__": "_verified_current_application_io"}
    exec(compile(raw, str(path), "exec"), scope)
    return scope


def prepare_application(native_receipt, server, fixture, portal, code):
    """Return only fixed transport bytes and raw application roles."""
    native = verify_native(native_receipt, server, fixture)
    manifest_raw, manifest, files = verify_portal(portal)
    verify_code(code)
    resolver = canonical({"graphs": native["graphs"], "nativeReceiptSha256": NATIVE_PIN,
                          "schema": "bluey-jobs-qa-mock-offline-graphs-v1"})
    portal_receipt = canonical({"archiveSha256": PORTAL_PIN, "codePacketSha256": CODE_PIN,
                                "manifestSha256": PORTAL_MANIFEST, "staticFiles": len(files),
                                "schema": "bluey-jobs-qa-mock-application-static-v1"})
    return resolver, portal_receipt, manifest_raw, manifest, files


def install_application(*, run, reservation_sha256, source_tree, native_receipt, server, fixture, portal, code, admitted):
    """Linux-only staging once a separately authorized caller admits the exact run."""
    need(source_tree == SOURCE_TREE and callable(admitted), "source_tree")
    scope = load_installer()
    need(scope["_initial_root"](), "platform")
    resolver, portal_receipt, manifest_raw, manifest, files = prepare_application(
        native_receipt, server, fixture, portal, code,
    )

    def native_check(receipt, graph):
        need(receipt == native_receipt and graph == resolver, "native_rebind")

    def portal_check(seal, receipt, archive):
        need(seal == manifest_raw and receipt == portal_receipt and archive == portal, "portal_rebind")
        return manifest, json.loads(portal_receipt), files

    scope.update(TREE=SOURCE_TREE, NATIVE_ARTIFACTS=dict(ARTIFACTS), _native=native_check, _portal=portal_check)
    return scope["install_application"](
        run=run, reservation_sha256=reservation_sha256, native_receipt=native_receipt,
        resolver=resolver, server=server, fixture=fixture, portal_archive=portal,
        portal_manifest=manifest_raw, portal_receipt=portal_receipt, admitted=admitted,
    )
