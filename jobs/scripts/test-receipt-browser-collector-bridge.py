"""Exercise the POSIX Python/Node descriptor handoff without root or Jobs effects."""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import tempfile
import unittest

DIRECTORY = Path(__file__).parent.resolve()
BRIDGE = runpy.run_path(str(DIRECTORY / "receipt-browser-collector-bridge.py"))
NODE = os.environ.get("BLUEY_FIXTURE_NODE", "")
ORIGIN = 20_000_000_000_000_000  # Absolute JS number would lose precision.
ZERO_COUNTS = {"unitStopped": True, "actorProcesses": 0, "namespaceProcesses": 0}


def digest(blob):
    return hashlib.sha256(blob).hexdigest()


class BridgeTests(unittest.TestCase):
    def setUp(self):
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        self.root = Path(self.stack.enter_context(tempfile.TemporaryDirectory()))
        self.next_idx = 0
        self.payload = [b"original stdout\n", b"original stderr\n"]
        self.streams = {k: self.artifact(v) for k, v in zip(("stdout", "stderr"), self.payload)}
        self.actor_frame = BRIDGE["wire"]({"stdoutSha256": digest(self.payload[0]),
                                          "stderrSha256": digest(self.payload[1])}, sort=True)
        self.case, self.timeout = self.make_run(False), self.make_run(True)

    def artifact(self, data, terminal=False, writable=False):
        self.next_idx += 1
        path = self.root / str(self.next_idx)
        with os.fdopen(os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600), "wb") as file:
            file.write(data)
        fd = os.open(path, os.O_RDWR if writable else os.O_RDONLY)
        self.stack.callback(os.close, fd)
        os.lseek(fd, len(data), os.SEEK_SET)
        info = os.fstat(fd)
        return {"fd": fd, "device": info.st_dev, "inode": info.st_ino,
                **({"bytes": len(data), "sha256": digest(data)} if terminal else {})}

    def make_run(self, late):
        i = 2 if late else 1
        run = str(i) * 32
        scope = {"run": run, "boot": "12345678-abcd-1234-abcd-123456789abc",
                 "invocation": str(i + 2) * 32, "challenge": str(i + 4) * 32,
                 "unit": "bluey-jobs-receipt-" + run + ".service", "actorUid": os.getuid(),
                 "root": f"/tmp/bluey-test-runs-0123456789ab/run.1794466886.1000{i}.16299199040"}
        waiting = {**scope, "rootDevice": 10, "rootInode": 900 + i, "workloadNetns": 300 + i}
        supervised = {"supervisorNetns": 100, "interfaces": ["lo"], "defaultRoutes": 0}
        launch = {k: scope[k] for k in ("run", "boot", "invocation")}
        status = {"code": None, "deadline": True, "signal": "SIGTERM"} if late else {
            "code": 0, "deadline": False, "signal": None}
        sha = digest(f"release {i}".encode())
        body = BRIDGE["wire"]({**scope, "status": status, "releaseSha256": sha}, sort=True)
        offset = i * 50
        evidence = {"launchObserver": {**launch, "mountId": 71, "eventsInode": 9000 + i},
                    "release": {"releaseSha256": sha, "publicationAfterNs": ORIGIN + offset + 10},
                    "terminal": {"record": {**scope, "status": status}, "exitNs": ORIGIN + offset + 20},
                    "boundNs": ORIGIN + offset + 1, "zeroAcceptedNs": ORIGIN + offset + 30,
                    "stoppedNs": ORIGIN + offset + 40, "postStop": ZERO_COUNTS, "observer": {},
                    "terminalArtifact": self.artifact(body, terminal=True), "runtimeAdmitted": False}
        return waiting, supervised, evidence

    def prepare(self, streams=None):
        return BRIDGE["prepare"](self.case, self.timeout, self.actor_frame,
                                 self.streams if streams is None else streams, ORIGIN)

    def node(self, code, frame, fds, ms=2000):
        self.assertTrue(NODE.startswith("/"), "fixture_node_path")
        url = json.dumps((DIRECTORY / BRIDGE["ENTRY"]).as_uri())
        return BRIDGE["invoke"]([NODE, "--input-type=module", "-e", "import * as bridge from " + url + ";" + code],
                                frame, fds, BRIDGE["clock"]() + ms * 1_000_000)

    def test_real_child_reads_all_four_original_files_at_offset_zero(self):
        frame, fds = self.prepare()
        data = self.node("""
import {readFileSync, fstatSync, readSync} from 'node:fs';
const value = bridge.decodeControl(readFileSync(0));
const refs = [value.streams.stdout,value.streams.stderr,value.terminalArtifacts.case,value.terminalArtifacts.timeout];
const bytes = refs.map((r) => {
  const s = fstatSync(r.fd,{bigint:true});
  if (s.dev !== r.device || s.ino !== r.inode) throw Error('identity');
  const b = Buffer.alloc(Number(s.size));
  if (readSync(r.fd,b,0,b.length,0) !== b.length) throw Error('read');
  return b.toString();
});
process.stdout.write(JSON.stringify({bytes, raw: value.terminalFrame.toString(),
  order: JSON.parse(value.supervisorTranscript).caseScope.marksNs})+'\\n');""", frame, fds)
        result = json.loads(data)
        self.assertEqual(result["bytes"][:2], [x.decode() for x in self.payload])
        self.assertEqual(json.loads(result["bytes"][2])["status"]["code"], 0)
        self.assertEqual(json.loads(result["bytes"][3])["status"]["signal"], "SIGTERM")
        self.assertEqual(result["raw"], self.actor_frame.decode())
        self.assertEqual(result["order"], {"bound": 51, "released": 60, "terminal": 70, "zero": 80, "stopped": 90})
        for fd in fds:
            self.assertEqual(os.lseek(fd, 0, os.SEEK_CUR), os.fstat(fd).st_size)

    def test_descriptor_identities_stay_bigints_in_node(self):
        original, fds = self.prepare()
        control = json.loads(original)
        control["streams"]["stdout"]["inode"] = "18446744073709551615"
        code = "import {readFileSync} from 'node:fs'; const r=bridge.decodeControl(readFileSync(0));" \
               "process.stdout.write(r.streams.stdout.inode.toString());"
        self.assertEqual(self.node(code, BRIDGE["wire"](control, sort=True), fds), b"18446744073709551615")
        control["streams"]["stdout"]["inode"] = "18446744073709551616"
        with self.assertRaisesRegex(RuntimeError, "bridge_child_stderr"):
            self.node(code, BRIDGE["wire"](control, sort=True), fds)

    def test_normal_and_timeout_cannot_share_challenge_or_invalid_time(self):
        self.timeout[0]["challenge"] = self.case[0]["challenge"]
        self.timeout[2]["terminal"]["record"]["challenge"] = self.case[0]["challenge"]
        with self.assertRaisesRegex(RuntimeError, "scope_reuse"):
            self.prepare()
        self.timeout = self.make_run(True)
        self.timeout[2]["zeroAcceptedNs"] = self.timeout[2]["terminal"]["exitNs"]
        with self.assertRaisesRegex(RuntimeError, "time_order"):
            self.prepare()

    def test_alias_and_writable_original_refused_by_parent(self):
        with self.assertRaisesRegex(RuntimeError, "fd_alias"):
            self.prepare({"stdout": self.streams["stdout"], "stderr": self.streams["stdout"]})
        with self.assertRaisesRegex(RuntimeError, "descriptor_identity"):
            self.prepare({"stdout": self.artifact(b"writable", writable=True), "stderr": self.streams["stderr"]})

    def test_transport_caps_stdout_stderr_and_duration(self):
        frame, fds = self.prepare()
        for code, reason in [('console.log("x".repeat(4096));', "child_stdout"),
                             ('console.error("expected");', "child_stderr"),
                             ('setInterval(()=>{},1000);', "child_deadline")]:
            with self.subTest(reason=reason), self.assertRaisesRegex(RuntimeError, reason):
                self.node("import {readFileSync} from 'node:fs';readFileSync(0);" + code,
                          frame, fds, ms=500)

    @unittest.skipIf(sys.platform == "linux", "non-Linux refusal only")
    def test_privileged_entry_refuses_darwin_before_admission(self):
        with self.assertRaisesRegex(RuntimeError, "linux_root"):
            BRIDGE["verify_pair"](None, None, None, None, 0, node="/invalid", node_sha256="",
                                  module_directory="/invalid", collector_uid=1, collector_gid=1,
                                  independently_admitted=self.fail, deadline_ns=0)


if __name__ == "__main__":
    unittest.main()
