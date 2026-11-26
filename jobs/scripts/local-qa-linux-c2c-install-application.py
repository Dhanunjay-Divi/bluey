"""Install the sealed 8e971 combined QA server, fixture, and portal once.

This is deliberately a staging primitive: it never executes an ELF, starts a
service, changes a unit, or publishes a release.  Its caller must separately
admit the exact run before and after this bounded write operation.
"""
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import stat
import tarfile


BASE = Path("/run/bluey-jobs-c2c-application")
TREE = "8e971541d3ab8d1b8f5c4c9eebafa8fec81406d5"
SOURCE_RECEIPT = "671f1bd26e077f346bc0f8652ac7c45b520469a1c0c982522746c33d2cf77a60"
SOURCE_INVENTORY = "a7dd3071bd03f53718ceb9db8b4de9571fe0aa1a6ef6e29f840a2b20e6a89ea5"
SOURCE_ARCHIVE = "6ed16a7a17ba2c324a54d847574b05945a8ceeb21b8548ea3644f7af21a55f44"
BUILD_MAP = "7965590341c27d19c58520cc81570d4c770a68626fe2caf90e9155cadffd3285"
FIXTURE_SOURCE = "b431616f47f2f7ccb752e64c6ddaec7971f8a10c868c2fbbed1b5ea4f5961f38"
NATIVE_RECEIPT = "7995aef600a191f0b02d3f11012a4f503adf4c32219a0fbd7c9cd40473930e3e"
RESOLVER = "f6572c944641495b8933bbffd463ad1e773bd38c426cf3f73549945b3685d541"
LOCK = "23125668f1c5560bf5ffce020e81b4d2337f3b4c86be5a29ba06b063e45bffc9"
PROJECTION = "fae6b3df7ba95ddfaffc9b3e19a73d2da48734f93fcf54c7dde9cc85d343f956"
PORTAL_ARCHIVE = "9b1090ef7e0a5fc98e1bcfeb34ed9cac4f7e42d7b09734400028f33a97ab3199"
PORTAL_MANIFEST = "1441b6884155c4d78d0ecfeeff7709ba020a70f584ed29cbd2c07f248cf20469"
PORTAL_RECEIPT = "3d88967ea03dfac2e7cc704d2857f985247087c6819071144f270d5f47fc3586"
RUN = re.compile(r"^[0-9a-f]{32}$")
SHA = re.compile(r"^[0-9a-f]{64}$")
MAX_RECEIPT = 64 << 10
MAX_PORTAL = 2 << 20
RD = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
DIR = RD | os.O_DIRECTORY

NATIVE_ARTIFACTS = {
    "bluey-server": (111198656, "8adff1b0bf73d451bc58ad7e7050658693a3dbae2b4f379ea0c73779b906351f"),
    "bluey-jobs-api": (96910032, "85627b351428fdc60b7e2ebb9909ab801294c12071d9d5c9d5dc957092f789f1"),
    "jobs-nonprod-browser-fixture": (29931688, "0c4f7908d8f3fece7369921385a44a7f7023796ccfc19409e0042c324710fa94"),
}


def _need(value, label):
    if not value:
        raise RuntimeError("local_qa_linux_c2c_application_refused:" + label)


def _sha(raw):
    return hashlib.sha256(raw).hexdigest()


