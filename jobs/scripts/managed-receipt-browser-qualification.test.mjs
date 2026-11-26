import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { verifyBrowserReceiptQualification } from "./managed-receipt-browser-qualification.mjs";
import { managedReceiptCases } from "./managed-receipt-postgres-proof.mjs";

const sha = (value) => createHash("sha256").update(value).digest("hex");
const asObserverLine = (obj) => `{${Object.entries(obj).map(([key, value]) =>
  `${JSON.stringify(key)}: ${JSON.stringify(value)}`).join(", ")}}\n`;
const [selected] = managedReceiptCases("interview-prep-browser");
const boot = "0a000000-0000-0000-0000-000000000001";

function scope(mode, i) {
  const run = String(i).repeat(32), invocation = String(i + 2).repeat(32);
  const root = `/tmp/bluey-test-runs-0123456789ab/run.1794466886.${10000 + i}.16299199040`;
  const identity = { run, boot, invocation };
  return { mode, ...identity, unit: `bluey-jobs-receipt-${run}.service`, root,
    rootDevice: 25, rootInode: 1000 + i, mountId: 32, eventsInode: 123 + i,
    marksNs: { bound: 1, released: 10, terminal: 20, zero: 30, stopped: 40 },
    network: { supervisorNamespace: 100, workloadNamespace: 200 + i, interfaces: ["lo"], defaultRoutes: 0 },
    observer: {
      stdout: asObserverLine({ event: "jobs_browser_closure_bound", ...identity, mountId: 32, eventsInode: 123 + i })
        + asObserverLine({ event: "jobs_browser_closure_observed", ...identity, populated: 0, sameEventsDescriptor: true }),
      stderr: "", code: 0, signal: null, sha256: "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4",
    },
    postStop: { unitStopped: true, actorProcesses: 0, namespaceProcesses: 0 },
    terminal: mode === "case" ? { code: 0, signal: null, deadline: false }
      : { code: null, signal: "SIGKILL", deadline: true },
  };
}

function fixture() {
  const caseScope = scope("case", 1), timeoutScope = scope("timeout-control", 2);
  const database = "bluey_jobs_case_" + "f".repeat(24);
  const name = selected.name, names = [name, "db::tests::unselected"];
  const rows = [
    { event: "nonprod_postgres_ready", address: "127.0.0.1", port: 50000, database: "bluey_jobs_nonprod",
      dataDirectory: `${caseScope.root}/nonprod-postgres/data`, systemIdentifier: "100",
      version: "PostgreSQL 17.11 (fixture) on x86_64-pc-linux-gnu, compiled by GCC, 64-bit", vector: "0.8.7",
      platform: "linux-x64", serverVersionNum: 170011 },
    { event: "nonprod_postgres_test_inventory", names, count: names.length, sha256: sha(JSON.stringify(names)) },
    { event: "nonprod_postgres_case_started", test: name, database },
    "running 1 test", `test ${name} ... ${selected.witnesses[0]}`, ...selected.witnesses.slice(1), "ok",
    "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1.00s",
    { event: "nonprod_postgres_case_finished", test: name, exitCode: 0, signal: null },
    { event: "nonprod_postgres_cases_finished", tests: 1, passed: 1, failed: [] },
    { event: "nonprod_postgres_finished", status: 0, shutdownAttempted: true, serverExited: true },
    "",
  ];
  const wrapper = { success: true, bad: false, terminal: { code: 0, signal: null }, stderr: "",
    stdout: rows.map((row) => typeof row === "string" ? row : JSON.stringify(row)).join("\n") };
  const evidence = { sourceReceiptSha256: "aa2ea7b02b17407e9a80871db8515002880da490b7ef144118d2003be6d34727",
    stdoutSha256: sha(wrapper.stdout), stderrSha256: sha(wrapper.stderr), caseScope, timeoutScope };
  const launch = ({ run, boot, invocation, unit, root, rootDevice, rootInode, mountId, eventsInode }) =>
    ({ run, boot, invocation, unit, root, rootDevice, rootInode, mountId, eventsInode });
  return { wrapper, evidence, trustedLaunch: { case: launch(caseScope), timeout: launch(timeoutScope) } };
}
const raw = (evidence) => JSON.stringify(evidence) + "\n";

