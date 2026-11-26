import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs, { closeSync, constants, fstatSync, mkdtempSync, openSync, readFileSync,
  rmSync, statSync, writeFileSync } from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { managedReceiptCases } from "./managed-receipt-postgres-proof.mjs";
import { collectReceiptBrowserTerminal, parseSupervisorTerminal,
  qualifyCollectedReceiptBrowser } from "./receipt-browser-terminal-collector.mjs";

const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const base = {
  run: "ab".repeat(16), boot: "12345678-abcd-1234-abcd-123456789abc",
  invocation: "cd".repeat(16), challenge: "ef".repeat(16),
};
const root = "/tmp/bluey-test-runs-0123456789ab/run.1794466886.10001.16299199040";
const unit = { code: 0, signal: null, deadline: false };
const [item] = managedReceiptCases("interview-prep-browser");

function fixtureBytes() {
  const name = item.name, names = [name, "db::tests::unselected"];
  return Buffer.from([
    { event: "nonprod_postgres_ready", address: "127.0.0.1", port: 50000, database: "bluey_jobs_nonprod",
      dataDirectory: root + "/nonprod-postgres/data", systemIdentifier: "100",
      version: "PostgreSQL 17.11 (fixture) on x86_64-pc-linux-gnu, compiled by GCC, 64-bit",
      vector: "0.8.7", platform: "linux-x64", serverVersionNum: 170011 },
    { event: "nonprod_postgres_test_inventory", names, count: names.length, sha256: sha(JSON.stringify(names)) },
    { event: "nonprod_postgres_case_started", test: name, database: "bluey_jobs_case_" + "a".repeat(24) },
    "running 1 test", `test ${name} ... ${item.witnesses[0]}`, ...item.witnesses.slice(1), "ok",
    "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1.00s",
    { event: "nonprod_postgres_case_finished", test: name, exitCode: 0, signal: null },
    { event: "nonprod_postgres_cases_finished", tests: 1, passed: 1, failed: [] },
    { event: "nonprod_postgres_finished", status: 0, shutdownAttempted: true, serverExited: true },
    "",
  ].map((row) => typeof row === "string" ? row : JSON.stringify(row)).join("\n"));
}

const asWire = (obj) => Buffer.from(JSON.stringify(Object.fromEntries(
  Object.keys(obj).sort().map((key) => [key, obj[key]]))) + "\n");

