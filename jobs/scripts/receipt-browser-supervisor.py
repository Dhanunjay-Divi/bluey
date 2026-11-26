"""Own the evidence sequence for one already-held synthetic Linux unit."""
from contextlib import ExitStack, contextmanager
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import time

OBSERVER_SHA = "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4"
BARRIER_SHA = "8a043d7b14ea86cab295b58215b7773aafca7bd3fd4bbf3fa5aee6a8a454dadf"
WRITER_SHA = "8aeeba6bb06e37976201ea4b88c8a532ab6dbb8ab4bc56615fb7e8c3ad1d6014"
TERMINAL_SHA = "65a9a84ea0858c19a442b28320920c1e9ae481fc4c714747e2e7602283b5f8e0"
POST_STOP_SHA = "26a34e0cb65ad3d34d2d72b3cfb9ae58f280fcb99fa3ff968ae8bce5f3d46a48"
MANAGER_STOP_SHA = "b41eeb361e5511885995b1318c08beec22fc7699d41f67a915e4a8012ba4a1e9"
LIMIT = 4096


def require(ok, reason):
    if not ok:
        raise RuntimeError(reason)


def clock():
    return time.clock_gettime_ns(time.CLOCK_MONOTONIC)


def pinned(name, digest):
    path = Path(__file__).resolve().with_name(name)
    with path.open("rb") as file:
        source = file.read(32 * 1024)
        require(file.read(1) == b"" and hashlib.sha256(source).hexdigest() == digest,
                "supervisor_source_pin")
    return path, source


def load(name, digest):
    path, source = pinned(name, digest)
    module = {"__name__": "verified_supervisor_helper", "__file__": str(path)}
    exec(compile(source, str(path), "exec"), module)
    return module


def observer_frame(event, items):
    return (json.dumps({"event": event, **items}) + "\n").encode("ascii")


def bound_frame(line, scope):
    value = json.loads(line)
    require(isinstance(value, dict), "supervisor_bound_shape")
    items = {key: scope[key] for key in ("run", "boot", "invocation")}
    for key in ("mountId", "eventsInode"):
        item = value.get(key)
        require(type(item) is int and 0 < item < 2**53, "supervisor_bound_descriptor")
        items[key] = item
    require(line == observer_frame("jobs_browser_closure_bound", items), "supervisor_bound_scope")
    return items


class ObserverPipe:
    """One child, original stdout, no inherited stdin and no unbounded reads."""

    def __init__(self, child, select):
        self.child, self.select = child, select
        self.data = bytearray()
        select.register(child.stdout, selectors.EVENT_READ, True)
        select.register(child.stderr, selectors.EVENT_READ, False)

    def pump(self, deadline):
        remaining = (deadline - clock()) / 1_000_000_000
        require(remaining > 0, "supervisor_observer_deadline")
        ready = self.select.select(remaining)
        require(ready, "supervisor_observer_deadline")
        for key, _ in ready:
            data = os.read(key.fd, LIMIT + 1 - len(self.data) if key.data else 1)
            if not data:
                self.select.unregister(key.fileobj)
            elif not key.data:
                raise RuntimeError("supervisor_observer_stderr")
            else:
                self.data.extend(data)
                require(len(self.data) <= LIMIT, "supervisor_observer_output")

    def first(self, deadline):
        while b"\n" not in self.data and self.select.get_map():
            self.pump(deadline)
        require(self.data.count(b"\n") == 1 and self.data.endswith(b"\n")
                and self.live(), "supervisor_observer_not_held")
        return bytes(self.data)

    def live(self):
        return self.child.poll() is None

    def complete(self, deadline):
        while self.select.get_map():
            self.pump(deadline)
        remaining = (deadline - clock()) / 1_000_000_000
        require(remaining > 0 and self.child.wait(timeout=remaining) == 0 and clock() < deadline,
                "supervisor_observer_exit")
        return bytes(self.data)


@contextmanager
def open_observer(argv):
    child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, cwd="/", start_new_session=True,
                             env={"PATH": "/usr/bin:/bin", "LC_ALL": "C", "LANG": "C"})
    try:
        with selectors.DefaultSelector() as select:
            yield ObserverPipe(child, select)
    finally:
        try:
            # This is our own freshly created session, never the unit.
            if child.returncode is None:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                child.wait(timeout=2)
        finally:
            child.stdout.close()
            child.stderr.close()


def join(scope, observer, bind_terminal, publish, export, admitted, release_deadline, deadline):
    """Transport-independent sequence; the root entry owns each source."""
    first = observer.first(release_deadline)
    authority = bound_frame(first, scope)
    bound_ns = clock()
    require(admitted() is True, "supervisor_unadmitted")
    capture = bind_terminal()
    published = publish(first, observer.live)
    stdout = observer.complete(deadline)
    expected = observer_frame("jobs_browser_closure_observed", {
        **{key: scope[key] for key in ("run", "boot", "invocation")},
        "populated": 0, "sameEventsDescriptor": True,
    })
    require(stdout == first + expected and clock() < deadline and admitted() is True,
            "supervisor_observer_zero")
    zero_accepted = clock()
    observed = capture(published["publicationAfterNs"])
    require(published["publicationAfterNs"] < observed["exitNs"] < zero_accepted < observed["observedNs"]
            and clock() < deadline and admitted() is True,
            "supervisor_terminal_order")
    # Last effect: export only after the exact held child proved zero and
    # the bound manager read reconfirmed its terminal state.
    reference = export(observed, published)
    try:
        require(clock() < deadline and admitted() is True, "supervisor_export_deadline")
    except BaseException:
        os.close(reference["fd"])
        raise
    return {"launchObserver": authority, "boundNs": bound_ns, "release": published, "terminal": observed,
            "terminalArtifact": reference, "zeroAcceptedNs": zero_accepted,
            "observer": {"stdout": stdout.decode("ascii"), "stderr": "", "code": 0,
                         "signal": None, "sha256": OBSERVER_SHA}, "runtimeAdmitted": False}