test("joins the exact Linux case to independent success and timeout closure records", () => {
  const { wrapper, evidence, trustedLaunch } = fixture();
  assert.deepEqual(verifyBrowserReceiptQualification(wrapper, raw(evidence), trustedLaunch), {
    profile: "interview-prep-browser", caseName: selected.name, wrapperObservedServerExit: true,
    caseObserverSha256: sha(evidence.caseScope.observer.stdout),
    timeoutObserverSha256: sha(evidence.timeoutScope.observer.stdout),
    closureRecordsBound: true, supervisorTranscriptSha256: sha(raw(evidence)), runtimeAdmitted: false,
  });
});

for (const [label, mutate, failure] of [
  ["a passing inner transcript without outer closure", (e) => delete e.caseScope.observer, "browser_scope_shape"],
  ["the bind-only transcript", (e) => {
    e.caseScope.observer.stdout = e.caseScope.observer.stdout.split("\n")[0] + "\n";
  }, "browser_observer_lines"],
  ["observer output from another invocation", (e) => {
    e.caseScope.observer.stdout = e.timeoutScope.observer.stdout;
  }, "browser_observer_identity"],
  ["a nonzero cgroup", (e) => {
    e.caseScope.observer.stdout = e.caseScope.observer.stdout.replace('"populated": 0', '"populated": 1');
  }, "browser_observer_not_empty"],
  ["a duplicate observer field", (e) => {
    e.caseScope.observer.stdout = e.caseScope.observer.stdout.replace('"populated": 0', '"populated": 1, "populated": 0');
  }, "browser_observer_noncanonical"],
  ["an observer terminated after emitting zero", (e) => {
    e.caseScope.observer.code = null; e.caseScope.observer.signal = "SIGKILL";
  }, "browser_observer_terminal"],
  ["release before observer bind", (e) => {
    e.caseScope.marksNs.released = e.caseScope.marksNs.bound;
  }, "browser_supervisor_order"],
  ["an observation outside the bound", (e) => { e.caseScope.marksNs.stopped = 1_800_000_000_002; }, "browser_supervisor_order"],
  ["a remaining actor process", (e) => { e.caseScope.postStop.actorProcesses = 1; }, "browser_post_stop_processes"],
  ["a namespace with another interface", (e) => { e.caseScope.network.interfaces.push("eth0"); }, "browser_network_interfaces"],
  ["a timeout control that returned normally", (e) => {
    e.timeoutScope.terminal = { code: 0, signal: null, deadline: false };
  }, "browser_timeout_terminal"],
  ["a timeout with missing zero evidence", (e) => { e.timeoutScope.observer.stdout = ""; }, "browser_observer_lines"],
  ["a timeout reusing the first run root", (e, launch) => {
    e.timeoutScope.root = launch.timeout.root = e.caseScope.root;
  }, "browser_scope_reuse"],
  ["a genuine closure spliced onto another test root", (e) => {
    const { run, invocation, unit, mountId, eventsInode, observer } = e.timeoutScope;
    Object.assign(e.caseScope, { run, invocation, unit, mountId, eventsInode, observer });
  }, "browser_unit_launch_binding"],
  ["a substituted cgroup descriptor", (e) => {
    e.caseScope.observer.stdout = e.caseScope.observer.stdout.replace('"eventsInode": 124', '"eventsInode": 777');
  }, "browser_observer_launch_descriptor"],
  ["a case attached to stale logs", (e) => { e.stdoutSha256 = sha("older"); }, "browser_wrapper_stream_binding"],
  ["a different overlay", (e) => { e.sourceReceiptSha256 = sha("other"); }, "browser_source_receipt_pin"],
]) {
  test(`refuses ${label}`, () => {
    const { wrapper, evidence, trustedLaunch } = fixture();
    mutate(evidence, trustedLaunch);
    assert.throws(() => verifyBrowserReceiptQualification(wrapper, raw(evidence), trustedLaunch), new RegExp(failure));
  });
}

test("requires a canonical supervisor transcript and separate launch authority", () => {
  const { wrapper, evidence, trustedLaunch } = fixture();
  assert.throws(() => verifyBrowserReceiptQualification(wrapper, raw(evidence)), /browser_launch_authority/);
  assert.throws(() => verifyBrowserReceiptQualification(wrapper, raw(evidence).replace(
    '"actorProcesses":0', '"actorProcesses":1,"actorProcesses":0'), trustedLaunch), /browser_supervisor_noncanonical/);
});
