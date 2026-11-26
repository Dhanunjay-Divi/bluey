// Ordinary API -> dispatcher -> real runner-client reproduction in a private PG.
// No browser, Temporal service, external posting, native measurement or submit.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, createHmac, generateKeyPairSync, randomBytes, randomUUID } from "node:crypto";
import { constants } from "node:fs";
import { open, lstat, mkdir, realpath, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { connect } from "node:net";
import { basename, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { pathToFileURL } from "node:url";
import { startNonprodObjectStore } from "./nonprod-object-store.mjs";

const UUID = "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const ACCOUNT = new RegExp(`^ui_fixture_${UUID}$`);
const MAX_BODY = 1024 * 1024;
const OPERATION_MS = 15_000;
class FixtureGateError extends Error {}
const fail = (code) => { throw new FixtureGateError(code); };
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
export const PROTECTED_CLAIM_TABLES = Object.freeze([
  "jobs_applications", "jobs_browser_sessions", "jobs_attempt_reservations", "jobs_packet_metering",
  "balance_ledger_entries", "jobs_resume_versions", "jobs_application_evidence", "jobs_execution_leases",
  "jobs_application_ats_certification_bindings", "jobs_execution_lease_volume_bindings", "jobs_runner_volume_residencies",
  "jobs_workflow_commands", "jobs_managed_cloud_workflow_bindings", "jobs_managed_cloud_request_start_authorities",
]);
export const AUTO_REPEAT_PROTECTED_TABLES = Object.freeze([...PROTECTED_CLAIM_TABLES,
  "jobs_runner_account_subjects", "jobs_runner_volume_authority_uses",
  "jobs_execution_lease_process_runtime_bindings", "jobs_submission_evidence_capacity",
  "jobs_ats_certification_canary_reservations", "jobs_managed_cloud_irreversible_effect_receipts",
]);
// A prepared claim may create lease/ATS/volume bindings and refresh the reserved
// attempt's timestamp. It must not change candidate, billing or workflow state.
export const PREPARED_IMMUTABLE_TABLES = Object.freeze(PROTECTED_CLAIM_TABLES.filter((table) => ![
  "jobs_attempt_reservations", "jobs_execution_leases", "jobs_application_ats_certification_bindings",
  "jobs_execution_lease_volume_bindings", "jobs_runner_volume_residencies",
].includes(table)));
export const RECOVERY_IMMUTABLE_TABLES = Object.freeze(PREPARED_IMMUTABLE_TABLES.filter((table) =>
  !["jobs_applications", "jobs_browser_sessions"].includes(table)));
export const RECOVERY_STABLE_TABLES = Object.freeze(AUTO_REPEAT_PROTECTED_TABLES.filter((table) =>
  !["jobs_applications", "jobs_browser_sessions", "jobs_attempt_reservations", "jobs_execution_leases"].includes(table)));

const UNAVAILABLE = "Execution lease is not available.";
const VOLUME_INVALID = "Runner-volume authority is invalid.";
export const PREPARED_NEGATIVE_CASES = Object.freeze([
  ["bearer_only", "claim", "http", 401, "Unauthorized", "absent", "not_checked", null],
  ["signed_body_changed", "claim", "http", 401, "Unauthorized", "fresh", "not_checked", null],
  ["owner_mismatch", "claim", "http", 400, "Execution lease owner does not match the authenticated worker.", "fresh", "not_checked", null],
  ["worker_nonce_replay", "claim", "http", 401, "Unauthorized", "replay", "not_checked", null],
  ["proof_volume_mismatch", "claim", "http", 400, "Runner-volume lease proof does not match the claim.", "fresh", "not_checked", null],
  ["proof_process_mismatch", "claim", "http", 400, "Runner-volume lease proof does not match the claim.", "fresh", "not_checked", null],
  ["proof_invalid_version", "claim", "http", 400, "Invalid runner-volume request.", "fresh", "not_checked", null],
  ["proof_bad_signature", "claim", "client", 401, VOLUME_INVALID, "fresh", "fresh", "request_failed"],
  ["proof_bad_payload", "claim", "client", 401, VOLUME_INVALID, "fresh", "fresh", "request_failed"],
  ["proof_consumed_replay", "claim", "http", 409, UNAVAILABLE, "fresh", "consumed", null],
  ["wrong_application", "claim", "client", 404, "Execution lease target not found.", "fresh", "fresh", "request_failed"],
  ["wrong_run", "claim", "client", 409, UNAVAILABLE, "fresh", "fresh", "lease_unavailable"],
  ["wrong_profile", "claim", "client", 409, UNAVAILABLE, "fresh", "fresh", "lease_unavailable"],
  ["runtime_grant", "claim", "client", 409, UNAVAILABLE, "fresh", "fresh", "lease_unavailable"],
  ["runtime_hash", "claim", "client", 409, UNAVAILABLE, "fresh", "fresh", "lease_unavailable"],
  ["runtime_instance", "claim", "client", 409, UNAVAILABLE, "fresh", "fresh", "lease_unavailable"],
  ["wrong_worker", "claim", "client", 401, VOLUME_INVALID, "fresh", "fresh", "request_failed"],
  ["heartbeat_token", "heartbeat", "http", 409, UNAVAILABLE, "fresh", "not_checked", null],
  ["heartbeat_fence", "heartbeat", "http", 409, UNAVAILABLE, "fresh", "not_checked", null],
  ["finish_token", "finish", "http", 409, UNAVAILABLE, "fresh", "not_checked", null],
  ["finish_fence", "finish", "http", 409, UNAVAILABLE, "fresh", "not_checked", null],
  ["recovery_token", "recovery", "client", 409, UNAVAILABLE, "fresh", "not_checked", "request_failed"],
  ["recovery_fence", "recovery", "client", 409, UNAVAILABLE, "fresh", "not_checked", "request_failed"],
  ["recovery_owner", "recovery", "client", 409, UNAVAILABLE, "fresh", "not_checked", "request_failed"],
].map(([id, operation, via, status, body, noncePolicy, proofPolicy, clientCode]) =>
  Object.freeze({ id, operation, via, status, responseSha256: hash(body), noncePolicy, proofPolicy, clientCode })));
export const DEFERRED_PROTOCOL_NEGATIVES = Object.freeze(["cross_account_admission", "readiness_rotation", "policy_head_rotation"]);

// Cleanup failure is sticky: repeated callers await the same failed outcome.
export function singleFlightCleanup(operation) {
  let pending;
  return () => { pending ??= (async () => operation())(); return pending; };
}

export function exactRecord(value, fields, label) {
  assert.ok(value && typeof value === "object" && !Array.isArray(value), label);
  assert.deepEqual(Object.keys(value).sort(), [...fields].sort(), label);
}
export function closedFailureDiagnostic(error) {
  const match = typeof error?.stack === "string"
    ? error.stack.match(/\/jobs\/scripts\/nonprod-runner-control-plane\.mjs:(\d+):\d+/) : null;
  const label = (error instanceof FixtureGateError || (error?.code === "ERR_ASSERTION" && match))
    && typeof error.message === "string" ? error.message.split("\n", 1)[0] : null;
  const code = typeof label === "string" && /^[a-zA-Z0-9_]{1,100}$/.test(label) ? label : "closed_stage_failure";
  const name = ["Error", "TypeError", "SyntaxError", "AssertionError", "AbortError", "TimeoutError", "ExecutionLeaseError"]
    .includes(error?.name) ? error.name : "unknown";
  const systemCode = ["ERR_ASSERTION", "ERR_INVALID_ARG_TYPE", "ABORT_ERR", "ENOENT", "ECONNREFUSED", "EPIPE"]
    .includes(error?.code) ? error.code : null;
  const causeCode = ["UND_ERR_SOCKET", "UND_ERR_CONNECT_TIMEOUT", "ECONNRESET", "ECONNREFUSED", "HPE_INVALID_HEADER_TOKEN"]
    .includes(error?.cause?.code) ? error.cause.code : null;
  return { code, errorType: name, systemCode, causeCode, callSiteLine: match ? Number(match[1]) : null };
}
export function apiPanicDiagnostic(tail) {
  const match = tail.match(/panicked at [^\n]*?([A-Za-z0-9_-]+\.rs):(\d+):\d+/);
  const category = !match ? null
    : tail.includes("Cannot start a runtime from within a runtime") ? "nested_runtime"
    : tail.includes("error retrieving column") ? "postgres_row_type"
    : tail.includes("already borrowed") ? "borrow_conflict" : "other";
  return { detected: Boolean(match), sourceFile: match ? match[1] : null,
    sourceLine: match ? Number(match[2]) : null, category, tailSha256: hash(tail) };
}

// Exact public error constants only; never emit a response body or a captured field.
export function httpResponseDiagnostic(status, body) {
  const known = new Map([
    ["Invalid execution lease request.", "invalid_execution_request"],
    ["Runner-volume lease proof does not match the claim.", "volume_proof_claim_mismatch"],
    ["Invalid runner-volume request.", "invalid_volume_request"],
    ["Invalid managed-cloud execution authority.", "invalid_managed_authority"],
    ["Managed-cloud execution authority must be complete.", "incomplete_managed_authority"],
    ["Managed-cloud execution authority is required.", "missing_managed_authority"],
    ["Execution lease target not found.", "execution_target_missing"],
    ["Execution lease is not available.", "execution_unavailable"],
    ["Unauthorized", "unauthorized"],
  ]);
  return { status: Number.isInteger(status) && status >= 100 && status <= 599 ? status : null,
    category: known.get(body) || "unclassified", responseSha256: hash(body) };
}

// Read only the owned private cluster log; never emit SQL, values, identifiers,
// native error text, connection details or candidate data from it.
export function postgresErrorDiagnostic(tail) {
  const errors = [...tail.matchAll(/\bERROR:\s+(?:([0-9A-Z]{5}):\s+)?([^\n]*)/g)];
  const latest = errors.at(-1);
  const message = latest?.[2] || "";
  const category = !latest ? null
    : /column reference .* is ambiguous/.test(message) ? "ambiguous_column"
    : /column .* does not exist/.test(message) ? "missing_column"
    : /relation .* does not exist/.test(message) ? "missing_relation"
    : /syntax error/.test(message) ? "syntax"
    : /(?:could not determine data type|inconsistent types deduced) .*parameter/.test(message) ? "parameter_type"
    : /operator does not exist/.test(message) ? "operator_type"
    : /violates .*constraint/.test(message) ? "constraint"
    : /invalid input syntax for type/.test(message) ? "invalid_input"
    : /deadlock detected/.test(message) ? "deadlock" : "other";
  return { detected: Boolean(latest), sqlState: latest?.[1] || null, category, tailSha256: hash(tail) };
}

export function assertCausalRedReport(report, pidExists = (pid) => {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === "ESRCH") return false; throw error; }
}) {
  exactRecord(report, ["version", "scenario", "sourceAccepted", "claim", "claimReadinessConfirmed", "distinctIds", "queueReplay",
    "protectedClaimStateUnchanged", "protectedClaimFamilies", "gatewayPosts", "gatewayObservationMs", "temporalAccepted", "externalEffects",
    "processes", "listenersClosed", "objectStore"], "report_shape");
  assert.equal(report.version, 1, "report_version");
  assert.equal(report.scenario, "causal_red", "report_scenario");
  assert.equal(report.sourceAccepted, false, "red_not_source_acceptance");
  exactRecord(report.claim, ["operation", "code", "status", "httpRequests", "responseSha256"], "claim_shape");
  assert.equal(report.claim.operation, "claim", "claim_operation");
  assert.equal(report.claim.code, "request_failed", "claim_code");
  assert.equal(report.claim.status, 404, "claim_status");
  assert.equal(report.claim.httpRequests, 1, "actual_single_http_claim");
  assert.equal(report.claim.responseSha256, hash("Execution lease target not found."), "exact_target_error");
  assert.equal(report.protectedClaimStateUnchanged, true, "report_protectedClaimStateUnchanged");
  assertCommonProtocolReport(report, pidExists);
}

export function assertPreparedLifecycleReport(report, pidExists = (pid) => {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === "ESRCH") return false; throw error; }
}) {
  assertReviewFirstProtocolReport(report, false, pidExists);
}

export function assertCheckpointRecoveryCausalRedReport(report, pidExists = (pid) => {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === "ESRCH") return false; throw error; }
}) {
  assertReviewFirstProtocolReport(report, true, pidExists);
}

function assertReviewFirstProtocolReport(report, recoveryRed, pidExists) {
  exactRecord(report, ["version", "scenario", "sourceAccepted", "claim", "claimReadinessConfirmed", "distinctIds", "queueReplay",
    "protectedClaimFamilies", "heartbeat", "release", "terminalReplay", "gatewayPosts", "gatewayObservationMs", "temporalAccepted",
    "externalEffects", "immutableFamilies", "immutableStateUnchanged", "attemptBindingUnchanged", "effectState",
    "processes", "listenersClosed", "objectStore", ...(recoveryRed ? ["recovery"] : [])], "prepared_report_shape");
  assert.equal(report.scenario, recoveryRed ? "checkpoint_session_causal_red" : "prepared_lifecycle", "prepared_report_scenario");
  assertReviewFirstClaimFields(report);
  if (recoveryRed) {
    exactRecord(report.recovery, ["operation", "code", "status", "httpRequests", "responseSha256", "checkpointVersion",
      "checkpointPhase", "exactLeaseCredentials", "freshTransportNonce", "preHeartbeatConfirmed", "protectedStateUnchanged",
      "retainedHeartbeatStatus", "retainedHeartbeatExact", "effectAuthorityZero"], "recovery_red_shape");
    assert.equal(report.recovery.operation, "recovery", "recovery_red_operation");
    assert.equal(report.recovery.code, "request_failed", "recovery_red_code");
    assert.equal(report.recovery.status, 404, "recovery_red_status");
    assert.equal(report.recovery.httpRequests, 1, "recovery_red_single_http_request");
    assert.equal(report.recovery.responseSha256, hash("Execution lease target not found."), "recovery_red_public_error");
    assert.equal(report.recovery.checkpointVersion, 2, "recovery_red_version");
    assert.equal(report.recovery.checkpointPhase, "prepared", "recovery_red_prepared");
    assert.equal(report.recovery.retainedHeartbeatStatus, 200, "recovery_red_retained_heartbeat_status");
    for (const key of ["exactLeaseCredentials", "freshTransportNonce", "preHeartbeatConfirmed", "protectedStateUnchanged", "retainedHeartbeatExact", "effectAuthorityZero"]) {
      assert.equal(report.recovery[key], true, `recovery_red_${key}`);
    }
  }
  assertPreparedTransitions(report);
  assertCommonProtocolReport(report, pidExists, recoveryRed ? AUTO_REPEAT_PROTECTED_TABLES : PROTECTED_CLAIM_TABLES);
}

function assertReviewFirstClaimFields(report) {
  exactRecord(report.claim, ["httpRequests", "status", "phase", "fence", "submissionMode", "leaseCount",
    "volumeBindingCount", "exactVolumeBindingCount", "atsBindingCount", "atsApplicationBindingCount", "atsRunBindingCount", "runAliasCount"], "prepared_claim_shape");
  assert.equal(report.claim.httpRequests, 1, "prepared_single_claim");
  assert.equal(report.claim.status, 200, "prepared_claim_status");
  assert.equal(report.claim.phase, "prepared", "prepared_claim_phase");
  assert.ok(Number.isSafeInteger(report.claim.fence) && report.claim.fence > 0, "prepared_claim_fence");
  assert.equal(report.claim.submissionMode, "review_first", "prepared_review_first_only");
  assert.equal(report.claim.leaseCount, 1, "prepared_lease_count");
  assert.equal(report.claim.volumeBindingCount, 1, "prepared_volume_count");
  assert.equal(report.claim.exactVolumeBindingCount, 1, "prepared_exact_volume_count");
  assert.equal(report.claim.atsBindingCount, 0, "review_first_no_account_ats_binding");
  assert.equal(report.claim.atsApplicationBindingCount, 0, "review_first_no_application_ats_binding");
  assert.equal(report.claim.atsRunBindingCount, 0, "review_first_no_run_ats_binding");
  assert.equal(report.claim.runAliasCount, 0, "prepared_no_run_alias");
}

