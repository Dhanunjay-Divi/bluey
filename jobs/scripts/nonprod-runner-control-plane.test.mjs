import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";
import { apiPanicDiagnostic, assertAutoSubmitPreparedRepeatReport, assertAutoSubmitRepeatCausalRedReport, assertCausalRedReport, assertCheckpointPreparedRecoveryReport, assertCheckpointRecoveryCausalRedReport, assertPreparedLifecycleReport, assertPreparedProtocolNegativeReport, AUTO_REPEAT_PROTECTED_TABLES, closedFailureDiagnostic, DEFERRED_PROTOCOL_NEGATIVES, httpResponseDiagnostic, postgresErrorDiagnostic, PREPARED_IMMUTABLE_TABLES, PREPARED_NEGATIVE_CASES, PROTECTED_CLAIM_TABLES, RECOVERY_IMMUTABLE_TABLES, RECOVERY_STABLE_TABLES, singleFlightCleanup } from "./nonprod-runner-control-plane.mjs";

const valid = () => ({ version: 1, scenario: "causal_red", sourceAccepted: false,
  claim: { operation: "claim", code: "request_failed", status: 404, httpRequests: 1,
    responseSha256: createHash("sha256").update("Execution lease target not found.").digest("hex") },
  claimReadinessConfirmed: true, distinctIds: true, queueReplay: true, protectedClaimStateUnchanged: true,
  protectedClaimFamilies: [...PROTECTED_CLAIM_TABLES], gatewayPosts: 1, gatewayObservationMs: 11_000,
  temporalAccepted: false, externalEffects: 0,
  processes: [101, 102, 103].map((pid) => ({ pid, code: 0, signal: null })), listenersClosed: true,
  objectStore: { put: 1, get: 1, denied: 0, objects: 1 } });

test("causal-red report validates observed denial without calling it source acceptance", () => {
  assert.doesNotThrow(() => assertCausalRedReport(valid(), () => false));
});

for (const [name, change] of [
  ["a setup rejection cannot masquerade as the intended failure", (value) => { value.claim.status = 409; }],
  ["a different target response cannot pass", (value) => { value.claim.responseSha256 = "0".repeat(64); }],
  ["a missing HTTP request cannot pass", (value) => { value.claim.httpRequests = 0; }],
  ["an in-memory rejection cannot pass", (value) => { value.claim.code = "invalid_state"; }],
  ["run and session aliases cannot qualify the ordinary boundary", (value) => { value.distinctIds = false; }],
  ["missing current readiness cannot pass", (value) => { value.claimReadinessConfirmed = false; }],
  ["queue duplication cannot pass", (value) => { value.queueReplay = false; }],
  ["authority mutations cannot pass", (value) => { value.protectedClaimStateUnchanged = false; }],
  ["omitted authority families cannot pass", (value) => { value.protectedClaimFamilies.pop(); }],
  ["a second gateway delivery cannot pass", (value) => { value.gatewayPosts = 2; }],
  ["an insufficient retry observation cannot pass", (value) => { value.gatewayObservationMs = 10_999; }],
  ["synthetic evidence cannot be called Temporal acceptance", (value) => { value.temporalAccepted = true; }],
  ["causal red cannot be called accepted source", (value) => { value.sourceAccepted = true; }],
  ["external effects cannot pass", (value) => { value.externalEffects = 1; }],
  ["missing process exits cannot pass", (value) => { value.processes.pop(); }],
  ["forced process shutdown cannot pass", (value) => { value.processes[0].signal = "SIGKILL"; }],
  ["unclean API exits cannot pass", (value) => { value.processes[0].code = 1; }],
  ["duplicate process evidence cannot pass", (value) => { value.processes[1].pid = value.processes[0].pid; }],
  ["listener residue cannot pass", (value) => { value.listenersClosed = false; }],
  ["missing source readback cannot pass", (value) => { value.objectStore.get = 0; }],
  ["storage denial cannot pass", (value) => { value.objectStore.denied = 1; }],
  ["extra source objects cannot pass", (value) => { value.objectStore.objects = 2; }],
  ["unexpected report data cannot pass", (value) => { value.rawResponse = "unreviewed"; }],
]) {
  test(name, () => {
    const value = valid(); change(value);
    assert.throws(() => assertCausalRedReport(value, () => false));
  });
}

test("a live owned process cannot pass cleanup", () => {
  assert.throws(() => assertCausalRedReport(valid(), () => true));
});

test("process-inspection ambiguity is not proof of cleanup", () => {
  assert.throws(() => assertCausalRedReport(valid(), () => { throw new Error("unknown_pid_state"); }));
});

test("repeated cleanup callers share success and execute once", async () => {
  let calls = 0;
  const stop = singleFlightCleanup(async () => { calls += 1; });
  const first = stop(); assert.equal(stop(), first);
  await first; await stop(); assert.equal(calls, 1);
});

test("cleanup failure remains rejected for every caller instead of reporting success", async () => {
  let calls = 0;
  const error = new Error("synthetic_cleanup_failure");
  const stop = singleFlightCleanup(async () => { calls += 1; throw error; });
  const first = stop(); assert.equal(stop(), first);
  await assert.rejects(first, (value) => value === error);
  await assert.rejects(stop(), (value) => value === error);
  assert.equal(calls, 1);
});

test("closed failure metadata retains type and owned call-site without private error text", () => {
  const privateText = "synthetic private candidate and bearer token must not escape";
  const value = closedFailureDiagnostic({ name: "TypeError", code: "ERR_INVALID_ARG_TYPE", message: privateText,
    stack: privateText + "\n at request (/private/tmp/owned/jobs/scripts/nonprod-runner-control-plane.mjs:123:45)" });
  assert.deepEqual(value, { code: "closed_stage_failure", errorType: "TypeError", systemCode: "ERR_INVALID_ARG_TYPE", causeCode: null, callSiteLine: 123 });
  assert.ok(!JSON.stringify(value).includes(privateText));
});