def _stamp(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode,
            info.st_nlink, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def _directory_identity(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_gid, stat.S_IMODE(info.st_mode))


def _initial_root():
    return (platform.system() == "Linux" and platform.machine() == "x86_64"
            and os.getuid() == os.geteuid() == 0
            and all(os.path.samestat(os.stat("/proc/self/ns/" + kind), os.stat("/proc/1/ns/" + kind))
                    for kind in ("mnt", "user", "pid")))


def _json(raw, maximum):
    _need(type(raw) is bytes and 0 < len(raw) <= maximum, "wire_size")
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("local_qa_linux_c2c_application_refused:wire_json") from error
    return value


def _native(receipt_raw, resolver_raw):
    _need(_sha(receipt_raw) == NATIVE_RECEIPT and _sha(resolver_raw) == RESOLVER, "native_wire_pin")
    receipt = _json(receipt_raw, MAX_RECEIPT)
    resolver = _json(resolver_raw, MAX_RECEIPT)
    _need(json.dumps(receipt, indent=2).encode() + b"\n" == receipt_raw
          and json.dumps(resolver, sort_keys=True).encode() + b"\n" == resolver_raw, "native_wire_canonical")
    expected = {"artifacts", "artifactsExecuted", "buildExecuted", "buildSourceMapSha256",
                "fixtureSourceSha256", "offlineResolver", "qaOnly", "releaseArtifact",
                "runtimeQualified", "schema", "sourceArchiveSha256", "sourceInventorySha256",
                "sourceReceiptSha256", "sourceRows", "sourceTree", "target", "toolchainAdmissionBase"}
    _need(type(receipt) is dict and set(receipt) == expected
          and receipt["schema"] == "bluey-jobs-c2c-native-crossbuild-qa-v1"
          and receipt["sourceTree"] == TREE and receipt["sourceReceiptSha256"] == SOURCE_RECEIPT
          and receipt["sourceInventorySha256"] == SOURCE_INVENTORY and receipt["sourceArchiveSha256"] == SOURCE_ARCHIVE
          and receipt["buildSourceMapSha256"] == BUILD_MAP and receipt["fixtureSourceSha256"] == FIXTURE_SOURCE
          and receipt["sourceRows"] == 938 and receipt["target"] == "x86_64-unknown-linux-gnu.2.39"
          and receipt["buildExecuted"] is True and receipt["qaOnly"] is True
          and all(receipt[key] is False for key in ("artifactsExecuted", "releaseArtifact", "runtimeQualified")),
          "native_receipt")
    _need(type(resolver) is dict and resolver == receipt["offlineResolver"]
          and resolver == {"feature": "integration-test-support", "lockSha256": LOCK, "offlineLocked": True,
                           "packageProjectionSha256": PROJECTION, "resolvedPackages": 353,
                           "schema": "bluey-jobs-c2c-native-offline-resolver-v1",
                           "sourceReceiptSha256": SOURCE_RECEIPT, "sourceTree": TREE,
                           "target": "x86_64-unknown-linux-gnu"}, "resolver")
    _need(type(receipt["artifacts"]) is list and len(receipt["artifacts"]) == 3, "native_artifacts")
    rows = {}
    for row in receipt["artifacts"]:
        _need(type(row) is dict and set(row) == {"bytes", "name", "sha256", "spoolName"}
              and row["spoolName"] in NATIVE_ARTIFACTS and row["spoolName"] not in rows
              and row["name"] == ("examples/jobs-nonprod-browser-fixture" if row["spoolName"] == "jobs-nonprod-browser-fixture" else row["spoolName"])
              and (row["bytes"], row["sha256"]) == NATIVE_ARTIFACTS[row["spoolName"]], "native_artifact")
        rows[row["spoolName"]] = row
    _need(list(rows) == ["bluey-server", "bluey-jobs-api", "jobs-nonprod-browser-fixture"], "native_order")
    return receipt, resolver


def _portal(manifest_raw, receipt_raw, archive_raw):
    manifest = _json(manifest_raw, 128 << 10)
    receipt = _json(receipt_raw, MAX_RECEIPT)
    _need(_sha(manifest_raw) == PORTAL_MANIFEST and _sha(receipt_raw) == PORTAL_RECEIPT
          and _sha(archive_raw) == PORTAL_ARCHIVE and len(archive_raw) == 1508201
          and json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode() + b"\n" == manifest_raw
          and json.dumps(receipt, sort_keys=True, separators=(",", ":")).encode() + b"\n" == receipt_raw
          and type(manifest) is dict and len(manifest) == 44
          and all(type(name) is str and name.startswith("/") and ".." not in name
                  and type(digest) is str and SHA.fullmatch(digest) for name, digest in manifest.items()), "portal_manifest")
    required = {"apiExecuted", "archiveBytes", "archiveSha256", "browserExecuted", "c2cHandoffTests",
                "inputMapSha256", "inputRows", "manifestSha256", "nativeSourceReceiptSha256", "portalBundleSha256",
                "portalFiles", "postgresExecuted", "releaseAdmitted", "resourcesByHtml", "schema",
                "sourceInventorySha256", "sourceTree", "staticChecks", "staticFiles", "testCounts"}
    _need(type(receipt) is dict and set(receipt) == required
          and receipt["schema"] == "bluey-jobs-c2c-exact-static-qa-v1"
          and receipt["sourceTree"] == TREE and receipt["sourceInventorySha256"] == SOURCE_INVENTORY
          and receipt["nativeSourceReceiptSha256"] == SOURCE_RECEIPT
          and receipt["manifestSha256"] == PORTAL_MANIFEST and receipt["archiveSha256"] == PORTAL_ARCHIVE
          and receipt["archiveBytes"] == len(archive_raw) and receipt["staticFiles"] == 44
          and receipt["portalFiles"] == 36 and receipt["inputRows"] == 123
          and all(receipt[key] is False for key in ("apiExecuted", "browserExecuted", "postgresExecuted", "releaseAdmitted")),
          "portal_receipt")
    files = {}
    try:
        with tarfile.open(fileobj=io.BytesIO(archive_raw), mode="r:gz") as archive:
            members = archive.getmembers()
            _need(len(members) == 45 and members[0].name == "static-manifest.json", "portal_archive_members")
            for member in members:
                _need(member.type == tarfile.REGTYPE and member.uid == member.gid == member.mtime == 0
                      and member.mode == 0o600 and not member.linkname and not member.pax_headers
                      and member.name not in files and not member.name.startswith("/") and ".." not in PurePosixPath(member.name).parts,
                      "portal_member")
                raw = archive.extractfile(member).read(member.size + 1)
                _need(len(raw) == member.size, "portal_member_size")
                files[member.name] = raw
    except (tarfile.TarError, OSError, EOFError) as error:
        raise RuntimeError("local_qa_linux_c2c_application_refused:portal_archive") from error
    _need(files.pop("static-manifest.json", None) == manifest_raw and len(files) == 44, "portal_manifest_member")
    for path, raw in files.items():
        route = "/" + path.removeprefix("web/")
        _need(route in manifest and _sha(raw) == manifest[route], "portal_file_hash")
    _need(set(manifest) == {"/" + name.removeprefix("web/") for name in files}, "portal_paths")
    return manifest, receipt, files


def _open_base():
    _need(BASE.resolve(strict=True) == BASE, "base_path")
    for item in (BASE, *BASE.parents):
        row = item.lstat()
        _need(stat.S_ISDIR(row.st_mode) and row.st_uid == 0 and not row.st_mode & 0o7022, "base_ancestry")
    fd = os.open(BASE, DIR)
    try:
        info = os.fstat(fd)
        _need(stat.S_ISDIR(info.st_mode) and info.st_uid == info.st_gid == 0 and stat.S_IMODE(info.st_mode) == 0o755
              and os.path.samestat(info, BASE.lstat()), "base")
        return fd, info
    except BaseException:
        os.close(fd)
        raise


def _write(parent, name, raw, mode):
    _need("/" not in name and raw, "leaf")
    fd = os.open(name, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, mode, dir_fd=parent)
    try:
        view = memoryview(raw)
        while view:
            written = os.write(fd, view)
            _need(written > 0, "write")
            view = view[written:]
        os.fchmod(fd, mode)
        os.fsync(fd)
        info = os.fstat(fd)
        _need(stat.S_ISREG(info.st_mode) and info.st_uid == info.st_gid == 0 and stat.S_IMODE(info.st_mode) == mode
              and info.st_nlink == 1 and info.st_size == len(raw)
              and os.pread(fd, len(raw) + 1, 0) == raw and os.path.samestat(info, os.stat(name, dir_fd=parent, follow_symlinks=False)),
              "write_readback")
    finally:
        os.close(fd)


def _mkdir(parent, name, mode):
    os.mkdir(name, mode, dir_fd=parent)
    fd = os.open(name, DIR, dir_fd=parent)
    try:
        os.fchmod(fd, mode)
        os.fsync(fd)
        os.fsync(parent)
        info = os.fstat(fd)
        _need(stat.S_ISDIR(info.st_mode) and info.st_uid == info.st_gid == 0 and stat.S_IMODE(info.st_mode) == mode
              and os.path.samestat(info, os.stat(name, dir_fd=parent, follow_symlinks=False)), "directory")
        return fd
    except BaseException:
        os.close(fd)
        raise


def _put_path(root_fd, path, raw):
    parts = PurePosixPath(path).parts
    _need(parts and all(part not in (".", "..") for part in parts), "portal_path")
    current = root_fd
    held = []
    try:
        for part in parts[:-1]:
            try:
                child = os.open(part, DIR, dir_fd=current)
            except FileNotFoundError:
                child = _mkdir(current, part, 0o555)
            held.append(child)
            current = child
        _write(current, parts[-1], raw, 0o444)
    finally:
        for fd in reversed(held):
            os.close(fd)


def _read_leaf(directory, leaf, expected, mode, *, owner, group):
    fd = os.open(leaf, RD, dir_fd=directory)
    try:
        before = os.fstat(fd)
        _need(stat.S_ISREG(before.st_mode) and (before.st_uid, before.st_gid) == (owner, group)
              and stat.S_IMODE(before.st_mode) == mode and before.st_nlink == 1
              and before.st_size == len(expected), "final_leaf_metadata")
        view, cursor = memoryview(expected), 0
        while cursor < len(view):
            data = os.read(fd, min(1 << 20, len(view) - cursor))
            _need(bool(data) and data == view[cursor:cursor + len(data)], "final_leaf")
            cursor += len(data)
        named = os.stat(leaf, dir_fd=directory, follow_symlinks=False)
        _need(not os.read(fd, 1) and _stamp(before) == _stamp(os.fstat(fd)) == _stamp(named), "final_leaf")
    finally:
        os.close(fd)


def _verify_inventory(run_fd, server, fixture, native_receipt, resolver, portal_receipt, manifest, files,
                      *, owner=0, group=0, inert=False):
    expected = {"api/bluey-server": (server, 0o555), "fixture/jobs-nonprod-browser-fixture": (fixture, 0o500),
                "evidence/native-receipt.json": (native_receipt, 0o400), "evidence/resolver.json": (resolver, 0o400),
                "evidence/portal-receipt.json": (portal_receipt, 0o400), "portal/static-manifest.json": (manifest, 0o444),
                **{"portal/" + path: (raw, 0o444) for path, raw in files.items()}}
    directories = {name.as_posix() for path in expected for name in PurePosixPath(path).parents}
    dirs_seen, files_seen = set(), set()
    for where, subdirs, leaves, fd in os.fwalk(".", follow_symlinks=False, dir_fd=run_fd):
        here, info = PurePosixPath(where), os.fstat(fd)
        name = here.as_posix()
        desired = 0o700 if name in {"fixture", "evidence"} else 0o755 if name == "." else 0o555
        _need(name in directories and name not in dirs_seen and (info.st_uid, info.st_gid) == (owner, group)
              and stat.S_IMODE(info.st_mode) == (0o700 if inert else desired), "final_directory")
        dirs_seen.add(name)
        for subdir in subdirs:
            _need((here / subdir).as_posix() in directories
                  and stat.S_ISDIR(os.stat(subdir, dir_fd=fd, follow_symlinks=False).st_mode), "final_subdirectory")
        for leaf in leaves:
            path = (here / leaf).as_posix()
            _need(path in expected and path not in files_seen, "final_entries")
            raw, mode = expected[path]
            _read_leaf(fd, leaf, raw, 0o600 if inert else mode, owner=owner, group=group)
            files_seen.add(path)
        _need(_stamp(info) == _stamp(os.fstat(fd)), "final_directory_changed")
    _need(dirs_seen == directories and files_seen == expected.keys(), "final_inventory")


def install_application(*, run, reservation_sha256, native_receipt, resolver, server, fixture, portal_archive,
                        portal_manifest, portal_receipt, admitted):
    """Stage fixed 8e971 inputs under the one fixed runtime root.

    This journey uses the combined server's account, admin and Jobs routes on
    one isolated origin. The native receipt includes the Jobs-only binary,
    but that binary has no role in this journey and is not transferred here.
    The returned paths/digests do not authorize launch.
    """
    _need(_initial_root() and callable(admitted) and type(run) is str and RUN.fullmatch(run)
          and run != "0" * 32 and type(reservation_sha256) is str and SHA.fullmatch(reservation_sha256)
          and reservation_sha256 != "0" * 64, "platform")
    _native(native_receipt, resolver)
    _, _, files = _portal(portal_manifest, portal_receipt, portal_archive)
    payloads = {"bluey-server": server, "jobs-nonprod-browser-fixture": fixture}
    for name, raw in payloads.items():
        size, digest = NATIVE_ARTIFACTS[name]
        _need(type(raw) is bytes and len(raw) == size and _sha(raw) == digest and raw.startswith(b"\x7fELF"), "elf")
    cap = {"run": run, "reservationSha256": reservation_sha256, "sourceTree": TREE, "nativeReceiptSha256": _sha(native_receipt),
           "resolverSha256": _sha(resolver), "portalReceiptSha256": _sha(portal_receipt),
           "portalManifestSha256": _sha(portal_manifest)}
    _need(admitted(dict(cap)) is True and _initial_root(), "admitted_before")
    base, base_info = _open_base()
    run_fd = api_fd = fixture_fd = portal_fd = evidence_fd = -1
    try:
        try:
            os.stat(run, dir_fd=base, follow_symlinks=False)
        except FileNotFoundError:
            pass
        else:
            _need(False, "run_present")
        run_fd = _mkdir(base, run, 0o755)
        api_fd = _mkdir(run_fd, "api", 0o555)
        fixture_fd = _mkdir(run_fd, "fixture", 0o700)
        portal_fd = _mkdir(run_fd, "portal", 0o555)
        evidence_fd = _mkdir(run_fd, "evidence", 0o700)
        try:
            _write(api_fd, "bluey-server", server, 0o555)
            _write(fixture_fd, "jobs-nonprod-browser-fixture", fixture, 0o500)
            _write(evidence_fd, "native-receipt.json", native_receipt, 0o400)
            _write(evidence_fd, "resolver.json", resolver, 0o400)
            _write(evidence_fd, "portal-receipt.json", portal_receipt, 0o400)
            _write(portal_fd, "static-manifest.json", portal_manifest, 0o444)
            for path, raw in files.items():
                _put_path(portal_fd, path, raw)
            _need(admitted(dict(cap)) is True and _initial_root(), "admitted_after")
            _verify_inventory(run_fd, server, fixture, native_receipt, resolver, portal_receipt,
                              portal_manifest, files)
            now = os.fstat(base)
            _need(_directory_identity(now) == _directory_identity(base_info)
                  and BASE.resolve(strict=True) == BASE and os.path.samestat(now, BASE.lstat())
                  and os.path.samestat(os.fstat(run_fd), os.stat(run, dir_fd=base, follow_symlinks=False)), "base_changed")
        finally:
            for fd in (api_fd, fixture_fd, portal_fd, evidence_fd):
                if fd >= 0:
                    os.close(fd)
        result = {"root": str(BASE / run), "apiPath": str(BASE / run / "api/bluey-server"),
                  "fixturePath": str(BASE / run / "fixture/jobs-nonprod-browser-fixture"),
                  "webDirectory": str(BASE / run / "portal/web"),
                  "portalManifestPath": str(BASE / run / "portal/static-manifest.json"),
                  "nativeReceiptSha256": cap["nativeReceiptSha256"], "resolverSha256": cap["resolverSha256"],
                  "portalReceiptSha256": cap["portalReceiptSha256"], "portalManifestSha256": cap["portalManifestSha256"],
                  "sourceTree": TREE, "reservationSha256": reservation_sha256,
                  "releasePublished": False, "runtimeQualified": False}
        return result
    finally:
        if run_fd >= 0:
            os.close(run_fd)
        os.close(base)
