#!/usr/bin/env python3
"""Private D1 artifact transport. No image code, network client, or deletion operation.

Guest reader: one no-follow source FD, framed bytes and same-FD digest.
Host writer: all writes/reopens are relative to retained directory FDs. A path
replacement between operations refuses; it is never followed to another target.
Actual guest Python admission/throughput remains an explicit qualification gate.
"""
import hashlib
import base64
import importlib.util
import json
import os
import platform
import re
import secrets
import stat
import struct
import sys

CAPS = {"runner.oci.tar": 4 * 1024 ** 3, "inventory.json": 32 * 1024 ** 2,
        "measurement.json": 128 * 1024, "report.json": 16 * 1024}
ORDER = tuple(CAPS)
RESERVE = sum(CAPS.values()) + 1024 * 1024
FLOOR = 32 * 1024 ** 3
GUEST_ROOT = "/var/tmp/bluey-jobs-runner-image-p639"
READER_PATH = "/var/tmp/bluey-jobs-artifact-reader-p639/reader.py"
SOURCE_PATHS = {name: GUEST_ROOT + ("/fixture/" if name == "runner.oci.tar" else "/out/") + name
                for name in ORDER}
HEX = re.compile(r"^[a-f0-9]{64}$")
NOFOLLOW = os.O_NOFOLLOW
DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | NOFOLLOW


class Refusal(Exception):
    pass


def need(value, code):
    if not value:
        raise Refusal(code)


def keys(value, expected):
    need(type(value) is dict and set(value) == set(expected), "artifact_shape")


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=True, allow_nan=False).encode("ascii")


def decode(raw, limit):
    need(0 < len(raw) <= limit and raw.isascii(), "artifact_control_bound")
    value = json.loads(raw.decode("ascii"))
    need(canonical(value) == raw, "artifact_control_canonical")
    return value


def digest_fd(fd, limit):
    os.lseek(fd, 0, os.SEEK_SET)
    count, digest = 0, hashlib.sha256()
    while True:
        chunk = os.read(fd, 65536)
        if not chunk:
            return count, digest.hexdigest()
        count += len(chunk)
        need(count <= limit, "artifact_byte_bound")
        digest.update(chunk)


def identity(value):
    return {"dev": str(value.st_dev), "ino": str(value.st_ino), "uid": value.st_uid,
            "mode": stat.S_IMODE(value.st_mode), "nlink": value.st_nlink,
            "size": value.st_size, "mtimeNs": str(value.st_mtime_ns),
            "ctimeNs": str(value.st_ctime_ns)}


def regular(value, owner, cap, mode=None):
    need(stat.S_ISREG(value.st_mode) and value.st_uid == owner and value.st_nlink == 1
         and 0 < value.st_size <= cap and value.st_mode & 0o022 == 0
         and (mode is None or stat.S_IMODE(value.st_mode) == mode), "artifact_regular_file")


def open_absolute_directory(path):
    need(type(path) is str and path.startswith("/") and os.path.normpath(path) == path
         and path != "/" and os.path.realpath(path) == path, "artifact_directory_path")
    fd = os.open("/", DIRECTORY)
    try:
        for part in path.split("/")[1:]:
            next_fd = os.open(part, DIRECTORY, dir_fd=fd)
            os.close(fd)
            fd = next_fd
        return fd
    except BaseException:
        os.close(fd)
        raise


def open_source(path):
    directory, name = os.path.split(path)
    parent = open_absolute_directory(directory)
    fd = None
    try:
        fd = os.open(name, os.O_RDONLY | NOFOLLOW, dir_fd=parent)
        need(identity(os.fstat(fd)) == identity(os.stat(name, dir_fd=parent,
                                                      follow_symlinks=False)), "artifact_source_changed")
        return parent, name, fd
    except BaseException:
        if fd is not None:
            os.close(fd)
        os.close(parent)
        raise


def runtime_file(path):
    parent, name, fd = open_source(path)
    try:
        before = os.fstat(fd)
        regular(before, 0, 128 * 1024 ** 2)
        need(before.st_mode & 0o111 != 0, "artifact_runtime_not_executable")
        count, digest = digest_fd(fd, 128 * 1024 ** 2)
        need(count == before.st_size and identity(before) == identity(os.fstat(fd))
             == identity(os.stat(name, dir_fd=parent, follow_symlinks=False)), "artifact_runtime_changed")
        return {"canonicalPath": path, "version": platform.python_version(),
                "platform": platform.system(), "architecture": platform.machine(),
                "uid": before.st_uid, "mode": stat.S_IMODE(before.st_mode),
                "nlink": before.st_nlink, "size": count, "sha256": digest,
                "protocolVersion": 1}, {"dev": str(before.st_dev), "ino": str(before.st_ino)}
    finally:
        os.close(fd)
        os.close(parent)