test("unknown failure types and codes do not escape closed diagnostics", () => {
  assert.deepEqual(closedFailureDiagnostic({ name: "private", code: "private", message: "synthetic_private_token_value" }),
    { code: "closed_stage_failure", errorType: "unknown", systemCode: null, causeCode: null, callSiteLine: null });
});

test("transport diagnostic emits only allowlisted cause code, never its payload", () => {
  const value = closedFailureDiagnostic({ name: "TypeError", message: "synthetic private text", cause: { code: "UND_ERR_SOCKET", message: "synthetic private cause" } });
  assert.equal(value.causeCode, "UND_ERR_SOCKET");
  assert.equal(value.code, "closed_stage_failure");
  assert.equal(closedFailureDiagnostic({ cause: { code: "synthetic_private_code" } }).causeCode, null);
  assert.ok(!JSON.stringify(value).includes("synthetic private"));
});

test("API panic fingerprint strips directories and raw private panic text", () => {
  const tail = "thread synthetic panicked at /private/owned/secret/row.rs:42:9:\nerror retrieving column 0: synthetic private candidate\n";
  assert.deepEqual(apiPanicDiagnostic(tail), { detected: true, sourceFile: "row.rs", sourceLine: 42,
    category: "postgres_row_type", tailSha256: createHash("sha256").update(tail).digest("hex") });
  assert.ok(!JSON.stringify(apiPanicDiagnostic(tail)).includes("synthetic private candidate"));
  assert.equal(apiPanicDiagnostic("synthetic nonpanic").detected, false);
});

test("private PostgreSQL diagnostic emits only a closed category and log hash", () => {
  const tail = '2026 fixture ERROR: column reference "synthetic_private_column" is ambiguous\n'
    + 'STATEMENT: SELECT synthetic_private_candidate FROM synthetic_private_table\n';
  assert.deepEqual(postgresErrorDiagnostic(tail), { detected: true, sqlState: null, category: "ambiguous_column",
    tailSha256: createHash("sha256").update(tail).digest("hex") });
  assert.ok(!JSON.stringify(postgresErrorDiagnostic(tail)).includes("synthetic_private"));
});

test("private PostgreSQL diagnostic retains an explicitly logged SQLSTATE without guessing", () => {
  const tail = 'ERROR: 42703: column "synthetic_private_column" does not exist\n';
  assert.equal(postgresErrorDiagnostic(tail).sqlState, "42703");
  assert.equal(postgresErrorDiagnostic(tail).category, "missing_column");
  assert.equal(postgresErrorDiagnostic('ERROR: unclassified synthetic private text\n').category, "other");
});

test("PostgreSQL startup output is not evidence of a database error", () => {
  const value = postgresErrorDiagnostic("LOG: synthetic private startup\n");
  assert.equal(value.detected, false); assert.equal(value.category, null); assert.equal(value.sqlState, null);
});

test("HTTP failure diagnostic classifies only exact public constants and hashes private bodies", () => {
  assert.equal(httpResponseDiagnostic(400, "Invalid execution lease request.").category, "invalid_execution_request");
  assert.equal(httpResponseDiagnostic(400, "Invalid runner-volume request.").category, "invalid_volume_request");
  const privateBody = "Invalid execution lease request. synthetic private candidate";
  const diagnostic = httpResponseDiagnostic(400, privateBody);
  assert.deepEqual(diagnostic, { status: 400, category: "unclassified",
    responseSha256: createHash("sha256").update(privateBody).digest("hex") });
  assert.ok(!JSON.stringify(diagnostic).includes("synthetic private"));
  assert.equal(httpResponseDiagnostic(999, "Unauthorized").status, null);
});

const prepared = () => {
  const { protectedClaimStateUnchanged: _, ...common } = valid();
  return { ...common, scenario: "prepared_lifecycle",
    claim: { httpRequests: 1, status: 200, phase: "prepared", fence: 1, submissionMode: "review_first",
      leaseCount: 1, volumeBindingCount: 1, exactVolumeBindingCount: 1, atsBindingCount: 0, atsApplicationBindingCount: 0,
      atsRunBindingCount: 0, runAliasCount: 0 },
    heartbeat: { httpRequests: 1, observedUpdate: true, sameTokenAndFence: true, phase: "prepared" },
    release: { httpRequests: 1, status: 204, phase: "released", noIrreversible: true },
    immutableFamilies: [...PREPARED_IMMUTABLE_TABLES], immutableStateUnchanged: true, attemptBindingUnchanged: true,
    effectState: { submitted: 0, evidence: 0, irreversible: 0 },
    terminalReplay: { httpRequests: 1, status: 204, freshNonce: true, unchanged: true } };
};

test("prepared protocol report requires a real lifecycle without calling it source acceptance", () => {
  assert.doesNotThrow(() => assertPreparedLifecycleReport(prepared(), () => false));
  assert.throws(() => assertCausalRedReport(prepared(), () => false));
  assert.throws(() => assertPreparedLifecycleReport(valid(), () => false));
});

