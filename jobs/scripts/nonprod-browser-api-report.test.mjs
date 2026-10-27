import assert from "node:assert/strict";
import { test } from "node:test";
import { assertBrowserApiReport, BROWSER_API_CASES } from "./nonprod-browser-api-report.mjs";

function report() {
  return {
    version: 2, tests: 5, passed: 5, cases: [...BROWSER_API_CASES], failed: [],
    binaries: [{ pid: 12345, code: 0, signal: null }, { pid: 12346, code: 0, signal: null }],
    requests: { jobs: 25, account: 3 },
  };
}

test("all named browser cases and both absent owned APIs are required", () => {
  const seen = [];
  assertBrowserApiReport(report(), (pid) => { seen.push(pid); return false; });
  assert.deepEqual(seen, [12345, 12346]);
  assert.throws(() => assertBrowserApiReport(report(), () => true), /still exists/);
  assert.throws(() => assertBrowserApiReport(report(), () => { throw new Error("ambiguous"); }),
    /ambiguous/);
});

test("partial, duplicated, reordered or fabricated case reports cannot qualify", () => {
  for (const change of [
    { version: 1 }, { tests: 1, passed: 1 }, { passed: 4 }, { passed: "5" },
    { cases: [] }, { cases: BROWSER_API_CASES.slice(0, -1) },
    { cases: [...BROWSER_API_CASES].reverse() },
    { cases: BROWSER_API_CASES.map(() => "profile_reload") },
    { cases: [...BROWSER_API_CASES, "submitted"] }, { failed: ["fixture_failure"] },
    { extra: true }, { tests: Number.NaN },
  ]) assert.throws(() => assertBrowserApiReport({ ...report(), ...change }, () => false));
  for (const value of [null, [], {}, 5, "report"]) {
    assert.throws(() => assertBrowserApiReport(value, () => false));
  }
});

test("signal death, duplicate process evidence and ambiguous identities fail closed", () => {
  const first = report().binaries[0];
  for (const binaries of [
    [], [first], [first, first], [first, first, first],
    [first, { pid: 12346, code: null, signal: "SIGKILL" }],
    [first, { pid: 12346, code: 1, signal: null }],
    [first, { pid: 12346, code: 0, signal: "SIGTERM" }],
    [first, { pid: 1, code: 0, signal: null }],
    [first, { pid: 1.5, code: 0, signal: null }],
    [first, { pid: 2_147_483_648, code: 0, signal: null }],
    [first, { pid: 12346, code: 0, signal: null, extra: true }],
  ]) assert.throws(() => assertBrowserApiReport({ ...report(), binaries }, () => false));
});

test("request evidence must be complete bounded integer counters", () => {
  for (const requests of [
    {}, { jobs: 1 }, { jobs: 0, account: 1 }, { jobs: 1, account: -1 },
    { jobs: "1", account: 1 }, { jobs: 1.5, account: 1 },
    { jobs: Infinity, account: 1 }, { jobs: 100_001, account: 1 },
    { jobs: 1, account: 1, extra: 1 }, null, [],
  ]) assert.throws(() => assertBrowserApiReport({ ...report(), requests }, () => false));
});