def source_digest(path):
    parent, name, fd = open_source(path)
    try:
        before = os.fstat(fd)
        regular(before, before.st_uid, 128 * 1024)
        count, digest = digest_fd(fd, 128 * 1024)
        need(count == before.st_size and identity(before) == identity(os.fstat(fd))
             == identity(os.stat(name, dir_fd=parent, follow_symlinks=False)), "artifact_reader_changed")
        return digest
    finally:
        os.close(fd)
        os.close(parent)


def validate_guest_runtime_approval(approval):
    keys(approval, ("runtime", "readerSourceSha256"))
    runtime = approval["runtime"]
    keys(runtime, ("canonicalPath", "version", "platform", "architecture", "uid", "mode",
                   "nlink", "size", "sha256", "protocolVersion"))
    need(all(type(runtime[field]) is int for field in ("uid", "mode", "nlink", "size", "protocolVersion"))
         and 0 <= runtime["mode"] <= 0o777 and runtime["mode"] & 0o022 == 0
         and runtime["mode"] & 0o111 != 0 and 0 < runtime["size"] <= 128 * 1024 ** 2
         and re.fullmatch(r"3\.[0-9]{1,2}\.[0-9]{1,3}", runtime["version"]), "artifact_reader_approval")
    need(type(runtime["canonicalPath"]) is str
         and re.fullmatch(r"/usr/bin/python3\.[0-9]{1,2}", runtime["canonicalPath"])
         and runtime["platform"] == "Linux" and runtime["architecture"] == "x86_64"
         and runtime["protocolVersion"] == 1 and runtime["uid"] == 0
         and runtime["nlink"] == 1 and HEX.fullmatch(runtime["sha256"])
         and HEX.fullmatch(approval["readerSourceSha256"]), "artifact_reader_approval")
    return runtime


def guest_discover(request):
    # Observation only: this bootstrap cannot authorize a reader, transfer or image.
    keys(request, ("readerSourceSha256",))
    need(type(request["readerSourceSha256"]) is str
         and HEX.fullmatch(request["readerSourceSha256"]), "artifact_reader_approval")
    need(sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode
         and sys.flags.ignore_environment and os.getuid() == os.geteuid() == 0
         and sys.executable == "/usr/bin/python3"
         and os.path.realpath(__file__) == READER_PATH, "artifact_guest_runtime")
    path = os.path.realpath(sys.executable)
    need(re.fullmatch(r"/usr/bin/python3\.[0-9]{1,2}", path), "artifact_guest_runtime")
    runtime, _continuity = runtime_file(path)
    script = os.lstat(READER_PATH)
    regular(script, 0, 128 * 1024, 0o500)
    need(source_digest(READER_PATH) == request["readerSourceSha256"],
         "artifact_reader_approval_mismatch")
    candidate = {"runtime": runtime, "readerSourceSha256": request["readerSourceSha256"]}
    validate_guest_runtime_approval(candidate)
    return {"version": 1, "qualified": False, "candidate": candidate,
            "claims": {"image": False, "runnerStartup": False, "release": False}}


def guest_runtime(approval):
    runtime = validate_guest_runtime_approval(approval)
    need(sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode
         and sys.flags.ignore_environment and os.getuid() == os.geteuid() == 0,
         "artifact_guest_runtime")
    need(sys.executable == runtime["canonicalPath"]
         and os.path.realpath(sys.executable) == sys.executable
         and os.path.realpath(__file__) == READER_PATH, "artifact_guest_runtime")
    observed, continuity = runtime_file(sys.executable)
    script = os.lstat(READER_PATH)
    regular(script, 0, 128 * 1024, 0o500)
    need(observed == runtime and source_digest(READER_PATH) == approval["readerSourceSha256"],
         "artifact_reader_approval_mismatch")
    return {"runtime": observed, "continuity": continuity,
            "readerSourceSha256": approval["readerSourceSha256"]}


def frame(output, value):
    raw = canonical(value)
    need(len(raw) <= 4096, "artifact_frame_bound")
    output.write(struct.pack(">I", len(raw)))
    output.write(raw)
    output.flush()


