"""Root-only, no-CLI receiver for one admitted C2C preparation request.

It writes a small immutable bootstrap beside neither the actor nor manager
trees, invokes the retained plan module, and returns a framed transport reply.
It does not launch, release, clean up, or repair partial staging.
"""
import hashlib
import os
from pathlib import Path
import platform
import re
import stat
import types


BOOTSTRAP_BASE = Path("/run/bluey-jobs-c2c-bootstrap")
TRANSPORT = Path(__file__).resolve().with_name("local-qa-linux-c2c-preparation-transport.py")
TRANSPORT_SHA256 = "9c2fa11ea5b65f55432c1ff3883ac7780eb2d581e7168d27229ec04209605635"
SHA = re.compile(r"^[0-9a-f]{64}$")
TREE = re.compile(r"^[0-9a-f]{40}$")
RUN = re.compile(r"^[0-9a-f]{32}$")
RD = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
DIR = RD | os.O_DIRECTORY
MAX_SOURCE = 128 << 10
ACTOR_PLAN = "local-qa-linux-c2c-actor-plan.py"
TRIO = {
    ACTOR_PLAN: "3c7846a6b6a442229b8fbefedb10d5de89b658fd531e3c0807225689332f7676",
    "local-qa-linux-c2c-stage-code-packet.py": "1941e8ae352b1d3e0dd4cbf676e2b3d4f9c76a66d54c59a59d15afc1ab08a661",
    "local-qa-linux-c2c-code-packet.py": "694adfec5310035d94858d626fa5f84c2e093347f0f5fec6c020b03fff0a9870",
}


def _need(value, label):
    if not value:
        raise RuntimeError("local_qa_linux_c2c_receive_preparation_refused:" + label)


def _stamp(info):
    return (info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode,
            info.st_nlink, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def _read_fd(fd, limit):
    before = os.fstat(fd)
    _need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and 0 < before.st_size <= limit, "read_metadata")
    parts, remaining = [], before.st_size + 1
    while remaining:
        piece = os.read(fd, min(65536, remaining))
        if not piece:
            break
        parts.append(piece)
        remaining -= len(piece)
    body = b"".join(parts)
    _need(len(body) == before.st_size and _stamp(before) == _stamp(os.fstat(fd)), "read_changed")
    return body, before


def _read_module(path, expected):
    _need(type(expected) is str and SHA.fullmatch(expected) is not None
          and path.is_absolute() and path.resolve(strict=True) == path, "module_path")
    fd = os.open(path, RD)
    try:
        body, before = _read_fd(fd, MAX_SOURCE)
        named = path.lstat()
        _need(stat.S_ISREG(before.st_mode) and before.st_uid == 0 and stat.S_IMODE(before.st_mode) == 0o600
              and _stamp(before) == _stamp(named) and hashlib.sha256(body).hexdigest() == expected, "module_pin")
    finally:
        os.close(fd)
    return _module_from_bytes(path, body)


def _module_from_bytes(path, body):
    module = types.ModuleType("_pinned_c2c_preparation_" + path.stem.replace("-", "_"))
    module.__file__ = str(path)
    exec(compile(body, str(path), "exec"), module.__dict__)
    return module


def _root_linux():
    return (platform.system() == "Linux" and platform.machine() == "x86_64" and os.getuid() == os.geteuid() == 0
            and all(os.path.samestat(os.stat("/proc/self/ns/" + name), os.stat("/proc/1/ns/" + name))
                    for name in ("mnt", "user", "pid")))


def _capability(value):
    keys = {"run", "reservationSha256", "packetSha256", "codeInventorySha256", "sourceTree",
            "sourceInventorySha256", "browserInventorySha256"}
    hashes = keys - {"run", "sourceTree"}
    _need(type(value) is dict and set(value) == keys and type(value["run"]) is str
          and RUN.fullmatch(value["run"]) is not None and value["run"] != "0" * 32
          and type(value["sourceTree"]) is str and TREE.fullmatch(value["sourceTree"]) is not None
          and value["sourceTree"] != "0" * 40
          and all(type(value[key]) is str and SHA.fullmatch(value[key]) is not None and value[key] != "0" * 64
                  for key in hashes), "capability")
    return dict(value)