export function assertPreparedProtocolNegativeReport(report, expectedSubmissionMode, pidExists = (pid) => {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === "ESRCH") return false; throw error; }
}) {
  assert.ok(["review_first", "auto_submit"].includes(expectedSubmissionMode), "negative_expected_mode");
  assert.equal(report?.claim?.submissionMode, expectedSubmissionMode, "negative_invocation_mode");
  const auto = expectedSubmissionMode === "auto_submit";
  exactRecord(report, ["version", "scenario", "sourceAccepted", "claim", "claimReadinessConfirmed", "distinctIds", "queueReplay",
    "protectedClaimFamilies", "heartbeat", "release", "terminalReplay", "gatewayPosts", "gatewayObservationMs", "temporalAccepted",
    "externalEffects", "immutableFamilies", "immutableStateUnchanged", "attemptBindingUnchanged", "effectState", "processes",
    "listenersClosed", "objectStore", "negativeMatrix", ...(auto ? ["autoAuthorization", "ats", "autoEffectState"] : [])], "negative_report_shape");
  assert.equal(report.scenario, "prepared_protocol_negatives", "negative_scenario");
  if (auto) assertAutoSubmitFrozenFields(report); else assertReviewFirstClaimFields(report);
  exactRecord(report.negativeMatrix, ["cases", "deferred", "retainedHeartbeatStatus", "retainedHeartbeatExact", "effectAuthorityZero"], "negative_matrix_shape");
  assert.deepEqual(report.negativeMatrix.deferred, DEFERRED_PROTOCOL_NEGATIVES, "negative_deferred_scope");
  assert.ok(Array.isArray(report.negativeMatrix.cases), "negative_cases_array");
  assert.equal(report.negativeMatrix.cases.length, PREPARED_NEGATIVE_CASES.length, "negative_case_count");
  for (const [index, spec] of PREPARED_NEGATIVE_CASES.entries()) {
    const entry = report.negativeMatrix.cases[index];
    exactRecord(entry, ["id", "operation", "via", "status", "responseSha256", "clientCode", "httpRequests", "nonceFresh", "proofFresh",
      "inputsBound", "stateUnchanged"], "negative_case_shape");
    for (const field of ["id", "operation", "via", "status", "responseSha256", "clientCode"]) assert.equal(entry[field], spec[field], `negative_case_${field}`);
    assert.equal(entry.httpRequests, 1, "negative_case_real_request");
    assert.equal(entry.nonceFresh, spec.noncePolicy === "fresh" ? true : spec.noncePolicy === "replay" ? false : null, "negative_case_nonce");
    assert.equal(entry.proofFresh, spec.proofPolicy === "fresh" ? true : spec.proofPolicy === "consumed" ? false : null, "negative_case_proof");
    assert.equal(entry.inputsBound, true, "negative_case_inputs");
    assert.equal(entry.stateUnchanged, true, "negative_case_state");
  }
  assert.equal(report.negativeMatrix.retainedHeartbeatStatus, 200, "negative_retained_heartbeat");
  assert.equal(report.negativeMatrix.retainedHeartbeatExact, true, "negative_retained_exact");
  assert.equal(report.negativeMatrix.effectAuthorityZero, true, "negative_zero_effect_authority");
  assertPreparedTransitions(report);
  assertCommonProtocolReport(report, pidExists, AUTO_REPEAT_PROTECTED_TABLES, auto ? 4 : 3);
}

function assertPreparedTransitions(report, immutableTables = PREPARED_IMMUTABLE_TABLES) {
  exactRecord(report.heartbeat, ["httpRequests", "observedUpdate", "sameTokenAndFence", "phase"], "heartbeat_shape");
  assert.ok(Number.isSafeInteger(report.heartbeat.httpRequests) && report.heartbeat.httpRequests >= 1
    && report.heartbeat.httpRequests <= 20, "heartbeat_request_count");
  assert.equal(report.heartbeat.observedUpdate, true, "heartbeat_observed_update");
  assert.equal(report.heartbeat.sameTokenAndFence, true, "heartbeat_same_authority");
  assert.equal(report.heartbeat.phase, "prepared", "heartbeat_prepared");
  exactRecord(report.release, ["httpRequests", "status", "phase", "noIrreversible"], "release_shape");
  assert.equal(report.release.httpRequests, 1, "single_release");
  assert.equal(report.release.status, 204, "release_status");
  assert.equal(report.release.phase, "released", "release_phase");
  assert.equal(report.release.noIrreversible, true, "no_irreversible_action");
  exactRecord(report.terminalReplay, ["httpRequests", "status", "freshNonce", "unchanged"], "terminal_replay_shape");
  assert.equal(report.terminalReplay.httpRequests, 1, "single_terminal_replay");
  assert.equal(report.terminalReplay.status, 204, "terminal_replay_status");
  assert.equal(report.terminalReplay.freshNonce, true, "terminal_replay_fresh_nonce");
  assert.equal(report.terminalReplay.unchanged, true, "terminal_replay_unchanged");
  assert.deepEqual(report.immutableFamilies, immutableTables, "prepared_immutable_families");
  assert.equal(report.immutableStateUnchanged, true, "prepared_immutable_state_unchanged");
  assert.equal(report.attemptBindingUnchanged, true, "prepared_attempt_binding_unchanged");
  exactRecord(report.effectState, ["submitted", "evidence", "irreversible"], "prepared_effect_state_shape");
  assert.deepEqual(report.effectState, { submitted: 0, evidence: 0, irreversible: 0 }, "prepared_effect_state");
  assert.equal(report.externalEffects, Object.values(report.effectState).reduce((sum, value) => sum + value, 0),
    "prepared_effects_derived_from_readback");
}

export function assertAutoSubmitRepeatCausalRedReport(report, pidExists = (pid) => {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === "ESRCH") return false; throw error; }
}) {
  assertAutoSubmitRepeatReport(report, false, pidExists);
}

export function assertAutoSubmitPreparedRepeatReport(report, pidExists = (pid) => {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === "ESRCH") return false; throw error; }
}) {
  assertAutoSubmitRepeatReport(report, true, pidExists);
}

function assertAutoSubmitRepeatReport(report, green, pidExists) {
  exactRecord(report, ["version", "scenario", "sourceAccepted", "claim", "claimReadinessConfirmed", "distinctIds", "queueReplay",
    "protectedClaimFamilies", "heartbeat", "release", "terminalReplay", "gatewayPosts", "gatewayObservationMs", "temporalAccepted",
    "externalEffects", "immutableFamilies", "immutableStateUnchanged", "attemptBindingUnchanged", "effectState",
    "processes", "listenersClosed", "objectStore", "autoAuthorization", "ats", "repeat", "autoEffectState"], "auto_red_report_shape");
  assert.equal(report.scenario, green ? "auto_submit_prepared_repeat" : "auto_submit_repeat_causal_red", "auto_repeat_scenario");
  assertAutoSubmitFrozenFields(report);
  assertAutoSubmitRepeatFields(report, green);
  assertPreparedTransitions(report);
  assertCommonProtocolReport(report, pidExists, AUTO_REPEAT_PROTECTED_TABLES, 4);
}

function assertAutoSubmitFrozenFields(report) {
  exactRecord(report.autoAuthorization, ["active", "revision", "identityAndSourceBound", "admissionBound", "noManualApproval"], "auto_authorization_shape");
  for (const key of ["active", "identityAndSourceBound", "admissionBound", "noManualApproval"]) {
    assert.equal(report.autoAuthorization[key], true, `auto_authorization_${key}`);
  }
  assert.ok(Number.isSafeInteger(report.autoAuthorization.revision) && report.autoAuthorization.revision > 0, "auto_authorization_revision");
  exactRecord(report.claim, ["httpRequests", "status", "phase", "fence", "submissionMode", "leaseCount", "volumeBindingCount",
    "exactVolumeBindingCount", "atsBindingCount", "atsApplicationBindingCount", "atsRunBindingCount", "runAliasCount"], "auto_claim_shape");
  assert.equal(report.claim.httpRequests, 1, "auto_initial_single_claim");
  assert.equal(report.claim.status, 200, "auto_initial_claim_status");
  assert.equal(report.claim.phase, "prepared", "auto_initial_claim_phase");
  assert.equal(report.claim.submissionMode, "auto_submit", "auto_initial_mode");
  assert.ok(Number.isSafeInteger(report.claim.fence) && report.claim.fence > 0, "auto_initial_fence");
  for (const key of ["leaseCount", "volumeBindingCount", "exactVolumeBindingCount", "atsBindingCount", "atsApplicationBindingCount", "atsRunBindingCount"]) {
    assert.equal(report.claim[key], 1, `auto_claim_${key}`);
  }
  assert.equal(report.claim.runAliasCount, 0, "auto_no_run_alias");
  exactRecord(report.ats, ["phase", "fence", "bindingSha256", "nonceBound", "expiresFuture", "exactFrozenAuthority",
    "exactPreparedLease", "noPhaseBOrInvalidation", "unchangedAfterRepeat"], "auto_ats_shape");
  assert.equal(report.ats.phase, "preflight", "auto_ats_phase");
  assert.equal(report.ats.fence, 0, "auto_ats_fence");
  assert.match(report.ats.bindingSha256, /^[a-f0-9]{64}$/, "auto_ats_binding_digest");
  for (const key of ["nonceBound", "expiresFuture", "exactFrozenAuthority", "exactPreparedLease", "noPhaseBOrInvalidation", "unchangedAfterRepeat"]) {
    assert.equal(report.ats[key], true, `auto_ats_${key}`);
  }
  exactRecord(report.autoEffectState, ["capacity", "canaryReservations", "irreversibleReceipts"], "auto_effect_state_shape");
  assert.deepEqual(report.autoEffectState, { capacity: 0, canaryReservations: 0, irreversibleReceipts: 0 }, "auto_no_effect_authority");
}

function assertAutoSubmitRepeatFields(report, green) {
  if (green) {
    exactRecord(report.repeat, ["operation", "status", "httpRequests", "freshVolumeProof", "freshTransportNonce", "sameClaimFields",
      "readinessReplayed", "tokenRotated", "previousFence", "fence", "atsUnchanged", "immutableStateUnchanged",
      "attemptBindingUnchanged", "exactLeaseVolumeProcessBinding", "stableAccountSubjects", "staleHeartbeatStatus",
      "staleFinishStatus", "staleStateUnchanged", "currentHeartbeatStatus", "currentHeartbeatExact"], "auto_green_repeat_shape");
    assert.equal(report.repeat.operation, "claim", "auto_green_operation");
    assert.equal(report.repeat.status, 200, "auto_green_status");
    assert.equal(report.repeat.httpRequests, 1, "auto_green_single_request");
    assert.equal(report.repeat.previousFence, report.claim.fence, "auto_green_previous_fence");
    assert.ok(Number.isSafeInteger(report.repeat.fence) && report.repeat.fence > report.repeat.previousFence, "auto_green_fence_increases");
    for (const key of ["freshVolumeProof", "freshTransportNonce", "sameClaimFields", "readinessReplayed", "tokenRotated",
      "atsUnchanged", "immutableStateUnchanged", "attemptBindingUnchanged", "exactLeaseVolumeProcessBinding",
      "stableAccountSubjects", "staleStateUnchanged", "currentHeartbeatExact"]) {
      assert.equal(report.repeat[key], true, `auto_green_${key}`);
    }
    assert.equal(report.repeat.staleHeartbeatStatus, 409, "auto_green_stale_heartbeat");
    assert.equal(report.repeat.staleFinishStatus, 409, "auto_green_stale_finish");
    assert.equal(report.repeat.currentHeartbeatStatus, 200, "auto_green_current_heartbeat");
  } else {
    exactRecord(report.repeat, ["operation", "code", "status", "httpRequests", "responseSha256", "freshVolumeProof", "freshTransportNonce",
    "sameClaimFields", "readinessReplayed", "protectedStateUnchanged", "retainedHeartbeatStatus", "retainedHeartbeatExact"], "auto_repeat_shape");
  assert.equal(report.repeat.operation, "claim", "auto_repeat_operation");
  assert.equal(report.repeat.code, "lease_unavailable", "auto_repeat_code");
  assert.equal(report.repeat.status, 409, "auto_repeat_status");
  assert.equal(report.repeat.httpRequests, 1, "auto_repeat_single_request");
  assert.equal(report.repeat.responseSha256, hash("Execution lease is not available."), "auto_repeat_public_body");
  for (const key of ["freshVolumeProof", "freshTransportNonce", "sameClaimFields", "readinessReplayed", "protectedStateUnchanged", "retainedHeartbeatExact"]) {
    assert.equal(report.repeat[key], true, `auto_repeat_${key}`);
  }
  assert.equal(report.repeat.retainedHeartbeatStatus, 200, "auto_repeat_retained_heartbeat");
  }
}

export function assertCheckpointPreparedRecoveryReport(report, expectedSubmissionMode, pidExists = (pid) => {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === "ESRCH") return false; throw error; }
}) {
  assert.ok(["review_first", "auto_submit"].includes(expectedSubmissionMode), "checkpoint_expected_mode_required");
  assert.equal(report?.claim?.submissionMode, expectedSubmissionMode, "checkpoint_invocation_mode");
  const auto = expectedSubmissionMode === "auto_submit";
  exactRecord(report, ["version", "scenario", "sourceAccepted", "claim", "claimReadinessConfirmed", "distinctIds", "queueReplay",
    "protectedClaimFamilies", "heartbeat", "release", "terminalReplay", "gatewayPosts", "gatewayObservationMs", "temporalAccepted",
    "externalEffects", "immutableFamilies", "immutableStateUnchanged", "attemptBindingUnchanged", "effectState", "processes",
    "listenersClosed", "objectStore", "checkpointRecovery", ...(auto ? ["autoAuthorization", "ats", "autoEffectState"] : [])], "checkpoint_green_shape");
  assert.equal(report.scenario, "checkpoint_prepared_recovery", "checkpoint_green_scenario");
  if (auto) assertAutoSubmitFrozenFields(report);
  else {
    exactRecord(report.claim, ["httpRequests", "status", "phase", "fence", "submissionMode", "leaseCount", "volumeBindingCount",
      "exactVolumeBindingCount", "atsBindingCount", "atsApplicationBindingCount", "atsRunBindingCount", "runAliasCount"], "checkpoint_claim_shape");
    assert.equal(report.claim.submissionMode, "review_first", "checkpoint_review_first_mode");
    assert.equal(report.claim.httpRequests, 1, "checkpoint_single_claim");
    assert.equal(report.claim.status, 200, "checkpoint_claim_status");
    assert.equal(report.claim.phase, "prepared", "checkpoint_claim_phase");
    assert.ok(Number.isSafeInteger(report.claim.fence) && report.claim.fence > 0, "checkpoint_claim_fence");
    for (const key of ["leaseCount", "volumeBindingCount", "exactVolumeBindingCount"]) assert.equal(report.claim[key], 1, `checkpoint_${key}`);
    for (const key of ["atsBindingCount", "atsApplicationBindingCount", "atsRunBindingCount", "runAliasCount"]) assert.equal(report.claim[key], 0, `checkpoint_${key}`);
  }
  exactRecord(report.checkpointRecovery, ["operation", "status", "httpRequests", "checkpointVersion", "checkpointPhase", "freshTransportNonce",
    "exactLeaseCredentials", "observedReleased", "canonicalStateExact", "applicationStableSha256", "sessionStableSha256", "recoverySha256",
    "leaseAuthorityUnchanged", "stableFamilies", "stableStateUnchanged", "replayHttpRequests", "replayStatus", "replayFreshNonce", "replayUnchanged",
    "terminalFinishUnchanged", "effectAuthorityZero"], "checkpoint_recovery_shape");
  const recovery = report.checkpointRecovery;
  assert.equal(recovery.operation, "recovery", "checkpoint_operation");
  assert.equal(recovery.status, 200, "checkpoint_recovery_status");
  assert.equal(recovery.httpRequests, 1, "checkpoint_recovery_single_http");
  assert.equal(recovery.checkpointVersion, 2, "checkpoint_recovery_version");
  assert.equal(recovery.checkpointPhase, "prepared", "checkpoint_recovery_phase");
  assert.equal(recovery.replayHttpRequests, 1, "checkpoint_replay_single_http");
  assert.equal(recovery.replayStatus, 200, "checkpoint_replay_status");
  assert.deepEqual(recovery.stableFamilies, RECOVERY_STABLE_TABLES, "checkpoint_stable_families");
  for (const key of ["freshTransportNonce", "exactLeaseCredentials", "observedReleased", "canonicalStateExact", "leaseAuthorityUnchanged",
    "stableStateUnchanged", "replayFreshNonce", "replayUnchanged", "terminalFinishUnchanged", "effectAuthorityZero"]) assert.equal(recovery[key], true, `checkpoint_${key}`);
  for (const key of ["applicationStableSha256", "sessionStableSha256", "recoverySha256"]) assert.match(recovery[key], /^[a-f0-9]{64}$/, `checkpoint_${key}`);
  assertPreparedTransitions(report, RECOVERY_IMMUTABLE_TABLES);
  assertCommonProtocolReport(report, pidExists, AUTO_REPEAT_PROTECTED_TABLES, auto ? 7 : 6);
}