def exact_read(stream, size):
    value = bytearray()
    while len(value) < size:
        chunk = stream.read(min(65536, size - len(value)))
        need(bool(chunk), "artifact_short_frame")
        value.extend(chunk)
    return bytes(value)


def read_frame(stream):
    size = struct.unpack(">I", exact_read(stream, 4))[0]
    need(0 < size <= 4096, "artifact_frame_bound")
    return decode(exact_read(stream, size), 4096)


def guest_archive_descriptor(approval, expected_reader, owner):
    need(type(owner) is int and 0 < owner <= 4294967295, "artifact_failed_descriptor")
    reader = guest_runtime(approval)
    need(reader == expected_reader, "artifact_reader_continuity")
    parent, leaf, fd = open_source(SOURCE_PATHS["runner.oci.tar"])
    try:
        before = os.fstat(fd)
        regular(before, owner, CAPS["runner.oci.tar"], 0o400)
        size, digest = digest_fd(fd, CAPS["runner.oci.tar"])
        need(size == before.st_size and identity(before) == identity(os.fstat(fd))
             == identity(os.stat(leaf, dir_fd=parent, follow_symlinks=False)), "artifact_source_changed")
        need(guest_runtime(approval) == reader, "artifact_reader_continuity")
        return {"version": 1, "artifact": "runner.oci.tar", "size": size,
                "sha256": digest, "source": identity(before), "reader": reader}
    finally:
        os.close(fd)
        os.close(parent)


def guest_stream(name, approval, expected_reader, expected_sha, owner, output, expected_source=None):
    need(name in CAPS and HEX.fullmatch(expected_sha), "artifact_id")
    reader = guest_runtime(approval)
    need(reader == expected_reader, "artifact_reader_continuity")
    parent, leaf, fd = open_source(SOURCE_PATHS[name])
    try:
        before = os.fstat(fd)
        regular(before, owner, CAPS[name])
        source = identity(before)
        need(expected_source is None or (name == "runner.oci.tar" and source == expected_source
             and source["mode"] == 0o400), "artifact_frozen_source_changed")
        frame(output, {"version": 1, "artifact": name, "size": before.st_size,
                       "source": source, "reader": reader})
        count, digest = 0, hashlib.sha256()
        while True:
            chunk = os.read(fd, 65536)
            if not chunk:
                break
            count += len(chunk)
            need(count <= before.st_size, "artifact_source_grew")
            digest.update(chunk)
            output.write(chunk)
        need(count == before.st_size and digest.hexdigest() == expected_sha,
             "artifact_source_digest")
        need(source == identity(os.fstat(fd))
             == identity(os.stat(leaf, dir_fd=parent, follow_symlinks=False)), "artifact_source_changed")
        need(guest_runtime(approval) == reader, "artifact_reader_continuity")
        frame(output, {"version": 1, "artifact": name, "size": count,
                       "sha256": digest.hexdigest(), "source": source})
    finally:
        os.close(fd)
        os.close(parent)


def guest_probe(approval):
    reader = guest_runtime(approval)
    # Actual O_NOFOLLOW and same-FD read/hash probe, before any builder starts.
    parent = open_absolute_directory(GUEST_ROOT)
    name = "reader-probe-" + secrets.token_hex(12)
    os.mkdir(name, 0o700, dir_fd=parent)
    directory = os.open(name, DIRECTORY, dir_fd=parent)
    try:
        fd = os.open("regular", os.O_RDWR | os.O_CREAT | os.O_EXCL | NOFOLLOW, 0o600,
                     dir_fd=directory)
        try:
            os.write(fd, b"bluey-d1-reader-probe\n")
            need(digest_fd(fd, 64) == (22, hashlib.sha256(b"bluey-d1-reader-probe\n").hexdigest()),
                 "artifact_reader_probe")
        finally:
            os.close(fd)
        os.symlink("regular", "alias", dir_fd=directory)
        rejected = False
        try:
            alias = os.open("alias", os.O_RDONLY | NOFOLLOW, dir_fd=directory)
            os.close(alias)
        except OSError as error:
            import errno
            rejected = error.errno == errno.ELOOP
        need(rejected, "artifact_reader_probe")
        need(guest_runtime(approval) == reader, "artifact_reader_continuity")
        return {"version": 1, "reader": reader, "noFollowProbe": True, "sameFdProbe": True}
    finally:
        os.close(directory)
        os.close(parent)
    # Probe files are intentionally retained within the exact disposable VM root.