def _open_base():
    _need(BOOTSTRAP_BASE.is_absolute() and BOOTSTRAP_BASE.resolve(strict=True) == BOOTSTRAP_BASE, "bootstrap_base")
    fd = os.open(BOOTSTRAP_BASE, DIR)
    try:
        info, named = os.fstat(fd), BOOTSTRAP_BASE.lstat()
        _need(stat.S_ISDIR(info.st_mode) and info.st_uid == info.st_gid == 0 and stat.S_IMODE(info.st_mode) == 0o755
              and _stamp(info) == _stamp(named), "bootstrap_base_metadata")
        return fd
    except BaseException:
        os.close(fd)
        raise


def _base_current(fd):
    info, named = os.fstat(fd), BOOTSTRAP_BASE.lstat()
    _need(BOOTSTRAP_BASE.resolve(strict=True) == BOOTSTRAP_BASE and stat.S_ISDIR(info.st_mode)
          and (info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode)
          == (named.st_dev, named.st_ino, 0, 0, stat.S_IFDIR | 0o755)
          == (named.st_dev, named.st_ino, named.st_uid, named.st_gid, named.st_mode), "bootstrap_base_changed")


def _absent(parent, name):
    try:
        os.stat(name, dir_fd=parent, follow_symlinks=False)
    except FileNotFoundError:
        return
    raise RuntimeError("local_qa_linux_c2c_receive_preparation_refused:bootstrap_present")


def _write_leaf(parent, name, body, owner, group):
    fd = os.open(name, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600, dir_fd=parent)
    try:
        view = memoryview(body)
        while view:
            count = os.write(fd, view)
            _need(count > 0, "bootstrap_write")
            view = view[count:]
        os.fchmod(fd, 0o600)
        os.fsync(fd)
        info = os.fstat(fd)
        _need(stat.S_ISREG(info.st_mode) and (info.st_uid, info.st_gid) == (owner, group)
              and stat.S_IMODE(info.st_mode) == 0o600
              and info.st_nlink == 1 and info.st_size == len(body)
              and hashlib.sha256(os.pread(fd, len(body) + 1, 0)).digest() == hashlib.sha256(body).digest()
              and _stamp(info) == _stamp(os.stat(name, dir_fd=parent, follow_symlinks=False)), "bootstrap_readback")
    finally:
        os.close(fd)


def _install(parent, run, bootstrap, *, owner=0, group=0):
    _need(type(run) is str and RUN.fullmatch(run) and run != "0" * 32, "bootstrap_run")
    _need(type(bootstrap) is dict and set(bootstrap) == set(TRIO)
          and all(type(bootstrap[name]) is bytes and 0 < len(bootstrap[name]) <= MAX_SOURCE
                  and hashlib.sha256(bootstrap[name]).hexdigest() == digest for name, digest in TRIO.items()),
          "bootstrap_payload")
    os.mkdir(run, 0o700, dir_fd=parent)
    run_fd = os.open(run, DIR, dir_fd=parent)
    try:
        initial, named = os.fstat(run_fd), os.stat(run, dir_fd=parent, follow_symlinks=False)
        _need(stat.S_ISDIR(initial.st_mode) and (initial.st_uid, initial.st_gid) == (owner, group)
              and stat.S_IMODE(initial.st_mode) == 0o700 and _stamp(initial) == _stamp(named), "bootstrap_directory")
        for name in sorted(TRIO):
            _write_leaf(run_fd, name, bootstrap[name], owner, group)
        os.fsync(run_fd)
        os.fsync(parent)
        _need(set(os.listdir(run_fd)) == set(TRIO), "bootstrap_entries")
        return run_fd
    except BaseException:
        os.close(run_fd)
        raise