for (const [name, change] of [
  ["missing actual claim", (value) => { value.claim.httpRequests = 0; }],
  ["denied claim", (value) => { value.claim.status = 409; }],
  ["unproven auto-submit mode", (value) => { value.claim.submissionMode = "auto_submit"; }],
  ["missing lease binding", (value) => { value.claim.leaseCount = 0; }],
  ["duplicate volume binding", (value) => { value.claim.volumeBindingCount = 2; }],
  ["mismatched sole volume binding", (value) => { value.claim.exactVolumeBindingCount = 0; }],
  ["unexpected account ATS binding", (value) => { value.claim.atsBindingCount = 1; }],
  ["unexpected application ATS binding", (value) => { value.claim.atsApplicationBindingCount = 1; }],
  ["unexpected run ATS binding", (value) => { value.claim.atsRunBindingCount = 1; }],
  ["browser session run alias", (value) => { value.claim.runAliasCount = 1; }],
  ["invalid lease fence", (value) => { value.claim.fence = 0; }],
  ["no heartbeat request", (value) => { value.heartbeat.httpRequests = 0; }],
  ["no observed heartbeat write", (value) => { value.heartbeat.observedUpdate = false; }],
  ["heartbeat token drift", (value) => { value.heartbeat.sameTokenAndFence = false; }],
  ["irreversible heartbeat phase", (value) => { value.heartbeat.phase = "click_started"; }],
  ["cached finish instead of HTTP release", (value) => { value.release.httpRequests = 0; }],
  ["failed release", (value) => { value.release.status = 409; }],
  ["irreversible local attempt", (value) => { value.release.noIrreversible = false; }],
  ["cached promise instead of terminal HTTP replay", (value) => { value.terminalReplay.httpRequests = 0; }],
  ["reused transport nonce", (value) => { value.terminalReplay.freshNonce = false; }],
  ["terminal replay mutation", (value) => { value.terminalReplay.unchanged = false; }],
  ["omitted immutable state families", (value) => { value.immutableFamilies.pop(); }],
  ["billing or workflow state mutation", (value) => { value.immutableStateUnchanged = false; }],
  ["reservation binding mutation", (value) => { value.attemptBindingUnchanged = false; }],
  ["manufactured submission evidence", (value) => { value.effectState.evidence = 1; }],
  ["constant effect count contrary to readback", (value) => { value.externalEffects = null; }],
  ["unqualified source acceptance", (value) => { value.sourceAccepted = true; }],
  ["private fields in report", (value) => { value.leaseToken = "synthetic-private-token"; }],
]) test(`prepared report rejects ${name}`, () => {
  const value = prepared(); change(value);
  assert.throws(() => assertPreparedLifecycleReport(value, () => false));
});

const autoRed = () => {
  const value = prepared();
  value.scenario = "auto_submit_repeat_causal_red";
  value.claim.submissionMode = "auto_submit";
  value.claim.atsBindingCount = 1;
  value.claim.atsApplicationBindingCount = 1;
  value.claim.atsRunBindingCount = 1;
  value.protectedClaimFamilies = [...AUTO_REPEAT_PROTECTED_TABLES];
  value.processes.push({ pid: 104, code: 0, signal: null });
  value.autoAuthorization = { active: true, revision: 1, identityAndSourceBound: true,
    admissionBound: true, noManualApproval: true };
  value.ats = { phase: "preflight", fence: 0, bindingSha256: "a".repeat(64), nonceBound: true,
    expiresFuture: true, exactFrozenAuthority: true, exactPreparedLease: true,
    noPhaseBOrInvalidation: true, unchangedAfterRepeat: true };
  value.repeat = { operation: "claim", code: "lease_unavailable", status: 409, httpRequests: 1,
    responseSha256: createHash("sha256").update("Execution lease is not available.").digest("hex"),
    freshVolumeProof: true, freshTransportNonce: true, sameClaimFields: true, readinessReplayed: true,
    protectedStateUnchanged: true, retainedHeartbeatStatus: 200, retainedHeartbeatExact: true };
  value.autoEffectState = { capacity: 0, canaryReservations: 0, irreversibleReceipts: 0 };
  return value;
};

test("auto-submit repeat report records a real contract red, not a green lifecycle or review-first proof", () => {
  assert.doesNotThrow(() => assertAutoSubmitRepeatCausalRedReport(autoRed(), () => false));
  assert.throws(() => assertPreparedLifecycleReport(autoRed(), () => false));
  assert.throws(() => assertAutoSubmitRepeatCausalRedReport(prepared(), () => false));
  assert.equal(AUTO_REPEAT_PROTECTED_TABLES.length, 20);
  assert.equal(new Set(AUTO_REPEAT_PROTECTED_TABLES).size, 20);
});