def host_runtime(producer_path, expected_sha):
    need(source_digest(producer_path) == expected_sha, "artifact_host_producer_changed")
    spec = importlib.util.spec_from_file_location("artifact_host_identity", producer_path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    module.capture_local([os.getpid()])
    _, native = module.reviewed_python_pair(module.FilesystemObserver(), sys.executable)
    portable, continuity = runtime_file(native.canonical_path)
    need(source_digest(producer_path) == expected_sha, "artifact_host_producer_changed")
    return {"runtime": portable, "continuity": continuity, "producerSourceSha256": expected_sha,
            "writerSourceSha256": source_digest(os.path.realpath(__file__))}


def space(fd):
    info = os.fstatvfs(fd)
    need(info.f_bavail * info.f_frsize >= RESERVE + FLOOR, "artifact_free_space")


def directory_identity(fd):
    value = os.fstat(fd)
    need(stat.S_ISDIR(value.st_mode) and value.st_uid == os.getuid()
         and stat.S_IMODE(value.st_mode) == 0o700, "artifact_private_directory")
    return {"dev": str(value.st_dev), "ino": str(value.st_ino)}


def assert_persistent_destination(path):
    need(not re.search(r"/(?:bluey-test-runs-[^/]+|bluey-jobs-linux-p639-[^/]+)(?:/|$)", path),
         "artifact_destination_scope")


def parent_fd(request):
    path = request["parent"]
    need(type(path) is str and path.startswith("/"), "artifact_destination_scope")
    assert_persistent_destination(path)
    need(type(request["filesystemDev"]) is str and request["filesystemDev"].isdigit(),
         "artifact_filesystem")
    for forbidden in request["excludedRoots"]:
        need(type(forbidden) is str and forbidden.startswith("/")
             and not (path == forbidden or path.startswith(forbidden + "/")
                      or forbidden.startswith(path + "/")), "artifact_destination_scope")
    fd = open_absolute_directory(path)
    try:
        need(directory_identity(fd)["dev"] == request["filesystemDev"], "artifact_filesystem")
        space(fd)
        return fd
    except BaseException:
        os.close(fd)
        raise


def bound_directories(request, failed=False):
    binding = request["binding"]
    keys(binding, ("parentIdentity", "childIdentity", "childName", "hostRuntime"))
    pattern = r"phase639-failed-image-artifacts-[a-f0-9]{24}" if failed else r"phase639-image-artifacts-[a-f0-9]{24}"
    need(re.fullmatch(pattern, binding["childName"]),
         "artifact_child_name")
    parent = parent_fd(request)
    try:
        need(directory_identity(parent) == binding["parentIdentity"], "artifact_parent_changed")
        child = os.open(binding["childName"], DIRECTORY, dir_fd=parent)
        try:
            need(directory_identity(child) == binding["childIdentity"]
                 and binding["childIdentity"]["dev"] == binding["parentIdentity"]["dev"],
                 "artifact_child_changed")
            return parent, child
        except BaseException:
            os.close(child)
            raise
    except BaseException:
        os.close(parent)
        raise


def open_output(directory, name):
    space(directory)
    fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | NOFOLLOW, 0o600, dir_fd=directory)
    try:
        value = os.fstat(fd)
        need(stat.S_ISREG(value.st_mode) and value.st_nlink == 1 and value.st_uid == os.getuid()
             and stat.S_IMODE(value.st_mode) == 0o600
             and value.st_dev == os.fstat(directory).st_dev, "artifact_output_identity")
        return fd
    except BaseException:
        os.close(fd)
        raise


def write_all(fd, chunk):
    view = memoryview(chunk)
    while view:
        written = os.write(fd, view)
        need(written > 0, "artifact_write_failed")
        view = view[written:]


def readback(directory, name, cap, expected_sha=None):
    fd = os.open(name, os.O_RDONLY | NOFOLLOW, dir_fd=directory)
    try:
        before = os.fstat(fd)
        regular(before, os.getuid(), cap, 0o600)
        need(before.st_dev == os.fstat(directory).st_dev, "artifact_output_identity")
        size, digest = digest_fd(fd, cap)
        need(size == before.st_size and (expected_sha is None or digest == expected_sha)
             and identity(before) == identity(os.fstat(fd))
             == identity(os.stat(name, dir_fd=directory, follow_symlinks=False)), "artifact_readback")
        return {"name": name, "size": size, "sha256": digest, "identity": identity(before)}
    finally:
        os.close(fd)


