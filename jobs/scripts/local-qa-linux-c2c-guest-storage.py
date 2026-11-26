"""Read-only storage admission for the disposable Linux C2C Jobs QA guest.

This verifier neither mounts, remounts, creates, changes nor executes anything.
It only accepts pre-existing, root-owned base directories and their held mount
and capacity observations.  Stagers remain responsible for all writes.
"""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import PurePosixPath
import platform
import re
import stat


DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
MOUNTINFO_LIMIT = 128 << 10
RUN = {
    "/run/bluey-jobs-c2c-browser": "/var/lib/bluey-jobs-c2c-runtime/browser",
    "/run/bluey-jobs-c2c-application": "/var/lib/bluey-jobs-c2c-runtime/application",
    "/run/bluey-jobs-c2c-postgres": "/var/lib/bluey-jobs-c2c-runtime/postgres",
}
INTAKE = "/var/lib/bluey-jobs-c2c-intake"
ALL = tuple((*RUN, INTAKE))
EXEC_SUPPORT = ("/opt/bluey-jobs-c2c-launcher", "/tmp")
WATCHED = (*ALL, *EXEC_SUPPORT)
NAMESPACES = ("mnt", "pid", "user", "net", "cgroup")
PAYLOAD_BYTES = 533_939_096 + 111_198_656 + 29_931_688 + (2 << 20) + 47_718_773 + 240_820
RESERVE_BYTES = 1 << 30
REQUIRED_BYTES = PAYLOAD_BYTES + RESERVE_BYTES
DECIMAL = re.compile(r"[1-9][0-9]{0,9}")
DEVICE = re.compile(r"[0-9]{1,9}:[0-9]{1,9}")


def need(value, reason):
    if not value:
        raise ValueError("c2c_guest_storage_refused:" + reason)


def clean_path(value, *, allow_root=False):
    if allow_root and value == "/":
        return True
    return (type(value) is str and value.startswith("/") and value != "/" and "\\" not in value
            and str(PurePosixPath(value)) == value and "//" not in value
            and all(part not in ("", ".", "..") for part in value.split("/")[1:]))


def options(text):
    need(type(text) is str and text, "mount_options")
    values = text.split(",")
    need(all(values) and len(values) == len(set(values)), "mount_options")
    return frozenset(values)


def parse_mountinfo(raw):
    """Parse the narrow Linux mountinfo projection used by this preflight."""
    need(type(raw) is bytes and 0 < len(raw) <= MOUNTINFO_LIMIT and raw.endswith(b"\n"), "mountinfo_bound")
    try:
        lines = raw[:-1].decode("ascii").split("\n")
    except UnicodeDecodeError as error:
        raise ValueError("c2c_guest_storage_refused:mountinfo_ascii") from error
    rows = []
    for line in lines:
        left, separator, right = line.partition(" - ")
        fields, tail = left.split(), right.split()
        need(separator == " - " and len(fields) >= 6 and len(tail) == 3, "mountinfo_shape")
        mount_id, parent, device, root, mountpoint, flags = fields[:6]
        optional = fields[6:]
        need(DECIMAL.fullmatch(mount_id) and DECIMAL.fullmatch(parent) and DEVICE.fullmatch(device)
             and clean_path(root, allow_root=True)
             and clean_path(mountpoint, allow_root=True)
             and all("\\" not in item and item for item in optional)
             and tail[0] and tail[1] and tail[2], "mountinfo_fields")
        need(mount_id not in {row["mount_id"] for row in rows}, "mountinfo_duplicate")
        rows.append({"mount_id": int(mount_id), "parent": int(parent), "device": device,
                     "root": root, "mountpoint": mountpoint, "options": options(flags),
                     "optional": tuple(optional), "fstype": tail[0], "source": tail[1],
                     "superoptions": options(tail[2])})
    need(rows and len(rows) <= 512, "mountinfo_rows")
    return tuple(rows)


def mounted_at(rows, path):
    matches = [row for row in rows if row["mountpoint"] == "/"
               or path == row["mountpoint"] or path.startswith(row["mountpoint"] + "/")]
    need(matches, "mount_selection")
    return max(matches, key=lambda row: (len(row["mountpoint"]), row["mount_id"]))


def initial_guest_view(readlink=os.readlink, *, system=platform.system, machine=platform.machine,
                       uid=os.getuid, euid=os.geteuid):
    need(system() == "Linux" and machine() == "x86_64" and uid() == euid() == 0, "initial_platform")
    values = {name: readlink("/proc/self/ns/" + name) for name in NAMESPACES}
    need(values == {name: readlink("/proc/1/ns/" + name) for name in NAMESPACES}
         and all(re.fullmatch(name + r":\[[1-9][0-9]{0,19}\]", values[name]) for name in NAMESPACES),
         "initial_namespaces")
    return values


