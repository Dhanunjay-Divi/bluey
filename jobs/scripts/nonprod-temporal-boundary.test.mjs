import assert from "node:assert/strict";
import { test } from "node:test";
import { assertRuntimeReport } from "./run-nonprod-temporal-tests.mjs";

function report() {
  return { version: 1, tests: 10, passed: 10, failed: [],
    servers: [12345, 12346].map((pid) => ({ pid, code: 0, signal: null })),
    latency: { samples: 20, p50Ms: 1, p95Ms: 3, maxMs: 5 } };
}

test("clean positive report still requires every owned PID absent", () => {
  const seen = [];
  assertRuntimeReport(report(), (pid) => { seen.push(pid); return false; });
  assert.deepEqual(seen, [12345, 12346]);
  assert.throws(() => assertRuntimeReport(report(), () => true), /still exists/);
  assert.throws(() => assertRuntimeReport(report(), () => { throw new Error("ambiguous"); }), /ambiguous/);
});

test("a completed Temporal report requires the producer's exact cases, two distinct exits and sample count", () => {
  const complete = report();
  assert.throws(() => assertRuntimeReport({ ...complete, tests: 1, passed: 1 }, () => false),
    /exact Temporal case count/);
  assert.throws(() => assertRuntimeReport({ ...complete, servers: complete.servers.slice(0, 1) }, () => false),
    /two Temporal exits/);
  assert.throws(() => assertRuntimeReport({ ...complete, servers: [complete.servers[0], complete.servers[0]] }, () => false),
    /duplicate Temporal exit PID/);
  assert.throws(() => assertRuntimeReport({ ...complete, latency: { ...complete.latency, samples: 21 } }, () => false),
    /exact Temporal sample count/);
});

test("signal death, nonzero shutdown, omitted cases and malformed reports cannot pass", () => {
  const invalid = [
    { ...report(), tests: 0, passed: 0 },
    { ...report(), passed: 9 },
    { ...report(), failed: ["case"] },
    { ...report(), servers: [] },
    { ...report(), servers: [{ pid: 12345, code: null, signal: "SIGKILL" }, report().servers[1]] },
    { ...report(), servers: [{ pid: 12345, code: 1, signal: null }, report().servers[1]] },
    { ...report(), servers: [{ pid: 12345, code: 0, signal: "SIGINT" }, report().servers[1]] },
    { ...report(), servers: [{ pid: 1, code: 0, signal: null }, report().servers[1]] },
    { ...report(), extra: true },
    { ...report(), latency: { samples: 19, p50Ms: 1, p95Ms: 3, maxMs: 5 } },
    { ...report(), latency: { samples: 20, p50Ms: 3, p95Ms: 1, maxMs: 5 } },
  ];
  for (const value of invalid) assert.throws(() => assertRuntimeReport(value, () => false));
});