def record(directory, name, value):
    raw = canonical(value)
    need(len(raw) <= 65536, "artifact_record_bound")
    fd = open_output(directory, name)
    try:
        write_all(fd, raw)
        os.fsync(fd)
        created = identity(os.fstat(fd))
    finally:
        os.close(fd)
    saved = readback(directory, name, 65536, hashlib.sha256(raw).hexdigest())
    need(saved["identity"] == created, "artifact_output_changed")
    os.fsync(directory)
    return saved


def read_record(directory, name):
    saved = readback(directory, name, 65536)
    fd = os.open(name, os.O_RDONLY | NOFOLLOW, dir_fd=directory)
    try:
        with os.fdopen(os.dup(fd), "rb", closefd=True) as stream:
            raw = exact_read(stream, saved["size"])
        need(identity(os.fstat(fd)) == saved["identity"], "artifact_record_changed")
        return decode(raw, 65536), saved
    finally:
        os.close(fd)


def host_check(request):
    need(os.open in os.supports_dir_fd and os.stat in os.supports_dir_fd
         and os.mkdir in os.supports_dir_fd and os.listdir in os.supports_fd,
         "artifact_fd_relative_unavailable")
    parent = parent_fd(request)
    try:
        need(os.listdir(parent) == [], "artifact_parent_not_empty")
        os.fsync(parent)
        return {"version": 1, "parentIdentity": directory_identity(parent)}
    finally:
        os.close(parent)


def validate_source_binding(source):
    keys(source, ("sourceCommit", "sourceArchiveSha256", "toolSourcesSha256",
                             "harnessSha256", "imageToolSourcesSha256"))
    need(re.fullmatch(r"[a-f0-9]{40}", source["sourceCommit"])
         and all(HEX.fullmatch(value) for name, value in source.items() if name != "sourceCommit"),
         "artifact_source_binding")


def validate_failed_descriptor(value):
    keys(value, ("version", "artifact", "size", "sha256", "source", "reader"))
    source = value["source"]
    keys(source, ("dev", "ino", "uid", "mode", "nlink", "size", "mtimeNs", "ctimeNs"))
    need(type(value["version"]) is int and value["version"] == 1
         and value["artifact"] == "runner.oci.tar" and type(value["size"]) is int
         and 0 < value["size"] <= CAPS["runner.oci.tar"]
         and type(value["sha256"]) is str and HEX.fullmatch(value["sha256"])
         and value["sha256"] != "0" * 64, "artifact_failed_descriptor")
    need(all(type(source[field]) is str and re.fullmatch(r"(?:0|[1-9][0-9]{0,23})", source[field])
             for field in ("dev", "ino", "mtimeNs", "ctimeNs"))
         and source["ino"] != "0"
         and type(source["uid"]) is int and 0 < source["uid"] <= 4294967295
         and type(source["mode"]) is int and source["mode"] == 0o400
         and type(source["nlink"]) is int and source["nlink"] == 1
         and type(source["size"]) is int and source["size"] == value["size"], "artifact_failed_descriptor")
    reader = value["reader"]
    keys(reader, ("runtime", "continuity", "readerSourceSha256"))
    validate_guest_runtime_approval({"runtime": reader["runtime"], "readerSourceSha256": reader["readerSourceSha256"]})
    keys(reader["continuity"], ("dev", "ino"))
    need(all(type(item) is str and re.fullmatch(r"(?:0|[1-9][0-9]{0,19})", item)
             for item in reader["continuity"].values()) and reader["continuity"]["ino"] != "0",
         "artifact_failed_descriptor")


def host_init(request, runtime, failed=False):
    validate_source_binding(request["source"])
    if failed:
        validate_failed_descriptor(request["descriptor"])
    parent = parent_fd(request)
    try:
        need(directory_identity(parent) == request["parentIdentity"]
             and os.listdir(parent) == [], "artifact_parent_not_empty")
        name = ("phase639-failed-image-artifacts-" if failed else "phase639-image-artifacts-") + secrets.token_hex(12)
        os.mkdir(name, 0o700, dir_fd=parent)
        child = os.open(name, DIRECTORY, dir_fd=parent)
        try:
            binding = {"parentIdentity": directory_identity(parent), "childIdentity": directory_identity(child),
                       "childName": name, "hostRuntime": runtime}
            need(binding["parentIdentity"]["dev"] == binding["childIdentity"]["dev"],
                 "artifact_filesystem")
            started = {"version": 1, "state": "incomplete",
                   "qualified": False, "d2Authority": False, "releaseAuthority": False,
                   "binding": binding, "source": request["source"],
                   "artifacts": ["runner.oci.tar"] if failed else list(ORDER)}
            if failed:
                started["descriptor"] = request["descriptor"]
            record(child, "failure-retention-started.json" if failed else "export-started.json", started)
            os.fsync(parent)
            return binding
        finally:
            os.close(child)
    finally:
        os.close(parent)


