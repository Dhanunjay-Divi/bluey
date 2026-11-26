"""Join two stopped synthetic runs and pass original held FDs to the collector."""
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import stat
import subprocess
import sys
import time

LIMIT = 65536
SAFE_INT = 2**53 - 1
SHA = re.compile(r"[0-9a-f]{64}")
SCOPE = ("run", "boot", "invocation", "challenge", "root", "actorUid", "unit")
SOURCE = "aa2ea7b02b17407e9a80871db8515002880da490b7ef144118d2003be6d34727"
ENTRY = "receipt-browser-collector-bridge.mjs"
MODULES = {
    ENTRY: "f55c8aabdd607b3b0bd9296d57b9d96d23a7cf4d2d5dbd68cc2241840c037e4a",
    "receipt-browser-terminal-collector.mjs": "eb60c80f21c1afa13b36ae9dc9b91fc85363c04a2082a236ce0cf75d88dcae5a",
    "managed-receipt-browser-qualification.mjs": "973423ed4bc10f447cc6a25364c2ed18066677fb7e9e5ea006246a20f4e58c87",
    "managed-receipt-postgres-proof.mjs": "a791ead9e0180d5ca8a5f730a70b62d7cf2a4f4b6f2a8665190962235d056c18",
}


def require(ok, reason):
    if not ok:
        raise RuntimeError(reason)


def clock():
    return time.clock_gettime_ns(time.CLOCK_MONOTONIC)


def sha(value):
    return type(value) is str and SHA.fullmatch(value) is not None


def wire(value, *, sort=False):
    return (json.dumps(value, sort_keys=sort, ensure_ascii=True, allow_nan=False,
                       separators=(",", ":")) + "\n").encode("ascii")


def positive(value):
    return type(value) is int and 0 < value <= SAFE_INT


