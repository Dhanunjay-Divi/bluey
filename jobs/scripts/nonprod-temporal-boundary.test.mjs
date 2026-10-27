import assert from "node:assert/strict";
import { test } from "node:test";
import { assertRuntimeReport } from "./run-nonprod-temporal-tests.mjs";

function report() {
  return { version: 1, tests: 10, passed: 10, failed: [],
    servers: [{ pid: 12345, code: 0, signal: null }],
    latency: { samples: 20, p50Ms: 1, p95Ms: 3, maxMs: 5 } };
}

test("clean positive report still requires every owned PID absent", () => {
  const seen = [];
  assertRuntimeReport(report(), (pid) => { seen.push(pid); return false; });
  assert.deepEqual(seen, [12345]);
  assert.throws(() => assertRuntimeReport(report(), () => true), /still exists/);
  assert.throws(() => assertRuntimeReport(report(), () => { throw new Error("ambiguous"); }), /ambiguous/);
});

test("signal death, nonzero shutdown, omitted cases and malformed reports cannot pass", () => {
  const invalid = [
    { ...report(), tests: 0, passed: 0 },
    { ...report(), passed: 9 },
    { ...report(), failed: ["case"] },
    { ...report(), servers: [] },
    { ...report(), servers: [{ pid: 12345, code: null, signal: "SIGKILL" }] },
    { ...report(), servers: [{ pid: 12345, code: 1, signal: null }] },
    { ...report(), servers: [{ pid: 12345, code: 0, signal: "SIGINT" }] },
    { ...report(), servers: [{ pid: 1, code: 0, signal: null }] },
    { ...report(), extra: true },
    { ...report(), latency: { samples: 19, p50Ms: 1, p95Ms: 3, maxMs: 5 } },
    { ...report(), latency: { samples: 20, p50Ms: 3, p95Ms: 1, maxMs: 5 } },
  ];
  for (const value of invalid) assert.throws(() => assertRuntimeReport(value, () => false));
});