def receive(directory, name, stream, expected):
    need(name in CAPS, "artifact_id")
    header = read_frame(stream)
    keys(header, ("version", "artifact", "size", "source", "reader"))
    need(type(header["version"]) is int and header["version"] == 1 and header["artifact"] == name
         and type(header["size"]) is int and 0 < header["size"] <= CAPS[name]
         and canonical(header["reader"]) == canonical(expected["reader"]), "artifact_header")
    source = header["source"]
    keys(source, ("dev", "ino", "uid", "mode", "nlink", "size", "mtimeNs", "ctimeNs"))
    need(all(type(source[field]) is str and re.fullmatch(r"(?:0|[1-9][0-9]{0,23})", source[field])
             for field in ("dev", "ino", "mtimeNs", "ctimeNs")), "artifact_header")
    need(type(source["size"]) is int and source["size"] == header["size"]
         and type(source["uid"]) is int and source["uid"] == expected["owner"]
         and type(source["nlink"]) is int and source["nlink"] == 1
         and type(source["mode"]) is int and 0 <= source["mode"] <= 0o777 and source["mode"] & 0o022 == 0,
         "artifact_header")
    if "source" in expected:
        need(name == "runner.oci.tar" and source == expected["source"], "artifact_frozen_source_changed")
    fd = open_output(directory, name)
    digest, remaining = hashlib.sha256(), header["size"]
    try:
        while remaining:
            chunk = exact_read(stream, min(65536, remaining))
            digest.update(chunk)
            write_all(fd, chunk)
            remaining -= len(chunk)
        trailer = read_frame(stream)
        keys(trailer, ("version", "artifact", "size", "sha256", "source"))
        need(type(trailer["version"]) is int and type(trailer["size"]) is int
             and canonical(trailer["source"]) == canonical(source)
             and trailer == {"version": 1, "artifact": name, "size": header["size"],
                         "sha256": digest.hexdigest(), "source": source}
             and digest.hexdigest() == expected["sha256"] and stream.read(1) == b"", "artifact_trailer")
        os.fsync(fd)
        created = identity(os.fstat(fd))
    finally:
        os.close(fd)
    result = readback(directory, name, CAPS[name], expected["sha256"])
    need(result["identity"] == created, "artifact_output_changed")
    os.fsync(directory)
    return result


def host_failed_operation(operation, request, runtime, stream):
    need(runtime == request["binding"]["hostRuntime"], "artifact_host_runtime_changed")
    parent, child = bound_directories(request, failed=True)
    try:
        started, _ = read_record(child, "failure-retention-started.json")
        keys(started, ("version", "state", "qualified", "d2Authority", "releaseAuthority",
                       "binding", "source", "artifacts", "descriptor"))
        need(type(started["version"]) is int and started["version"] == 1
             and started["state"] == "incomplete" and started["qualified"] is False
             and started["d2Authority"] is False and started["releaseAuthority"] is False
             and started["binding"] == request["binding"] and started["artifacts"] == ["runner.oci.tar"],
             "artifact_failed_started_binding")
        validate_source_binding(started["source"])
        descriptor = started["descriptor"]
        validate_failed_descriptor(descriptor)
        if operation == "host-failure-receive":
            need(set(os.listdir(child)) == {"failure-retention-started.json"}, "artifact_child_entries")
            return receive(child, "runner.oci.tar", stream, {"reader": descriptor["reader"],
                "owner": descriptor["source"]["uid"], "sha256": descriptor["sha256"],
                "source": descriptor["source"]})
        need(operation == "host-failure-manifest", "artifact_operation")
        need(set(os.listdir(child)) == {"failure-retention-started.json", "runner.oci.tar"},
             "artifact_child_entries")
        received = request["received"]
        keys(received, ("name", "size", "sha256", "identity"))
        saved = readback(child, "runner.oci.tar", CAPS["runner.oci.tar"], descriptor["sha256"])
        need(saved == received and saved["size"] == descriptor["size"], "artifact_output_changed")
        checks = request["checks"]
        keys(checks, ("postObservation", "sourceUnchanged", "normalCleanup"))
        need(all(item is True for item in checks.values()), "artifact_failed_checks")
        # This records failed-run bytes only. No operation can qualify this prefix.
        return record(child, "failure-retention-manifest.json", {"version": 1,
            "state": "failed_archive_retained", "qualified": False,
            "d2Authority": False, "releaseAuthority": False,
            "failureStage": "post_archive_freeze", "failureCode": "image_workload_failed",
            "binding": request["binding"], "source": started["source"],
            "descriptor": descriptor, "file": saved, "checks": checks})
    finally:
        os.close(child)
        os.close(parent)