for (const [name, change] of [
  ["a green-looking scenario", (value) => { value.scenario = "prepared_lifecycle"; }],
  ["a denied first claim", (value) => { value.claim.status = 409; }],
  ["review-first instead of auto-submit", (value) => { value.claim.submissionMode = "review_first"; }],
  ["no observed Phase A binding", (value) => { value.claim.atsBindingCount = 0; }],
  ["inactive Track consent", (value) => { value.autoAuthorization.active = false; }],
  ["missing consent revision", (value) => { value.autoAuthorization.revision = 0; }],
  ["unbound identity/source", (value) => { value.autoAuthorization.identityAndSourceBound = false; }],
  ["unbound frozen admission", (value) => { value.autoAuthorization.admissionBound = false; }],
  ["manual approval substituted for consent", (value) => { value.autoAuthorization.noManualApproval = false; }],
  ["consumed Phase A", (value) => { value.ats.phase = "consumed"; }],
  ["advanced Phase A fence", (value) => { value.ats.fence = 1; }],
  ["invalid frozen digest", (value) => { value.ats.bindingSha256 = "private"; }],
  ["unbound ATS nonce", (value) => { value.ats.nonceBound = false; }],
  ["expired ATS authority", (value) => { value.ats.expiresFuture = false; }],
  ["unchecked frozen authority", (value) => { value.ats.exactFrozenAuthority = false; }],
  ["unchecked prepared lease", (value) => { value.ats.exactPreparedLease = false; }],
  ["Phase B/invalidation state", (value) => { value.ats.noPhaseBOrInvalidation = false; }],
  ["changed ATS state", (value) => { value.ats.unchangedAfterRepeat = false; }],
  ["successful repeat instead of causal red", (value) => { value.repeat.status = 200; }],
  ["wrong client error classification", (value) => { value.repeat.code = "request_failed"; }],
  ["no actual repeat request", (value) => { value.repeat.httpRequests = 0; }],
  ["unrelated public response", (value) => { value.repeat.responseSha256 = "0".repeat(64); }],
  ["reused volume proof", (value) => { value.repeat.freshVolumeProof = false; }],
  ["reused transport nonce", (value) => { value.repeat.freshTransportNonce = false; }],
  ["changed claim fields", (value) => { value.repeat.sameClaimFields = false; }],
  ["missing repeat readiness", (value) => { value.repeat.readinessReplayed = false; }],
  ["repeat state mutation", (value) => { value.repeat.protectedStateUnchanged = false; }],
  ["lost retained heartbeat", (value) => { value.repeat.retainedHeartbeatStatus = 409; }],
  ["retained token/fence drift", (value) => { value.repeat.retainedHeartbeatExact = false; }],
  ["omitted runtime/subject/proof/effect families", (value) => { value.protectedClaimFamilies = [...PROTECTED_CLAIM_TABLES]; }],
  ["unclosed inspector process", (value) => { value.processes.pop(); }],
  ["capacity reservation", (value) => { value.autoEffectState.capacity = 1; }],
  ["canary reservation", (value) => { value.autoEffectState.canaryReservations = 1; }],
  ["irreversible receipt", (value) => { value.autoEffectState.irreversibleReceipts = 1; }],
  ["unproven source acceptance", (value) => { value.sourceAccepted = true; }],
  ["private ATS field", (value) => { value.ats.frozen = "synthetic-private"; }],
]) test(`auto-submit causal-red report rejects ${name}`, () => {
  const value = autoRed(); change(value);
  assert.throws(() => assertAutoSubmitRepeatCausalRedReport(value, () => false));
});

const autoGreen = () => {
  const value = autoRed();
  value.scenario = "auto_submit_prepared_repeat";
  value.repeat = { operation: "claim", status: 200, httpRequests: 1,
    freshVolumeProof: true, freshTransportNonce: true, sameClaimFields: true, readinessReplayed: true,
    tokenRotated: true, previousFence: value.claim.fence, fence: value.claim.fence + 1,
    atsUnchanged: true, immutableStateUnchanged: true, attemptBindingUnchanged: true,
    exactLeaseVolumeProcessBinding: true, stableAccountSubjects: true,
    staleHeartbeatStatus: 409, staleFinishStatus: 409, staleStateUnchanged: true,
    currentHeartbeatStatus: 200, currentHeartbeatExact: true };
  return value;
};

test("auto-submit green repeat is distinct from causal red and review-first, without claiming source acceptance", () => {
  assert.doesNotThrow(() => assertAutoSubmitPreparedRepeatReport(autoGreen(), () => false));
  assert.throws(() => assertAutoSubmitRepeatCausalRedReport(autoGreen(), () => false));
  assert.throws(() => assertPreparedLifecycleReport(autoGreen(), () => false));
  assert.throws(() => assertAutoSubmitPreparedRepeatReport(autoRed(), () => false));
});

for (const field of ["freshVolumeProof", "freshTransportNonce", "sameClaimFields", "readinessReplayed", "tokenRotated",
  "atsUnchanged", "immutableStateUnchanged", "attemptBindingUnchanged", "exactLeaseVolumeProcessBinding",
  "stableAccountSubjects", "staleStateUnchanged", "currentHeartbeatExact"]) {
  test(`auto-submit green repeat rejects missing ${field}`, () => {
    const value = autoGreen(); value.repeat[field] = false;
    assert.throws(() => assertAutoSubmitPreparedRepeatReport(value, () => false));
  });
}
for (const [name, change] of [
  ["red scenario", (v) => { v.scenario = "auto_submit_repeat_causal_red"; }],
  ["denied retry", (v) => { v.repeat.status = 409; }],
  ["no real HTTP retry", (v) => { v.repeat.httpRequests = 0; }],
  ["wrong previous fence", (v) => { v.repeat.previousFence += 1; }],
  ["same fence", (v) => { v.repeat.fence = v.repeat.previousFence; }],
  ["unsafe fence", (v) => { v.repeat.fence = Number.MAX_SAFE_INTEGER + 1; }],
  ["stale heartbeat accepted", (v) => { v.repeat.staleHeartbeatStatus = 200; }],
  ["stale finish accepted", (v) => { v.repeat.staleFinishStatus = 204; }],
  ["current heartbeat denied", (v) => { v.repeat.currentHeartbeatStatus = 409; }],
  ["missing consent", (v) => { v.autoAuthorization.active = false; }],
  ["review-first approval", (v) => { v.autoAuthorization.noManualApproval = false; }],
  ["changed frozen ATS", (v) => { v.ats.unchangedAfterRepeat = false; }],
  ["expired ATS", (v) => { v.ats.expiresFuture = false; }],
  ["first claim denied", (v) => { v.claim.status = 409; }],
  ["submission capacity", (v) => { v.autoEffectState.capacity = 1; }],
  ["unclosed process", (v) => { v.processes.pop(); }],
  ["unproven source acceptance", (v) => { v.sourceAccepted = true; }],
  ["private token field", (v) => { v.repeat.leaseToken = "private"; }],
]) test(`auto-submit green repeat rejects ${name}`, () => {
  const value = autoGreen(); change(value);
  assert.throws(() => assertAutoSubmitPreparedRepeatReport(value, () => false));
});