def anchor_from_fd(path, fd):
    """Capture one nofollow directory observation while its descriptor is held."""
    need(path in ALL and type(fd) is int and fd >= 0, "anchor_input")
    before = os.fstat(fd)
    ancestry = protected_ancestry(path)
    named = os.lstat(path)
    need(stat.S_ISDIR(before.st_mode) and before.st_uid == before.st_gid == 0
         and stat.S_IMODE(before.st_mode) == 0o755 and os.path.samestat(before, named)
         and os.path.realpath(path) == path and ancestry == protected_ancestry(path), "anchor_custody")
    info_fd = os.open("/proc/self/fdinfo/" + str(fd), os.O_RDONLY | os.O_CLOEXEC)
    try:
        fdinfo = os.pread(info_fd, 4096, 0)
    finally:
        os.close(info_fd)
    try:
        rows = [line for line in fdinfo.decode("ascii").splitlines() if line.startswith("mnt_id:")]
    except UnicodeDecodeError as error:
        raise ValueError("c2c_guest_storage_refused:fdinfo_ascii") from error
    need(len(rows) == 1 and re.fullmatch(r"mnt_id:\s*[1-9][0-9]{0,9}", rows[0]), "anchor_mnt_id")
    usage = os.fstatvfs(fd)
    after = os.fstat(fd)
    need(os.path.samestat(before, after) and os.path.samestat(before, os.lstat(path))
         and ancestry == protected_ancestry(path), "anchor_changed")
    return {"path": path, "st_dev": before.st_dev, "st_ino": before.st_ino, "mode": before.st_mode,
            "uid": before.st_uid, "gid": before.st_gid, "nlink": before.st_nlink,
            "mnt_id": int(rows[0].split()[-1]), "bavail": usage.f_bavail, "frsize": usage.f_frsize}


def protected_ancestry(path):
    """Root-owned, non-writable directory chain for a fixed absolute guest path."""
    need(clean_path(path), "anchor_path")
    snapshots = []
    current = "/"
    for component in path.split("/")[1:]:
        current = current.rstrip("/") + "/" + component
        observed = os.lstat(current)
        need(stat.S_ISDIR(observed.st_mode) and observed.st_uid == observed.st_gid == 0
             and stat.S_IMODE(observed.st_mode) & 0o022 == 0, "anchor_ancestry")
        snapshots.append((current, observed.st_dev, observed.st_ino, observed.st_mode,
                          observed.st_uid, observed.st_gid))
    return tuple(snapshots)


def read_bounded_file(path, limit):
    """Read a small proc file completely, refusing a too-large observation."""
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
    try:
        result = bytearray()
        while True:
            chunk = os.read(fd, min(8192, limit + 1 - len(result)))
            if not chunk:
                return bytes(result)
            result.extend(chunk)
            need(len(result) <= limit, "mountinfo_bound")
    finally:
        os.close(fd)


def check_anchor(path, value, mount):
    need(type(value) is dict and set(value) == {"path", "st_dev", "st_ino", "mode", "uid", "gid",
                                                  "nlink", "mnt_id", "bavail", "frsize"}
         and value["path"] == path and type(value["st_dev"]) is int
         and type(value["st_ino"]) is int and value["st_ino"] > 0
         and type(value["mode"]) is int and stat.S_ISDIR(value["mode"])
         and stat.S_IMODE(value["mode"]) == 0o755 and value["uid"] == value["gid"] == 0
         and type(value["nlink"]) is int and value["nlink"] >= 2
         and value["mnt_id"] == mount["mount_id"] and type(value["bavail"]) is int and value["bavail"] >= 0
         and type(value["frsize"]) is int and value["frsize"] > 0, "anchor_shape")
    unsigned_dev = value["st_dev"] & ((1 << 64) - 1)
    need((os.major(unsigned_dev), os.minor(unsigned_dev)) == tuple(map(int, mount["device"].split(":"))),
         "anchor_device")


def acceptable_payload_mount(row, expected_root, path):
    flags = row["options"] | row["superoptions"]
    need(row["mountpoint"] == path and row["root"] == expected_root and row["fstype"] == "ext4"
         and re.fullmatch(r"/dev/[A-Za-z0-9._/-]{1,128}", row["source"]) is not None
         and "rw" in row["options"] and {"nosuid", "nodev"} <= row["options"]
         and "ro" not in flags and "noexec" not in flags
         and not row["optional"], "payload_mount")


def acceptable_exec_support(row):
    flags = row["options"] | row["superoptions"]
    need(row["fstype"] == "ext4" and re.fullmatch(r"/dev/[A-Za-z0-9._/-]{1,128}", row["source"])
         and "rw" in row["options"] and "ro" not in flags and "noexec" not in flags,
         "exec_support_mount")