def host_bound_operation(operation, request, runtime, stream):
    need(runtime == request["binding"]["hostRuntime"], "artifact_host_runtime_changed")
    parent, child = bound_directories(request)
    try:
        started, _ = read_record(child, "export-started.json")
        keys(started, ("version", "state", "qualified", "d2Authority", "releaseAuthority",
                       "binding", "source", "artifacts"))
        need(type(started["version"]) is int and started["version"] == 1
             and started["artifacts"] == list(ORDER)
             and started["binding"] == request["binding"] and started["state"] == "incomplete"
             and started["qualified"] is False and started["d2Authority"] is False
             and started["releaseAuthority"] is False, "artifact_started_binding")
        if operation == "host-receive":
            need(set(os.listdir(child)) <= {"export-started.json", *ORDER}, "artifact_child_entries")
            return receive(child, request["artifact"], stream, request["expected"])
        if operation == "host-manifest":
            need(set(os.listdir(child)) == {"export-started.json", *ORDER}, "artifact_child_entries")
            keys(request["sha256s"], ORDER)
            need(all(HEX.fullmatch(value) for value in request["sha256s"].values()), "artifact_descriptor_binding")
            files = [readback(child, name, CAPS[name], request["sha256s"][name]) for name in ORDER]
            inspection = request["inspection"]
            need(inspection["expected"]["sourceCommit"] == started["source"]["sourceCommit"]
                 and request["sha256s"]["inventory.json"] == inspection["oci"]["inventorySha256"]
                 and request["sha256s"]["measurement.json"] == inspection["runtime"]["runtimeMeasurementSha256"]
                 and request["sha256s"]["report.json"] == hashlib.sha256(canonical(inspection) + b"\n").hexdigest(),
                 "artifact_descriptor_binding")
            manifest = {"version": 1, "state": "attachments_complete", "qualified": False,
                        "d2Authority": False, "releaseAuthority": False, "binding": request["binding"],
                        "source": started["source"], "reader": request["reader"], "files": files,
                        "inspection": request["inspection"]}
            return record(child, "export-manifest.json", manifest)
        need(operation == "host-qualify", "artifact_operation")
        need(set(os.listdir(child)) == {"export-started.json", "export-manifest.json", *ORDER},
             "artifact_child_entries")
        manifest, saved = read_record(child, "export-manifest.json")
        keys(manifest, ("version", "state", "qualified", "d2Authority", "releaseAuthority",
                        "binding", "source", "reader", "files", "inspection"))
        need(type(manifest["version"]) is int and manifest["version"] == 1
             and manifest["binding"] == request["binding"]
             and manifest["d2Authority"] is False and manifest["releaseAuthority"] is False
             and [entry["name"] for entry in manifest["files"]] == list(ORDER)
             and saved["sha256"] == request["manifestSha256"] and manifest["qualified"] is False
             and manifest["state"] == "attachments_complete" and manifest["source"] == started["source"],
             "artifact_manifest_binding")
        for entry in manifest["files"]:
            need(identity(os.stat(entry["name"], dir_fd=child, follow_symlinks=False)) == entry["identity"],
                 "artifact_final_file_changed")
        proof = request["qualification"]
        keys(proof, ("sourceCommit", "imageReportSha256", "harnessSha256", "toolSourcesSha256",
                     "postListenerChecked", "sourceUnchanged", "cleanup"))
        need(proof["sourceCommit"] == started["source"]["sourceCommit"]
             and proof["harnessSha256"] == started["source"]["harnessSha256"]
             and proof["toolSourcesSha256"] == started["source"]["imageToolSourcesSha256"]
             and proof["postListenerChecked"] is True and proof["sourceUnchanged"] is True
             and HEX.fullmatch(proof["imageReportSha256"])
             and HEX.fullmatch(proof["harnessSha256"]) and HEX.fullmatch(proof["toolSourcesSha256"]),
             "artifact_qualification")
        keys(proof["cleanup"], ("privateInventoriesEmpty", "ownedVmPidsAbsent", "socketAbsent",
                                "stateAbsent", "usernetSocketsAbsent"))
        need(all(value is True for value in proof["cleanup"].values()), "artifact_qualification")
        return record(child, "qualification.json", {"version": 1, "state": "d1_qualified",
                      "qualification": "isolated_measured_runner_image_only", "d2Authority": False,
                      "releaseAuthority": False, "externalTerminalZeroRequired": True,
                      "exportManifestSha256": saved["sha256"], "proof": proof})
    finally:
        os.close(child)
        os.close(parent)