const recoveryRed = () => {
  const value = prepared();
  value.scenario = "checkpoint_session_causal_red";
  value.protectedClaimFamilies = [...AUTO_REPEAT_PROTECTED_TABLES];
  value.recovery = { operation: "recovery", code: "request_failed", status: 404, httpRequests: 1,
    responseSha256: createHash("sha256").update("Execution lease target not found.").digest("hex"),
    checkpointVersion: 2, checkpointPhase: "prepared", exactLeaseCredentials: true, freshTransportNonce: true,
    preHeartbeatConfirmed: true, protectedStateUnchanged: true, retainedHeartbeatStatus: 200,
    retainedHeartbeatExact: true, effectAuthorityZero: true };
  return value;
};

test("checkpoint recovery report distinguishes a real missing-session red from a green lifecycle", () => {
  assert.doesNotThrow(() => assertCheckpointRecoveryCausalRedReport(recoveryRed(), () => false));
  assert.throws(() => assertPreparedLifecycleReport(recoveryRed(), () => false));
  assert.throws(() => assertCheckpointRecoveryCausalRedReport(prepared(), () => false));
  assert.throws(() => assertCheckpointRecoveryCausalRedReport(autoRed(), () => false));
});
for (const field of ["exactLeaseCredentials", "freshTransportNonce", "preHeartbeatConfirmed", "protectedStateUnchanged",
  "retainedHeartbeatExact", "effectAuthorityZero"]) test(`checkpoint causal-red report rejects missing ${field}`, () => {
  const value = recoveryRed(); value.recovery[field] = false;
  assert.throws(() => assertCheckpointRecoveryCausalRedReport(value, () => false));
});
for (const [name, change] of [
  ["green scenario", (v) => { v.scenario = "prepared_lifecycle"; }],
  ["unproven claim", (v) => { v.claim.status = 404; }],
  ["unproven heartbeat", (v) => { v.heartbeat.observedUpdate = false; }],
  ["wrong operation", (v) => { v.recovery.operation = "claim"; }],
  ["wrong error code", (v) => { v.recovery.code = "lease_unavailable"; }],
  ["unrelated error", (v) => { v.recovery.status = 500; }],
  ["unexpected success", (v) => { v.recovery.status = 200; }],
  ["no real HTTP request", (v) => { v.recovery.httpRequests = 0; }],
  ["wrong public error", (v) => { v.recovery.responseSha256 = "0".repeat(64); }],
  ["tokenless v1 checkpoint", (v) => { v.recovery.checkpointVersion = 1; }],
  ["irreversible checkpoint", (v) => { v.recovery.checkpointPhase = "final_submit_started"; }],
  ["unusable retained lease", (v) => { v.recovery.retainedHeartbeatStatus = 409; }],
  ["missing protected families", (v) => { v.protectedClaimFamilies = [...PROTECTED_CLAIM_TABLES]; }],
  ["unclean process", (v) => { v.processes[0].code = 1; }],
  ["source acceptance", (v) => { v.sourceAccepted = true; }],
  ["private token", (v) => { v.recovery.leaseToken = "private"; }],
]) test(`checkpoint causal-red report rejects ${name}`, () => {
  const value = recoveryRed(); change(value);
  assert.throws(() => assertCheckpointRecoveryCausalRedReport(value, () => false));
});

const checkpointGreen = (auto = false) => {
  const value = auto ? autoGreen() : prepared();
  delete value.repeat;
  value.scenario = "checkpoint_prepared_recovery";
  value.protectedClaimFamilies = [...AUTO_REPEAT_PROTECTED_TABLES];
  value.immutableFamilies = [...RECOVERY_IMMUTABLE_TABLES];
  value.processes = Array.from({ length: auto ? 7 : 6 }, (_, index) => ({ pid: 101 + index, code: 0, signal: null }));
  value.checkpointRecovery = { operation: "recovery", status: 200, httpRequests: 1, checkpointVersion: 2,
    checkpointPhase: "prepared", freshTransportNonce: true, exactLeaseCredentials: true, observedReleased: true,
    canonicalStateExact: true, applicationStableSha256: "a".repeat(64), sessionStableSha256: "b".repeat(64), recoverySha256: "c".repeat(64),
    leaseAuthorityUnchanged: true, stableFamilies: [...RECOVERY_STABLE_TABLES], stableStateUnchanged: true,
    replayHttpRequests: 1, replayStatus: 200, replayFreshNonce: true, replayUnchanged: true, terminalFinishUnchanged: true, effectAuthorityZero: true };
  return value;
};

