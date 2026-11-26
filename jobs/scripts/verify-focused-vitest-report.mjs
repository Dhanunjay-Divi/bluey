import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { isAbsolute, join, normalize } from "node:path";
import { stripVTControlCharacters } from "node:util";

export const FOCUSED_RUNNER_CASES = Object.freeze({
  "tests/submitted-result-recovery.test.ts": 18,
  "tests/execution-lease.test.ts": 48,
  "tests/container-hardening.test.ts": 4,
  "tests/server-session-coordination.test.ts": 3,
});

const MAX_BYTES = 2 * 1024 * 1024;

// The caller owns process, source/report custody and independent root cleanup.
// A JSON report by itself cannot prove any of those observations.
export function verifyFocusedVitestReport({ report, terminal, stdout, stderr, workspace }, expected = FOCUSED_RUNNER_CASES) {
  assert(terminal?.code === 0 && terminal.signal === null && terminal.error === null, "vitest_terminal");
  assert(typeof stdout === "string" && typeof stderr === "string"
    && Buffer.byteLength(stdout) <= MAX_BYTES && Buffer.byteLength(stderr) <= MAX_BYTES, "vitest_streams");
  assert.equal(stderr, "", "vitest_stderr");
  assert.doesNotMatch(stripVTControlCharacters(stdout),
    /failed to terminate|failed to close|terminat[^\r\n]*\bworker\b|unhandled (?:error|rejection)|close timed out/i,
    "vitest_teardown");
  assert(typeof workspace === "string" && isAbsolute(workspace) && normalize(workspace) === workspace,
    "vitest_workspace");
  assert(Buffer.isBuffer(report) && report.length > 0 && report.length <= MAX_BYTES, "vitest_report_size");
  const parsed = JSON.parse(report.toString("utf8"));
  assert(parsed?.success === true && Array.isArray(parsed.testResults), "vitest_report_success");
  const files = Object.entries(expected);
  assert(files.length > 0 && files.every(([path, count]) => (
    /^tests\/[a-z0-9-]+\.test\.ts$/.test(path) && Number.isSafeInteger(count) && count > 0
  )), "vitest_expected_files");
  const expectedPaths = new Map(files.map(([path, count]) => [join(workspace, path), count]));
  assert.equal(parsed.testResults.length, files.length, "vitest_file_count");
  for (const suite of parsed.testResults) {
    const count = expectedPaths.get(suite?.name);
    assert(count !== undefined, "vitest_unexpected_or_duplicate_file");
    expectedPaths.delete(suite.name);
    assert(suite.status === "passed" && suite.message === "", "vitest_suite_status");
    assert(Array.isArray(suite.assertionResults) && suite.assertionResults.length === count, "vitest_case_count");
    const names = new Set();
    for (const result of suite.assertionResults) {
      assert(result?.status === "passed" && Array.isArray(result.failureMessages)
        && result.failureMessages.length === 0, "vitest_case_status");
      assert(typeof result.fullName === "string" && result.fullName.trim().length > 0
        && !names.has(result.fullName), "vitest_case_name");
      names.add(result.fullName);
    }
  }
  const total = files.reduce((sum, [, count]) => sum + count, 0);
  assert(parsed.numTotalTests === total && parsed.numPassedTests === total
    && parsed.numFailedTests === 0 && parsed.numPendingTests === 0 && parsed.numTodoTests === 0,
  "vitest_case_totals");
  // Vitest counts nested describe groups here, not just physical test files.
  assert(Number.isSafeInteger(parsed.numTotalTestSuites) && parsed.numTotalTestSuites >= files.length
    && parsed.numPassedTestSuites === parsed.numTotalTestSuites && parsed.numFailedTestSuites === 0
    && parsed.numPendingTestSuites === 0, "vitest_suite_totals");
  assert.equal(parsed.snapshot?.failure, false, "vitest_snapshot_failure");
  return { files: files.length, passed: total, reportSha256: createHash("sha256").update(report).digest("hex") };
}