function assertCommonProtocolReport(report, pidExists, protectedTables = PROTECTED_CLAIM_TABLES, expectedProcessCount = 3) {
  assert.equal(report.version, 1, "report_version");
  assert.equal(report.sourceAccepted, false, "protocol_not_source_acceptance");
  for (const field of ["claimReadinessConfirmed", "distinctIds", "queueReplay", "listenersClosed"]) {
    assert.equal(report[field], true, `report_${field}`);
  }
  assert.deepEqual(report.protectedClaimFamilies, protectedTables, "protected_claim_families");
  assert.equal(report.gatewayPosts, 1, "gateway_single_post");
  assert.ok(Number.isSafeInteger(report.gatewayObservationMs) && report.gatewayObservationMs >= 11_000,
    "gateway_observation_bound");
  assert.equal(report.temporalAccepted, false, "no_temporal_claim");
  assert.equal(report.externalEffects, 0, "no_external_effects");
  assert.ok(Array.isArray(report.processes) && report.processes.length === expectedProcessCount, "owned_process_count");
  const pids = new Set();
  for (const child of report.processes) {
    exactRecord(child, ["pid", "code", "signal"], "process_shape");
    assert.ok(Number.isSafeInteger(child.pid) && child.pid > 1 && !pids.has(child.pid), "process_pid");
    pids.add(child.pid);
    assert.equal(child.code, 0, "process_exit");
    assert.equal(child.signal, null, "process_signal");
    assert.equal(pidExists(child.pid), false, "owned_process_still_exists");
  }
  exactRecord(report.objectStore, ["put", "get", "denied", "objects"], "store_shape");
  assert.equal(report.objectStore.put, 1, "one_source_put");
  assert.ok(Number.isSafeInteger(report.objectStore.get) && report.objectStore.get >= 1
    && report.objectStore.get <= 100, "source_readback");
  assert.equal(report.objectStore.denied, 0, "no_store_denial");
  assert.equal(report.objectStore.objects, 1, "one_source_object");
}

async function bounded(promise, label, milliseconds = OPERATION_MS) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => {
    timer = setTimeout(() => reject(new FixtureGateError(`timeout_${label}`)), milliseconds);
  })]); } finally { clearTimeout(timer); }
}
async function privateDirectory(path) {
  const info = await lstat(path);
  assert.ok(info.isDirectory() && !info.isSymbolicLink() && info.uid === process.getuid()
    && (info.mode & 0o7777) === 0o700, "private_directory_identity");
  assert.equal(await realpath(path), path, "private_directory_canonical");
}
async function privateBytes(path, maximum) {
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = await file.stat();
    assert.ok(before.isFile() && before.nlink === 1 && before.uid === process.getuid()
      && (before.mode & 0o7777) === 0o600 && before.size > 0 && before.size <= maximum, "private_file_identity");
    const bytes = Buffer.alloc(maximum + 1);
    const { bytesRead } = await file.read(bytes, 0, bytes.length, 0);
    const after = await file.stat();
    const pathInfo = await lstat(path);
    assert.ok(bytesRead === before.size && after.size === before.size && after.ino === before.ino
      && after.dev === before.dev && after.mtimeMs === before.mtimeMs && pathInfo.ino === before.ino
      && pathInfo.dev === before.dev && !pathInfo.isSymbolicLink(), "private_file_changed");
    return bytes.subarray(0, bytesRead);
  } finally { await file.close(); }
}
async function privateJson(path, maximum = 64 * 1024) {
  return JSON.parse((await privateBytes(path, maximum)).toString("utf8"));
}
function jwt(account, secret) {
  const encode = (value) => Buffer.from(JSON.stringify(value)).toString("base64url");
  const now = Math.floor(Date.now() / 1000);
  const body = `${encode({ alg: "HS256", typ: "JWT" })}.${encode({ sub: account, iat: now, exp: now + 900, kind: "access" })}`;
  return `${body}.${createHmac("sha256", secret).update(body).digest("base64url")}`;
}
function purgePolicy() {
  const pair = generateKeyPairSync("ed25519");
  const privateDer = pair.privateKey.export({ format: "der", type: "pkcs8" });
  const publicDer = pair.publicKey.export({ format: "der", type: "spki" });
  assert.equal(privateDer.length, 48, "purge_private_length");
  assert.equal(publicDer.length, 44, "purge_public_length");
  const policy = { keyId: `fixture-purge-${randomUUID()}`, seed: privateDer.subarray(16).toString("base64url"),
    publicKey: publicDer.subarray(12).toString("base64url") };
  privateDer.fill(0);
  return policy;
}
async function listen(server) {
  await bounded(new Promise((done, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => { server.off("error", reject); done(); });
  }), "listen");
  const address = server.address();
  assert.ok(address && typeof address !== "string" && address.address === "127.0.0.1", "loopback_listener");
  return address.port;
}
async function freePort() {
  const server = createServer(); const port = await listen(server);
  await bounded(new Promise((done) => server.close(done)), "port_reservation_close");
  return port;
}
async function responseText(response, maximum) {
  const reader = response.body?.getReader(); const chunks = []; let count = 0;
  if (reader) try {
    while (true) { const { value, done } = await reader.read(); if (done) break;
      count += value.length; assert.ok(count <= maximum, "api_response_bound"); chunks.push(value); }
  } finally { await reader.cancel(); reader.releaseLock(); }
  return Buffer.concat(chunks).toString("utf8");
}