def reference(record, terminal):
    require(type(record) is dict and set(record) ==
            ({"fd", "device", "inode", "bytes", "sha256"} if terminal else {"fd", "device", "inode"}),
            "bridge_reference")
    fd, device, inode = (record[k] for k in ("fd", "device", "inode"))
    require(type(fd) is int and 3 <= fd < 2**31 and type(device) is int and 0 <= device < 2**64
            and type(inode) is int and 0 < inode < 2**64, "bridge_descriptor_metadata")
    info = os.fstat(fd)
    require(stat.S_ISREG(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1
            and (info.st_dev, info.st_ino) == (device, inode)
            and fcntl.fcntl(fd, fcntl.F_GETFL) & os.O_ACCMODE == os.O_RDONLY,
            "bridge_descriptor_identity")
    if terminal:
        require(type(record["bytes"]) is int and 0 < record["bytes"] <= 4096
                and record["bytes"] == info.st_size and sha(record["sha256"]), "bridge_record_bound")
    return {**record, "device": str(device), "inode": str(inode)}


def run_scope(waiting, supervised, evidence, origin_ns, mode):
    scope = {k: waiting[k] for k in SCOPE}
    observed = evidence["terminal"]["record"]
    require(all(type(observed[k]) is type(v) and observed[k] == v for k, v in scope.items())
            and evidence["runtimeAdmitted"] is False, "bridge_scope_binding")
    observer = evidence["launchObserver"]
    require(all(scope[k] == observer[k] for k in ("run", "boot", "invocation")), "bridge_observer_binding")
    launch = {**scope, "rootDevice": waiting["rootDevice"], "rootInode": waiting["rootInode"],
              "mountId": observer["mountId"], "eventsInode": observer["eventsInode"],
              "releaseSha256": evidence["release"]["releaseSha256"]}
    require(sha(launch["releaseSha256"]) and all(positive(launch[k]) for k in (
        "actorUid", "rootDevice", "rootInode", "mountId", "eventsInode")), "bridge_launch_bounds")
    absolute = (evidence["boundNs"], evidence["release"]["publicationAfterNs"], evidence["terminal"]["exitNs"],
                evidence["zeroAcceptedNs"], evidence["stoppedNs"])
    require(all(type(t) is int for t in absolute) and type(origin_ns) is int and origin_ns >= 0
            and all(a < b for a, b in zip((origin_ns,) + absolute, absolute))
            and absolute[-1] - origin_ns <= SAFE_INT
            and absolute[-1] - absolute[0] <= 1_800_000_000_000, "bridge_time_order")
    require(evidence["postStop"] == {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0},
            "bridge_post_stop")
    network = {"supervisorNamespace": supervised["supervisorNetns"], "workloadNamespace": waiting["workloadNetns"],
               "interfaces": supervised["interfaces"], "defaultRoutes": supervised["defaultRoutes"]}
    require(all(positive(network[x]) for x in ("supervisorNamespace", "workloadNamespace"))
            and network["supervisorNamespace"] != network["workloadNamespace"]
            and network["interfaces"] == ["lo"] and network["defaultRoutes"] == 0, "bridge_network")
    binding = {k: v for k, v in launch.items() if k not in ("actorUid", "challenge", "releaseSha256")}
    marks = dict(zip(("bound", "released", "terminal", "zero", "stopped"), (x - origin_ns for x in absolute)))
    return launch, {**binding, "mode": mode, "marksNs": marks, "network": network,
                    "observer": evidence["observer"], "postStop": evidence["postStop"], "terminal": observed["status"]}


def prepare(case, timeout, terminal_frame, streams, origin_ns):
    main, main_scope = run_scope(*case, origin_ns, "case")
    late, late_scope = run_scope(*timeout, origin_ns, "timeout-control")
    require(main["boot"] == late["boot"]
            and all(main[k] != late[k] for k in ("run", "invocation", "root", "unit", "challenge"))
            and main_scope["network"]["supervisorNamespace"] == late_scope["network"]["supervisorNamespace"]
            and main_scope["network"]["workloadNamespace"] != late_scope["network"]["workloadNamespace"],
            "bridge_scope_reuse")
    require(type(terminal_frame) is bytes and 0 < len(terminal_frame) <= 4096, "bridge_frame_bound")
    actor = json.loads(terminal_frame)
    require(type(actor) is dict and all(sha(actor.get(k)) for k in ("stdoutSha256", "stderrSha256")),
            "bridge_actor_hashes")
    require(type(streams) is dict and set(streams) == {"stdout", "stderr"}, "bridge_streams")
    held = {k: reference(v, False) for k, v in streams.items()}
    term = {"case": reference(case[2]["terminalArtifact"], True),
            "timeout": reference(timeout[2]["terminalArtifact"], True)}
    refs = list(held.values()) + list(term.values())
    fds = tuple(value["fd"] for value in refs)
    require(len(set(fds)) == 4 and len({(r["device"], r["inode"]) for r in refs}) == 4, "bridge_fd_alias")
    transcript = {"sourceReceiptSha256": SOURCE, "stdoutSha256": actor["stdoutSha256"],
                  "stderrSha256": actor["stderrSha256"], "caseScope": main_scope, "timeoutScope": late_scope}
    frame = wire({"terminalFrameBase64": base64.b64encode(terminal_frame).decode("ascii"),
                  "release": {k: main[k] for k in SCOPE if k != "unit"}, "streams": held, "terminalArtifacts": term,
                  "supervisorTranscript": wire(transcript).decode("ascii"),
                  "trustedLaunch": {"case": main, "timeout": late}}, sort=True)
    require(len(frame) <= LIMIT, "bridge_control_bound")
    return frame, fds


def invoke(argv, payload, fds, deadline_ns, privileges=None):
    """Finite subprocess transport. The caller retains all original FDs."""
    options = {} if privileges is None else {"user": privileges[0], "group": privileges[1], "extra_groups": []}
    with subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          cwd="/", close_fds=True, pass_fds=fds,
                          env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"}, **options) as child:
        try:
            with selectors.DefaultSelector() as selector:
                os.set_blocking(child.stdin.fileno(), False)
                for stream, event, key in ((child.stdin, selectors.EVENT_WRITE, "in"),
                                           (child.stdout, selectors.EVENT_READ, "out"),
                                           (child.stderr, selectors.EVENT_READ, "err")):
                    selector.register(stream, event, key)
                offset, output = 0, bytearray()
                while selector.get_map():
                    timeout = (deadline_ns - clock()) / 1_000_000_000
                    require(timeout > 0, "bridge_child_deadline")
                    ready = selector.select(timeout)
                    require(ready, "bridge_child_deadline")
                    for key, _ in ready:
                        if key.data == "in":
                            try:
                                offset += os.write(key.fd, payload[offset:offset + 4096])
                            except BlockingIOError:
                                continue
                            if offset == len(payload):
                                selector.unregister(key.fileobj)
                                child.stdin.close()
                        else:
                            data = os.read(key.fd, 4097 - len(output) if key.data == "out" else 1)
                            if not data:
                                selector.unregister(key.fileobj)
                            else:
                                require(key.data == "out", "bridge_child_stderr")
                                output.extend(data)
                                require(len(output) <= 4096, "bridge_child_stdout")
            timeout = (deadline_ns - clock()) / 1_000_000_000
            require(timeout > 0 and child.wait(timeout=timeout) == 0
                    and clock() < deadline_ns, "bridge_child_exit")
            return bytes(output)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=2)