test("checkpoint green report requires exact lifecycle and unchanged authority in both modes", () => {
  for (const auto of [false, true]) assert.doesNotThrow(() => assertCheckpointPreparedRecoveryReport(checkpointGreen(auto), auto ? "auto_submit" : "review_first", () => false));
  assert.throws(() => assertPreparedLifecycleReport(checkpointGreen(), () => false));
  assert.throws(() => assertCheckpointRecoveryCausalRedReport(checkpointGreen(), () => false));
  assert.equal(RECOVERY_IMMUTABLE_TABLES.length, 7);
  assert.equal(RECOVERY_STABLE_TABLES.length, 16);
});
for (const field of ["freshTransportNonce", "exactLeaseCredentials", "observedReleased", "canonicalStateExact", "leaseAuthorityUnchanged",
  "stableStateUnchanged", "replayFreshNonce", "replayUnchanged", "terminalFinishUnchanged", "effectAuthorityZero"]) {
  test(`checkpoint green report rejects missing ${field}`, () => {
    const value = checkpointGreen(); value.checkpointRecovery[field] = false;
    assert.throws(() => assertCheckpointPreparedRecoveryReport(value, "review_first", () => false));
  });
}
for (const [name, change] of [
  ["causal red", (v) => { v.scenario = "checkpoint_session_causal_red"; }],
  ["rejected recovery", (v) => { v.checkpointRecovery.status = 404; }],
  ["no real recovery request", (v) => { v.checkpointRecovery.httpRequests = 0; }],
  ["version1 recovery", (v) => { v.checkpointRecovery.checkpointVersion = 1; }],
  ["unsafe checkpoint", (v) => { v.checkpointRecovery.checkpointPhase = "final_submit_started"; }],
  ["unvalidated application", (v) => { v.checkpointRecovery.applicationStableSha256 = "invalid"; }],
  ["unvalidated session", (v) => { v.checkpointRecovery.sessionStableSha256 = null; }],
  ["unvalidated recovery marker", (v) => { v.checkpointRecovery.recoverySha256 = ""; }],
  ["omitted stable table", (v) => { v.checkpointRecovery.stableFamilies.pop(); }],
  ["missing exact HTTP replay", (v) => { v.checkpointRecovery.replayHttpRequests = 0; }],
  ["rejected replay", (v) => { v.checkpointRecovery.replayStatus = 409; }],
  ["missing process exit", (v) => { v.processes.pop(); }],
  ["omitted immutable table", (v) => { v.immutableFamilies.pop(); }],
  ["unproven packet invariance", (v) => { v.immutableStateUnchanged = false; }],
  ["attempt binding drift", (v) => { v.attemptBindingUnchanged = false; }],
  ["unproven source acceptance", (v) => { v.sourceAccepted = true; }],
  ["private candidate payload", (v) => { v.checkpointRecovery.application = { private: true }; }],
]) test(`checkpoint green report rejects ${name}`, () => {
  const value = checkpointGreen(); change(value);
  assert.throws(() => assertCheckpointPreparedRecoveryReport(value, "review_first", () => false));
});
test("auto-submit checkpoint recovery requires original consent and frozen ATS proof", () => {
  for (const change of [(v) => { v.autoAuthorization.noManualApproval = false; },
    (v) => { v.ats.unchangedAfterRepeat = false; }, (v) => { v.autoEffectState.capacity = 1; },
    (v) => { v.repeat = {}; }]) {
    const value = checkpointGreen(true); change(value);
    assert.throws(() => assertCheckpointPreparedRecoveryReport(value, "auto_submit", () => false));
  }
});

test("checkpoint qualification binds mode to its invocation, not the untrusted report", () => {
  assert.throws(() => assertCheckpointPreparedRecoveryReport(checkpointGreen(), "auto_submit", () => false));
  assert.throws(() => assertCheckpointPreparedRecoveryReport(checkpointGreen(true), "review_first", () => false));
  assert.throws(() => assertCheckpointPreparedRecoveryReport(checkpointGreen(), undefined, () => false));
  assert.throws(() => assertCheckpointPreparedRecoveryReport(checkpointGreen(true), "automatic", () => false));
});

test("historical PostgreSQL boolean marker is not compared to SQLite's integer literal", () => {
  const source = readFileSync(new URL("../../server/src/db/jobs/execution_session_authority.rs", import.meta.url), "utf8");
  const pg = source.split("fn postgres_historical_session_command(")[1].split("fn sqlite_checkpoint_browser_session_id(")[0];
  const sqlite = source.split("fn sqlite_historical_session_command(")[1].split("fn postgres_historical_session_command(")[0];
  const schema = readFileSync(new URL("../../infra/postgres/server-runtime/033_jobs_managed_cloud_release_authority.sql", import.meta.url), "utf8");
  assert.match(schema, /ADD COLUMN IF NOT EXISTS managed_cloud_authority_required BOOLEAN/);
  assert.match(pg, /managed_cloud_authority_required IS TRUE/);
  assert.doesNotMatch(pg, /managed_cloud_authority_required\s*=\s*1/);
  assert.match(sqlite, /managed_cloud_authority_required=1/);
});