def _verify_bootstrap(parent, run, run_fd, bootstrap, *, owner=0, group=0):
    held, named = os.fstat(run_fd), os.stat(run, dir_fd=parent, follow_symlinks=False)
    _need(_stamp(held) == _stamp(named) and (held.st_uid, held.st_gid) == (owner, group)
          and stat.S_IMODE(held.st_mode) == 0o700
          and set(os.listdir(run_fd)) == set(TRIO), "bootstrap_changed")
    for name, digest in TRIO.items():
        fd = os.open(name, RD, dir_fd=run_fd)
        try:
            body, info = _read_fd(fd, len(bootstrap[name]))
            named_leaf = os.stat(name, dir_fd=run_fd, follow_symlinks=False)
            _need(body == bootstrap[name] and hashlib.sha256(body).hexdigest() == digest
                  and stat.S_IMODE(info.st_mode) == 0o600 and (info.st_uid, info.st_gid) == (owner, group)
                  and _stamp(info) == _stamp(named_leaf), "bootstrap_leaf")
        finally:
            os.close(fd)
    _need(_stamp(held) == _stamp(os.fstat(run_fd)) == _stamp(os.stat(run, dir_fd=parent, follow_symlinks=False))
          and set(os.listdir(run_fd)) == set(TRIO), "bootstrap_changed_after_read")


def receive_preparation(stream, expected_capability, *, admitted):
    """Receive and prepare one exact request, returning only its encoded response."""
    _need(_root_linux() and callable(admitted), "platform")
    cap = _capability(expected_capability)
    transport = _read_module(TRANSPORT, TRANSPORT_SHA256)
    _need(callable(getattr(transport, "read_request", None)) and callable(getattr(transport, "encode_response", None)),
          "transport_contract")
    packet, bootstrap, request_sha256 = transport.read_request(stream, dict(cap))
    _need(type(packet) is bytes and type(request_sha256) is str and SHA.fullmatch(request_sha256) is not None
          and hashlib.sha256(packet).hexdigest() == cap["packetSha256"], "request_packet")
    _need(admitted(dict(cap)) is True and _root_linux(), "admission_before")
    parent = _open_base()
    run_fd = -1
    try:
        _absent(parent, cap["run"])
        _base_current(parent)
        run_fd = _install(parent, cap["run"], bootstrap)
        _verify_bootstrap(parent, cap["run"], run_fd, bootstrap)
        _base_current(parent)
        actor_plan = _module_from_bytes(BOOTSTRAP_BASE / cap["run"] / ACTOR_PLAN, bootstrap[ACTOR_PLAN])
        _need(callable(getattr(actor_plan, "stage_code_and_plan", None)), "actor_plan_contract")
        actor_cap = {key: cap[key] for key in cap if key != "browserInventorySha256"}
        browser_root = "/run/bluey-jobs-c2c-browser/" + cap["run"]
        projection = {**actor_cap, "browserInventorySha256": cap["browserInventorySha256"], "browserRoot": browser_root}

        def plan_admitted(value):
            return value == projection and admitted(dict(cap)) is True and _root_linux()

        _need(plan_admitted(dict(projection)), "admission_plan_before")
        staged = actor_plan.stage_code_and_plan(packet, actor_cap, cap["browserInventorySha256"], admitted=plan_admitted)
        _need(admitted(dict(cap)) is True and _root_linux(), "admission_after")
        _verify_bootstrap(parent, cap["run"], run_fd, bootstrap)
        _base_current(parent)
        _need(type(staged) is dict and staged.get("runtimeQualified") is False and staged.get("launchAdmitted") is False,
              "plan_result")
        return transport.encode_response(staged, dict(cap), request_sha256)
    finally:
        if run_fd >= 0:
            os.close(run_fd)
        os.close(parent)