def pinned_file(path, digest, limit):
    require(path.is_absolute() and path.resolve(strict=True) == path and sha(digest), "bridge_module_path")
    for parent in path.parents:
        info = parent.stat()
        require(stat.S_ISDIR(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o022,
                "bridge_module_parent")
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW), "rb") as file:
        info = os.fstat(file.fileno())
        require(info.st_uid == 0 and stat.S_ISREG(info.st_mode) and not info.st_mode & 0o022
                and info.st_nlink == 1 and 0 < info.st_size <= limit, "bridge_module_identity")
        def stable(item):
            return (item.st_dev, item.st_ino, item.st_uid, item.st_mode, item.st_nlink,
                    item.st_size, item.st_mtime_ns, item.st_ctime_ns)

        checksum = hashlib.sha256()
        size = 0
        for block in iter(lambda: file.read(1024 * 1024), b""):
            size += len(block)
            require(size <= limit, "bridge_module_size")
            checksum.update(block)
        require(size == info.st_size and checksum.hexdigest() == digest
                and stable(info) == stable(os.fstat(file.fileno())), "bridge_module_digest")


def verify_pair(case, timeout, terminal_frame, streams, origin_ns, *, node, node_sha256, module_directory,
                collector_uid, collector_gid, independently_admitted, deadline_ns):
    """Linux supervisor: hand pinned, unprivileged Node only input and four FDs.

    Dispatcher owns original descriptors and closes them on all outcomes,
    along with the bounded process unit and its network isolation.
    """
    require(sys.platform == "linux" and os.getuid() == os.geteuid() == 0, "bridge_linux_root")
    require(callable(independently_admitted) and independently_admitted() is True
            and type(deadline_ns) is int and clock() < deadline_ns, "bridge_unadmitted")
    require(type(collector_uid) is int and type(collector_gid) is int
            and 0 < collector_uid < 2**32 - 1 and 0 < collector_gid < 2**32 - 1
            and all(collector_uid != row[0]["actorUid"] for row in (case, timeout)), "bridge_child_identity")
    payload, fds = prepare(case, timeout, terminal_frame, streams, origin_ns)
    node, directory = Path(node), Path(module_directory)

    def pins():
        pinned_file(node, node_sha256, 256 * 1024**2)
        for name, digest in MODULES.items():
            pinned_file(directory / name, digest, 64 * 1024)

    pins()
    entry = (directory / ENTRY).as_uri()
    start = "import {collectFromStdin} from " + json.dumps(entry) + ";collectFromStdin();"
    output = invoke([str(node), "--input-type=module", "-e", start], payload, fds, deadline_ns,
                    (collector_uid, collector_gid))
    pins()
    receipt = json.loads(output)
    require(type(receipt) is dict and output == wire(receipt) and receipt.get("closureRecordsBound") is True
            and receipt.get("runtimeAdmitted") is False
            and receipt.get("unitTerminalSha256") == case[2]["terminalArtifact"]["sha256"]
            and receipt.get("timeoutTerminalSha256") == timeout[2]["terminalArtifact"]["sha256"]
            and independently_admitted() is True and clock() < deadline_ns, "bridge_result")
    return receipt
