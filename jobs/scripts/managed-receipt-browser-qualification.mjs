// Final evidence join for the browser-first case. The caller must collect
// these streams outside the disposable unit from the pinned observer and
// trusted supervisor. Shape and cross-binding are not producer authentication.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { proveManagedReceiptPostgres } from "./managed-receipt-postgres-proof.mjs";

const NONCE = /^[a-f0-9]{32}$/;
const BOOT = /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/;
const SHA = /^[a-f0-9]{64}$/;
const OBSERVER_SHA = "a6f6a1763a009b35ea8a039752e859b4cb7eefe319fa79af7385cb2ecd2eb9e4";
const SOURCE_RECEIPT = "aa2ea7b02b17407e9a80871db8515002880da490b7ef144118d2003be6d34727";
const PROFILE = "interview-prep-browser";
const PLATFORM = "linux-x64";
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const natural = (value) => Number.isSafeInteger(value) && value > 0;

function fields(value, names, label) {
  assert(value && typeof value === "object" && !Array.isArray(value), label);
  assert.deepEqual(Object.keys(value).sort(), names.split(" ").sort(), label);
}

function matches(value, regex) {
  return typeof value === "string" && regex.test(value);
}

// Current Python observer emits exactly flat JSON objects with a single LF
// and default separator spaces. Reconstruct it to refuse duplicate keys,
// extra whitespace, or multiple projections of the same result.
function observerJson(value) {
  return `{${Object.entries(value).map(([key, val]) =>
    `${JSON.stringify(key)}: ${JSON.stringify(val)}`).join(", ")}}`;
}

function verifyObserver(observer, scope) {
  fields(observer, "stdout stderr code signal sha256", "browser_observer_record");
  assert.equal(observer.sha256, OBSERVER_SHA, "browser_observer_pin");
  assert(observer.code === 0 && observer.signal === null && observer.stderr === "", "browser_observer_terminal");
  assert(typeof observer.stdout === "string" && Buffer.byteLength(observer.stdout) <= 4096, "browser_observer_output");
  const lines = observer.stdout.split("\n");
  assert(lines.length === 3 && lines[2] === "", "browser_observer_lines");
  const [bound, empty] = lines.slice(0, 2).map((line) => {
    const row = JSON.parse(line);
    assert(row && typeof row === "object" && !Array.isArray(row), "browser_observer_json_shape");
    assert.equal(observerJson(row), line, "browser_observer_noncanonical");
    return row;
  });
  fields(bound, "event run boot invocation mountId eventsInode", "browser_observer_bound");
  fields(empty, "event run boot invocation populated sameEventsDescriptor", "browser_observer_empty");
  assert.equal(bound.event, "jobs_browser_closure_bound", "browser_observer_live_event");
  assert.equal(empty.event, "jobs_browser_closure_observed", "browser_observer_zero_event");
  for (const row of [bound, empty]) {
    for (const key of ["run", "boot", "invocation"]) assert.equal(row[key], scope[key], "browser_observer_identity");
  }
  assert(natural(bound.mountId) && natural(bound.eventsInode), "browser_observer_descriptor");
  assert(bound.mountId === scope.mountId && bound.eventsInode === scope.eventsInode, "browser_observer_launch_descriptor");
  assert(empty.populated === 0 && empty.sameEventsDescriptor === true, "browser_observer_not_empty");
  return hash(observer.stdout);
}

