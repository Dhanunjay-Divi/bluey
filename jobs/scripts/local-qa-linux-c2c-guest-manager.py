"""Dedicated guest coordinator for the synthetic Jobs C2C journey.

The caller must have separately admitted the disposable Linux guest, sealed
resources and exact manager observation/cleanup adapters. No CLI or default
activation path is provided.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import selectors
import stat
import subprocess
import sys
import time


SYSTEMD_RUN = Path("/usr/bin/systemd-run")
SOURCE_BASE = Path("/opt/bluey-jobs-c2c-launcher")
CODE_BASE = Path("/run/bluey-jobs-c2c-code")
GATE_BASE = Path("/run/bluey-jobs-c2c-gates")
WAITER = "local-qa-linux-c2c-wait-actor.py"
EXPECTED_WAITER_SHA = "f7e4ef89cafd917ec5312eca615883e68fd1ae6a7da61fa8a19aca2010696d4a"
EXPECTED_LAUNCHER_SHA = "8ad1d3f2d3976b88da64a55b1de9111997603115c7a56ae2d5d0a0afdcbb55c8"
RUN = re.compile(r"[0-9a-f]{32}")
SHA = re.compile(r"[0-9a-f]{64}")
TREE = re.compile(r"[0-9a-f]{40}")
PARENT = re.compile(r"/tmp/bluey-test-runs-[0-9a-f]{12}")
PYTHON = re.compile(r"/usr/bin/python3\.[0-9]{1,2}")
MAX_FRAME = 16 << 10
MANAGER_ENV = {"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"}
UNSET_SERVICE_ENV = ("GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE", "GIT_CONFIG",
                     "GIT_CONFIG_PARAMETERS", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES",
                     "GIT_EXEC_PATH", "BASH_ENV", "ENV", "NODE_OPTIONS", "NODE_PATH", "PYTHONHOME", "PYTHONPATH",
                     "LD_PRELOAD", "LD_LIBRARY_PATH", "LD_AUDIT", "BLUEY_DATABASE_URL", "BLUEY_TEST_POSTGRES_URL")
ZERO = {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0}
HELPERS = {
    "protocol": ("local-qa-linux-c2c-release-protocol.py", "850b48cd7e36778d7c1be4d4eaf8f5ead5f00eadd90f0ea18a42db366c5a57ec"),
    "observer": ("local-qa-linux-c2c-manager-observer.py", "57d60374c15596bfe0594b707892f7a847c3c5a459da6564778525729c886530"),
    "stager": ("local-qa-linux-c2c-stage-actor-inputs.py", "73719c3608fd1b50a35503e828ce9a5f7adc48ce1a9690af92817f716ae30c13"),
    "publisher": ("local-qa-linux-c2c-release-writer.py", "4a9a08727453f2b8e658b1c15fd5a7544f3e691c4d9db37e0ad3c4fdfaf09cda"),
}
RUNTIME_HELPERS = {
    "custody": ("local-qa-linux-c2c-launch-custody.py", "49c887c278408e057005940d9b97ce9579279938c81170bf4780bb9feb22cfca"),
    "teardown": ("local-qa-linux-c2c-guest-teardown.py", "4cc2c01f50f4d83f4ec702036645ed8769052a24f78f6af9bb84cd1c1a9987cd"),
}


def need(ok, why):
    if not ok:
        raise ValueError("c2c_guest_manager_refused:" + why)


def match(rex, value):
    return type(value) is str and rex.fullmatch(value) is not None


def root_linux():
    return sys.platform == "linux" and platform.machine() == "x86_64" and os.getuid() == os.geteuid() == 0


def initial_guest_view():
    return root_linux() and all(os.path.samestat(os.stat(f"/proc/self/ns/{kind}"),
                                                os.stat(f"/proc/1/ns/{kind}"))
                                for kind in ("mnt", "pid", "user", "net", "cgroup"))


def monotonic_ns():
    return time.clock_gettime_ns(time.CLOCK_MONOTONIC)


def build_plan(run, uid, gid, run_parent, plan_sha256, python_path):
    need(match(RUN, run) and run != "0" * 32 and match(SHA, plan_sha256)
         and match(PARENT, run_parent) and match(PYTHON, python_path)
         and all(type(n) is int and 0 < n < 2**31 for n in (uid, gid)), "plan_scope")
    source = SOURCE_BASE / run
    code = CODE_BASE / run
    gate = GATE_BASE / run
    unit = f"bluey-jobs-qa-{run}.service"
    properties = ("PrivateNetwork=yes", "PrivateTmp=no", "NoNewPrivileges=yes", "CapabilityBoundingSet=",
                  "AmbientCapabilities=", "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK",
                  "ProtectSystem=strict", "ProtectHome=yes", "KillMode=control-group", "ExitType=cgroup",
                  "Restart=no", "RemainAfterExit=yes", "RuntimeMaxSec=600", "TimeoutStopSec=15",
                  "MemoryMax=4G", "CPUQuota=200%", "TasksMax=384", f"ReadWritePaths={run_parent}",
                  "UnsetEnvironment=" + " ".join(UNSET_SERVICE_ENV))
    service_env = (*MANAGER_ENV.items(), ("HOME", run_parent), ("BLUEY_TEST_RUN_PARENT", run_parent),
                   ("GIT_CONFIG_NOSYSTEM", "1"), ("GIT_CONFIG_GLOBAL", "/dev/null"), ("GIT_CONFIG_COUNT", "1"),
                   ("GIT_CONFIG_KEY_0", "safe.directory"), ("GIT_CONFIG_VALUE_0", str(source)))
    argv = (str(SYSTEMD_RUN), "--system", "--quiet", "--wait", "--pipe", "--service-type=exec",
            f"--unit={unit}", f"--uid={uid}", f"--gid={gid}", f"--working-directory={source}",
            *(f"--property={item}" for item in properties), *(f"--setenv={k}={v}" for k, v in service_env),
            "--", str(source / "scripts/run-bluey-tests.sh"), "--", python_path, "-I", "-S", "-B",
            str(code / WAITER), "--run", run, "--plan-sha256", plan_sha256)
    return {"run": run, "unit": unit, "uid": uid, "gid": gid, "runParent": run_parent,
            "source": source, "code": code, "gate": gate, "planSha256": plan_sha256,
            "python": Path(python_path), "argv": argv}


def observer_plan(plan):
    return {key: plan[key] for key in ("run", "unit", "uid", "gid", "runParent")}


def stamp(st):
    return (st.st_dev, st.st_ino, st.st_mode, st.st_uid, st.st_gid, st.st_nlink,
            st.st_size, st.st_mtime_ns, st.st_ctime_ns)


def protected_parent(path):
    need(path.is_absolute() and path.resolve(strict=True) == path, "protected_path")
    for directory in (path, *path.parents):
        item = directory.lstat()
        need(stat.S_ISDIR(item.st_mode) and item.st_uid == 0 and not item.st_mode & 0o022, "protected_parent")


def pinned_file(path, digest, limit, *, executable=False, retain=False):
    protected_parent(path.parent)
    need(match(SHA, digest) and path.resolve(strict=True) == path, "file_pin_shape")
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
    with os.fdopen(os.open(path, flags), "rb") as handle:
        before = os.fstat(handle.fileno())
        need(stat.S_ISREG(before.st_mode) and before.st_uid == 0 and before.st_nlink == 1
             and not before.st_mode & 0o7022 and 0 < before.st_size <= limit
             and (not executable or before.st_mode & 0o111), "pinned_file_metadata")
        body = handle.read(before.st_size + 1)
        need(len(body) == before.st_size and hashlib.sha256(body).hexdigest() == digest
             and stamp(before) == stamp(os.fstat(handle.fileno())) == stamp(path.lstat()), "pinned_file_changed")
    return (stamp(before), body) if retain else stamp(before)


def input_snapshot(plan, pins):
    need(type(pins) is dict and set(pins) == {"systemdRun", "python", "launcher", "waiter"}
         and pins["launcher"] == EXPECTED_LAUNCHER_SHA and pins["waiter"] == EXPECTED_WAITER_SHA,
         "approved_inputs")
    for directory in (plan["source"], plan["source"] / ".git", plan["gate"], plan["code"]):
        protected_parent(directory)
    parent = Path(plan["runParent"])
    item = parent.lstat()
    need(parent.resolve(strict=True) == parent and stat.S_ISDIR(item.st_mode)
         and item.st_uid == plan["uid"] and item.st_gid == plan["gid"]
         and stat.S_IMODE(item.st_mode) == 0o700 and not any(parent.iterdir()), "fresh_scratch_parent")
    return (stamp(item), pinned_file(SYSTEMD_RUN, pins["systemdRun"], 8 << 20, executable=True),
            pinned_file(plan["python"], pins["python"], 32 << 20, executable=True),
            pinned_file(plan["source"] / "scripts/run-bluey-tests.sh", pins["launcher"], 256 << 10, executable=True),
            pinned_file(plan["code"] / WAITER, pins["waiter"], 128 << 10),
            pinned_file(plan["gate"] / "plan.json", plan["planSha256"], 4096))


def start_waiting(plan_args, pins, *, independently_admitted, unit_absent):
    """Start only the waiting actor in the exact separately admitted unit."""
    need(initial_guest_view() and callable(independently_admitted) and callable(unit_absent), "linux_root")
    plan = build_plan(**plan_args)
    need(independently_admitted(dict(plan)) is True, "guest_not_admitted")
    initial = input_snapshot(plan, pins)
    need(unit_absent(plan["unit"]) is True and input_snapshot(plan, pins) == initial
         and initial_guest_view() and independently_admitted(dict(plan)) is True, "unit_not_fresh")
    child = subprocess.Popen(plan["argv"], cwd="/", env=MANAGER_ENV, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, close_fds=True)
    return child, plan


class UnitPipe:
    def __init__(self, child):
        self.child, self.buffer = child, bytearray()
        self.selector = selectors.DefaultSelector()
        self.selector.register(child.stdout, selectors.EVENT_READ, True)
        self.selector.register(child.stderr, selectors.EVENT_READ, False)

    def pump(self, deadline):
        remain = (deadline - monotonic_ns()) / 1_000_000_000
        need(remain > 0, "pipe_timeout")
        ready = self.selector.select(remain)
        need(ready and monotonic_ns() < deadline, "pipe_timeout")
        for entry, _ in ready:
            data = os.read(entry.fd, MAX_FRAME + 1 - len(self.buffer) if entry.data else 1)
            if not data:
                self.selector.unregister(entry.fileobj)
            else:
                need(entry.data, "unit_stderr")
                self.buffer.extend(data)
                need(len(self.buffer) <= MAX_FRAME, "pipe_frame_bound")

    def line(self, deadline):
        while b"\n" not in self.buffer and self.selector.get_map():
            self.pump(deadline)
        offset = self.buffer.find(b"\n")
        need(offset > 0 and offset == len(self.buffer) - 1 and monotonic_ns() < deadline, "unit_frame")
        row = bytes(self.buffer)
        self.buffer.clear()
        return row

    def finish(self, deadline):
        while self.selector.get_map():
            self.pump(deadline)
        remain = (deadline - monotonic_ns()) / 1_000_000_000
        need(remain > 0 and not self.buffer and self.child.wait(timeout=remain) == 0, "unit_transport")

    def close(self):
        self.selector.close()
        self.child.stdout.close()
        self.child.stderr.close()


def numeric_zero(value):
    return type(value) is int and value == 0


def zero_proof(value):
    need(type(value) is dict and set(value) == set(ZERO) and value["unitStopped"] is True
         and numeric_zero(value["actorProcesses"]) and numeric_zero(value["namespaceProcesses"]), "unit_teardown")


def terminal(raw, source_tree, source_inventory):
    need(type(raw) is bytes and 0 < len(raw) <= MAX_FRAME, "terminal_size")
    value = json.loads(raw.decode("ascii"))
    need(type(value) is dict and set(value) == {"event", "sourceTree", "sourceInventorySha256", "evidence"}
         and raw == (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
         and value["event"] == "local_qa_linux_c2c_native_completed" and value["sourceTree"] == source_tree
         and value["sourceInventorySha256"] == source_inventory, "terminal_provenance")
    data = value["evidence"]
    need(type(data) is dict and set(data) == {"matchReads", "navigationsAborted", "mutations", "unexpected", "stableCensus"}
         and type(data["matchReads"]) is int and data["matchReads"] == 1
         and type(data["navigationsAborted"]) is int and data["navigationsAborted"] == 1
         and numeric_zero(data["mutations"]) and numeric_zero(data["unexpected"])
         and data["stableCensus"] is True, "terminal_census")
    return value


def spec_for_launch(raw, waiting, source_tree, source_inventory, fixture_sha256):
    """Check the release-critical projection before the privileged write."""
    need(type(raw) is bytes and 0 < len(raw) <= 65_536, "spec_size")
    value = json.loads(raw.decode("ascii"))
    need(type(value) is dict and set(value) == {"version", "sourceTree", "sourceInventorySha256", "binding",
                                               "browser", "fixture", "unit"}
         and raw == (json.dumps(value, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
         and type(value["version"]) is int and value["version"] == 1
         and value["sourceTree"] == source_tree and value["sourceInventorySha256"] == source_inventory,
         "spec_provenance")
    unit, bind = value["unit"], value["binding"]
    expected_fixture = f"{waiting['root']}/fixtures/jobs-nonprod-browser-fixture"
    need(type(unit) is dict and type(bind) is dict
         and all(unit.get(k) == waiting[k] for k in ("mainPid", "bootId", "invocationId", "unitName"))
         and all(bind.get(k) == waiting[k] for k in ("root", "unitCgroup", "networkNamespace"))
         and value["fixture"] == {"artifactPath": expected_fixture, "artifactSha256": fixture_sha256},
         "spec_live_binding")
    return hashlib.sha256(raw).hexdigest()


def bind_preparation(*, parse_waiting, compose, observe, stage, verify_staged, issue_release,
                     verify_admitted_artifacts, fixture_path, fixture_sha256, source_tree, source_inventory):
    """Close the composition-to-publication handoff; no stage/release retries.

    This binds already admitted root-manager functions. Concrete stager and
    publisher retain their own Linux/root and filesystem checks.
    """
    need(match(SHA, fixture_sha256) and match(TREE, source_tree) and match(SHA, source_inventory)
         and all(callable(step) for step in (parse_waiting, compose, observe, stage, verify_staged,
                                            issue_release, verify_admitted_artifacts)), "preparation_scope")
    staged = None
    stage_attempted = publication_attempted = False

    def prepare(waiting, observation):
        nonlocal staged, stage_attempted
        need(not stage_attempted, "stage_already_attempted")
        stage_attempted = True
        need(observe(dict(waiting)) == observation, "prepare_observation")
        raw = compose(dict(waiting), dict(observation))
        digest = spec_for_launch(raw, waiting, source_tree, source_inventory, fixture_sha256)

        def admitted(live):
            return (live == waiting and observe(dict(live)) == observation
                    and verify_admitted_artifacts(dict(live), digest) is True)

        need(admitted(waiting), "prepare_artifact_custody")
        fingerprint = stage(dict(waiting), spec_bytes=raw, spec_sha256=digest, fixture_path=fixture_path,
                            fixture_sha256=fixture_sha256, independently_admitted=admitted)
        need(type(fingerprint) is dict and fingerprint.get("specSha256") == digest
             and fingerprint.get("fixtureSha256") == fixture_sha256
             and fingerprint.get("releasePublished") is False
             and verify_staged(dict(waiting), dict(fingerprint)) is True, "staged_fingerprint")
        staged = dict(waiting), dict(observation), digest, dict(fingerprint)
        return digest

    def publish(waiting_raw, digest):
        nonlocal publication_attempted
        need(staged is not None and not publication_attempted, "publication_state")
        publication_attempted = True
        waiting, observation, expected_digest, fingerprint = staged
        need(parse_waiting(waiting_raw) == waiting and digest == expected_digest, "release_staged_binding")

        def read_current(live):
            need(live == waiting, "release_waiting_changed")
            current = observe(dict(live))
            need(current == observation, "release_observation_changed")
            return current

        def remeasure(live, spec_digest):
            return (live == waiting and spec_digest == expected_digest
                    and verify_admitted_artifacts(dict(live), spec_digest) is True
                    and verify_staged(dict(live), dict(fingerprint)) is True)

        need(remeasure(waiting, digest), "release_fingerprint_changed")
        return issue_release(waiting_raw, digest, independently_observe=read_current,
                             verify_admitted_artifacts=remeasure)

    return prepare, publish


def bind_admitted_release(plan, *, helper_pins, systemctl_sha256, compose,
                          verify_admitted_artifacts, fixture_path, fixture_sha256, source_tree, source_inventory):
    """Bind only the exact root-owned helper set installed for this manager."""
    need(root_linux() and type(helper_pins) is dict and helper_pins.keys() == HELPERS.keys(), "root_helpers")
    here = Path(__file__).resolve(strict=True).parent
    modules = {}
    for name, (leaf, fixed_pin) in HELPERS.items():
        pin = helper_pins[name]
        need(match(SHA, pin) and fixed_pin == pin, "helper_pin")
        path = here / leaf
        _, raw = pinned_file(path, pin, 256 << 10, retain=True)
        scope = {"__name__": "_admitted_c2c_" + name, "__file__": str(path)}
        exec(compile(raw, str(path), "exec"), scope)
        modules[name] = scope
    parser = modules["protocol"]["parse_waiting"]
    observe = modules["observer"]["make_independent_observer"](observer_plan(plan), systemctl_sha256)
    stage = modules["stager"]
    prepare, publish = bind_preparation(parse_waiting=parser, compose=compose, observe=observe,
                                       stage=stage["stage_actor_inputs"], verify_staged=stage["verify_staged_actor_inputs"],
                                       issue_release=modules["publisher"]["issue_release"],
                                       verify_admitted_artifacts=verify_admitted_artifacts, fixture_path=fixture_path,
                                       fixture_sha256=fixture_sha256, source_tree=source_tree,
                                       source_inventory=source_inventory)
    return {"parse_waiting": parser, "observe": observe, "prepare": prepare, "publish": publish}


def coordinate(child, plan, *, parse_waiting, observe, prepare, publish, settle, recover, unbound_cleanup,
               source_tree, source_inventory, pipe_factory=UnitPipe):
    """Own one already started unit client; establish exact terminal/stop facts.

    The trusted root supervisor supplies independently measured observations,
    artifact composition/staging, writer and cgroup/mount-bound teardown
    adapters. `settle` and `recover` must act only on the bound invocation and
    return physical zero evidence. `unbound_cleanup` must use the supervisor's
    separately captured launch invocation, never the waiting frame; if it
    cannot prove ownership it must refuse. Client exit is not a stop proof.
    """
    need(match(TREE, source_tree) and match(SHA, source_inventory) and
         all(callable(step) for step in (parse_waiting, observe, prepare, publish, settle, recover, unbound_cleanup)),
         "coordination_scope")
    pipe = None
    waiting, owned, stop_attempted = None, False, False
    try:
        pipe = pipe_factory(child)
        deadline = monotonic_ns() + 620_000_000_000
        raw = pipe.line(min(deadline, monotonic_ns() + 45_000_000_000))
        waiting = parse_waiting(raw)
        need(child.poll() is None and waiting["run"] == plan["run"] and waiting["unitName"] == plan["unit"]
             and waiting["actorUid"] == plan["uid"] and str(Path(waiting["root"]).parent) == plan["runParent"],
             "waiting_launch")
        observed = observe(dict(waiting))
        expected = {key: value for key, value in waiting.items() if key not in {"schema", "nonce", "waitingNs"}}
        need(type(observed) is dict and observed.keys() == expected.keys()
             and all(type(observed[k]) is type(expected[k]) and observed[k] == expected[k] for k in expected),
             "waiting_observation")
        owned = True
        digest = prepare(dict(waiting), dict(observed))
        need(match(SHA, digest), "staged_spec")
        release = publish(raw, digest)
        need(type(release) is dict and match(SHA, release.get("releaseSha256"))
             and release.get("runtimeQualified") is False and release.get("productionQualified") is False,
             "release_receipt")
        receipt = terminal(pipe.line(deadline), source_tree, source_inventory)
        stop_attempted = True
        zero_proof(settle(dict(waiting), dict(observed)))
        pipe.finish(deadline)
        return {"schema": "bluey-jobs-c2c-guest-qa-v1", "sourceTree": source_tree,
                "sourceInventorySha256": source_inventory, "specSha256": digest,
                "releaseSha256": release["releaseSha256"], "actor": receipt["evidence"],
                "teardown": dict(ZERO), "syntheticJourneyPassed": True, "productionQualified": False}
    except BaseException:
        if not owned:
            zero_proof(unbound_cleanup(observer_plan(plan)))
        elif not stop_attempted:
            zero_proof(recover(dict(waiting), dict(observed)))
        raise
    finally:
        # This is only the exact transport child, never a unit/PID substitute.
        try:
            close_transport(child)
        finally:
            if pipe is not None:
                pipe.close()


def close_transport(child):
    if child.poll() is None:
        try:
            child.terminate()
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            try:
                child.kill()
            except ProcessLookupError:
                pass
            child.wait(timeout=3)


def drive_started(child, plan, *, capture, teardown, release, source_tree, source_inventory):
    """Acquire pre-frame custody before any actor-stream data influences teardown."""
    entered_coordination = False
    try:
        with capture(observer_plan(plan)) as (claim, netns):
            with teardown(observer_plan(plan), claim, netns) as stops:
                entered_coordination = True
                return coordinate(child, plan, **release, **stops, source_tree=source_tree,
                                  source_inventory=source_inventory)
    except BaseException as error:
        if not entered_coordination:
            # There is no proven physical teardown authority. Do not stop a
            # unit by name or claim it is gone just because the client exits.
            try:
                close_transport(child)
            finally:
                for stream in (child.stdout, child.stderr):
                    stream.close()
            raise RuntimeError("c2c_guest_launch_custody_unproven") from error
        raise


def run_in_guest_custody(projected, *, reserve, dispatch, drive):
    """Arm a durable external guest hold before the one dispatch attempt.

    The outer owner must keep this generation reserved on exception, client
    exit and manager death. `complete` is never called without a positive
    bound teardown result and a still-held reservation. Only the owner can
    resolve a retained hold by guest disposal or fresh exact reconciliation.
    """
    custody = reserve(dict(projected))
    need(type(custody) is dict and custody.keys() == {"held", "complete"}
         and all(callable(callback) for callback in custody.values()), "guest_custody")
    held, complete = custody["held"], custody["complete"]
    need(held() is True, "guest_custody_lost")
    child, launched = dispatch(held)
    result = drive(child, launched, held)
    need(type(result) is dict and result.get("syntheticJourneyPassed") is True, "case_result")
    proof = result.get("teardown")
    zero_proof(proof)
    need(held() is True and complete(dict(proof)) is True, "guest_custody_unconfirmed")
    return result


def run_admitted_journey(plan_args, pins, *, helper_pins, runtime_pins, systemctl_sha256, library_path,
                          library_sha256, independently_admitted, unit_absent, reserve_guest,
                          compose, verify_admitted_artifacts, fixture_path, fixture_sha256, source_tree,
                          source_inventory):
    """Root-only operator entry for already verified installed artifacts.

    `reserve_guest` belongs to the external disposable-guest owner. It must
    durably reserve the exact scope before returning and reject reuse while
    pending. A reservation is not a unit stop authority. The compose and
    admission adapters belong to the selected guest preparation.
    """
    need(initial_guest_view() and type(runtime_pins) is dict
         and runtime_pins == {key: digest for key, (_, digest) in RUNTIME_HELPERS.items()}
         and match(TREE, source_tree) and match(SHA, source_inventory)
         and all(callable(callback) for callback in (independently_admitted, unit_absent, reserve_guest)),
         "guest_journey_admission")
    plan = build_plan(**plan_args)
    projected = observer_plan(plan)
    here = Path(__file__).resolve(strict=True).parent
    modules = {}
    for name, (leaf, digest) in RUNTIME_HELPERS.items():
        path = here / leaf
        _, raw = pinned_file(path, digest, 256 << 10, retain=True)
        scope = {"__name__": "_admitted_c2c_" + name, "__file__": str(path)}
        exec(compile(raw, str(path), "exec"), scope)
        modules[name] = scope
    release = bind_admitted_release(plan, helper_pins=helper_pins, systemctl_sha256=systemctl_sha256,
                                    compose=compose, verify_admitted_artifacts=verify_admitted_artifacts,
                                    fixture_path=fixture_path, fixture_sha256=fixture_sha256,
                                    source_tree=source_tree, source_inventory=source_inventory)

    def dispatch(held):
        return start_waiting(plan_args, pins, independently_admitted=lambda actual: (
            observer_plan(actual) == projected and independently_admitted(actual) is True and held() is True
        ), unit_absent=unit_absent)

    def drive(child, launched, held):
        def capture(actual):
            need(actual == projected, "launch_projection")
            return modules["custody"]["capture_launch_claim"](
                actual, systemctl_sha256, independently_admitted=lambda scope: scope == projected and held() is True,
                deadline_ns=monotonic_ns() + 40_000_000_000,
            )

        def teardown(actual, claim, namespace_fd):
            need(actual == projected, "teardown_projection")
            return modules["teardown"]["bind_teardown"](
                actual, claim, namespace_fd, systemctl_sha256=systemctl_sha256, library_path=library_path,
                library_sha256=library_sha256, independently_admitted=held,
                bind_deadline_ns=monotonic_ns() + 12_000_000_000,
            )

        return drive_started(child, launched, capture=capture, teardown=teardown, release=release,
                             source_tree=source_tree, source_inventory=source_inventory)

    return run_in_guest_custody(projected, reserve=reserve_guest, dispatch=dispatch, drive=drive)
