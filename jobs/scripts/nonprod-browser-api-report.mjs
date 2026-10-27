// Test-only report boundary. Importing this module cannot start services or builds.
import assert from "node:assert/strict";

export const BROWSER_API_CASES = Object.freeze([
  "profile_reload",
  "onboarding_complete",
  "preference_track_reload",
  "auth_isolation",
  "effect_availability",
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
  exactRecord(report, ["version", "tests", "passed", "cases", "failed", "binaries", "requests"]);
  assert.equal(report.version, 2, "unexpected browser report version");
  assert.equal(report.tests, BROWSER_API_CASES.length, "incomplete browser case declaration");
  assert.equal(report.passed, BROWSER_API_CASES.length, "incomplete browser case execution");
  assert.deepEqual(report.cases, BROWSER_API_CASES, "missing or reordered browser cases");
  assert.deepEqual(report.failed, [], "browser failures cannot pass");
  assert(Array.isArray(report.binaries) && report.binaries.length === 2, "two API exits required");
  const seen = new Set();
  for (const binary of report.binaries) {
    exactRecord(binary, ["pid", "code", "signal"]);
    assert(Number.isSafeInteger(binary.pid) && binary.pid > 1 && binary.pid <= 2_147_483_647,
      "invalid owned API PID");
    assert(!seen.has(binary.pid), "duplicate API process evidence");
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
}
