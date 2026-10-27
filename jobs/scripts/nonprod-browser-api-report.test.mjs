import assert from "node:assert/strict";
import { test } from "node:test";
import { assertBrowserApiReport, BROWSER_API_CASES } from "./nonprod-browser-api-report.mjs";

function report() {
  return {
    version: 4, tests: BROWSER_API_CASES.length, passed: BROWSER_API_CASES.length,
    cases: [...BROWSER_API_CASES], failed: [],
    binaries: [{ pid: 12345, code: 0, signal: null }, { pid: 12346, code: 0, signal: null }],
    fixture: { pid: 12347, code: 0, signal: null },
    requests: { jobs: 25, account: 3 },
    object_store: { put: 1, get: 2, denied: 0, objects: 1 },
  };
}

test("all named browser cases and both absent owned APIs are required", () => {
  const seen = [];
  assertBrowserApiReport(report(), (pid) => { seen.push(pid); return false; });
  assert.deepEqual(seen, [12345, 12346, 12347]);
  assert.throws(() => assertBrowserApiReport(report(), () => true), /still exists/);
  assert.throws(() => assertBrowserApiReport(report(), () => { throw new Error("ambiguous"); }),
    /ambiguous/);
});

test("partial, duplicated, reordered or fabricated case reports cannot qualify", () => {
  for (const change of [
    { version: 1 }, { version: 2 }, { version: 3 }, { tests: 7, passed: 7 }, { passed: 8 }, { passed: "9" },
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

test("signed fixture bridge must also exit cleanly with a distinct absent PID", () => {
  for (const fixture of [
    null, {}, report().binaries[0], { pid: 12347, code: 1, signal: null },
    { pid: 12347, code: null, signal: "SIGKILL" },
    { pid: 12347, code: 0, signal: null, extra: true },
  ]) assert.throws(() => assertBrowserApiReport({ ...report(), fixture }, () => false));
  assert.throws(() => assertBrowserApiReport(report(), (pid) => pid === 12347), /still exists/);
});

test("source object must have one PUT, repeated exact readback and zero normal rejections", () => {
  for (const object_store of [null, {}, { put: 2, get: 2, denied: 0, objects: 1 },
    { put: 1, get: 1, denied: 0, objects: 1 }, { put: 1, get: 2, denied: 1, objects: 1 },
    { put: 1, get: 2, denied: 0, objects: 0 }, { put: 1, get: 2, denied: 0, objects: 2 },
    { put: 1, get: "2", denied: 0, objects: 1 }, { put: 1, get: 101, denied: 0, objects: 1 },
    { put: 1, get: 2, denied: 0, objects: 1, extra: true },
  ]) assert.throws(() => assertBrowserApiReport({ ...report(), object_store }, () => false));
});