export async function runControlPlaneReproduction() {
  assert.ok(process.argv.length === 3 && ["--expect-target-missing", "--prepared-lifecycle", "--expect-auto-submit-repeat-conflict", "--auto-submit-prepared-repeat", "--expect-checkpoint-session-missing", "--checkpoint-prepared-recovery", "--auto-submit-checkpoint-recovery", "--prepared-protocol-negatives", "--auto-submit-protocol-negatives"].includes(process.argv[2]),
    "explicit_protocol_scenario_required");
  const causalRed = process.argv[2] === "--expect-target-missing";
  const autoRepeatRed = process.argv[2] === "--expect-auto-submit-repeat-conflict";
  const autoRepeatGreen = process.argv[2] === "--auto-submit-prepared-repeat";
  const recoveryGreen = ["--checkpoint-prepared-recovery", "--auto-submit-checkpoint-recovery"].includes(process.argv[2]);
  const protocolNegatives = ["--prepared-protocol-negatives", "--auto-submit-protocol-negatives"].includes(process.argv[2]);
  const autoSubmit = autoRepeatRed || autoRepeatGreen || ["--auto-submit-checkpoint-recovery", "--auto-submit-protocol-negatives"].includes(process.argv[2]);
  const recoveryRed = process.argv[2] === "--expect-checkpoint-session-missing";
  const claimTables = autoSubmit || recoveryRed || recoveryGreen || protocolNegatives ? AUTO_REPEAT_PROTECTED_TABLES : PROTECTED_CLAIM_TABLES;
  const requestedRoot = process.env.BLUEY_TEST_RUN_ROOT;
  assert.equal(typeof requestedRoot, "string", "launcher_root_required");
  const root = await realpath(requestedRoot);
  assert.equal(requestedRoot, root, "launcher_root_canonical");
  assert.match(root, /^\/(?:private\/)?tmp\/bluey-test-runs-[^/]+\/run\.[0-9]+\.[0-9]+\.[A-Za-z0-9]+$/);
  await privateDirectory(root);
  const marker = (await privateBytes(join(root, ".bluey-test-run-v2"), 4096)).toString("utf8");
  const lines = marker.trimEnd().split("\n");
  assert.ok(lines.length === 9 && lines[0] === "magic=bluey-test-run-v2" && lines[1] === `run_root=${root}`
    && lines[2] === `run_name=${basename(root)}` && lines[3] === `owner_uid=${process.getuid()}`
    && lines[8] === "state=running", "launcher_marker");
  assert.equal(process.env.BLUEY_DATABASE_URL, undefined, "owner_database_forbidden");
  assert.equal(resolve(process.env.CARGO_TARGET_DIR || "missing"), join(root, "cargo-target"), "private_target");
  const database = new URL(process.env.BLUEY_TEST_POSTGRES_URL || "invalid:");
  assert.ok(["postgres:", "postgresql:"].includes(database.protocol) && database.hostname === "127.0.0.1"
    && database.username === "bluey_jobs_test" && database.pathname === "/bluey_jobs_nonprod", "private_database_url");
  const home = process.env.HOME;
  assert.ok(typeof home === "string" && home.startsWith(root + "/"), "private_home_required");
  await privateDirectory(home);
  const work = join(root, "runner-control-plane");
  await privateDirectory(work);
  const temp = join(work, "tmp"); await mkdir(temp, { mode: 0o700 });
  const adapterPath = join(work, "control-plane-client.mjs");
  const adapterInfo = await lstat(adapterPath);
  assert.ok(adapterInfo.isFile() && !adapterInfo.isSymbolicLink() && adapterInfo.uid === process.getuid(), "adapter_identity");
  const { createFixtureLeaseClient, fixtureWorkerAuthHeaders } = await import(pathToFileURL(adapterPath).href);
  const psql = await realpath("/opt/homebrew/opt/postgresql@17/bin/psql");
  assert.ok(psql.startsWith("/opt/homebrew/Cellar/postgresql@17/"), "pinned_psql");
  const pgEnv = { PATH: "/opt/homebrew/bin:/usr/bin:/bin", HOME: home, TMPDIR: temp, TMP: temp, TEMP: temp,
    PGHOST: "127.0.0.1", PGPORT: database.port, PGUSER: decodeURIComponent(database.username),
    PGPASSWORD: decodeURIComponent(database.password), PGDATABASE: database.pathname.slice(1) };
  const children = [];
  const processes = [];
  const listeners = [];
  const ports = [];
  let objectStore;
  let stage = "postgres_identity";
  let requestObservation = null;
  let interrupted = false;
  let activeLease;
  let approvalRequests = 0;
  const cancellation = new AbortController();
  function start(command, env, name, args = []) {
    const child = spawn(command, args, { env, stdio: ["pipe", "pipe", "pipe"] });
    let bytes = 0; let output = ""; let overflow = false; let diagnosticTail = "";
    const completion = new Promise((done) => {
      child.once("error", () => done({ pid: child.pid || 0, code: null, signal: "spawn_error" }));
      child.once("close", (code, signal) => done({ pid: child.pid || 0, code, signal }));
    });
    child.stdout.on("data", (chunk) => {
      bytes += chunk.length; if (bytes > MAX_BODY) { overflow = true; child.kill("SIGTERM"); }
      else output += chunk;
    });
    child.stderr.on("data", (chunk) => { diagnosticTail = (diagnosticTail + chunk).slice(-4096); });
    const fixtureFailure = () => {
      try {
        const value = JSON.parse(diagnosticTail.trim().split("\n").at(-1));
        if (value.event === "runner_control_plane_fixture_failed" && /^[a-z_]{1,80}$/.test(value.code)) return value.code;
      } catch {}
      return "unclassified";
    };
    const entry = { child, completion, name, output: () => output, overflow: () => overflow, fixtureFailure,
      panicDiagnostic: () => apiPanicDiagnostic(diagnosticTail) };
    children.push(entry);
    return entry;
  }
  async function sql(query, maximum = 64 * 1024) {
    const entry = start(psql, pgEnv, "psql", ["-X", "-A", "-t", "-q", "-v", "ON_ERROR_STOP=1"]);
    entry.child.stdin.end(query + ";\n");
    const outcome = await bounded(entry.completion, "psql");
    assert.ok(outcome.code === 0 && outcome.signal === null && !entry.overflow(), "psql_clean_exit");
    assert.ok(Buffer.byteLength(entry.output()) <= maximum, "sql_result_bound");
    return entry.output().trim();
  }
  async function fixture(operation, body) {
    await writeFile(join(root, `runner-control-plane-${operation}-request.json`), JSON.stringify(body) + "\n",
      { flag: "wx", mode: 0o600 });
    const executable = join(root, "cargo-target/debug/examples/jobs-nonprod-runner-control-plane-fixture");
    const info = await lstat(executable);
    assert.ok(info.isFile() && !info.isSymbolicLink() && info.uid === process.getuid()
      && (info.mode & 0o022) === 0 && (info.mode & 0o100) !== 0, "fixture_executable_identity");
    const entry = start(executable, { PATH: pgEnv.PATH, HOME: home, TMPDIR: temp, TMP: temp, TEMP: temp,
      SQLITE_TMPDIR: temp, BLUEY_TEST_RUN_ROOT: root, BLUEY_TEST_POSTGRES_URL: database.href,
      BLUEY_JOBS_DATA_KEY: dataKey }, `fixture_${operation}`);
    entry.child.stdin.end();
    const outcome = await bounded(entry.completion, "fixture");
    if (outcome.code !== 0 || outcome.signal !== null || entry.overflow()) fail(`fixture_${entry.fixtureFailure()}`);
    processes.push(outcome);
    return privateJson(join(root, `runner-control-plane-${operation}-result.json`));
  }
  const accountId = `ui_fixture_${randomUUID()}`;
  const email = `protocol-${randomUUID()}@example.test`;
  assert.match(accountId, ACCOUNT);
  const secret = randomBytes(32).toString("base64url");
  const dataKey = randomBytes(32).toString("base64");
  const workerSigningKey = randomBytes(48).toString("base64url");
  const workerId = `protocol-worker-${randomUUID()}`;
  const gatewayToken = randomBytes(48).toString("base64url");
  const report = { version: 1, scenario: causalRed ? "causal_red" : autoRepeatRed ? "auto_submit_repeat_causal_red"
    : autoRepeatGreen ? "auto_submit_prepared_repeat" : recoveryRed ? "checkpoint_session_causal_red"
      : recoveryGreen ? "checkpoint_prepared_recovery" : protocolNegatives ? "prepared_protocol_negatives" : "prepared_lifecycle", sourceAccepted: false,
    claim: null, claimReadinessConfirmed: false, distinctIds: false, queueReplay: false,
    ...(causalRed ? { protectedClaimStateUnchanged: false } : { heartbeat: null, release: null, terminalReplay: null,
      immutableFamilies: recoveryGreen ? RECOVERY_IMMUTABLE_TABLES : PREPARED_IMMUTABLE_TABLES,
      immutableStateUnchanged: false, attemptBindingUnchanged: false, effectState: null }),
    ...(autoSubmit ? { autoAuthorization: null, ats: null, ...(!recoveryGreen && !protocolNegatives ? { repeat: null } : {}), autoEffectState: null } : {}),
    ...(recoveryRed ? { recovery: null } : {}),
    ...(recoveryGreen ? { checkpointRecovery: null } : {}),
    ...(protocolNegatives ? { negativeMatrix: null } : {}),
    protectedClaimFamilies: claimTables,
    gatewayPosts: 0, gatewayObservationMs: 0, temporalAccepted: false,
    externalEffects: causalRed ? 0 : null, processes, listenersClosed: false, objectStore: null };
  const observeRequests = [];
  let observerError;
  let firstPostAt;
  const observer = createServer({ maxHeaderSize: 8192, requestTimeout: 5000 }, async (request, response) => {
    try {
      assert.equal(request.method, "POST", "observer_method");
      assert.equal(request.url, "/workflow-commands", "observer_path");
      assert.equal(request.headers.authorization, `Bearer ${gatewayToken}`, "observer_auth");
      let size = 0; const chunks = [];
      for await (const chunk of request) {
        size += chunk.length; assert.ok(size <= 16 * 1024, "observer_body_bound"); chunks.push(chunk);
      }
      const value = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      exactRecord(value, ["schemaVersion", "operation", "requestId", "workflowId", "payloadDigest", "managedCloud"],
        "observer_schema");
      assert.equal(value.schemaVersion, 3, "observer_schema_version");
      assert.equal(value.operation, "start", "observer_operation");
      observeRequests.push(value); firstPostAt ??= Date.now();
    } catch { observerError = "observer_request_rejected"; }
    response.writeHead(503, { "content-type": "application/json", "cache-control": "no-store",
      "x-content-type-options": "nosniff", connection: "close" }).end("{}");
  });
  const onSignal = () => {
    interrupted = true; cancellation.abort();
    for (const entry of children) if (entry.child.exitCode === null && entry.child.signalCode === null) entry.child.kill("SIGTERM");
  };
  for (const name of ["SIGHUP", "SIGINT", "SIGTERM"]) process.on(name, onSignal);
  const stop = singleFlightCleanup(async () => {
    await activeLease?.stopHeartbeat();
    cancellation.abort();
    for (const entry of children) if (entry.child.exitCode === null && entry.child.signalCode === null) entry.child.kill("SIGTERM");
    const outcomes = await Promise.allSettled(children.map(async (entry) => {
      let outcome;
      try { outcome = await bounded(entry.completion, "child_shutdown", 5000); }
      catch {
        entry.child.kill("SIGKILL");
        await bounded(entry.completion, "child_kill", 5000);
        fail("forced_child_shutdown");
      }
      // A fixture that already failed is the primary test failure, not evidence
      // that its process survived shutdown. Successful reports still require
      // zero exits for both fixtures via assertCausalRedReport.
      assert.ok(outcome.pid > 1 && (outcome.code !== null || outcome.signal !== null), "child_exit_unproven");
      if (entry.name === "jobs_api") {
        assert.ok(outcome.code === 0 && outcome.signal === null && !entry.overflow(), "unclean_api_shutdown");
        processes.push(outcome);
      }
    }));
    const listenerOutcomes = await Promise.allSettled(listeners.map(async (server) => {
      const closed = new Promise((done, reject) => server.close((error) => error ? reject(error) : done()));
      server.closeAllConnections(); await bounded(closed, "listener_shutdown");
    }));
    let storeFailed = false;
    if (objectStore) {
      report.objectStore = objectStore.counters();
      try { await bounded(objectStore.close(), "store_shutdown"); } catch { storeFailed = true; }
    }
    const closedPorts = await Promise.allSettled(ports.map((port) => bounded(new Promise((done, reject) => {
      const socket = connect({ host: "127.0.0.1", port });
      socket.once("connect", () => { socket.destroy(); reject(new Error("owned_port_still_listening")); });
      socket.once("error", (error) => { socket.destroy();
        if (error.code === "ECONNREFUSED") done(); else reject(new Error("owned_port_state_unknown")); });
      socket.setTimeout(1000, () => { socket.destroy(); reject(new Error("owned_port_probe_timeout")); });
    }), "port_closed")));
    report.listenersClosed = listenerOutcomes.every((item) => item.status === "fulfilled") && !storeFailed
      && closedPorts.every((item) => item.status === "fulfilled");
    assert.ok(outcomes.every((item) => item.status === "fulfilled") && report.listenersClosed, "cleanup_failure");
  });
  try {
    const identity = JSON.parse(await sql("SELECT json_build_object('data',current_setting('data_directory'),'host',host(inet_server_addr()),'port',inet_server_port(),'database',current_database(),'user',current_user)::text"));
    assert.deepEqual(identity, { data: join(root, "nonprod-postgres/data"), host: "127.0.0.1",
      port: Number(database.port), database: "bluey_jobs_nonprod", user: "bluey_jobs_test" }, "physical_postgres_identity");
    // Only this physically verified disposable role/cluster. New API/fixture
    // connections inherit explicit SQLSTATE logging; native text stays private.
    stage = "private_pg_error_metadata";
    await sql("ALTER ROLE bluey_jobs_test SET log_error_verbosity = 'verbose'");
    assert.equal(await sql("SHOW log_error_verbosity"), "verbose", "private_pg_verbose_metadata");
    const gatewayPort = await listen(observer); listeners.push(observer); ports.push(gatewayPort);
    const apiPort = await freePort(); const origin = `http://127.0.0.1:${apiPort}`;
    ports.push(apiPort);
    objectStore = await startNonprodObjectStore({ accountId });
    ports.push(Number(new URL(objectStore.endpoint).port));
    const purge = purgePolicy();
    const api = join(root, "cargo-target/debug/bluey-jobs-api");
    const apiInfo = await lstat(api);
    assert.ok(apiInfo.isFile() && !apiInfo.isSymbolicLink() && apiInfo.uid === process.getuid(), "api_binary_identity");
    const common = { PATH: pgEnv.PATH, HOME: home, TMPDIR: temp, TMP: temp, TEMP: temp, SQLITE_TMPDIR: temp,
      LANG: "C", TZ: "UTC", BLUEY_SERVER_DB_BACKEND: "postgres", BLUEY_DATABASE_URL: database.href,
      BLUEY_JWT_SECRET: secret, BLUEY_JOBS_DATA_KEY: dataKey,
      BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY_ID: purge.keyId, BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY: purge.seed,
      BLUEY_JOBS_RUNNER_PURGE_VERIFYING_KEYS_JSON: JSON.stringify([{ keyId: purge.keyId, publicKeyBase64url: purge.publicKey }]),
      BLUEY_JOBS_RUNNER_MINIMUM_BUILD_ID: "runner-630.0", BLUEY_JOBS_BETA_ENABLED: "1",
      BLUEY_JOBS_MODEL_GENERATION_ENABLED: "0", BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED: "0",
      BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED: "1", BLUEY_JOBS_MAILBOX_SYNC_ENABLED: "0",
      BLUEY_JOBS_GLOBAL_ARCHIVE_ENABLED: "0", BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED: "1",
      BLUEY_JOBS_WORKFLOW_COMMAND_RECONCILIATION_ENABLED: "0", BLUEY_JOBS_WORKFLOW_COMMAND_POLL_SECONDS: "1",
      BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED: "0", BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED: "0",
      BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED: "0", BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED: "0",
      BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED: "0", BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT: "staging",
      BLUEY_JOBS_MANAGED_CLOUD_REGION: "us-east-1", BLUEY_JOBS_MANAGED_CLOUD_CHANNEL: "general",
      BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN: origin, BLUEY_JOBS_WORKFLOW_ORIGIN: `http://127.0.0.1:${gatewayPort}`,
      BLUEY_JOBS_WORKFLOW_TOKEN: gatewayToken, BLUEY_JOBS_WORKER_SIGNING_KEY: workerSigningKey,
      BLUEY_JOBS_API_HOST: "127.0.0.1", BLUEY_JOBS_API_PORT: String(apiPort), ...objectStore.env, RUST_LOG: "warn" };
    stage = "api_start";
    const apiChild = start(api, common, "jobs_api"); apiChild.child.stdin.end();
    async function localFetch(url, options = {}) {
      const parsed = new URL(url);
      assert.equal(parsed.origin, origin, "api_network_boundary");
      assert.ok(parsed.pathname === "/health" || parsed.pathname.startsWith("/api/jobs/"), "api_path_boundary");
      return fetch(parsed, { ...options, redirect: "error", signal: AbortSignal.any([
        options.signal || new AbortController().signal, cancellation.signal, AbortSignal.timeout(OPERATION_MS),
      ]) });
    }
    const token = jwt(accountId, secret);
    async function request(path, method = "GET", body, extraHeaders = {}) {
      if (method === "POST" && path.endsWith("/approve")) approvalRequests += 1;
      const startedAt = Date.now();
      requestObservation = { method, status: null, elapsedMs: 0 };
      try {
        const response = await localFetch(origin + path, { method, headers: { authorization: `Bearer ${token}`,
          "content-type": "application/json", ...extraHeaders }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
        requestObservation.status = response.status;
        const text = await responseText(response, MAX_BODY);
        assert.equal(response.status, 200, `api_status_${stage}`);
        return text ? JSON.parse(text) : null;
      } finally { requestObservation.elapsedMs = Date.now() - startedAt; }
    }
    const readyDeadline = Date.now() + OPERATION_MS;
    while (true) {
      assert.equal(apiChild.child.exitCode, null, "api_early_exit");
      try { const health = await localFetch(origin + "/health"); await health.body?.cancel(); if (health.ok) break; } catch {}
      if (Date.now() > readyDeadline) fail("api_start_timeout"); await delay(100);
    }
    stage = "synthetic_account";
    assert.match(email, /^protocol-[0-9a-f-]{36}@example\.test$/);
    await sql(`INSERT INTO accounts(id,email,password_hash,trial_seconds_remaining,email_verified_at) VALUES('${accountId}','${email}','fixture-only',0,now());
      UPDATE jobs_public_beta_cohorts SET state='closed_to_new',hard_cap=1 WHERE id='public-v1';
      INSERT INTO jobs_public_beta_enrollments(cohort_id,account_id,source,admitted_at_ms) VALUES('public-v1','${accountId}','admin',1);
      UPDATE jobs_public_beta_cohorts SET assigned_count=1 WHERE id='public-v1'`);
    stage = "profile";
    let workspace = await request("/api/jobs/workspace");
    const profile = { ...workspace.profile, full_name: "Synthetic Candidate", email, phone: "555-0100",
      headline: "Software Engineer", current_location: "New York, NY", skills: autoSubmit ? ["Rust", "TypeScript", "PostgreSQL"] : ["TypeScript"],
      employment: [{ id: randomUUID(), company: "Synthetic Systems", title: "Software Engineer", location: "New York, NY",
        start_date: "2022-01", end_date: "", current: true, highlights: ["Built synthetic test systems"] }],
      resume_mode: "factual", default_submission_mode: "review_first" };
    await request("/api/jobs/profile", "PUT", profile);
    stage = "source";
    const sourceBytes = Buffer.from("Synthetic Candidate\nSoftware Engineer\nNew York, NY\nSynthetic Systems\nSoftware Engineer, 2022-01\nBuilt synthetic test systems\nSkills: "
      + (autoSubmit ? "Rust, TypeScript, PostgreSQL" : "TypeScript") + "\n");
    const source = await request("/api/jobs/resume-source", "POST", { request_id: randomUUID(),
      file_name: "synthetic-source.txt", media_type: "text/plain", bytes_base64: sourceBytes.toString("base64") });
    assert.equal(source.asset.sha256, hash(sourceBytes), "source_hash");
    await objectStore.assertStoredBytes(sourceBytes);
    workspace = await request("/api/jobs/workspace");
    const identityRow = workspace.application_identities.find((item) => item.verification_status === "verified");
    assert.ok(identityRow, "verified_synthetic_identity");
    const preferences = { ...workspace.preferences, desired_roles: ["Software Engineer"], desired_locations: ["New York, NY"],
      employment_types: ["full_time"], engagement_types: ["w2"], remote_preference: "any", apply_once_per_company: true };
    const taxonomy = await request("/api/jobs/taxonomy");
    const taxonomyHeaders = { "x-bluey-jobs-taxonomy-version": taxonomy.taxonomyVersion,
      "x-bluey-jobs-taxonomy-sha256": taxonomy.taxonomySha256 };
    const track = { id: randomUUID(), name: "Software Engineer", role: "Software Engineer", locations: ["New York, NY"],
      remote_preference: "any", application_identity_id: identityRow.id, policy: { role_family: "",
        relevant_employment_ids: [profile.employment[0].id], employment_types: ["full_time"], engagement_types: ["w2"],
        work_authorizations: [] }, active: true, match_count: 0, created_at_ms: 0, updated_at_ms: 0 };
    stage = "onboarding";
    workspace = await request("/api/jobs/onboarding/complete", "POST", { profile: workspace.profile, preferences, track }, taxonomyHeaders);
    const savedTrack = workspace.tracks.find((item) => item.id === track.id);
    assert.ok(savedTrack && savedTrack.policy.authority.review_state === "approved", "ordinary_track_policy_approved");
    stage = "bootstrap";
    const bootstrap = await fixture(autoSubmit ? "bootstrap-auto-submit" : "bootstrap", { version: 1,
      operation: autoSubmit ? "bootstrap_auto_submit" : "bootstrap", account_id: accountId,
      track_id: savedTrack.id, worker_id: workerId });
    exactRecord(bootstrap, ["version", "operation", "account_id", "track_id", "job_id", "managed_release_fixture_version",
      "scope", "volume", "runtimes"], "bootstrap_shape");
    assert.equal(bootstrap.version, 1, "bootstrap_version");
    assert.equal(bootstrap.operation, autoSubmit ? "bootstrap_auto_submit" : "bootstrap", "bootstrap_operation");
    assert.equal(bootstrap.account_id, accountId, "bootstrap_account");
    assert.equal(bootstrap.track_id, savedTrack.id, "bootstrap_track");
    assert.equal(bootstrap.managed_release_fixture_version, 2, "bootstrap_fixture_version");
    assert.deepEqual(bootstrap.scope, { environment: "staging", region: "us-east-1", channel: "general" }, "bootstrap_scope");
    exactRecord(bootstrap.volume, ["volume_id", "enrollment_epoch", "process_instance_id", "key_fingerprint",
      "runtime_grant_id", "runtime_sha256"], "bootstrap_volume_shape");
    assert.match(bootstrap.volume.volume_id, /^[A-Za-z0-9._:-]{1,240}$/, "bootstrap_volume_id");
    assert.ok(Number.isSafeInteger(bootstrap.volume.enrollment_epoch) && bootstrap.volume.enrollment_epoch >= 1,
      "bootstrap_volume_epoch");
    assert.equal(bootstrap.runtimes.length, 7, "bootstrap_runtime_count");
    for (const entry of bootstrap.runtimes) exactRecord(entry, ["instance", "session_token"], "runtime_entry_shape");
    let authorization;
    if (autoSubmit) {
      stage = "authorize_auto_submit";
      authorization = await request(`/api/jobs/tracks/${encodeURIComponent(savedTrack.id)}/auto-submit`, "POST");
      assert.equal(authorization.status, "active", "ordinary_auto_authorization_active");
      assert.equal(authorization.revoked_at_ms, null, "ordinary_auto_authorization_not_revoked");
      assert.equal(authorization.career_track_id, savedTrack.id, "ordinary_auto_authorization_track");
      assert.equal(authorization.application_identity_id, identityRow.id, "ordinary_auto_authorization_identity");
      assert.equal(authorization.source_resume_asset_id, source.asset.id, "ordinary_auto_authorization_source");
      assert.ok(Number.isSafeInteger(authorization.revision_no) && authorization.revision_no > 0, "ordinary_auto_authorization_revision");
    }
    stage = "prepare";
    const packet = await request("/api/jobs/applications", "POST", { job_id: bootstrap.job_id, mode: "factual",
      submission_mode: autoSubmit ? "auto_submit" : "review_first" });
    const applicationId = packet.application.id;
    if (autoSubmit) {
      assert.equal(packet.application.submission_mode, "auto_submit", "ordinary_auto_packet_mode");
      assert.equal(packet.application.state, "queued", "ordinary_auto_packet_queued");
      assert.ok(packet.application.match_score >= 80, "ordinary_auto_packet_default_threshold");
      const approved = packet.application.receipt.approved_execution;
      assert.equal(approved.schema_version, 3, "ordinary_auto_packet_schema");
      assert.equal(approved.admission.kind, "track_auto_submit", "ordinary_auto_packet_admission");
      assert.equal(approved.admission.authorization_id, authorization.id, "ordinary_auto_packet_authorization");
      assert.equal(approved.admission.revision_no, authorization.revision_no, "ordinary_auto_packet_revision");
      assert.match(approved.admission.authority_fingerprint, /^[a-f0-9]{64}$/, "ordinary_auto_packet_fingerprint");
      assert.ok(approved.admission.ats_certification && typeof approved.admission.ats_certification === "object", "ordinary_auto_packet_ats");
      report.autoAuthorization = { active: authorization.status === "active", revision: authorization.revision_no,
        identityAndSourceBound: authorization.application_identity_id === identityRow.id && authorization.source_resume_asset_id === source.asset.id,
        admissionBound: approved.admission.authorization_id === authorization.id && approved.admission.revision_no === authorization.revision_no,
        noManualApproval: approvalRequests === 0 };
    } else {
      assert.equal(packet.application.state, "awaiting_review", "ordinary_packet_review");
      stage = "approve";
      const approved = await request(`/api/jobs/applications/${encodeURIComponent(applicationId)}/approve`, "POST", {});
      assert.equal(approved.application.state, "queued", "ordinary_packet_queued");
    }
    stage = "queue";
    const queuePath = `/api/jobs/applications/${encodeURIComponent(applicationId)}/runs`;
    const queued = await request(queuePath, "POST", { runner: "cloud" });
    stage = "queue_identity";
    assert.match(queued.run_id, /^[a-f0-9]{40}$/, "ordinary_run_id");
    assert.equal(queued.browser_session.id, `cloud-${applicationId}`, "ordinary_session_id");
    assert.notEqual(queued.run_id, queued.browser_session.id, "distinct_run_session"); report.distinctIds = true;
    stage = "queue_replay";
    const repeated = await request(queuePath, "POST", { runner: "cloud" });
    assert.equal(repeated.run_id, queued.run_id, "queue_replay_run");
    assert.equal(repeated.browser_session.id, queued.browser_session.id, "queue_replay_session");
    assert.equal(repeated.workflow_command.command_id, queued.workflow_command.command_id, "queue_replay_command");
    assert.equal(repeated.workflow_command.request_id, queued.workflow_command.request_id, "queue_replay_request");
    assert.equal(repeated.workflow_command.replayed, true, "queue_replayed"); report.queueReplay = true;
    stage = "queue_counts";
    const queueCounts = JSON.parse(await sql(`SELECT json_build_object(
      'commands',(SELECT count(*) FROM jobs_workflow_commands WHERE account_id='${accountId}'),
      'sessions',(SELECT count(*) FROM jobs_browser_sessions WHERE account_id='${accountId}'),
      'attempts',(SELECT count(*) FROM jobs_attempt_reservations WHERE account_id='${accountId}'),
      'leases',(SELECT count(*) FROM jobs_execution_leases WHERE account_id='${accountId}'),
      'run_alias',(SELECT count(*) FROM jobs_browser_sessions WHERE account_id='${accountId}' AND id='${queued.run_id}'),
      'submitted',(SELECT count(*) FROM jobs_applications WHERE account_id='${accountId}' AND submitted_at_ms IS NOT NULL))::text`));
    assert.deepEqual(queueCounts, { commands: 1, sessions: 1, attempts: 1, leases: 0, run_alias: 0, submitted: 0 },
      "ordinary_queue_single_durable_authority");
    stage = "queue_delivery";
    const gatewayDeadline = Date.now() + OPERATION_MS;
    while (!observeRequests.length && Date.now() < gatewayDeadline) await delay(50);
    assert.equal(observerError, undefined, "gateway_observer_clean");
    assert.equal(observeRequests.length, 1, "actual_gateway_post");
    assert.equal(observeRequests[0].requestId, queued.workflow_command.request_id, "gateway_request_binding");
    assert.equal(observeRequests[0].workflowId, queued.workflow_id, "gateway_workflow_binding");
    assert.match(queued.workflow_command.request_id, /^wfreq-v2-[a-f0-9-]{36}$/, "durable_request_id");
    let durableState;
    do {
      durableState = await sql(`SELECT state FROM jobs_workflow_commands WHERE account_id='${accountId}' AND request_id='${queued.workflow_command.request_id}'`);
      if (durableState === "delivery_unknown") break;
      assert.ok(["pending", "claimed", "delivering"].includes(durableState), "dispatcher_intermediate_state");
      if (Date.now() >= gatewayDeadline) fail("dispatcher_completion_timeout");
      await delay(50);
    } while (true);
    const digest24 = (value) => hash(value).slice(0, 24);
    const browserProfileId = `${digest24(accountId)}:${digest24(identityRow.id)}`;
    stage = "post_queue";
    const post = await fixture(autoSubmit ? "post-queue-auto-submit" : "post-queue", { version: 1,
      operation: autoSubmit ? "post_queue_auto_submit" : "post_queue", account_id: accountId,
      application_id: applicationId, run_id: queued.run_id, browser_session_id: queued.browser_session.id,
      browser_profile_id: browserProfileId, command_id: queued.workflow_command.command_id,
      request_id: queued.workflow_command.request_id, workflow_id: queued.workflow_id });
    exactRecord(post, ["version", "operation", "account_id", "application_id", "run_id", "browser_session_id", "browser_profile_id",
      "command_id", "request_id", "workflow_id", "submission_mode", "managed_cloud_release", ...(autoSubmit ? ["auto_summary"] : [])], "post_queue_shape");
    assert.equal(post.submission_mode, autoSubmit ? "auto_submit" : "review_first", "post_queue_persisted_submission_mode");
    for (const [field, value] of Object.entries({ version: 1, operation: autoSubmit ? "post_queue_auto_submit" : "post_queue", account_id: accountId,
      application_id: applicationId, run_id: queued.run_id, browser_session_id: queued.browser_session.id,
      browser_profile_id: browserProfileId, command_id: queued.workflow_command.command_id,
      request_id: queued.workflow_command.request_id, workflow_id: queued.workflow_id })) {
      assert.equal(post[field], value, "post_queue_binding");
    }
    if (autoSubmit) {
      exactRecord(post.auto_summary, ["attempt_id", "auto_authorization_id", "auto_authorization_revision",
        "auto_authorization_fingerprint_sha256", "packet_checksum_sha256", "provider", "target_key_sha256", "manifest_sha256",
        "activation_sha256", "layout_set_sha256", "adapter_bundle_sha256", "runner_target_sha256"], "auto_handoff_summary_shape");
      assert.equal(post.auto_summary.auto_authorization_id, authorization.id, "auto_handoff_authorization");
      assert.equal(post.auto_summary.auto_authorization_revision, authorization.revision_no, "auto_handoff_revision");
      assert.equal(post.auto_summary.auto_authorization_fingerprint_sha256,
        packet.application.receipt.approved_execution.admission.authority_fingerprint, "auto_handoff_fingerprint");
      assert.equal(post.auto_summary.packet_checksum_sha256, packet.application.receipt.approved_execution.checksum, "auto_handoff_packet");
      assert.equal(post.auto_summary.runner_target_sha256, bootstrap.volume.runtime_sha256, "auto_handoff_runtime");
    }
    const key = await privateJson(join(root, autoSubmit ? "runner-control-plane-auto-submit-volume-key.json" : "runner-control-plane-volume-key.json"), 4096);
    exactRecord(key, ["version", "worker_id", "volume", "public_key_base64url", "signing_seed_base64url"], "key_shape");
    assert.deepEqual(key.volume, bootstrap.volume, "volume_result_binding");
    assert.notEqual(key.signing_seed_base64url, workerSigningKey, "separate_test_keys");
    const runtime = bootstrap.runtimes.find((entry) => entry.instance.role === "managed_runner")?.instance;
    assert.ok(runtime && runtime.workerId === workerId, "managed_runner_descriptor");
    async function snapshot(tables = claimTables) {
      return sql(tables.map((table) => `SELECT '${table}',count(*),md5(coalesce(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM ${table} t`).join(" UNION ALL "));
    }
    const before = await snapshot();
    const immutableBefore = causalRed ? null : await snapshot(PREPARED_IMMUTABLE_TABLES);
    const recoveryImmutableBefore = recoveryGreen ? await snapshot(RECOVERY_IMMUTABLE_TABLES) : null;
    const attemptBinding = () => sql(`SELECT coalesce(json_agg(s ORDER BY s.id),'[]'::json)::text FROM
      (SELECT id,account_id,application_id,runner,${recoveryGreen ? "" : "status,"}company_key,period_key,canonical_employer_id,
        canonical_employer_domain,employer_attestation_sha256,reserved_at_ms
        FROM jobs_attempt_reservations WHERE account_id='${accountId}') s`);
    const attemptBefore = causalRed ? null : await attemptBinding();
    stage = "claim_readiness";
    // Ordinary queue replay rechecks production all-role readiness. Workspace
    // reads can schedule background discovery writes, so are not used here.
    const freshReplay = await request(queuePath, "POST", { runner: "cloud" });
    assert.equal(freshReplay.run_id, queued.run_id, "fresh_readiness_run");
    assert.equal(freshReplay.browser_session.id, queued.browser_session.id, "fresh_readiness_session");
    assert.equal(freshReplay.workflow_command.command_id, queued.workflow_command.command_id, "fresh_readiness_command");
    assert.equal(freshReplay.workflow_command.request_id, queued.workflow_command.request_id, "fresh_readiness_request");
    assert.equal(freshReplay.workflow_command.replayed, true, "fresh_readiness_replayed");
    assert.equal(await snapshot(), before, "readiness_replay_changed_protected_state");
    report.claimReadinessConfirmed = true;
    let claimRequests = 0; let responseHash;
    const clientRequests = [];
    const client = createFixtureLeaseClient({ origin, workerSigningKey, key, runtime,
      fetch: async (url, options) => {
        const path = new URL(url).pathname;
        const allowed = ["/api/jobs/internal/execution-leases/claim", ...(!causalRed
          ? [`/api/jobs/internal/execution-leases/${queued.run_id}/heartbeat`,
            `/api/jobs/internal/execution-leases/${queued.run_id}/finish`,
            ...(recoveryRed || recoveryGreen ? [`/api/jobs/internal/execution-leases/${queued.run_id}/reconcile-checkpoint`] : [])] : [])];
        assert.ok(allowed.includes(path), "protocol_only_client_path");
        if (path.endsWith("/claim")) claimRequests += 1;
        const body = JSON.parse(options.body);
        if (path.endsWith("/finish")) assert.equal(body.outcome, "released", "safe_release_only");
        const startedAt = Date.now();
        requestObservation = { method: "POST", status: null, elapsedMs: 0 };
        let response;
        try { response = await localFetch(url, options); requestObservation.status = response.status; }
        finally { requestObservation.elapsedMs = Date.now() - startedAt; }
        const text = await responseText(response, causalRed ? 256 : 16 * 1024);
        if (!response.ok) requestObservation.response = httpResponseDiagnostic(response.status, text);
        if (causalRed) { assert.equal(response.status, 404, "red_target_status"); responseHash = hash(text); }
        clientRequests.push({ path, body, status: response.status,
          nonce: new Headers(options.headers).get("x-bluey-jobs-worker-nonce"), responseSha256: response.ok ? null : hash(text) });
        if (response.status === 204) return new Response(null, { status: 204, headers: response.headers });
        return new Response(text, { status: response.status, headers: response.headers });
      } });
    stage = "real_client_claim";
    const claimInput = { accountId, applicationId, runId: queued.run_id, browserProfileId,
      workflowRequestId: post.request_id, managedCloudRelease: post.managed_cloud_release };
    if (causalRed) {
      let rejection;
      try { activeLease = await client.claim(claimInput); await activeLease.finish("released"); fail("target_missing_red_not_observed"); }
      catch (error) { rejection = error; }
      assert.equal(rejection?.operation, "claim", "real_client_error_operation");
      assert.equal(rejection?.code, "request_failed", "real_client_error_code");
      assert.equal(rejection?.status, 404, "real_client_error_status");
      // Worker-auth nonce reservation is intentional and is not in this contract.
      assert.equal(await snapshot(), before, "protected_claim_state_changed"); report.protectedClaimStateUnchanged = true;
      report.claim = { operation: rejection.operation, code: rejection.code, status: rejection.status,
        httpRequests: claimRequests, responseSha256: responseHash };
    } else {
      activeLease = await client.claim(claimInput);
      const metadata = activeLease.checkpointMetadata();
      const leaseReadback = async () => JSON.parse(await sql(`SELECT row_to_json(s)::text FROM
        (SELECT account_id,application_id,run_id,browser_profile_id,owner_id,lease_token_sha256,fence,phase,
          lease_expires_at_ms,updated_at_ms,finished_at_ms FROM jobs_execution_leases WHERE run_id='${queued.run_id}') s`));
      const prepared = await leaseReadback();
      assert.equal(claimRequests, 1, "one_prepared_claim");
      assert.equal(prepared.account_id, accountId, "lease_account");
      assert.equal(prepared.application_id, applicationId, "lease_application");
      assert.equal(prepared.run_id, queued.run_id, "lease_run");
      assert.equal(prepared.browser_profile_id, browserProfileId, "lease_profile");
      assert.equal(prepared.owner_id, workerId, "lease_worker");
      assert.equal(prepared.lease_token_sha256, hash(metadata.leaseToken), "lease_token_binding");
      assert.equal(prepared.fence, metadata.fence, "lease_fence_binding");
      assert.equal(prepared.phase, "prepared", "lease_prepared");
      // Bind the fixture's production-decrypted mode to unchanged stored application bytes.
      // application_json is ciphertext, not a PostgreSQL JSON document.
      assert.equal(await snapshot(PREPARED_IMMUTABLE_TABLES), immutableBefore, "prepared_immutable_mode_binding");
      const bindings = JSON.parse(await sql(`SELECT json_build_object(
        'lease_count',(SELECT count(*) FROM jobs_execution_leases WHERE account_id='${accountId}'),
        'volume_count',(SELECT count(*) FROM jobs_execution_lease_volume_bindings WHERE run_id='${queued.run_id}'),
        'exact_volume_count',(SELECT count(*) FROM jobs_execution_lease_volume_bindings WHERE run_id='${queued.run_id}'
          AND volume_id='${bootstrap.volume.volume_id}' AND volume_epoch=${bootstrap.volume.enrollment_epoch}
          AND process_instance_id='${bootstrap.volume.process_instance_id}'),
        'ats_count',(SELECT count(*) FROM jobs_application_ats_certification_bindings WHERE account_id='${accountId}'),
        'ats_application_count',(SELECT count(*) FROM jobs_application_ats_certification_bindings WHERE application_id='${applicationId}'),
        'ats_run_count',(SELECT count(*) FROM jobs_application_ats_certification_bindings WHERE run_id='${queued.run_id}'),
        'run_alias',(SELECT count(*) FROM jobs_browser_sessions WHERE account_id='${accountId}' AND id='${queued.run_id}'))::text`));
      assert.equal(bindings.lease_count, 1, "prepared_lease_count");
      assert.equal(bindings.volume_count, 1, "prepared_volume_count");
      assert.equal(bindings.exact_volume_count, 1, "prepared_exact_volume_count");
      assert.equal(bindings.ats_count, autoSubmit ? 1 : 0, "prepared_account_ats_count");
      assert.equal(bindings.ats_application_count, autoSubmit ? 1 : 0, "prepared_application_ats_count");
      assert.equal(bindings.ats_run_count, autoSubmit ? 1 : 0, "prepared_run_ats_count");
      assert.equal(bindings.run_alias, 0, "prepared_no_run_alias");
      report.claim = { httpRequests: claimRequests, status: clientRequests[0].status, phase: prepared.phase,
        fence: prepared.fence, submissionMode: post.submission_mode, leaseCount: bindings.lease_count,
        volumeBindingCount: bindings.volume_count, exactVolumeBindingCount: bindings.exact_volume_count,
        atsBindingCount: bindings.ats_count,
        atsApplicationBindingCount: bindings.ats_application_count, atsRunBindingCount: bindings.ats_run_count,
        runAliasCount: bindings.run_alias };
      const autoEffects = async () => JSON.parse(await sql(`SELECT json_build_object(
        'capacity',(SELECT count(*) FROM jobs_submission_evidence_capacity WHERE account_id='${accountId}'),
        'canaryReservations',(SELECT count(*) FROM jobs_ats_certification_canary_reservations WHERE account_id='${accountId}'),
        'irreversibleReceipts',(SELECT count(*) FROM jobs_managed_cloud_irreversible_effect_receipts WHERE account_id='${accountId}'))::text`));
      const atsReadback = () => sql(`SELECT coalesce(json_agg(s ORDER BY s.binding_id),'[]'::json)::text FROM
        (SELECT *
           FROM jobs_application_ats_certification_bindings WHERE account_id='${accountId}') s`);
      let inspectedAts;
      if (autoSubmit) {
        stage = "inspect_auto_submit_claim";
        const inspection = await fixture("inspect-auto-submit-claim", { version: 1, operation: "inspect_auto_submit_claim",
          account_id: accountId, application_id: applicationId, run_id: queued.run_id,
          browser_session_id: queued.browser_session.id, browser_profile_id: browserProfileId,
          lease_token_sha256: hash(metadata.leaseToken), lease_fence: metadata.fence });
        exactRecord(inspection, ["version", "operation", "account_id", "application_id", "run_id", "browser_session_id",
          "browser_profile_id", "lease_token_sha256", "lease_fence", "ats_account_count", "ats_application_count", "ats_run_count",
          "binding_sha256", "nonce_sha256", "phase", "binding_fence", "expires_future", "exact_frozen_authority",
          "exact_prepared_lease", "no_phase_b_or_invalidation"], "auto_inspection_shape");
        for (const [field, value] of Object.entries({ version: 1, operation: "inspect_auto_submit_claim", account_id: accountId,
          application_id: applicationId, run_id: queued.run_id, browser_session_id: queued.browser_session.id,
          browser_profile_id: browserProfileId, lease_token_sha256: hash(metadata.leaseToken), lease_fence: metadata.fence,
          ats_account_count: 1, ats_application_count: 1, ats_run_count: 1, phase: "preflight", binding_fence: 0,
          expires_future: true, exact_frozen_authority: true, exact_prepared_lease: true, no_phase_b_or_invalidation: true })) {
          assert.equal(inspection[field], value, "auto_inspection_binding");
        }
        assert.equal(inspection.nonce_sha256, hash(metadata.leaseToken), "auto_inspection_nonce");
        assert.match(inspection.binding_sha256, /^[a-f0-9]{64}$/, "auto_inspection_digest");
        report.ats = { phase: inspection.phase, fence: inspection.binding_fence, bindingSha256: inspection.binding_sha256,
          nonceBound: inspection.nonce_sha256 === hash(metadata.leaseToken), expiresFuture: inspection.expires_future,
          exactFrozenAuthority: inspection.exact_frozen_authority, exactPreparedLease: inspection.exact_prepared_lease,
          noPhaseBOrInvalidation: inspection.no_phase_b_or_invalidation, unchangedAfterRepeat: false };
        inspectedAts = await atsReadback();
        assert.deepEqual(await autoEffects(), { capacity: 0, canaryReservations: 0, irreversibleReceipts: 0 }, "auto_initial_no_effect_authority");
      }
      stage = "observed_heartbeat";
      const heartbeatDeadline = Date.now() + 10_000;
      let heartbeaten;
      do {
        await delay(100); heartbeaten = await leaseReadback();
        if (Date.now() > heartbeatDeadline) fail("heartbeat_not_observed");
      } while (heartbeaten.updated_at_ms <= prepared.updated_at_ms);
      await activeLease.stopHeartbeat();
      assert.equal(activeLease.heartbeatFailureCode, undefined, "heartbeat_client_success");
      assert.equal(heartbeaten.lease_token_sha256, prepared.lease_token_sha256, "heartbeat_retains_token");
      assert.equal(heartbeaten.fence, prepared.fence, "heartbeat_retains_fence");
      assert.equal(heartbeaten.phase, "prepared", "heartbeat_no_irreversible");
      assert.ok(heartbeaten.lease_expires_at_ms > prepared.lease_expires_at_ms, "heartbeat_extends_lease");
      report.heartbeat = { httpRequests: clientRequests.filter((item) => item.path.endsWith("/heartbeat") && item.status === 200).length,
        observedUpdate: true, sameTokenAndFence: true, phase: heartbeaten.phase };
      if (protocolNegatives) {
        // All cases run against this one ordinarily admitted prepared lease. No trusted
        // rows are seeded or changed to arrange a denial, and no heartbeat races a snapshot.
        const originalClaim = clientRequests[0].body;
        const claimPath = "/api/jobs/internal/execution-leases/claim";
        const recoveryPath = `/api/jobs/internal/execution-leases/${queued.run_id}/reconcile-checkpoint`;
        const nonces = new Set(clientRequests.map((item) => item.nonce));
        const proofIds = new Set([originalClaim.volume_proof.requestId]);
        const cases = [];
        let replayRequest;
        const different = (value) => `${value[0] === "a" ? "b" : "a"}${value.slice(1)}`;
        const without = (body, fields) => Object.fromEntries(Object.entries(body).filter(([field]) => !fields.includes(field)));
        const recoveryInput = { accountId, applicationId, runId: queued.run_id, ownerId: workerId,
          leaseToken: metadata.leaseToken, fence: metadata.fence, checkpointVersion: 2, checkpointPhase: "prepared" };
        const recoveryBody = { account_id: accountId, application_id: applicationId, owner_id: workerId,
          fence: metadata.fence, lease_token: metadata.leaseToken, checkpoint_version: 2, checkpoint_phase: "prepared" };
        for (const spec of PREPARED_NEGATIVE_CASES) {
          stage = `negative_${spec.id}`;
          const beforeCase = await snapshot();
          const observed = [];
          const capture = async (url, options) => {
            const parsed = new URL(url);
            assert.equal(parsed.origin, origin, "negative_exact_origin");
            assert.ok([claimPath, recoveryPath,
              `/api/jobs/internal/execution-leases/${queued.run_id}/heartbeat`,
              `/api/jobs/internal/execution-leases/${queued.run_id}/finish`].includes(parsed.pathname)
              && !parsed.search && !parsed.hash, "negative_protocol_only_path");
            assert.equal(parsed.pathname, spec.operation === "claim" ? claimPath : spec.operation === "recovery" ? recoveryPath
              : `/api/jobs/internal/execution-leases/${queued.run_id}/${spec.operation}`, "negative_exact_operation_before_http");
            assert.equal(options.method, "POST", "negative_post_only");
            assert.equal(typeof options.body, "string", "negative_string_body");
            assert.equal(observed.length, 0, "negative_no_second_http_request");
            if (spec.operation === "finish") assert.equal(JSON.parse(options.body).outcome, "released", "negative_safe_release_only");
            const headers = new Headers(options.headers);
            requestObservation = { method: "POST", status: null, elapsedMs: 0 };
            const started = Date.now();
            let response;
            try { response = await localFetch(url, options); requestObservation.status = response.status; }
            finally { requestObservation.elapsedMs = Date.now() - started; }
            const text = await responseText(response, 16 * 1024);
            if (!response.ok) requestObservation.response = httpResponseDiagnostic(response.status, text);
            observed.push({ path: parsed.pathname, body: JSON.parse(options.body), headers,
              status: response.status, responseSha256: hash(text) });
            return new Response(response.status === 204 ? null : text, { status: response.status, headers: response.headers });
          };
          let rejection;
          if (spec.via === "client") {
            const fault = { proof_bad_signature: "proof_signature", proof_bad_payload: "proof_payload",
              runtime_grant: "runtime_grant", runtime_hash: "runtime_hash", runtime_instance: "runtime_instance",
              wrong_worker: "wrong_worker" }[spec.id];
            const negativeClient = createFixtureLeaseClient({ origin, workerSigningKey, key, runtime, fetch: capture, claimFault: fault });
            try {
              if (spec.operation === "claim") {
                const input = { ...claimInput };
                if (spec.id === "wrong_application") input.applicationId = different(input.applicationId);
                if (spec.id === "wrong_run") input.runId = different(input.runId);
                if (spec.id === "wrong_profile") input.browserProfileId = different(input.browserProfileId);
                const unexpected = await negativeClient.claim(input);
                // A surprising success is a failure, not denial evidence; stop its timer
                // before unwinding. The physically owned private cluster is then destroyed.
                await unexpected.stopHeartbeat();
              } else {
                const input = { ...recoveryInput };
                if (spec.id === "recovery_token") input.leaseToken = different(input.leaseToken);
                if (spec.id === "recovery_fence") input.fence += 1;
                if (spec.id === "recovery_owner") input.ownerId = different(input.ownerId);
                await negativeClient.reconcileCheckpoint(input);
              }
            } catch (error) { rejection = error; }
            assert.equal(rejection?.operation, spec.operation, "negative_actual_client_operation");
            assert.equal(rejection?.code, spec.clientCode, "negative_actual_client_code");
            assert.equal(rejection?.status, spec.status, "negative_actual_client_status");
          } else {
            let path = claimPath;
            let body = structuredClone(originalClaim);
            let headers;
            if (spec.id === "worker_nonce_replay") {
              assert.ok(replayRequest, "negative_replay_request_exists");
              ({ path, body, headers } = replayRequest);
            } else {
              if (spec.id === "owner_mismatch") body.owner_id = different(workerId);
              if (spec.id === "proof_volume_mismatch") body.volume_id = different(body.volume_id);
              if (spec.id === "proof_process_mismatch") body.process_instance_id = different(body.process_instance_id);
              if (spec.id === "proof_invalid_version") body.volume_proof.version = 2;
              if (["heartbeat", "finish"].includes(spec.operation)) {
                path = `/api/jobs/internal/execution-leases/${queued.run_id}/${spec.operation}`;
                body = { account_id: accountId, application_id: applicationId,
                  lease_token: metadata.leaseToken, fence: metadata.fence,
                  ...(spec.operation === "finish" ? { outcome: "released" } : {}) };
                if (spec.id.endsWith("_token")) body.lease_token = different(body.lease_token);
                else body.fence += 1;
              }
              headers = spec.id === "bearer_only" ? { authorization: `Bearer ${token}` }
                : fixtureWorkerAuthHeaders({ workerSigningKey, workerId, path, body: JSON.stringify(body) });
              if (spec.id === "owner_mismatch") replayRequest = { path, body: structuredClone(body), headers: { ...headers } };
            }
            const serialized = JSON.stringify(body) + (spec.id === "signed_body_changed" ? " " : "");
            if (spec.id === "signed_body_changed") {
              assert.notEqual(hash(serialized), headers["x-bluey-jobs-worker-content-sha256"], "negative_body_actually_changed");
            }
            const response = await capture(origin + path, { method: "POST",
              headers: { ...headers, "content-type": "application/json" }, body: serialized });
            await response.body?.cancel();
          }
          assert.equal(observed.length, 1, "negative_one_actual_http_request");
          const wire = observed[0];
          assert.equal(wire.status, spec.status, "negative_exact_http_status");
          assert.equal(wire.responseSha256, spec.responseSha256, "negative_exact_http_body");
          assert.equal(wire.path, spec.operation === "claim" ? claimPath : spec.operation === "recovery" ? recoveryPath
            : `/api/jobs/internal/execution-leases/${queued.run_id}/${spec.operation}`, "negative_operation_path");
          const nonce = wire.headers.get("x-bluey-jobs-worker-nonce");
          const nonceFresh = spec.noncePolicy === "absent" ? null : !nonces.has(nonce);
          if (spec.noncePolicy === "absent") assert.equal(nonce, null, "negative_bearer_has_no_nonce");
          else {
            assert.match(nonce, /^[a-f0-9]{32}$/, "negative_actual_nonce");
            assert.equal(nonceFresh, spec.noncePolicy === "fresh", "negative_expected_nonce_freshness");
            if (spec.noncePolicy === "replay") {
              assert.equal(nonce, replayRequest.headers["x-bluey-jobs-worker-nonce"], "negative_exact_replayed_nonce");
              assert.deepEqual(wire.body, replayRequest.body, "negative_exact_replayed_body");
              for (const [field, value] of Object.entries(replayRequest.headers)) assert.equal(wire.headers.get(field), value, "negative_exact_replayed_headers");
            }
            nonces.add(nonce);
          }
          let proofFresh = null;
          if (spec.operation === "claim") {
            const alterations = { owner_mismatch: "owner_id", worker_nonce_replay: "owner_id",
              proof_volume_mismatch: "volume_id", proof_process_mismatch: "process_instance_id",
              wrong_application: "application_id", wrong_run: "run_id", wrong_profile: "browser_profile_id",
              runtime_grant: "runtime_grant_id", runtime_hash: "runtime_sha256",
              runtime_instance: "managed_cloud_runtime_instance_id", wrong_worker: "owner_id" };
            const field = alterations[spec.id];
            assert.deepEqual(without(wire.body, ["volume_proof", ...(field ? [field] : [])]),
              without(originalClaim, ["volume_proof", ...(field ? [field] : [])]), "negative_claim_other_fields_bound");
            if (field) assert.notEqual(wire.body[field], originalClaim[field], "negative_claim_intended_field_changed");
            const sentWorker = wire.headers.get("x-bluey-jobs-worker-id");
            if (spec.id === "wrong_worker") {
              assert.notEqual(sentWorker, workerId, "negative_alternate_worker");
              assert.equal(wire.body.owner_id, sentWorker, "negative_alternate_worker_owner_bound");
            } else if (spec.id !== "bearer_only") assert.equal(sentWorker, workerId, "negative_original_worker_bound");
            const proof = wire.body.volume_proof;
            if (spec.proofPolicy === "fresh") {
              proofFresh = !proofIds.has(proof.requestId);
              assert.equal(proofFresh, true, "negative_fresh_proof_request");
              assert.match(proof.requestId, /^rva-[A-Za-z0-9_-]{32}$/, "negative_proof_request_grammar");
              for (const field of ["version", "audience", "operation", "volumeId", "enrollmentEpoch", "processInstanceId"]) {
                assert.equal(proof[field], originalClaim.volume_proof[field], "negative_fresh_proof_fixed_fields");
              }
              proofIds.add(proof.requestId);
            } else {
              const expected = structuredClone(originalClaim.volume_proof);
              if (spec.id === "proof_invalid_version") expected.version = 2;
              assert.deepEqual(proof, expected, "negative_reused_proof_exact");
              if (spec.proofPolicy === "consumed") {
                proofFresh = !proofIds.has(proof.requestId);
                assert.equal(proofFresh, false, "negative_proof_already_consumed");
              }
            }
          } else {
            const field = spec.id.endsWith("_token") ? "lease_token" : spec.id.endsWith("_fence") ? "fence" : "owner_id";
            const expected = spec.operation === "recovery" ? recoveryBody
              : { account_id: accountId, application_id: applicationId, lease_token: metadata.leaseToken, fence: metadata.fence,
                ...(spec.operation === "finish" ? { outcome: "released" } : {}) };
            assert.deepEqual(without(wire.body, [field]), without(expected, [field]), "negative_later_other_fields_bound");
            assert.equal(wire.body[field], field === "fence" ? expected[field] + 1 : different(expected[field]), "negative_later_exact_bad_credential");
            assert.equal(wire.headers.get("x-bluey-jobs-worker-id"), workerId, "negative_later_worker_bound");
          }
          assert.equal(await snapshot(), beforeCase, "negative_twenty_families_unchanged");
          cases.push({ id: spec.id, operation: spec.operation, via: spec.via, status: wire.status,
            responseSha256: wire.responseSha256, clientCode: rejection?.code ?? null, httpRequests: observed.length,
            nonceFresh, proofFresh, inputsBound: true, stateUnchanged: true });
        }
        stage = "negative_retained_heartbeat";
        const prior = await leaseReadback();
        const path = `/api/jobs/internal/execution-leases/${queued.run_id}/heartbeat`;
        const body = JSON.stringify({ account_id: accountId, application_id: applicationId,
          lease_token: metadata.leaseToken, fence: metadata.fence });
        const headers = fixtureWorkerAuthHeaders({ workerSigningKey, workerId, path, body });
        assert.equal(nonces.has(headers["x-bluey-jobs-worker-nonce"]), false, "negative_retained_fresh_nonce");
        const response = await localFetch(origin + path, { method: "POST", headers: { ...headers, "content-type": "application/json" }, body });
        await response.body?.cancel();
        assert.equal(response.status, 200, "negative_retained_heartbeat_status");
        const retained = await leaseReadback();
        for (const field of ["account_id", "application_id", "run_id", "browser_profile_id", "owner_id", "lease_token_sha256", "fence", "phase"]) {
          assert.equal(retained[field], prepared[field], "negative_retained_exact_lease");
        }
        assert.ok(retained.lease_expires_at_ms > prior.lease_expires_at_ms, "negative_retained_lease_extended");
        assert.deepEqual(await autoEffects(), { capacity: 0, canaryReservations: 0, irreversibleReceipts: 0 }, "negative_zero_effect_authority");
        if (autoSubmit) {
          assert.equal(await atsReadback(), inspectedAts, "negative_frozen_ats_unchanged");
          report.ats.unchangedAfterRepeat = true;
        }
        report.negativeMatrix = { cases, deferred: DEFERRED_PROTOCOL_NEGATIVES,
          retainedHeartbeatStatus: response.status, retainedHeartbeatExact: true, effectAuthorityZero: true };
      }
      if (recoveryRed) {
        stage = "checkpoint_session_causal_red";
        const beforeRecovery = await snapshot();
        let rejection;
        try {
          await client.reconcileCheckpoint({ accountId, applicationId, runId: queued.run_id, ownerId: workerId,
            leaseToken: metadata.leaseToken, fence: metadata.fence, checkpointVersion: 2, checkpointPhase: "prepared" });
          fail("checkpoint_session_red_not_observed");
        } catch (error) { rejection = error; }
        assert.equal(rejection?.operation, "recovery", "recovery_real_operation");
        assert.equal(rejection?.code, "request_failed", "recovery_real_code");
        assert.equal(rejection?.status, 404, "recovery_real_status");
        const recoveries = clientRequests.filter((item) => item.path.endsWith("/reconcile-checkpoint"));
        assert.equal(recoveries.length, 1, "recovery_one_real_request");
        const observed = recoveries[0];
        assert.equal(observed.status, 404, "recovery_observed_status");
        assert.equal(observed.responseSha256, hash("Execution lease target not found."), "recovery_exact_response");
        assert.deepEqual(observed.body, { account_id: accountId, application_id: applicationId, owner_id: workerId,
          fence: metadata.fence, lease_token: metadata.leaseToken, checkpoint_version: 2, checkpoint_phase: "prepared" }, "recovery_exact_credentials");
        assert.ok(observed.nonce && clientRequests.filter((item) => item.nonce === observed.nonce).length === 1, "recovery_fresh_nonce");
        assert.equal(await snapshot(), beforeRecovery, "recovery_unchanged_protected_state");
        const prior = await leaseReadback();
        const path = `/api/jobs/internal/execution-leases/${queued.run_id}/heartbeat`;
        const body = JSON.stringify({ account_id: accountId, application_id: applicationId,
          lease_token: metadata.leaseToken, fence: metadata.fence });
        const headers = fixtureWorkerAuthHeaders({ workerSigningKey, workerId, path, body });
        assert.ok(clientRequests.every((item) => item.nonce !== headers["x-bluey-jobs-worker-nonce"]), "recovery_retained_fresh_nonce");
        const response = await localFetch(origin + path, { method: "POST", headers: { ...headers, "content-type": "application/json" }, body });
        await response.body?.cancel();
        assert.equal(response.status, 200, "recovery_retained_heartbeat_status");
        const retained = await leaseReadback();
        assert.equal(retained.lease_token_sha256, hash(metadata.leaseToken), "recovery_retained_token");
        assert.equal(retained.fence, metadata.fence, "recovery_retained_fence");
        assert.equal(retained.phase, "prepared", "recovery_retained_phase");
        assert.ok(retained.lease_expires_at_ms > prior.lease_expires_at_ms, "recovery_retained_expiry_extended");
        report.recovery = { operation: rejection.operation, code: rejection.code, status: rejection.status, httpRequests: recoveries.length,
          responseSha256: observed.responseSha256, checkpointVersion: observed.body.checkpoint_version,
          checkpointPhase: observed.body.checkpoint_phase, exactLeaseCredentials: true, freshTransportNonce: true,
          preHeartbeatConfirmed: report.heartbeat.observedUpdate, protectedStateUnchanged: true,
          retainedHeartbeatStatus: response.status, retainedHeartbeatExact: true, effectAuthorityZero: false };
      }
      let recoveredSnapshot;
      if (recoveryGreen) {
        const inspect = async (suffix, inspectionStage) => {
          const value = await fixture(`checkpoint-${suffix}`, { version: 1, operation: "inspect_checkpoint_recovery",
            stage: inspectionStage, account_id: accountId, application_id: applicationId, run_id: queued.run_id,
            browser_session_id: queued.browser_session.id, browser_profile_id: browserProfileId });
          exactRecord(value, ["version", "operation", "stage", "account_id", "application_id", "run_id", "browser_session_id", "browser_profile_id",
            "submission_mode", "application_stable_sha256", "session_stable_sha256", "application_state", "session_status", "session_current_step",
            "submitted_absent", "takeover_absent", "lease_phase", "attempt_status", "cloud_recovery", "cloud_recovery_sha256"], "checkpoint_inspector_shape");
          for (const [field, expected] of Object.entries({ version: 1, operation: "inspect_checkpoint_recovery", stage: inspectionStage,
            account_id: accountId, application_id: applicationId, run_id: queued.run_id, browser_session_id: queued.browser_session.id,
            browser_profile_id: browserProfileId, submission_mode: post.submission_mode, submitted_absent: true, takeover_absent: true })) {
            assert.equal(value[field], expected, "checkpoint_inspector_binding");
          }
          for (const field of ["application_stable_sha256", "session_stable_sha256"]) assert.match(value[field], /^[a-f0-9]{64}$/, "checkpoint_stable_hash");
          if (suffix === "before") {
            assert.ok(["queued", "running"].includes(value.application_state), "checkpoint_initial_application");
            assert.ok(["queued", "running"].includes(value.session_status), "checkpoint_initial_session");
            assert.equal(value.lease_phase, "prepared", "checkpoint_initial_lease");
            assert.ok(["reserved", "running"].includes(value.attempt_status), "checkpoint_initial_attempt");
            assert.equal(value.cloud_recovery, null, "checkpoint_initial_no_recovery");
            assert.equal(value.cloud_recovery_sha256, null, "checkpoint_initial_no_recovery_hash");
          } else {
            assert.equal(value.application_state, "failed", "checkpoint_recovered_application");
            assert.equal(value.session_status, "failed", "checkpoint_recovered_session");
            assert.equal(value.session_current_step, "Browser run stopped before submission", "checkpoint_recovered_step");
            assert.equal(value.lease_phase, "released", "checkpoint_recovered_lease");
            assert.equal(value.attempt_status, "released", "checkpoint_recovered_attempt");
            exactRecord(value.cloud_recovery, ["schema_version", "status", "checkpoint_version", "checkpoint_phase", "lease_phase", "recorded_at_ms", "run_id"], "checkpoint_cloud_recovery_shape");
            for (const [field, expected] of Object.entries({ schema_version: 1, status: "released", checkpoint_version: 2,
              checkpoint_phase: "prepared", lease_phase: "prepared", run_id: queued.run_id })) assert.equal(value.cloud_recovery[field], expected, "checkpoint_cloud_recovery_binding");
            assert.ok(Number.isSafeInteger(value.cloud_recovery.recorded_at_ms) && value.cloud_recovery.recorded_at_ms > 0, "checkpoint_recorded_at");
            assert.match(value.cloud_recovery_sha256, /^[a-f0-9]{64}$/, "checkpoint_recovery_hash");
          }
          return value;
        };
        stage = "checkpoint_before_inspection";
        const initial = await inspect("before", "before_recovery");
        const stableState = await snapshot(RECOVERY_STABLE_TABLES);
        const leaseStable = () => sql(`SELECT (to_jsonb(t)-'phase'-'updated_at_ms'-'finished_at_ms')::text
          FROM jobs_execution_leases t WHERE run_id='${queued.run_id}' AND account_id='${accountId}' AND application_id='${applicationId}'`);
        const stableLease = await leaseStable();
        const input = { accountId, applicationId, runId: queued.run_id, ownerId: workerId,
          leaseToken: metadata.leaseToken, fence: metadata.fence, checkpointVersion: 2, checkpointPhase: "prepared" };
        stage = "checkpoint_real_recovery";
        await client.reconcileCheckpoint(input);
        const first = clientRequests.filter((item) => item.path.endsWith("/reconcile-checkpoint"));
        assert.equal(first.length, 1, "checkpoint_one_real_recovery");
        assert.equal(first[0].status, 200, "checkpoint_actual_recovery_status");
        assert.deepEqual(first[0].body, { account_id: accountId, application_id: applicationId, owner_id: workerId,
          lease_token: metadata.leaseToken, fence: metadata.fence, checkpoint_version: 2, checkpoint_phase: "prepared" }, "checkpoint_exact_recovery_credentials");
        assert.ok(first[0].nonce && clientRequests.filter((item) => item.nonce === first[0].nonce).length === 1, "checkpoint_fresh_recovery_nonce");
        const released = await leaseReadback();
        assert.equal(released.phase, "released", "checkpoint_observed_released");
        assert.ok(Number.isSafeInteger(released.finished_at_ms), "checkpoint_finished_timestamp");
        assert.equal(await leaseStable(), stableLease, "checkpoint_lease_authority_stable");
        stage = "checkpoint_after_inspection";
        const after = await inspect("after", "after_recovery");
        assert.equal(after.application_stable_sha256, initial.application_stable_sha256, "checkpoint_application_stable");
        assert.equal(after.session_stable_sha256, initial.session_stable_sha256, "checkpoint_session_stable");
        assert.equal(await snapshot(RECOVERY_STABLE_TABLES), stableState, "checkpoint_other_state_stable");
        assert.equal(await attemptBinding(), attemptBefore, "checkpoint_attempt_authority_stable");
        const afterSnapshot = await snapshot();
        stage = "checkpoint_fresh_exact_replay";
        await client.reconcileCheckpoint(input);
        const recoveries = clientRequests.filter((item) => item.path.endsWith("/reconcile-checkpoint"));
        assert.equal(recoveries.length, 2, "checkpoint_two_real_recoveries");
        assert.equal(recoveries[1].status, 200, "checkpoint_replay_status");
        assert.deepEqual(recoveries[1].body, recoveries[0].body, "checkpoint_replay_exact_request");
        assert.notEqual(recoveries[1].nonce, recoveries[0].nonce, "checkpoint_replay_fresh_nonce");
        assert.equal(await snapshot(), afterSnapshot, "checkpoint_replay_unchanged");
        const replay = await inspect("replay", "after_exact_replay");
        assert.deepEqual({ ...replay, stage: after.stage }, after, "checkpoint_replay_typed_state_unchanged");
        assert.equal(await snapshot(), afterSnapshot, "checkpoint_inspector_readonly");
        if (autoSubmit) {
          assert.equal(await atsReadback(), inspectedAts, "checkpoint_ats_unchanged");
          report.ats.unchangedAfterRepeat = true;
        }
        assert.deepEqual(await autoEffects(), { capacity: 0, canaryReservations: 0, irreversibleReceipts: 0 }, "checkpoint_no_effect_authority");
        recoveredSnapshot = await snapshot();
        report.checkpointRecovery = { operation: "recovery", status: first[0].status, httpRequests: 1, checkpointVersion: 2,
          checkpointPhase: "prepared", freshTransportNonce: true, exactLeaseCredentials: true, observedReleased: true,
          canonicalStateExact: true, applicationStableSha256: after.application_stable_sha256,
          sessionStableSha256: after.session_stable_sha256, recoverySha256: after.cloud_recovery_sha256,
          leaseAuthorityUnchanged: true, stableFamilies: RECOVERY_STABLE_TABLES, stableStateUnchanged: true,
          replayHttpRequests: 1, replayStatus: recoveries[1].status, replayFreshNonce: true, replayUnchanged: true,
          terminalFinishUnchanged: false, effectAuthorityZero: true };
      }
      if (autoRepeatRed || autoRepeatGreen) {
        stage = "auto_repeat_readiness";
        const beforeRepeat = await snapshot();
        const repeatReady = await request(queuePath, "POST", { runner: "cloud" });
        assert.equal(repeatReady.run_id, queued.run_id, "auto_repeat_readiness_run");
        assert.equal(repeatReady.browser_session.id, queued.browser_session.id, "auto_repeat_readiness_session");
        assert.equal(repeatReady.workflow_command.command_id, queued.workflow_command.command_id, "auto_repeat_readiness_command");
        assert.equal(repeatReady.workflow_command.request_id, queued.workflow_command.request_id, "auto_repeat_readiness_request");
        assert.equal(repeatReady.workflow_command.replayed, true, "auto_repeat_readiness_replayed");
        assert.equal(await snapshot(), beforeRepeat, "auto_repeat_readiness_unchanged");
        stage = "auto_repeat_claim";
        const stableSubjects = await snapshot(["jobs_runner_account_subjects"]);
        let rejection;
        if (autoRepeatRed) {
        try {
          const unexpected = await client.claim(claimInput);
          activeLease = unexpected;
          await unexpected.stopHeartbeat();
          fail("auto_repeat_conflict_red_not_observed");
        } catch (error) { rejection = error; }
        assert.equal(rejection?.operation, "claim", "auto_repeat_real_operation");
        assert.equal(rejection?.code, "lease_unavailable", "auto_repeat_real_code");
        assert.equal(rejection?.status, 409, "auto_repeat_real_status");
        } else {
          activeLease = await client.claim(claimInput);
          await activeLease.stopHeartbeat();
          assert.equal(activeLease.heartbeatFailureCode, undefined, "auto_green_heartbeat_clean");
        }
        assert.equal(claimRequests, 2, "auto_repeat_exact_second_claim");
        const claims = clientRequests.filter((item) => item.path.endsWith("/claim"));
        assert.equal(claims.length, 2, "auto_repeat_two_actual_requests");
        assert.notEqual(claims[0].body.volume_proof.requestId, claims[1].body.volume_proof.requestId, "auto_repeat_fresh_volume_proof");
        assert.notEqual(claims[0].nonce, claims[1].nonce, "auto_repeat_fresh_transport_nonce");
        const withoutProof = ({ volume_proof: _, ...body }) => body;
        assert.deepEqual(withoutProof(claims[0].body), withoutProof(claims[1].body), "auto_repeat_same_claim_fields");
        if (autoRepeatRed) {
          assert.equal(claims[1].responseSha256, hash("Execution lease is not available."), "auto_repeat_exact_public_response");
          assert.equal(await snapshot(), beforeRepeat, "auto_repeat_failed_claim_unchanged");
        } else assert.equal(claims[1].status, 200, "auto_green_repeat_status");
        assert.equal(await atsReadback(), inspectedAts, "auto_repeat_ats_summary_unchanged");
        report.ats.unchangedAfterRepeat = true;
        const currentMetadata = autoRepeatGreen ? activeLease.checkpointMetadata() : metadata;
        if (autoRepeatGreen) {
          assert.notEqual(currentMetadata.leaseToken, metadata.leaseToken, "auto_green_token_rotated");
          assert.ok(currentMetadata.fence > metadata.fence, "auto_green_fence_increased");
          const rotated = await leaseReadback();
          for (const field of ["account_id", "application_id", "run_id", "browser_profile_id", "owner_id", "phase"]) {
            assert.equal(rotated[field], prepared[field], "auto_green_exact_scope");
          }
          assert.equal(rotated.lease_token_sha256, hash(currentMetadata.leaseToken), "auto_green_token_bound");
          assert.equal(rotated.fence, currentMetadata.fence, "auto_green_fence_bound");
          assert.equal(await snapshot(PREPARED_IMMUTABLE_TABLES), immutableBefore, "auto_green_immutable");
          assert.equal(await attemptBinding(), attemptBefore, "auto_green_attempt");
          assert.equal(await snapshot(["jobs_runner_account_subjects"]), stableSubjects, "auto_green_subjects");
          const bindingCounts = JSON.parse(await sql(`SELECT json_build_object(
            'leases',(SELECT count(*) FROM jobs_execution_leases WHERE account_id='${accountId}'),
            'volumes',(SELECT count(*) FROM jobs_execution_lease_volume_bindings WHERE run_id='${queued.run_id}'),
            'exactVolume',(SELECT count(*) FROM jobs_execution_lease_volume_bindings WHERE run_id='${queued.run_id}'
              AND volume_id='${key.volume.volume_id}' AND volume_epoch=${key.volume.enrollment_epoch}
              AND process_instance_id='${key.volume.process_instance_id}'),
            'runtimes',(SELECT count(*) FROM jobs_execution_lease_process_runtime_bindings WHERE run_id='${queued.run_id}'),
            'exactRuntime',(SELECT count(*) FROM jobs_execution_lease_process_runtime_bindings WHERE run_id='${queued.run_id}'
              AND fence=${currentMetadata.fence} AND worker_id='${workerId}' AND runtime_grant_id='${key.volume.runtime_grant_id}'
              AND runtime_sha256='${key.volume.runtime_sha256}' AND volume_id='${key.volume.volume_id}'
              AND enrollment_epoch=${key.volume.enrollment_epoch} AND process_instance_id='${key.volume.process_instance_id}'))::text`));
          assert.deepEqual(bindingCounts, { leases: 1, volumes: 1, exactVolume: 1, runtimes: 2, exactRuntime: 1 }, "auto_green_exact_bindings");
          const staleSnapshot = await snapshot();
          const usedNonces = new Set(clientRequests.map((item) => item.nonce));
          const staleRequest = async (operation) => {
            const stalePath = `/api/jobs/internal/execution-leases/${queued.run_id}/${operation}`;
            const staleBody = JSON.stringify({ account_id: accountId, application_id: applicationId,
              lease_token: metadata.leaseToken, fence: metadata.fence, ...(operation === "finish" ? { outcome: "released" } : {}) });
            const staleHeaders = fixtureWorkerAuthHeaders({ workerSigningKey, workerId, path: stalePath, body: staleBody });
            const nonce = staleHeaders["x-bluey-jobs-worker-nonce"];
            assert.equal(usedNonces.has(nonce), false, "auto_green_stale_fresh_nonce"); usedNonces.add(nonce);
            const response = await localFetch(origin + stalePath, { method: "POST",
              headers: { ...staleHeaders, "content-type": "application/json" }, body: staleBody });
            await response.body?.cancel();
            assert.equal(response.status, 409, "auto_green_stale_denied");
            assert.equal(await snapshot(), staleSnapshot, "auto_green_stale_no_mutation");
            return response.status;
          };
          stage = "auto_green_stale_credentials";
          const staleHeartbeatStatus = await staleRequest("heartbeat");
          const staleFinishStatus = await staleRequest("finish");
          report.repeat = { operation: "claim", status: claims[1].status, httpRequests: 1,
            freshVolumeProof: true, freshTransportNonce: true, sameClaimFields: true,
            readinessReplayed: repeatReady.workflow_command.replayed, tokenRotated: true,
            previousFence: metadata.fence, fence: currentMetadata.fence, atsUnchanged: true,
            immutableStateUnchanged: true, attemptBindingUnchanged: true, exactLeaseVolumeProcessBinding: true,
            stableAccountSubjects: true, staleHeartbeatStatus, staleFinishStatus, staleStateUnchanged: true,
            currentHeartbeatStatus: null, currentHeartbeatExact: false };
        }
        stage = "auto_repeat_retained_heartbeat";
        const oldLease = await leaseReadback();
        const path = `/api/jobs/internal/execution-leases/${queued.run_id}/heartbeat`;
        const body = JSON.stringify({ account_id: accountId, application_id: applicationId,
          lease_token: currentMetadata.leaseToken, fence: currentMetadata.fence });
        const headers = fixtureWorkerAuthHeaders({ workerSigningKey, workerId, path, body });
        assert.ok(clientRequests.every((item) => item.nonce !== headers["x-bluey-jobs-worker-nonce"]), "auto_retained_heartbeat_fresh_nonce");
        const response = await localFetch(origin + path, { method: "POST", headers: { ...headers, "content-type": "application/json" }, body });
        await response.body?.cancel();
        assert.equal(response.status, 200, "auto_retained_heartbeat_status");
        const retained = await leaseReadback();
        assert.equal(retained.fence, currentMetadata.fence, "auto_retained_heartbeat_fence");
        assert.equal(retained.lease_token_sha256, hash(currentMetadata.leaseToken), "auto_retained_heartbeat_token");
        assert.equal(retained.phase, "prepared", "auto_retained_heartbeat_phase");
        assert.ok(retained.lease_expires_at_ms > oldLease.lease_expires_at_ms, "auto_retained_heartbeat_extended");
        if (autoRepeatGreen) {
          report.repeat.currentHeartbeatStatus = response.status;
          report.repeat.currentHeartbeatExact = true;
        } else report.repeat = { operation: rejection.operation, code: rejection.code, status: rejection.status, httpRequests: 1,
          responseSha256: claims[1].responseSha256, freshVolumeProof: true, freshTransportNonce: true, sameClaimFields: true,
          readinessReplayed: repeatReady.workflow_command.replayed, protectedStateUnchanged: true,
          retainedHeartbeatStatus: response.status, retainedHeartbeatExact: true };
      }
      stage = "safe_release";
      await activeLease.finish("released");
      if (recoveryGreen) {
        // Recovery already performed the release. This is a real terminal finish replay,
        // not a second release mutation or an ActiveExecutionLease cached Promise.
        assert.equal(await snapshot(), recoveredSnapshot, "checkpoint_terminal_finish_unchanged");
        report.checkpointRecovery.terminalFinishUnchanged = true;
      }
      const released = await leaseReadback();
      assert.equal(released.phase, "released", "lease_released");
      assert.ok(Number.isSafeInteger(released.finished_at_ms), "release_finished_time");
      assert.equal(activeLease.finalSubmitAttempted, false, "no_local_submit_attempt");
      const finish = clientRequests.filter((item) => item.path.endsWith("/finish"));
      assert.equal(finish.length, 1, "actual_single_finish_request");
      report.release = { httpRequests: finish.length, status: finish[0].status, phase: released.phase, noIrreversible: true };
      stage = "fresh_terminal_replay";
      const releasedSnapshot = await snapshot();
      const serialized = JSON.stringify(finish[0].body);
      const headers = fixtureWorkerAuthHeaders({ workerSigningKey, workerId, path: finish[0].path, body: serialized });
      assert.notEqual(headers["x-bluey-jobs-worker-nonce"], finish[0].nonce, "fresh_replay_nonce");
      const replay = await localFetch(origin + finish[0].path, { method: "POST",
        headers: { ...headers, "content-type": "application/json" }, body: serialized });
      await replay.body?.cancel();
      assert.equal(replay.status, 204, "fresh_finish_replay_status");
      assert.equal(await snapshot(), releasedSnapshot, "fresh_finish_replay_unchanged");
      report.terminalReplay = { httpRequests: 1, status: replay.status, freshNonce: true, unchanged: true };
      const noEffects = JSON.parse(await sql(`SELECT json_build_object(
        'submitted',(SELECT count(*) FROM jobs_applications WHERE account_id='${accountId}' AND submitted_at_ms IS NOT NULL),
        'evidence',(SELECT count(*) FROM jobs_application_evidence WHERE account_id='${accountId}'),
        'irreversible',(SELECT count(*) FROM jobs_execution_leases WHERE account_id='${accountId}' AND phase='click_started'))::text`));
      assert.deepEqual(noEffects, { submitted: 0, evidence: 0, irreversible: 0 }, "no_employer_effect_state");
      report.immutableStateUnchanged = recoveryGreen
        ? await snapshot(RECOVERY_IMMUTABLE_TABLES) === recoveryImmutableBefore
        : await snapshot(PREPARED_IMMUTABLE_TABLES) === immutableBefore;
      report.attemptBindingUnchanged = await attemptBinding() === attemptBefore;
      assert.equal(report.immutableStateUnchanged, true, "prepared_lifecycle_changed_immutable_state");
      assert.equal(report.attemptBindingUnchanged, true, "prepared_lifecycle_changed_attempt_binding");
      report.effectState = noEffects;
      if (autoSubmit) {
        report.autoEffectState = await autoEffects();
        assert.deepEqual(report.autoEffectState, { capacity: 0, canaryReservations: 0, irreversibleReceipts: 0 }, "auto_final_no_effect_authority");
        assert.equal(await atsReadback(), inspectedAts, "auto_release_preserves_frozen_ats");
      }
      if (recoveryRed) {
        assert.deepEqual(await autoEffects(), { capacity: 0, canaryReservations: 0, irreversibleReceipts: 0 }, "recovery_no_effect_authority");
        report.recovery.effectAuthorityZero = true;
      }
      report.externalEffects = [...Object.values(noEffects), ...Object.values(report.autoEffectState || {})]
        .reduce((sum, value) => sum + value, 0);
    }
    stage = "no_retry_observation";
    while (Date.now() - firstPostAt < 11_000) await delay(100);
    assert.equal(observerError, undefined, "gateway_observer_clean_after_wait");
    report.gatewayPosts = observeRequests.length; report.gatewayObservationMs = Date.now() - firstPostAt;
    assert.equal(report.gatewayPosts, 1, "no_second_effect_delivery");
    assert.equal(interrupted, false, "not_interrupted");
    stage = "cleanup"; await stop();
    if (causalRed) assertCausalRedReport(report); else if (autoRepeatRed) assertAutoSubmitRepeatCausalRedReport(report);
    else if (autoRepeatGreen) assertAutoSubmitPreparedRepeatReport(report);
    else if (recoveryRed) assertCheckpointRecoveryCausalRedReport(report);
    else if (recoveryGreen) assertCheckpointPreparedRecoveryReport(report, autoSubmit ? "auto_submit" : "review_first");
    else if (protocolNegatives) assertPreparedProtocolNegativeReport(report, autoSubmit ? "auto_submit" : "review_first");
    else assertPreparedLifecycleReport(report);
    const scenarioName = causalRed ? "causal-red" : autoRepeatRed ? "auto-submit-repeat-red"
      : autoRepeatGreen ? "auto-submit-prepared-repeat" : recoveryRed ? "checkpoint-session-red"
        : recoveryGreen ? "checkpoint-prepared-recovery" : protocolNegatives ? "prepared-protocol-negatives" : "prepared-lifecycle";
    await writeFile(join(root, `runner-control-plane-${scenarioName}.json`), JSON.stringify(report) + "\n", { flag: "wx", mode: 0o600 });
    process.stdout.write(JSON.stringify({ event: causalRed ? "runner_control_plane_causal_red" : autoRepeatRed
      ? "runner_control_plane_auto_repeat_causal_red" : autoRepeatGreen
        ? "runner_control_plane_auto_prepared_repeat" : recoveryRed ? "runner_control_plane_checkpoint_session_red"
          : recoveryGreen ? "runner_control_plane_checkpoint_prepared_recovery" : protocolNegatives
            ? "runner_control_plane_prepared_protocol_negatives" : "runner_control_plane_prepared_lifecycle",
      status: causalRed || recoveryRed ? 404 : autoRepeatRed ? 409 : 200, sourceAccepted: false, gatewayPosts: report.gatewayPosts, cleanup: true }) + "\n");
    return report;
  } catch (error) {
    let postgresWaits = null;
    try {
      // Only activity metadata from this physically verified private cluster:
      // never query text, bind values, connection strings or candidate content.
      postgresWaits = JSON.parse(await sql("SELECT coalesce(json_agg(s ORDER BY s.pid),'[]'::json)::text FROM (SELECT pid,state,wait_event_type,wait_event,pg_blocking_pids(pid) AS blockers FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() ORDER BY pid LIMIT 32) s", 16 * 1024));
    } catch {}
    let cleanup = true; try { await stop(); } catch { cleanup = false; }
    let postgresError = null;
    try { postgresError = postgresErrorDiagnostic((await privateBytes(join(root, "nonprod-postgres/postgres.log"), 64 * 1024))
      .toString("utf8")); } catch {}
    process.stdout.write(JSON.stringify({ event: "runner_control_plane_setup_or_gate_failed", stage,
      ...closedFailureDiagnostic(error), request: requestObservation, postgresWaits,
      apiPanic: children.find((entry) => entry.name === "jobs_api")?.panicDiagnostic() || null, postgresError, cleanup }) + "\n");
    throw new Error("runner_control_plane_not_qualified");
  } finally {
    for (const name of ["SIGHUP", "SIGINT", "SIGTERM"]) process.off(name, onSignal);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await runControlPlaneReproduction();
}
