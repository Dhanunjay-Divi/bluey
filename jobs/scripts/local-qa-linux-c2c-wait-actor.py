"""Wait inside the synthetic Jobs launcher for its root supervisor's release."""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import secrets
import stat
import sys
import time


GATES = Path("/run/bluey-jobs-c2c-gates")
CODE = Path("/run/bluey-jobs-c2c-code")
PROTOCOL = "local-qa-linux-c2c-release-protocol.py"
ACTOR = "local-qa-linux-c2c-native.mjs"
MARKER = ".bluey-test-run-v2"
SPEC = ".local-qa-linux-c2c-native.json"
FILE = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
RUN = re.compile(r"[0-9a-f]{32}")
DIGEST = re.compile(r"[0-9a-f]{64}")
TREE = re.compile(r"[0-9a-f]{40}")
ROOT = re.compile(r"/tmp/bluey-test-runs-[0-9a-f]{12}/run\.([1-9][0-9]{0,19})\.([1-9][0-9]{0,9})\.[0-9]{3,12}")
NAME = re.compile(r"local-qa-[a-z0-9-]{1,96}\.(?:py|mjs)")
MAX_WAIT_NS = 90_000_000_000


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def match(pattern, value):
    return type(value) is str and pattern.fullmatch(value) is not None


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def wire(value):
    return (json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def canonical(raw):
    value = json.loads(raw)
    need(type(value) is dict and wire(value) == raw, "canonical_record")
    return value


def identity(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_mode, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def named_bytes(fd, name, cap, *, uid, mode, digest=None):
    with os.fdopen(os.open(name, FILE, dir_fd=fd), "rb") as stream:
        before = os.fstat(stream.fileno())
        need(stat.S_ISREG(before.st_mode) and before.st_uid == uid and stat.S_IMODE(before.st_mode) == mode
             and before.st_nlink == 1 and 0 < before.st_size <= cap, "file_identity")
        raw = stream.read(before.st_size + 1)
        need(len(raw) == before.st_size and identity(before) == identity(os.fstat(stream.fileno()))
             == identity(os.stat(name, dir_fd=fd, follow_symlinks=False))
             and (digest is None or sha(raw) == digest), "file_pin")
    return raw, identity(before)


def protected_directory(path, stack):
    need(path.is_absolute() and path != Path("/"), "protected_path")
    fd = os.open("/", DIR)
    stack.callback(os.close, fd)
    for part in path.parts[1:]:
        need(part not in (".", ".."), "protected_path_component")
        fd = os.open(part, DIR, dir_fd=fd)
        stack.callback(os.close, fd)
        info = os.fstat(fd)
        need(info.st_uid == 0 and not info.st_mode & 0o7022, "protected_directory")
    need(path.resolve(strict=True) == path and os.path.samestat(os.fstat(fd), path.lstat()),
         "protected_directory_alias")
    return fd


def read_plan(gate, digest, run):
    raw, stamp = named_bytes(gate, "plan.json", 4096, uid=0, mode=0o444, digest=digest)
    plan = canonical(raw)
    need(set(plan) == {"schema", "run", "node", "codeInventorySha256", "sourceTree", "sourceInventorySha256"}
         and plan["schema"] == "bluey-jobs-c2c-actor-plan-v1" and plan["run"] == run
         and match(TREE, plan["sourceTree"]) and match(DIGEST, plan["sourceInventorySha256"])
         and match(DIGEST, plan["codeInventorySha256"]), "plan_fields")
    node = plan["node"]
    need(type(node) is dict and set(node) == {"path", "sha256"} and match(DIGEST, node["sha256"])
         and match(re.compile(r"/run/[A-Za-z0-9._/-]{1,220}/node"), node["path"])
         and all(part not in ("", ".", "..") for part in node["path"].split("/")[1:]), "node_path")
    return plan, stamp


def measure_code(fd, plan):
    raw, inventory_stamp = named_bytes(fd, "inventory.json", 32 << 10, uid=0, mode=0o444,
                                       digest=plan["codeInventorySha256"])
    value = canonical(raw)
    need(set(value) == {"version", "files"} and type(value["version"]) is int and value["version"] == 1
         and type(value["files"]) is list and 3 <= len(value["files"]) <= 80, "code_inventory")
    seen = {}
    protocol = None
    for row in value["files"]:
        need(type(row) is dict and set(row) == {"name", "sha256", "bytes"}
             and match(NAME, row["name"]) and match(DIGEST, row["sha256"])
             and type(row["bytes"]) is int and 0 < row["bytes"] <= 2 << 20, "code_row")
        name = row["name"]
        need(name not in seen, "code_duplicate")
        body, stamp = named_bytes(fd, name, row["bytes"], uid=0, mode=0o444, digest=row["sha256"])
        need(len(body) == row["bytes"], "code_size")
        seen[name] = stamp
        if name == PROTOCOL:
            protocol = body
    need({PROTOCOL, ACTOR, Path(__file__).name} <= seen.keys()
         and set(os.listdir(fd)) == set(seen) | {"inventory.json"} and protocol is not None, "code_closure")
    need(os.path.samestat(os.stat(Path(__file__).name, dir_fd=fd, follow_symlinks=False),
                          Path(__file__).lstat()), "actor_wrapper_pin")
    return protocol, inventory_stamp, seen


def directory_stamp(path, fd, owner):
    item = os.fstat(fd)
    need(item.st_uid == owner and stat.S_IMODE(item.st_mode) == 0o700
         and path.resolve(strict=True) == path and os.path.samestat(item, path.lstat()), "launcher_directory")
    return item.st_dev, item.st_ino


def proc_bytes(path, cap):
    with os.fdopen(os.open(path, FILE), "rb") as handle:
        raw = handle.read(cap + 1)
    need(0 < len(raw) <= cap and b"\0" not in raw, "linux_observation")
    return raw


def proc_start(raw, pid):
    prefix, sep, fields = raw.rpartition(b") ")
    parts = fields.rstrip(b"\n").split(b" ")
    need(raw.endswith(b"\n") and raw.count(b"\n") == 1 and prefix.startswith(f"{pid} (".encode())
         and sep == b") " and len(parts) >= 20 and re.fullmatch(rb"[A-Z]", parts[0])
         and parts[0] not in (b"Z", b"X")
         and re.fullmatch(rb"[1-9][0-9]{0,18}", parts[19]), "process_start")
    boot_ticks = int(parts[19])
    need(0 < boot_ticks < 2**63, "process_start_range")
    return boot_ticks


def marker_binding(raw, root, main_pid, owner, launcher_pgrp, actor_pgrp):
    found = ROOT.fullmatch(str(root))
    need(found and int(found[2]) == main_pid and launcher_pgrp > 1 and actor_pgrp > 1
         and launcher_pgrp != actor_pgrp, "launcher_groups")
    expected = (f"magic=bluey-test-run-v2\nrun_root={root}\nrun_name={root.name}\n"
                f"owner_uid={owner}\nowner_pid={main_pid}\nprocess_group_id={actor_pgrp}\n"
                f"launcher_group_id={launcher_pgrp}\ncreated_epoch={found[1]}\nstate=running\n")
    need(raw == expected.encode("ascii"), "running_launcher_marker")


def observe(run, root, parent_fd, root_fd):
    uid, actor = os.getuid(), os.getpid()
    main = int(ROOT.fullmatch(str(root))[2])  # The launcher is the systemd MainPID candidate.
    need(main != actor, "launcher_actor_alias")
    directory_stamp(root.parent, parent_fd, uid)
    dev, ino = directory_stamp(root, root_fd, uid)
    marker, _ = named_bytes(root_fd, MARKER, 4096, uid=uid, mode=0o600)
    marker_binding(marker, root, main, uid, os.getpgid(main), os.getpgrp())
    unit = f"bluey-jobs-qa-{run}.service"
    cgroup = f"/system.slice/{unit}"
    boot = proc_bytes("/proc/sys/kernel/random/boot_id", 128)
    need(re.fullmatch(rb"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}\n", boot), "boot_id")
    invocation = os.environ.get("INVOCATION_ID")
    need(match(RUN, invocation) and invocation != "0" * 32, "invocation")
    namespace = os.readlink("/proc/self/ns/net")
    starts = []
    for pid in (main, actor):
        path = Path(f"/proc/{pid}")
        need(proc_bytes(path / "cgroup", 4096) == f"0::{cgroup}\n".encode("ascii")
             and os.readlink(path / "ns/net") == namespace, "process_unit")
        starts.append(proc_start(proc_bytes(path / "stat", 4096), pid))
    return {"run": run, "unitName": unit, "bootId": boot.decode().strip().replace("-", ""),
            "invocationId": invocation, "unitCgroup": cgroup, "networkNamespace": namespace,
            "root": str(root), "rootDevice": dev, "rootInode": ino, "markerSha256": sha(marker),
            "actorUid": uid, "mainPid": main, "mainStartTime": starts[0], "actorPid": actor,
            "actorStartTime": starts[1]}


def absent_release(gate):
    try:
        os.stat("release.json", dir_fd=gate, follow_symlinks=False)
    except FileNotFoundError:
        return True
    return False


def wait(gate, waiting, release, *, clock, sleep):
    start = json.loads(waiting)["waitingNs"]
    deadline = start + MAX_WAIT_NS
    while clock() < deadline:
        try:
            raw, stamp = named_bytes(gate, "release.json", 4096, uid=0, mode=0o644)
        except FileNotFoundError:
            sleep(0.05)
            continue
        value = release(raw, waiting, clock())
        return raw, stamp, value
    raise ValueError("release_timeout")


def bind_spec(raw, plan, observed):
    spec = json.loads(raw)
    unit, binding = spec.get("unit", {}), spec.get("binding", {})
    need(spec.get("sourceTree") == plan["sourceTree"] and
         spec.get("sourceInventorySha256") == plan["sourceInventorySha256"] and
         all(unit.get(k) == observed[k] for k in ("mainPid", "bootId", "invocationId", "unitName")) and
         all(binding.get(k) == observed[k] for k in ("root", "unitCgroup", "networkNamespace")), "spec_observation")


def run_actor(run, plan_digest):
    need(sys.platform == "linux" and platform.machine() == "x86_64" and os.getuid() == os.geteuid() != 0
         and match(RUN, run) and run != "0" * 32 and match(DIGEST, plan_digest), "platform_or_arguments")
    root = Path(os.environ.get("BLUEY_TEST_RUN_ROOT", ""))
    need(ROOT.fullmatch(str(root)), "launcher_root")
    with ExitStack() as stack:
        gate_path, code_path = GATES / run, CODE / run
        gate = protected_directory(gate_path, stack)
        code = protected_directory(code_path, stack)
        gate_stat, code_stat = os.fstat(gate), os.fstat(code)
        need(absent_release(gate) and Path(__file__).resolve(strict=True) == code_path / Path(__file__).name,
             "nonfresh_waiter")
        parent_fd = os.open(root.parent, DIR)
        stack.callback(os.close, parent_fd)
        root_fd = os.open(root.name, DIR, dir_fd=parent_fd)
        stack.callback(os.close, root_fd)
        plan, plan_stamp = read_plan(gate, plan_digest, run)
        protocol, manifest_stamp, staged = measure_code(code, plan)
        release = {"__file__": str(code_path / PROTOCOL), "__name__": "_trusted_c2c_release"}
        exec(compile(protocol, str(code_path / PROTOCOL), "exec"), release)
        node = Path(plan["node"]["path"])
        node_parent = protected_directory(node.parent, stack)
        _, node_stamp = named_bytes(node_parent, node.name, 160 << 20, uid=0, mode=0o555,
                                    digest=plan["node"]["sha256"])
        observed = observe(run, root, parent_fd, root_fd)
        clock = lambda: time.clock_gettime_ns(time.CLOCK_MONOTONIC)
        wait_record = dict(observed, schema="bluey-jobs-c2c-waiting-v1",
                           nonce=secrets.token_hex(16), waitingNs=clock())
        waiting = release["encode"](wait_record)
        release["parse_waiting"](waiting)
        sys.stdout.buffer.write(waiting)
        sys.stdout.buffer.flush()
        raw, stamp, admitted = wait(gate, waiting, release["accept_release"], clock=clock, sleep=time.sleep)
        digest = admitted["specSha256"]
        spec, spec_stamp = named_bytes(root_fd, SPEC, 65_536, uid=os.getuid(), mode=0o600, digest=digest)
        bind_spec(spec, plan, observed)
        need(read_plan(gate, plan_digest, run) == (plan, plan_stamp)
             and measure_code(code, plan) == (protocol, manifest_stamp, staged)
             and named_bytes(node_parent, node.name, 160 << 20, uid=0, mode=0o555,
                             digest=plan["node"]["sha256"])[1] == node_stamp
             and node.resolve(strict=True) == node and identity(node.lstat()) == node_stamp
             and os.path.samestat(gate_stat, gate_path.lstat()) and os.path.samestat(code_stat, code_path.lstat())
             and observe(run, root, parent_fd, root_fd) == observed
             and named_bytes(gate, "release.json", 4096, uid=0, mode=0o644) == (raw, stamp)
             and named_bytes(root_fd, SPEC, 65_536, uid=os.getuid(), mode=0o600, digest=digest)[1] == spec_stamp,
             "release_readback")
        release["accept_release"](raw, waiting, clock())
        env = {"PATH": "/usr/bin:/bin", "HOME": str(root), "TMPDIR": str(root / "tmp"), "LANG": "C", "LC_ALL": "C",
               "TZ": "UTC", "BLUEY_TEST_RUN_ROOT": str(root), "INVOCATION_ID": observed["invocationId"]}
        # Keep PID, process group and runtime namespace. The actor independently
        # checks the same spec digest and systemd unit before opening dependencies.
        os.execve(node, [str(node), str(code_path / ACTOR), "--spec", str(root / SPEC), "--sha256", digest], env)


if __name__ == "__main__":
    try:
        need(__debug__ and sys.flags.isolated and sys.flags.dont_write_bytecode
             and len(sys.argv) == 5 and sys.argv[1] == "--run" and sys.argv[3] == "--plan-sha256", "arguments")
        run_actor(sys.argv[2], sys.argv[4])
    except (KeyError, TypeError, AttributeError, OSError, ValueError, RuntimeError):
        sys.stderr.write("local_qa_linux_c2c_wait_actor_refused\n")
        raise SystemExit(1) from None