def main():
    need(len(sys.argv) == 3, "artifact_arguments")
    operation = sys.argv[1]
    if operation in ("guest-discover", "guest-probe", "guest-read"):
        encoded = sys.argv[2]
        need(len(encoded) <= 87384 and re.fullmatch(r"[A-Za-z0-9_-]+", encoded), "artifact_arguments")
        raw = base64.urlsafe_b64decode(encoded + "=" * (-len(encoded) % 4))
        need(base64.urlsafe_b64encode(raw).decode("ascii").rstrip("=") == encoded, "artifact_arguments")
    else:
        raw = sys.argv[2].encode("ascii")
    request = decode(raw, 65536)
    if operation == "guest-discover":
        result = guest_discover(request)
    elif operation == "guest-probe":
        result = guest_probe(request)
    elif operation == "guest-read":
        if "describe" in request:
            keys(request, ("artifact", "approval", "reader", "owner", "describe"))
            need(request["describe"] is True and request["artifact"] == "runner.oci.tar", "artifact_id")
            result = guest_archive_descriptor(request["approval"], request["reader"], request["owner"])
        else:
            keys(request, ("artifact", "approval", "reader", "sha256", "owner",
                           *(("source",) if "source" in request else ())))
            if "source" in request:
                need(type(request["source"]) is dict, "artifact_frozen_source_changed")
            guest_stream(request["artifact"], request["approval"], request["reader"], request["sha256"],
                         request["owner"], sys.stdout.buffer, request.get("source"))
            return
    else:
        need(operation in ("host-check", "host-init", "host-receive", "host-manifest", "host-qualify",
                           "host-failure-init", "host-failure-receive", "host-failure-manifest"),
             "artifact_operation")
        common = ("parent", "filesystemDev", "excludedRoots", "producerPath", "producerSha256")
        extra = {"host-check": (), "host-init": ("parentIdentity", "source"),
                 "host-receive": ("binding", "artifact", "expected"),
                 "host-manifest": ("binding", "sha256s", "reader", "inspection"),
                 "host-qualify": ("binding", "manifestSha256", "qualification"),
                 "host-failure-init": ("parentIdentity", "source", "descriptor"),
                 "host-failure-receive": ("binding",),
                 "host-failure-manifest": ("binding", "received", "checks")}
        keys(request, (*common, *extra[operation]))
        need(type(request["excludedRoots"]) is list and len(request["excludedRoots"]) == 3
             and HEX.fullmatch(request["producerSha256"]), "artifact_request")
        runtime = host_runtime(request["producerPath"], request["producerSha256"])
        if operation == "host-check":
            result = host_check(request)
        elif operation == "host-init":
            result = host_init(request, runtime)
        elif operation == "host-failure-init":
            result = host_init(request, runtime, failed=True)
        elif operation.startswith("host-failure-"):
            result = host_failed_operation(operation, request, runtime, sys.stdin.buffer)
        else:
            result = host_bound_operation(operation, request, runtime, sys.stdin.buffer)
    raw = canonical(result)
    need(len(raw) <= 65536, "artifact_reply_bound")
    sys.stdout.buffer.write(raw + b"\n")
    sys.stdout.buffer.flush()


if __name__ == "__main__":
    try:
        main()
    except BaseException as error:
        code = str(error) if isinstance(error, Refusal) else "artifact_io_or_protocol_failed"
        if not re.fullmatch(r"artifact_[a-z_]{1,60}", code):
            code = "artifact_io_or_protocol_failed"
        # Avoid a second interpreter-shutdown traceback on a broken binary pipe.
        # OS exit closes FDs; every durable/partial output is intentionally retained.
        try:
            os.write(2, (code + "\n").encode("ascii"))
        finally:
            os._exit(1)
