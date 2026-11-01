// Test-only report boundary. Importing this module cannot start services or builds.
import assert from "node:assert/strict";

export const BROWSER_API_CASES = Object.freeze([
  "profile_reload",
  "onboarding_complete",
  "preference_track_reload",
  "auth_isolation",
  "effect_availability",
  "packet_review",
  "approval_fenced",
  "source_resume_readback",
  "review_first_queue",
]);
export const HUMAN_BROWSER_API_CASES = Object.freeze([
  "bootstrap",
  "ready",
  "operator_release_received",
  "prepared_before_reload",
  "prepared_after_reload",
  "account_isolation",
  "zero_effects",
  "cleanup",
]);

function exactRecord(value, fields) {
  assert(value && typeof value === "object" && !Array.isArray(value), "invalid report record");
  assert.deepEqual(Object.keys(value).sort(), [...fields].sort(), "unexpected report fields");
}

function processExists(pid) {
  try {
    process.kill(pid, 0); // Read-only inspection; a report never authorizes termination.
    return true;
  } catch (error) {
    if (error.code === "ESRCH") return false;
    throw error; // Permission or transport ambiguity is not proof of absence.
  }
}

export function assertBrowserApiReport(report, pidExists = processExists) {
  exactRecord(report, ["version", "tests", "passed", "cases", "failed", "binaries", "fixture", "requests", "object_store"]);
  assert.equal(report.version, 4, "unexpected browser report version");
  assert.equal(report.tests, BROWSER_API_CASES.length, "incomplete browser case declaration");
  assert.equal(report.passed, BROWSER_API_CASES.length, "incomplete browser case execution");
  assert.deepEqual(report.cases, BROWSER_API_CASES, "missing or reordered browser cases");
  assert.deepEqual(report.failed, [], "browser failures cannot pass");
  assert(Array.isArray(report.binaries) && report.binaries.length === 2, "two API exits required");
  const seen = new Set();
  for (const binary of [...report.binaries, report.fixture]) {
    exactRecord(binary, ["pid", "code", "signal"]);
    assert(Number.isSafeInteger(binary.pid) && binary.pid > 1 && binary.pid <= 2_147_483_647,
      "invalid owned API PID");
    assert(!seen.has(binary.pid), "duplicate owned process evidence");
    seen.add(binary.pid);
    assert.equal(binary.code, 0, "nonclean API exit cannot pass");
    assert.equal(binary.signal, null, "signal termination cannot pass");
    assert.equal(pidExists(binary.pid), false, "owned API process still exists");
  }
  exactRecord(report.requests, ["jobs", "account"]);
  for (const count of Object.values(report.requests)) {
    assert(Number.isSafeInteger(count) && count > 0 && count <= 100_000,
      "invalid API request count");
  }
  exactRecord(report.object_store, ["put", "get", "denied", "objects"]);
  assert.equal(report.object_store.put, 1, "source replay must not create another PUT");
  assert(Number.isSafeInteger(report.object_store.get) && report.object_store.get >= 2
    && report.object_store.get <= 100, "source upload and replay both require readback");
  assert.equal(report.object_store.denied, 0, "normal source journey must have no storage rejection");
  assert.equal(report.object_store.objects, 1, "one canonical source object required");
}

