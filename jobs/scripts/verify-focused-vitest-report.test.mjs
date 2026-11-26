import assert from "node:assert/strict";
import test from "node:test";
import { FOCUSED_RUNNER_CASES, verifyFocusedVitestReport } from "./verify-focused-vitest-report.mjs";

const workspace = "/private/tmp/owned-runner-gate/jobs/runner";
const terminal = { code: 0, signal: null, error: null };

function sample() {
  return {
    success: true, numTotalTests: 73, numPassedTests: 73,
    numFailedTests: 0, numPendingTests: 0, numTodoTests: 0,
    numTotalTestSuites: 8, numPassedTestSuites: 8, numFailedTestSuites: 0, numPendingTestSuites: 0,
    snapshot: { failure: false },
    testResults: Object.entries(FOCUSED_RUNNER_CASES).map(([file, count]) => ({
      name: `${workspace}/${file}`, status: "passed", message: "",
      assertionResults: Array.from({ length: count }, (_, i) => ({
        status: "passed", failureMessages: [], fullName: `Synthetic case ${i}`,
      })),
    })),
  };
}

const verify = (row, overrides = {}) => verifyFocusedVitestReport({
  report: Buffer.from(JSON.stringify(row)), terminal, workspace, stdout: "", stderr: "", ...overrides,
});

test("accepts exact file paths and individual case counts without assuming describe groups equal files", () => {
  const result = verify(sample());
  assert.equal(result.files, 4);
  assert.equal(result.passed, 73);
  assert.match(result.reportSha256, /^[a-f0-9]{64}$/);
});

for (const [label, change, reason] of [
  ["foreign root", (row) => { row.testResults[0].name = row.testResults[0].name.replace("owned-", "foreign-"); },
    /vitest_unexpected_or_duplicate_file/],
  ["duplicate file", (row) => { row.testResults[1].name = row.testResults[0].name; },
    /vitest_unexpected_or_duplicate_file/],
  ["missing case", (row) => { row.testResults[0].assertionResults.pop(); }, /vitest_case_count/],
  ["substituted count", (row) => { row.numPassedTests -= 1; }, /vitest_case_totals/],
  ["skipped case", (row) => { row.testResults[0].assertionResults[0].status = "skipped"; }, /vitest_case_status/],
  ["duplicate case", (row) => { row.testResults[0].assertionResults[0].fullName = "Synthetic case 1"; },
    /vitest_case_name/],
  ["pending describe", (row) => { row.numPendingTestSuites = 1; }, /vitest_suite_totals/],
  ["snapshot failure", (row) => { row.snapshot.failure = true; }, /vitest_snapshot_failure/],
]) {
  test(`rejects ${label} even with a successful terminal`, () => {
    const row = sample();
    change(row);
    assert.throws(() => verify(row), reason);
  });
}

test("requires the real process exit and rejects teardown warnings despite green assertion counters", () => {
  assert.throws(() => verify(sample(), { terminal: { ...terminal, signal: "SIGTERM" } }), /vitest_terminal/);
  assert.throws(() => verify(sample(), { stderr: "Unexpected worker stderr" }), /vitest_stderr/);
  assert.throws(() => verify(sample(), { stdout: "\x1b[31mFailed to terminate worker\x1b[0m" }), /vitest_teardown/);
});