def topology(selected, anchors, namespaces):
    """Capacity-independent identity for the required before/after comparison."""
    value = {
        "schema": "bluey-jobs-c2c-guest-storage-topology-v1",
        "namespaces": dict(namespaces),
        "paths": {
            path: {
                "mountId": selected[path]["mount_id"], "device": selected[path]["device"],
                "source": selected[path]["source"], "fstype": selected[path]["fstype"],
                "root": selected[path]["root"], "mountpoint": selected[path]["mountpoint"],
                "options": sorted(selected[path]["options"]),
                "superoptions": sorted(selected[path]["superoptions"]),
                "optional": sorted(selected[path]["optional"]),
                **({"anchorDev": anchors[path]["st_dev"], "anchorIno": anchors[path]["st_ino"],
                    "anchorMntId": anchors[path]["mnt_id"], "anchorMode": anchors[path]["mode"]}
                   if path in anchors else {}),
            } for path in WATCHED
        },
    }
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("ascii")
    return {"sha256": hashlib.sha256(encoded).hexdigest(), "value": value}


def verify_guest_storage_projection(mountinfo, anchors, namespaces, *, expected_topology=None,
                                    minimum_available_bytes=REQUIRED_BYTES):
    """Pure storage admission using held-descriptor observations supplied by caller."""
    rows = parse_mountinfo(mountinfo)
    need(type(anchors) is dict and set(anchors) == set(ALL)
         and type(namespaces) is dict and set(namespaces) == set(NAMESPACES)
         and all(re.fullmatch(name + r":\[[1-9][0-9]{0,19}\]", namespaces[name]) for name in NAMESPACES),
         "projection_shape")
    selected = {path: mounted_at(rows, path) for path in WATCHED}
    for path, expected_root in RUN.items():
        acceptable_payload_mount(selected[path], expected_root, path)
    intake = selected[INTAKE]
    need(intake["fstype"] == "ext4" and re.fullmatch(r"/dev/[A-Za-z0-9._/-]{1,128}", intake["source"])
         and "rw" in intake["options"] and "ro" not in (intake["options"] | intake["superoptions"]),
         "intake_mount")
    for path in EXEC_SUPPORT:
        acceptable_exec_support(selected[path])
    for path in ALL:
        check_anchor(path, anchors[path], selected[path])
    device = selected[INTAKE]["device"]
    source = selected[INTAKE]["source"]
    need(all(selected[path]["device"] == device and selected[path]["source"] == source for path in WATCHED),
         "filesystem_split")
    need(type(minimum_available_bytes) is int and RESERVE_BYTES <= minimum_available_bytes <= REQUIRED_BYTES,
         "capacity_threshold")
    frsizes = {anchor["frsize"] for anchor in anchors.values()}
    need(len(frsizes) == 1, "capacity_observation_drift")
    frsize = frsizes.pop()
    bavail = min(anchor["bavail"] for anchor in anchors.values())
    available = bavail * frsize
    need(available >= minimum_available_bytes, "capacity")
    observed_topology = topology(selected, anchors, namespaces)
    if expected_topology is not None:
        need(type(expected_topology) is dict and set(expected_topology) == {"sha256", "value"}
             and expected_topology == observed_topology, "topology_changed")
    return {"schema": "bluey-jobs-c2c-guest-storage-v1", "mountIds": {path: selected[path]["mount_id"] for path in WATCHED},
            "device": device, "source": source, "availableBytes": available,
            "requiredBytes": REQUIRED_BYTES, "minimumAvailableBytes": minimum_available_bytes,
            "topology": observed_topology, "namespaces": dict(namespaces),
            "readOnlyPreflight": True, "runtimeQualified": False}


def verify_guest_storage(*, expected_topology=None, minimum_available_bytes=REQUIRED_BYTES):
    """Actual read-only guest preflight; base paths must already exist."""
    namespaces = initial_guest_view()
    with ExitStack() as stack:
        descriptors = {path: stack.enter_context(_opened(path)) for path in ALL}
        anchors = {path: anchor_from_fd(path, fd) for path, fd in descriptors.items()}
        mountinfo = read_bounded_file("/proc/self/mountinfo", MOUNTINFO_LIMIT)
        return verify_guest_storage_projection(
            mountinfo, anchors, namespaces, expected_topology=expected_topology,
            minimum_available_bytes=minimum_available_bytes)


class _opened:
    def __init__(self, path):
        self.path, self.fd = path, -1

    def __enter__(self):
        self.fd = os.open(self.path, DIR)
        return self.fd

    def __exit__(self, *unused):
        os.close(self.fd)