def supervise_waiting_unit(waiting, supervised, output_fd, mode, *,
                           independently_admitted, release_deadline_ns, deadline_ns):
    """Root-only. The dispatcher still owns the unit and any stop/census.

    The independent admission predicate must survive legitimate actor exit:
    it proves ownership/lease, not actor liveness. This starts only the
    read-only observer. It never starts, stops or grants authority to a unit.
    """
    require(sys.platform == "linux" and os.getuid() == os.geteuid() == 0,
            "supervisor_linux_root")
    require(callable(independently_admitted) and independently_admitted() is True,
            "supervisor_unadmitted")
    now = clock()
    require(type(release_deadline_ns) is int and type(deadline_ns) is int
            and now < release_deadline_ns < deadline_ns <= now + 1_799_000_000_000,
            "supervisor_deadline")
    terminal = load("receipt-browser-unit-terminal.py", TERMINAL_SHA)
    writer = load("receipt-browser-release-writer.py", WRITER_SHA)
    barrier = load("receipt-browser-launch-barrier.py", BARRIER_SHA)
    _, source = pinned("interview-prep-linux-closure.py", OBSERVER_SHA)
    scope = terminal["closed_scope"]({key: waiting[key] for key in terminal["SCOPE"]})
    timeout = (deadline_ns - now + 999_999_999) // 1_000_000_000
    argv = [sys.executable, "-IB", "-c", source.decode("ascii"),
            scope["run"], scope["boot"], scope["invocation"], str(timeout)]
    with ExitStack() as stack:
        gate = barrier["protected_run_gate"](scope["run"], stack)
        observer = stack.enter_context(open_observer(argv))
        return join(
            scope, observer,
            lambda: terminal["bind_unit_terminal"](
                scope, mode, independently_admitted=independently_admitted, deadline_ns=deadline_ns),
            lambda bound, live: writer["write_after_bound"](
                waiting, supervised, bound, live, deadline_ns=release_deadline_ns,
                independently_admitted=independently_admitted),
            lambda observation, publication: terminal["export_unit_terminal"](
                output_fd, gate, observation, waiting, publication),
            independently_admitted, release_deadline_ns, deadline_ns,
        )


def accept_after_stop(evidence, stop_exact_unit, census, admitted, deadline_ns):
    try:
        scope = evidence["terminal"]["record"]
        require(clock() < deadline_ns and admitted() is True, "supervisor_stop_unadmitted")
        require(stop_exact_unit(scope["unit"], scope["invocation"], deadline_ns) is True,
                "supervisor_stop_not_confirmed")
        post_stop = census()
        stopped = clock()
        require(stopped < deadline_ns and admitted() is True
                and post_stop == {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0},
                "supervisor_stop_unverified")
        return {**evidence, "postStop": post_stop, "stoppedNs": stopped}
    except BaseException:
        os.close(evidence["terminalArtifact"]["fd"])
        raise


def supervise_and_verify_stop(waiting, supervised, output_fd, mode, *,
                              stop_exact_unit, independently_admitted,
                              release_deadline_ns, deadline_ns):
    """Per-unit dispatcher entry; no hardcoded mutation or stop policy.

    The dispatcher supplies the already-admitted, bounded system-manager
    stop operation for this exact unit and invocation, and owns emergency
    cleanup even when this evidence path fails before a unit stops.
    """
    require(sys.platform == "linux" and os.getuid() == os.geteuid() == 0, "supervisor_linux_root")
    require(callable(stop_exact_unit), "supervisor_stop_not_bound")
    post_stop = load("receipt-browser-post-stop.py", POST_STOP_SHA)
    with post_stop["bind_post_stop"](waiting, independently_admitted=independently_admitted,
                                    deadline_ns=deadline_ns) as census:
        evidence = supervise_waiting_unit(
            waiting, supervised, output_fd, mode, independently_admitted=independently_admitted,
            release_deadline_ns=release_deadline_ns, deadline_ns=deadline_ns)
        return accept_after_stop(evidence, stop_exact_unit, census, independently_admitted, deadline_ns)


def under_manager_stop(reference, supervise, independently_admitted, deadline_ns):
    evidence = None
    try:
        with reference as stop:
            evidence = supervise(stop)
        require(clock() < deadline_ns and independently_admitted() is True, "supervisor_manager_release")
        return evidence
    except BaseException:
        if evidence is not None:
            os.close(evidence["terminalArtifact"]["fd"])
        raise


def supervise_with_manager_stop(waiting, supervised, output_fd, mode, *,
                                 library_path, library_sha256, independently_admitted,
                                 release_deadline_ns, deadline_ns):
    """Keep the systemd unit referenced until held zero, stop and census finish."""
    require(sys.platform == "linux" and os.getuid() == os.geteuid() == 0, "supervisor_linux_root")
    manager = load("receipt-browser-manager-stop.py", MANAGER_STOP_SHA)
    reference = manager["bind_manager_stop"](
        waiting, mode, library_path=library_path, library_sha256=library_sha256,
        independently_admitted=independently_admitted, deadline_ns=deadline_ns)
    return under_manager_stop(
        reference, lambda stop: supervise_and_verify_stop(
            waiting, supervised, output_fd, mode, stop_exact_unit=stop,
            independently_admitted=independently_admitted, release_deadline_ns=release_deadline_ns,
            deadline_ns=deadline_ns),
        independently_admitted, deadline_ns,
    )