export function assertHumanBrowserApiReport(report, pidExists = processExists) {
  exactRecord(report, [
    "version", "mode", "cases", "passed", "ready", "operator_release_received",
    "hold_ms", "account_status_before", "account_status_after", "unauthenticated_status",
    "account_rows", "profile_rows", "application_rows", "prepared_rows", "submitted_rows",
    "browser_sessions", "workflow_commands", "run_events", "irreversible_receipts",
    "profile_stable", "application_stable", "resume_stable", "isolated_application_rows",
    "network_canary_blocked", "page_errors", "server_errors", "engine_ps",
    "engine_lsof", "ports_refused", "binaries", "fixture", "requests", "object_store",
  ]);
  assert.equal(report.version, 1, "unexpected human report version");
  assert.equal(report.mode, "human_hold", "unexpected human report mode");
  assert.deepEqual(report.cases, HUMAN_BROWSER_API_CASES, "incomplete human case execution");
  assert.equal(report.passed, HUMAN_BROWSER_API_CASES.length, "incomplete human pass count");
  assert.equal(report.ready, true, "ready handshake required");
  assert.equal(report.operator_release_received, true, "operator release test control required");
  assert(Number.isSafeInteger(report.hold_ms) && report.hold_ms >= 0 && report.hold_ms <= 600_000,
    "human hold exceeded ten minutes from JWT issuance");
  assert.equal(report.account_status_before, 200, "account must be readable before reload");
  assert.equal(report.account_status_after, 200, "account must be readable after reload");
  assert.equal(report.unauthenticated_status, 401, "unauthenticated workspace must be denied exactly");
  assert.equal(report.account_rows, 1, "one fixture account row required");
  assert.equal(report.profile_rows, 1, "one fixture profile row required");
  assert.equal(report.application_rows, 1, "one prepared application required");
  assert.equal(report.prepared_rows, 1, "application must remain awaiting review");
  assert.equal(report.submitted_rows, 0, "human hold must not submit");
  for (const field of ["browser_sessions", "workflow_commands", "run_events",
    "irreversible_receipts", "isolated_application_rows"]) {
    assert.equal(report[field], 0, `${field} must remain empty`);
  }
  for (const field of ["profile_stable", "application_stable", "resume_stable"]) {
    assert.equal(report[field], true, `${field} readback required`);
  }
  assert.equal(report.network_canary_blocked, true, "nonlocal canary must be intercepted");
  assert.equal(report.page_errors, 0, "page errors cannot qualify");
  assert.equal(report.server_errors, 0, "server errors cannot qualify");
  assert.equal(report.engine_ps, 0, "owned Chromium process remains in ps");
  assert.equal(report.engine_lsof, 0, "owned Chromium open files remain in lsof");
  assert.equal(report.ports_refused, 4, "all API, proxy and object-store ports must refuse");
  assert(Array.isArray(report.binaries) && report.binaries.length === 2, "two API exits required");
  const seen = new Set();
  for (const binary of [...report.binaries, report.fixture]) {
    exactRecord(binary, ["pid", "code", "signal"]);
    assert(Number.isSafeInteger(binary.pid) && binary.pid > 1 && binary.pid <= 2_147_483_647,
      "invalid owned process PID");
    assert(!seen.has(binary.pid), "duplicate owned process evidence");
    seen.add(binary.pid);
    assert.equal(binary.code, 0, "nonclean process exit cannot pass");
    assert.equal(binary.signal, null, "signal termination cannot pass");
    assert.equal(pidExists(binary.pid), false, "owned process still exists");
  }
  exactRecord(report.requests, ["jobs", "account", "canary_blocked", "denied_non_origin",
    "denied_popups", "denied_downloads", "denied_websockets"]);
  assert(Number.isSafeInteger(report.requests.jobs) && report.requests.jobs > 0
    && report.requests.jobs <= 100_000, "invalid Jobs request count");
  assert(Number.isSafeInteger(report.requests.account) && report.requests.account > 0
    && report.requests.account <= 100_000, "invalid account request count");
  assert.equal(report.requests.canary_blocked, 1, "exactly one nonlocal canary must be blocked");
  for (const field of ["denied_non_origin", "denied_popups", "denied_downloads",
    "denied_websockets"]) assert.equal(report.requests[field], 0, `${field} must remain zero`);
  exactRecord(report.object_store, ["put", "get", "denied", "objects"]);
  assert.deepEqual(report.object_store, { put: 0, get: 0, denied: 0, objects: 0 },
    "human hold accepts no imported owner file or object-store traffic");
}