// Independent contract fixture: a change to the controller's matrix must be deliberately
// reviewed here, not silently inherited through PREPARED_NEGATIVE_CASES in the valid report.
const negativeSpecs = [
  ["bearer_only", "claim", "http", 401, "Unauthorized", "absent", "not_checked", null],
  ["signed_body_changed", "claim", "http", 401, "Unauthorized", "fresh", "not_checked", null],
  ["owner_mismatch", "claim", "http", 400, "Execution lease owner does not match the authenticated worker.", "fresh", "not_checked", null],
  ["worker_nonce_replay", "claim", "http", 401, "Unauthorized", "replay", "not_checked", null],
  ["proof_volume_mismatch", "claim", "http", 400, "Runner-volume lease proof does not match the claim.", "fresh", "not_checked", null],
  ["proof_process_mismatch", "claim", "http", 400, "Runner-volume lease proof does not match the claim.", "fresh", "not_checked", null],
  ["proof_invalid_version", "claim", "http", 400, "Invalid runner-volume request.", "fresh", "not_checked", null],
  ["proof_bad_signature", "claim", "client", 401, "Runner-volume authority is invalid.", "fresh", "fresh", "request_failed"],
  ["proof_bad_payload", "claim", "client", 401, "Runner-volume authority is invalid.", "fresh", "fresh", "request_failed"],
  ["proof_consumed_replay", "claim", "http", 409, "Execution lease is not available.", "fresh", "consumed", null],
  ["wrong_application", "claim", "client", 404, "Execution lease target not found.", "fresh", "fresh", "request_failed"],
  ["wrong_run", "claim", "client", 409, "Execution lease is not available.", "fresh", "fresh", "lease_unavailable"],
  ["wrong_profile", "claim", "client", 409, "Execution lease is not available.", "fresh", "fresh", "lease_unavailable"],
  ["runtime_grant", "claim", "client", 409, "Execution lease is not available.", "fresh", "fresh", "lease_unavailable"],
  ["runtime_hash", "claim", "client", 409, "Execution lease is not available.", "fresh", "fresh", "lease_unavailable"],
  ["runtime_instance", "claim", "client", 409, "Execution lease is not available.", "fresh", "fresh", "lease_unavailable"],
  ["wrong_worker", "claim", "client", 401, "Runner-volume authority is invalid.", "fresh", "fresh", "request_failed"],
  ["heartbeat_token", "heartbeat", "http", 409, "Execution lease is not available.", "fresh", "not_checked", null],
  ["heartbeat_fence", "heartbeat", "http", 409, "Execution lease is not available.", "fresh", "not_checked", null],
  ["finish_token", "finish", "http", 409, "Execution lease is not available.", "fresh", "not_checked", null],
  ["finish_fence", "finish", "http", 409, "Execution lease is not available.", "fresh", "not_checked", null],
  ["recovery_token", "recovery", "client", 409, "Execution lease is not available.", "fresh", "not_checked", "request_failed"],
  ["recovery_fence", "recovery", "client", 409, "Execution lease is not available.", "fresh", "not_checked", "request_failed"],
  ["recovery_owner", "recovery", "client", 409, "Execution lease is not available.", "fresh", "not_checked", "request_failed"],
].map(([id, operation, via, status, body, noncePolicy, proofPolicy, clientCode]) => ({
  id, operation, via, status, responseSha256: createHash("sha256").update(body).digest("hex"),
  noncePolicy, proofPolicy, clientCode,
}));
const deferredNegatives = ["cross_account_admission", "readiness_rotation", "policy_head_rotation"];
const protocolNegatives = (mode) => {
  const value = mode === "auto_submit" ? autoGreen() : prepared();
  delete value.repeat;
  value.scenario = "prepared_protocol_negatives";
  value.protectedClaimFamilies = [...AUTO_REPEAT_PROTECTED_TABLES];
  value.negativeMatrix = { cases: negativeSpecs.map((spec) => ({
    id: spec.id, operation: spec.operation, via: spec.via, status: spec.status,
    responseSha256: spec.responseSha256, clientCode: spec.clientCode, httpRequests: 1,
    nonceFresh: spec.noncePolicy === "fresh" ? true : spec.noncePolicy === "replay" ? false : null,
    proofFresh: spec.proofPolicy === "fresh" ? true : spec.proofPolicy === "consumed" ? false : null,
    inputsBound: true, stateUnchanged: true,
  })), deferred: [...deferredNegatives], retainedHeartbeatStatus: 200,
  retainedHeartbeatExact: true, effectAuthorityZero: true };
  return value;
};

test("prepared-negative contract pins all24 ordered cases and explicitly deferred scope", () => {
  assert.equal(negativeSpecs.length, 24);
  assert.equal(new Set(negativeSpecs.map((spec) => spec.id)).size, 24);
  assert.deepEqual(PREPARED_NEGATIVE_CASES, negativeSpecs);
  assert.deepEqual(DEFERRED_PROTOCOL_NEGATIVES, deferredNegatives);
  assert.equal(Object.isFrozen(PREPARED_NEGATIVE_CASES), true);
  assert.equal(PREPARED_NEGATIVE_CASES.every(Object.isFrozen), true);
  assert.equal(Object.isFrozen(DEFERRED_PROTOCOL_NEGATIVES), true);
});

test("consumed proof requires the transaction-time409 contract, not a cryptographic401 or any4xx", () => {
  for (const mode of ["review_first", "auto_submit"]) {
    const value = protocolNegatives(mode);
    const entry = value.negativeMatrix.cases.find((item) => item.id === "proof_consumed_replay");
    entry.status = 401;
    entry.responseSha256 = createHash("sha256").update("Runner-volume authority is invalid.").digest("hex");
    assert.throws(() => assertPreparedProtocolNegativeReport(value, mode, () => false));
  }
});

test("managed run mismatch requires the frozen-command409, not missing-target404", () => {
  for (const mode of ["review_first", "auto_submit"]) {
    const value = protocolNegatives(mode);
    const entry = value.negativeMatrix.cases.find((item) => item.id === "wrong_run");
    entry.status = 404;
    entry.clientCode = "request_failed";
    entry.responseSha256 = createHash("sha256").update("Execution lease target not found.").digest("hex");
    assert.throws(() => assertPreparedProtocolNegativeReport(value, mode, () => false));
  }
});