function verifyScope(scope, mode, expected) {
  const binding = "run boot invocation unit root rootDevice rootInode mountId eventsInode";
  fields(expected, binding, "browser_launch_binding_shape");
  fields(scope, `mode ${binding} marksNs network observer postStop terminal`,
    "browser_scope_shape");
  assert.equal(scope.mode, mode, "browser_scope_mode");
  assert(matches(scope.run, NONCE) && matches(scope.boot, BOOT)
    && matches(scope.invocation, NONCE) && scope.invocation !== "0".repeat(32), "browser_scope_identity");
  assert.equal(scope.unit, `bluey-jobs-receipt-${scope.run}.service`, "browser_scope_unit");
  assert(matches(scope.root, /^\/tmp\/bluey-test-runs-[a-f0-9]{12}\/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}$/),
    "browser_scope_root");
  assert(natural(scope.rootDevice) && natural(scope.rootInode), "browser_scope_root_identity");
  for (const key of binding.split(" ")) {
    assert.equal(scope[key], expected[key], "browser_unit_launch_binding");
  }
  fields(scope.marksNs, "bound released terminal zero stopped", "browser_supervisor_marks");
  const sequence = ["bound", "released", "terminal", "zero", "stopped"].map((key) => scope.marksNs[key]);
  assert(sequence.every(natural) && sequence.every((time, i) => i === 0 || sequence[i - 1] < time)
    && sequence.at(-1) - sequence[0] <= 1_800_000_000_000, "browser_supervisor_order");
  fields(scope.network, "supervisorNamespace workloadNamespace interfaces defaultRoutes", "browser_network_shape");
  assert(natural(scope.network.supervisorNamespace) && natural(scope.network.workloadNamespace)
    && scope.network.supervisorNamespace !== scope.network.workloadNamespace, "browser_network_namespace");
  assert.deepEqual(scope.network.interfaces, ["lo"], "browser_network_interfaces");
  assert.equal(scope.network.defaultRoutes, 0, "browser_network_route");
  fields(scope.postStop, "unitStopped actorProcesses namespaceProcesses", "browser_post_stop_shape");
  assert(scope.postStop.unitStopped === true && scope.postStop.actorProcesses === 0
    && scope.postStop.namespaceProcesses === 0, "browser_post_stop_processes");
  fields(scope.terminal, "code signal deadline", "browser_scope_terminal");
  if (mode === "case") {
    assert.deepEqual(scope.terminal, { code: 0, signal: null, deadline: false }, "browser_case_terminal");
  } else {
    assert(scope.terminal.code === null && ["SIGTERM", "SIGKILL"].includes(scope.terminal.signal)
      && scope.terminal.deadline === true, "browser_timeout_terminal");
  }
  return verifyObserver(scope.observer, scope);
}

// Validation consumes a trusted supervisor's already exported records. In
// particular, it never starts/quits a service or confers runtime admission.
export function verifyBrowserReceiptQualification(wrapper, transcript, trustedLaunch) {
  // The launch records come from the trusted controller before releasing
  // the workload; never reconstruct them from the finished transcript.
  fields(trustedLaunch, "case timeout", "browser_launch_authority");
  assert(typeof transcript === "string" && Buffer.byteLength(transcript) <= 64 * 1024,
    "browser_supervisor_transcript");
  const evidence = JSON.parse(transcript);
  assert.equal(JSON.stringify(evidence) + "\n", transcript, "browser_supervisor_noncanonical");
  fields(evidence, "sourceReceiptSha256 stdoutSha256 stderrSha256 caseScope timeoutScope", "browser_evidence_shape");
  assert.equal(evidence.sourceReceiptSha256, SOURCE_RECEIPT, "browser_source_receipt_pin");
  const main = evidence.caseScope, timeout = evidence.timeoutScope;
  const browserProof = proveManagedReceiptPostgres(wrapper, main?.root, PROFILE, PLATFORM);
  assert(matches(evidence.stdoutSha256, SHA) && matches(evidence.stderrSha256, SHA)
    && evidence.stdoutSha256 === hash(wrapper.stdout) && evidence.stderrSha256 === hash(wrapper.stderr),
  "browser_wrapper_stream_binding");
  const completion = verifyScope(main, "case", trustedLaunch.case);
  const failedControl = verifyScope(timeout, "timeout-control", trustedLaunch.timeout);
  assert(main.boot === timeout.boot && main.run !== timeout.run && main.invocation !== timeout.invocation
    && main.root !== timeout.root && main.network.workloadNamespace !== timeout.network.workloadNamespace,
  "browser_scope_reuse");
  assert(main.network.supervisorNamespace === timeout.network.supervisorNamespace,
    "browser_supervisor_namespace_changed");
  return { profile: PROFILE, caseName: browserProof.cases[0].name,
    wrapperObservedServerExit: true, caseObserverSha256: completion, timeoutObserverSha256: failedControl,
    closureRecordsBound: true, supervisorTranscriptSha256: hash(transcript), runtimeAdmitted: false };
}