function fixture(t, out = fixtureBytes(), err = Buffer.alloc(0), actualExport = false) {
  const directory = mkdtempSync(join(tmpdir(), "bluey-terminal-fixture-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  if (actualExport) {
    const py = process.env.BLUEY_FIXTURE_PYTHON;
    assert(py?.startsWith("/"), "fixture_python_path");
    const script = fileURLToPath(new URL("./receipt-browser-stream-export.py", import.meta.url));
    const status = spawnSync(py, ["-IB", "-c", `
import base64, importlib.util, json, os, sys
spec=importlib.util.spec_from_file_location("exporter",sys.argv[1])
exporter=importlib.util.module_from_spec(spec)
spec.loader.exec_module(exporter)
scope,data,err=json.loads(sys.stdin.read())
fd=os.open(sys.argv[2],os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
try:
    st=os.fstat(fd)
    fn=exporter.bind_exporter(fd,scope,st.st_dev,st.st_ino)
    assert fn(scope,base64.b64decode(data),base64.b64decode(err)) is True
    try:
        fn(scope,b"",b"")
        raise AssertionError("unexpected repeated export")
    except RuntimeError as exc:
        assert str(exc)=="stream_export_binding"
finally:
    os.close(fd)
`, script, directory], {
      input: JSON.stringify([base, out.toString("base64"), err.toString("base64")]),
      encoding: "utf8", timeout: 3000,
    });
    assert.deepEqual([status.status, status.stderr, status.stdout], [0, "", ""], status.error?.message);
  } else {
    writeFileSync(join(directory, "wrapper.stdout"), out, { mode: 0o600, flag: "wx" });
    writeFileSync(join(directory, "wrapper.stderr"), err, { mode: 0o600, flag: "wx" });
  }
  const streams = Object.fromEntries(["stdout", "stderr"].map((name) => {
    const fd = openSync(join(directory, "wrapper." + name), constants.O_RDONLY | constants.O_NOFOLLOW);
    t.after(() => closeSync(fd));
    const { dev: device, ino: inode } = fstatSync(fd, { bigint: true });
    return [name, { fd, device, inode }];
  }));
  const terminal = { ...base, event: "jobs_browser_owned_wrapper_terminal", code: 0, signalNumber: null,
    stdoutBytes: out.length, stderrBytes: err.length, stdoutSha256: sha(out), stderrSha256: sha(err),
    inputReadback: true, runtimeAdmitted: false };
  return { directory, trusted: { ...base, root, actorUid: process.getuid() }, streams, terminal };
}

test("joins real Python-exported file bytes to the exact browser proof", (t) => {
  const f = fixture(t, fixtureBytes(), Buffer.from("synthetic diagnostics\n"), true);
  const result = collectReceiptBrowserTerminal(asWire(f.terminal), f.trusted, f.streams, unit);
  assert.equal(result.proof.cases[0].name, item.name);
  assert.equal(result.wrapper.stdout, fixtureBytes().toString("utf8"));
  assert.equal(result.wrapper.stderr, "synthetic diagnostics\n");
  assert.equal(result.runtimeAdmitted, false);
  assert.equal(statSync(join(f.directory, "wrapper.stdout")).nlink, 1);
});

for (const [label, mutate, reason] of [
  ["another release challenge", (f) => { f.terminal.challenge = "12".repeat(16); }, "receipt_release_binding"],
  ["closed stream digest drift", (f) => { f.terminal.stdoutSha256 = "00".repeat(32); }, "receipt_stream_digest"],
  ["substituted descriptor", (f) => { f.streams.stdout.device += 1n; }, "receipt_stream_identity"],
  ["stream alias", (f) => { f.streams.stderr = f.streams.stdout; }, "receipt_stream_alias"],
  ["false readback", (f) => { f.terminal.inputReadback = false; }, "receipt_terminal_policy"],
  ["nonzero wrapper despite finished unit", (f) => { f.terminal.code = 3; }, "receipt_wrapper_terminal"],
  ["a truncated artifact", (f) => { f.terminal.stdoutBytes += 1; }, "receipt_stream_identity"],
  ["an oversized byte claim", (f) => { f.terminal.stderrBytes = 4 * 1024 ** 2 + 1; }, "receipt_stream_bound"],
]) {
  test("refuses " + label, (t) => {
    const f = fixture(t);
    mutate(f);
    assert.throws(() => collectReceiptBrowserTerminal(asWire(f.terminal), f.trusted, f.streams, unit),
      new RegExp(reason));
  });
}

test("a finished unit and a canonical terminal are independent requirements", (t) => {
  const f = fixture(t);
  assert.throws(() => collectReceiptBrowserTerminal(asWire(f.terminal), f.trusted, f.streams,
    { code: null, signal: "SIGTERM", deadline: true }), /receipt_unit_terminal/);
  const raw = Buffer.from(asWire(f.terminal).toString().replace('"code":0', '"code":0,"code":0'));
  assert.throws(() => collectReceiptBrowserTerminal(raw, f.trusted, f.streams, unit),
    /receipt_terminal_noncanonical/);
});

test("captured waiting/control lines cannot become the wrapper terminal transcript", (t) => {
  const f = fixture(t, Buffer.concat([fixtureBytes(), Buffer.from('{"event":"jobs_browser_waiting_for_release"}\n')]));
  assert.throws(() => collectReceiptBrowserTerminal(asWire(f.terminal), f.trusted, f.streams, unit),
    /wrapper_finished_order/);
});

test("same-size file rewrite is refused before parsing", (t) => {
  const f = fixture(t);
  const path = join(f.directory, "wrapper.stdout");
  const changed = readFileSync(path);
  changed[0] = 0x61;
  writeFileSync(path, changed);
  assert.throws(() => collectReceiptBrowserTerminal(asWire(f.terminal), f.trusted, f.streams, unit),
    /receipt_stream_digest/);
});

test("invalid UTF-8 cannot be repaired into an accepted stream", (t) => {
  const f = fixture(t, fixtureBytes(), Buffer.from([0xff]));
  assert.throws(() => collectReceiptBrowserTerminal(asWire(f.terminal), f.trusted, f.streams, unit),
    /encoded data|encoding|UTF-8/i);
});

test("reads the Python supervisor's canonical terminal bytes without reserialization", () => {
  const py = process.env.BLUEY_FIXTURE_PYTHON;
  assert(py?.startsWith("/"), "fixture_python_path");
  const script = fileURLToPath(new URL("./receipt-browser-unit-terminal.py", import.meta.url));
  const record = { ...base, root, actorUid: 1001, unit: "bluey-jobs-receipt-" + base.run + ".service",
    status: { code: null, deadline: true, signal: "SIGTERM" } };
  const releaseSha256 = sha("same-run release");
  const child = spawnSync(py, ["-IB", "-c", [
    "import json, runpy, sys",
    "render = runpy.run_path(sys.argv[1])['render_terminal']",
    "sys.stdout.buffer.write(render({'record': json.loads(sys.stdin.read())}, sys.argv[2]))",
  ].join("\n"), script, releaseSha256], {
    input: JSON.stringify(record), encoding: "utf8", timeout: 3000,
  });
  assert.deepEqual([child.status, child.stderr], [0, ""], child.error?.message);
  assert.deepEqual(parseSupervisorTerminal(child.stdout), { ...record, releaseSha256 });
});

test("passes the held files to the final verifier only for the released unit", (t) => {
  const f = fixture(t);
  function scope(timeout) {
    const s = timeout ? { run: "23".repeat(16), boot: base.boot, invocation: "45".repeat(16),
      root: root.replace(".10001.", ".10002.") } : {
      run: base.run, boot: base.boot, invocation: base.invocation, root,
    };
    const i = timeout ? 2 : 1;
    const { run, boot, invocation } = s;
    const observerBase = { run, boot, invocation };
    const mountId = 32, eventsInode = 123 + i;
    const fmt = (row) => "{" + Object.entries(row).map(([k, v]) =>
      `${JSON.stringify(k)}: ${JSON.stringify(v)}`).join(", ") + "}\n";
    const trusted = { ...s, unit: `bluey-jobs-receipt-${run}.service`,
      rootDevice: 25, rootInode: 1000 + i, mountId, eventsInode };
    return { trusted, observed: {
      ...trusted, mode: timeout ? "timeout-control" : "case",
      marksNs: { bound: 1, released: 10, terminal: 20, zero: 30, stopped: 40 },
      network: { supervisorNamespace: 100, workloadNamespace: 200 + i, interfaces: ["lo"], defaultRoutes: 0 },
      observer: { stdout: fmt({ event: "jobs_browser_closure_bound", ...observerBase, mountId, eventsInode })
        + fmt({ event: "jobs_browser_closure_observed", ...observerBase, populated: 0, sameEventsDescriptor: true }),
      stderr: "", code: 0, signal: null, sha256: "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4" },
      postStop: { unitStopped: true, actorProcesses: 0, namespaceProcesses: 0 },
      terminal: timeout ? { code: null, signal: "SIGKILL", deadline: true } : unit,
    } };
  }
  const normal = scope(false), timeout = scope(true);
  const { rootDevice, rootInode, mountId, eventsInode, ...timeoutScope } = timeout.trusted;
  const timeoutAdmission = { ...timeoutScope, challenge: "fe".repeat(16), actorUid: f.trusted.actorUid,
    releaseSha256: sha("timeout release") };
  const caseRecord = { ...f.trusted, unit: normal.trusted.unit, releaseSha256: sha("case release"), status: unit };
  const timeoutRecord = { ...timeoutAdmission, status: timeout.observed.terminal };
  const fakeRootFiles = new Set();
  let nextFile = 0;
  function artifact(record) {
    const body = asWire({ ...record, status: { code: record.status.code,
      deadline: record.status.deadline, signal: record.status.signal } });
    const path = join(f.directory, "supervisor-" + ++nextFile);
    writeFileSync(path, body, { mode: 0o600, flag: "wx" });
    const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    t.after(() => closeSync(fd));
    fakeRootFiles.add(fd);
    const { dev: device, ino: inode } = fstatSync(fd, { bigint: true });
    return { fd, device, inode, bytes: body.length, sha256: sha(body) };
  }
  const args = {
    terminalFrame: asWire(f.terminal), release: f.trusted, streams: f.streams,
    terminalArtifacts: { case: artifact(caseRecord), timeout: artifact(timeoutRecord) },
    supervisorTranscript: JSON.stringify({
      sourceReceiptSha256: "aa2ea7b02b17407e9a80871db8515002880da490b7ef144118d2003be6d34727",
      stdoutSha256: f.terminal.stdoutSha256, stderrSha256: f.terminal.stderrSha256,
      caseScope: normal.observed, timeoutScope: timeout.observed,
    }) + "\n",
    trustedLaunch: { case: { ...normal.trusted, challenge: base.challenge, actorUid: f.trusted.actorUid,
      releaseSha256: caseRecord.releaseSha256 },
      timeout: { ...timeoutAdmission, rootDevice, rootInode, mountId, eventsInode } },
  };
  if (process.getuid() !== 0) {
    assert.throws(() => qualifyCollectedReceiptBrowser(args), /receipt_stream_identity/);
  }
  const realStat = fs.fstatSync;
  // Stand in for Linux root ownership only. File contents, offsets, hashes,
  // normal and timeout scopes and the rest of stat are real checks.
  const patched = t.mock.method(fs, "fstatSync", (fd, options) => {
    const info = realStat(fd, options);
    if (fakeRootFiles.has(fd)) info.uid = 0n;
    return info;
  });
  syncBuiltinESMExports();
  try {
    const receipt = qualifyCollectedReceiptBrowser(args);
    assert.equal(receipt.closureRecordsBound, true);
    assert.equal(receipt.runtimeAdmitted, false);
    assert.equal(receipt.unitTerminalSha256, args.terminalArtifacts.case.sha256);
    assert.equal(receipt.timeoutTerminalSha256, args.terminalArtifacts.timeout.sha256);
    const set = (changes) => ({ ...args, terminalArtifacts: { ...args.terminalArtifacts, ...changes } });
    assert.throws(() => qualifyCollectedReceiptBrowser(set({ case: caseRecord })),
      /receipt_terminal_artifact_reference/);
    assert.throws(() => qualifyCollectedReceiptBrowser(set({ timeout: args.terminalArtifacts.case })),
      /receipt_terminal_artifact_alias/);
    assert.throws(() => qualifyCollectedReceiptBrowser(set({ timeout: {
      ...args.terminalArtifacts.timeout, sha256: sha("not the original bytes"),
    } })), /receipt_stream_digest/);
    assert.throws(() => qualifyCollectedReceiptBrowser(set({ case: {
      ...args.terminalArtifacts.case, inode: args.terminalArtifacts.case.inode + 1n,
    } })), /receipt_stream_identity/);
    assert.throws(() => qualifyCollectedReceiptBrowser(set({
      case: artifact({ ...caseRecord, invocation: timeout.trusted.invocation }),
    })), /receipt_unit_terminal_scope/);
    assert.throws(() => qualifyCollectedReceiptBrowser(set({
      timeout: artifact({ ...timeoutRecord, releaseSha256: sha("different release") }),
    })), /receipt_unit_terminal_scope/);
    assert.throws(() => qualifyCollectedReceiptBrowser(set({ timeout: artifact({ ...timeoutRecord, status: unit }) })),
      /receipt_timeout_terminal_mismatch/);
    const reused = set({ timeout: artifact({ ...timeoutRecord, challenge: base.challenge }) });
    reused.trustedLaunch = { ...args.trustedLaunch, timeout: { ...args.trustedLaunch.timeout,
      challenge: base.challenge } };
    assert.throws(() => qualifyCollectedReceiptBrowser(reused), /receipt_unit_reused_challenge/);
  } finally {
    patched.mock.restore();
    syncBuiltinESMExports();
  }
  const canonical = asWire({ ...caseRecord, status: { code: 0, deadline: false, signal: null } }).toString();
  assert.deepEqual(parseSupervisorTerminal(canonical), caseRecord);
  assert.throws(() => parseSupervisorTerminal(canonical.replace('{"actorUid":',
    '{"actorUid":1,"actorUid":')), /receipt_supervisor_terminal_noncanonical/);
  assert.throws(() => parseSupervisorTerminal(canonical.trimEnd()), /receipt_supervisor_terminal_noncanonical/);
  assert.throws(() => qualifyCollectedReceiptBrowser({
    ...args, release: { ...f.trusted, challenge: "aa".repeat(16) },
  }), /receipt_launch_scope/);
});