for (const mode of ["review_first", "auto_submit"]) {
  test(`prepared negatives accept only complete synthetic ${mode} evidence without a runtime claim`, () => {
    assert.doesNotThrow(() => assertPreparedProtocolNegativeReport(protocolNegatives(mode), mode, () => false));
    assert.throws(() => assertPreparedLifecycleReport(protocolNegatives(mode), () => false));
    assert.throws(() => assertCheckpointPreparedRecoveryReport(protocolNegatives(mode), mode, () => false));
  });
  for (const [index, spec] of negativeSpecs.entries()) {
    test(`${mode} negative ${spec.id} rejects every missing altered or private case field`, () => {
      const correct = protocolNegatives(mode).negativeMatrix.cases[index];
      const mutations = {
        id: "unexpected_case", operation: spec.operation === "claim" ? "finish" : "claim",
        via: spec.via === "http" ? "client" : "http", status: 200, responseSha256: "0".repeat(64),
        clientCode: spec.clientCode === null ? "request_failed" : null, httpRequests: 0,
        nonceFresh: correct.nonceFresh === null ? false : !correct.nonceFresh,
        proofFresh: correct.proofFresh === null ? false : !correct.proofFresh,
        inputsBound: false, stateUnchanged: false,
      };
      for (const [field, invalid] of Object.entries(mutations)) {
        const changed = protocolNegatives(mode); changed.negativeMatrix.cases[index][field] = invalid;
        assert.throws(() => assertPreparedProtocolNegativeReport(changed, mode, () => false), field);
        const missing = protocolNegatives(mode); delete missing.negativeMatrix.cases[index][field];
        assert.throws(() => assertPreparedProtocolNegativeReport(missing, mode, () => false), `missing ${field}`);
      }
      const privateReport = protocolNegatives(mode);
      privateReport.negativeMatrix.cases[index].leaseToken = "synthetic-private-token";
      assert.throws(() => assertPreparedProtocolNegativeReport(privateReport, mode, () => false));
    });
  }
  for (const [name, mutate] of [
    ["omitted matrix", (v) => { delete v.negativeMatrix; }],
    ["omitted case", (v) => { v.negativeMatrix.cases.pop(); }],
    ["duplicate case", (v) => { v.negativeMatrix.cases[1] = v.negativeMatrix.cases[0]; }],
    ["reordered cases", (v) => { v.negativeMatrix.cases.reverse(); }],
    ["extra case", (v) => { v.negativeMatrix.cases.push(v.negativeMatrix.cases[0]); }],
    ["nonarray cases", (v) => { v.negativeMatrix.cases = {}; }],
    ["missing deferred gate", (v) => { v.negativeMatrix.deferred.pop(); }],
    ["reordered deferred gates", (v) => { v.negativeMatrix.deferred.reverse(); }],
    ["pretended deferred coverage", (v) => { v.negativeMatrix.deferred = []; }],
    ["retained heartbeat denied", (v) => { v.negativeMatrix.retainedHeartbeatStatus = 409; }],
    ["retained authority changed", (v) => { v.negativeMatrix.retainedHeartbeatExact = false; }],
    ["effect authority introduced", (v) => { v.negativeMatrix.effectAuthorityZero = false; }],
    ["unproven actual claim", (v) => { v.claim.status = 401; }],
    ["session alias", (v) => { v.claim.runAliasCount = 1; }],
    ["omitted protected table", (v) => { v.protectedClaimFamilies.pop(); }],
    ["wrong scenario", (v) => { v.scenario = "prepared_lifecycle"; }],
    ["source acceptance", (v) => { v.sourceAccepted = true; }],
    ["missing heartbeat write", (v) => { v.heartbeat.observedUpdate = false; }],
    ["release denied", (v) => { v.release.status = 409; }],
    ["terminal replay changed state", (v) => { v.terminalReplay.unchanged = false; }],
    ["private matrix payload", (v) => { v.negativeMatrix.rawBody = "synthetic-private-body"; }],
    ["private report payload", (v) => { v.workerKey = "synthetic-private-key"; }],
    ["missing process exit", (v) => { v.processes.pop(); }],
    ["unclosed listeners", (v) => { v.listenersClosed = false; }],
  ]) test(`${mode} prepared-negative report rejects ${name}`, () => {
    const value = protocolNegatives(mode); mutate(value);
    assert.throws(() => assertPreparedProtocolNegativeReport(value, mode, () => false));
  });
  test(`${mode} prepared-negative matrix rejects every omitted summary field`, () => {
    for (const field of Object.keys(protocolNegatives(mode).negativeMatrix)) {
      const value = protocolNegatives(mode); delete value.negativeMatrix[field];
      assert.throws(() => assertPreparedProtocolNegativeReport(value, mode, () => false));
    }
  });
  test(`${mode} prepared-negative report rejects every omitted top-level field`, () => {
    for (const field of Object.keys(protocolNegatives(mode))) {
      const value = protocolNegatives(mode); delete value[field];
      assert.throws(() => assertPreparedProtocolNegativeReport(value, mode, () => false), field);
    }
  });
}

test("prepared-negative mode comes from invocation, never the untrusted report", () => {
  for (const mode of ["review_first", "auto_submit"]) {
    const value = protocolNegatives(mode);
    for (const invalid of [undefined, null, "automatic", mode === "review_first" ? "auto_submit" : "review_first"]) {
      assert.throws(() => assertPreparedProtocolNegativeReport(value, invalid, () => false));
    }
    delete value.claim.submissionMode;
    assert.throws(() => assertPreparedProtocolNegativeReport(value, mode, () => false));
  }
});

test("auto-submit negative reports retain consent and frozen ATS without extra authority", () => {
  for (const change of [(v) => { v.autoAuthorization.active = false; },
    (v) => { v.autoAuthorization.noManualApproval = false; }, (v) => { v.ats.unchangedAfterRepeat = false; },
    (v) => { v.ats.nonceBound = false; }, (v) => { v.autoEffectState.canaryReservations = 1; },
    (v) => { delete v.ats; }, (v) => { v.repeat = {}; }]) {
    const value = protocolNegatives("auto_submit"); change(value);
    assert.throws(() => assertPreparedProtocolNegativeReport(value, "auto_submit", () => false));
  }
});

test("wrapper explicitly binds negative modes and preflights unknown faults before missing keys", () => {
  const source = readFileSync(new URL("./run-nonprod-runner-control-plane-tests.mjs", import.meta.url), "utf8");
  assert.match(source, /process\.argv\[2\] === "--prepared-protocol-negatives" \? "review_first"/);
  assert.match(source, /process\.argv\[2\] === "--auto-submit-protocol-negatives" \? "auto_submit"/);
  assert.match(source, /assertPreparedProtocolNegativeReport\(report, protocolNegativeMode\)/);
  assert.doesNotMatch(source, /assertPreparedProtocolNegativeReport\(report, report/);
  assert.match(source, /createFixtureLeaseClient\(\{ claimFault: "unexpected" \}\)/);
  assert.match(source, /error\.message === "fixture_claim_fault"/);
  assert.match(source, /prepared-protocol-negatives/);
});
