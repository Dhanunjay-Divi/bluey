import assert from "node:assert/strict";
import { test } from "node:test";
import { createHash } from "node:crypto";
import { chmodSync, mkdirSync, mkdtempSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  assertBrowserInputsUnchanged, browserApiSourcePaths, browserPortalSourcePaths, snapshotBrowserInputs,
} from "./nonprod-browser-api-inputs.mjs";
import {
  assertBrowserApiReport, assertBrowserApiRunReceipt, assertHumanBrowserApiReport,
  BROWSER_API_CASES, HUMAN_BROWSER_API_CASES,
} from "./nonprod-browser-api-report.mjs";

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

function inputFixture(t) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "bluey-browser-inputs-")));
  t.after(() => rmSync(root, { recursive: true }));
  const path = join(root, "fixture");
  const bytes = Buffer.alloc(2 * 64 * 1024 + 1, 0x61);
  writeFileSync(path, bytes, { mode: 0o700 });
  return { root, path, bytes };
}

test("browser input snapshots refuse added, deleted and renamed entries", (t) => {
  const { root, path } = inputFixture(t);
  const first = snapshotBrowserInputs([root]);
  assertBrowserInputsUnchanged(first, snapshotBrowserInputs([root]));
  const newPath = join(root, "new");
  writeFileSync(newPath, "new");
  assert.throws(() => assertBrowserInputsUnchanged(first, snapshotBrowserInputs([root])), /inventory changed/);
  rmSync(newPath);
  renameSync(path, newPath);
  assert.throws(() => assertBrowserInputsUnchanged(first, snapshotBrowserInputs([root])), /inventory changed/);
  rmSync(newPath);
  assert.throws(() => assertBrowserInputsUnchanged(first, snapshotBrowserInputs([root])), /inventory changed/);
});

test("browser artifact snapshots hash large files and refuse byte, identity and mode drift", (t) => {
  const { root, path, bytes } = inputFixture(t);
  const snapshot = () => snapshotBrowserInputs([path], { executables: true });
  const first = snapshot();
  assert.equal(first.get(path).sha256, createHash("sha256").update(bytes).digest("hex"));
  writeFileSync(path, Buffer.alloc(bytes.length, 0x62));
  assert.throws(() => assertBrowserInputsUnchanged(first, snapshot()), /changed since snapshot/);
  const current = snapshot();
  renameSync(path, join(root, "retained"));
  writeFileSync(path, Buffer.alloc(bytes.length, 0x62), { mode: 0o700 });
  assert.throws(() => assertBrowserInputsUnchanged(current, snapshot()), /changed since snapshot/);
  chmodSync(path, 0o722);
  assert.throws(snapshot, /owned executable/);
  rmSync(path);
  symlinkSync("retained", path);
  assert.throws(snapshot, /noncanonical browser input/);
});

test("browser portal inputs detect public asset and TypeScript config drift", (t) => {
  const { root } = inputFixture(t);
  for (const directory of ["portal", "portal/src", "portal/public"]) {
    mkdirSync(join(root, directory));
  }
  const files = ["portal/src/main.tsx", "portal/index.html", "portal/package.json",
    "portal/public/us-locations.json", "portal/tsconfig.json", "package.json", "package-lock.json"];
  for (const file of files) writeFileSync(join(root, file), "baseline");
  const paths = browserPortalSourcePaths(root);
  const before = snapshotBrowserInputs(paths);
  assertBrowserInputsUnchanged(before, snapshotBrowserInputs(paths));
  for (const file of ["portal/public/us-locations.json", "portal/tsconfig.json"]) {
    const previous = snapshotBrowserInputs(paths);
    writeFileSync(join(root, file), "modified");
    assert.throws(() => assertBrowserInputsUnchanged(previous, snapshotBrowserInputs(paths)),
      /changed since snapshot/);
  }
  const previous = snapshotBrowserInputs(paths);
  writeFileSync(join(root, "portal/public/unexpected.json"), "{}");
  assert.throws(() => assertBrowserInputsUnchanged(previous, snapshotBrowserInputs(paths)),
    /inventory changed/);
});

test("browser API inputs detect handler, routing, migration, lock and shared-crate drift", (t) => {
  const { root } = inputFixture(t);
  const paths = browserApiSourcePaths(root);
  const files = [
    "Cargo.toml", "Cargo.lock", "server/Cargo.toml", "server/Cargo.lock",
    "server/src/api/jobs_interview_prep.rs", "server/src/api/router/completion.rs",
    "server/examples/jobs-nonprod-browser-fixture.rs",
    "crates/cue-core/Cargo.toml", "crates/cue-core/src/lib.rs",
    "infra/postgres/server-runtime/002_jobs.sql", "infra/sqlite/server-runtime/066_jobs.sql",
  ];
  for (const file of files) {
    const target = join(root, file);
    mkdirSync(join(target, ".."), { recursive: true });
    writeFileSync(target, "baseline");
  }
  assert.throws(() => browserApiSourcePaths("relative"), /must be absolute/);
  for (const file of files) {
    const before = snapshotBrowserInputs(paths);
    assertBrowserInputsUnchanged(before, snapshotBrowserInputs(paths));
    writeFileSync(join(root, file), "modified");
    assert.throws(() => assertBrowserInputsUnchanged(before, snapshotBrowserInputs(paths)), /changed since snapshot/);
  }
  const before = snapshotBrowserInputs(paths);
  writeFileSync(join(root, "infra/postgres/server-runtime/003_new.sql"), "new");
  assert.throws(() => assertBrowserInputsUnchanged(before, snapshotBrowserInputs(paths)), /inventory changed/);
});

test("all named browser cases and both absent owned APIs are required", () => {
  const seen = [];
  assertBrowserApiReport(report(), (pid) => { seen.push(pid); return false; });
  assert.deepEqual(seen, [12345, 12346, 12347]);
  assert.throws(() => assertBrowserApiReport(report(), () => true), /still exists/);
  assert.throws(() => assertBrowserApiReport(report(), () => { throw new Error("ambiguous"); }),
    /ambiguous/);
});

test("partial, duplicated or reordered case declarations cannot pass validation", () => {
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

test("both receipt modes require the caller's exact run nonce and reject stale complete reports", () => {
  const nonce = "a".repeat(64);
  for (const [humanHold, payload] of [[false, report()], [true, humanReport()]]) {
    const receipt = { version: 1, nonce, report: payload };
    assert.equal(assertBrowserApiRunReceipt(receipt, nonce, humanHold, () => false), payload);
    const mustNotInspectPid = () => assert.fail("unbound receipt reached process inspection");
    assert.throws(() => assertBrowserApiRunReceipt(receipt, "b".repeat(64), humanHold, mustNotInspectPid),
      /nonce mismatch/);
    for (const invalid of [undefined, nonce.slice(1), nonce.toUpperCase(), nonce + "\n"]) {
      assert.throws(() => assertBrowserApiRunReceipt({ ...receipt, nonce: invalid }, nonce, humanHold, mustNotInspectPid),
        /invalid browser qualification nonce/);
    }
    assert.throws(() => assertBrowserApiRunReceipt(payload, nonce, humanHold, mustNotInspectPid),
      /unexpected report fields/);
    assert.throws(() => assertBrowserApiRunReceipt({ ...receipt, version: 2 }, nonce, humanHold, mustNotInspectPid),
      /unexpected browser receipt version/);
    assert.throws(() => assertBrowserApiRunReceipt(receipt, nonce, !humanHold, mustNotInspectPid),
      /unexpected report fields/);
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

function humanReport() {
  return {
    version: 1, mode: "human_hold", cases: [...HUMAN_BROWSER_API_CASES],
    passed: HUMAN_BROWSER_API_CASES.length, ready: true, operator_release_received: true,
    hold_ms: 120_000, account_status_before: 200, account_status_after: 200,
    unauthenticated_status: 401, account_rows: 1, profile_rows: 1,
    application_rows: 1, prepared_rows: 1, submitted_rows: 0,
    browser_sessions: 0, workflow_commands: 0, run_events: 0,
    irreversible_receipts: 0, profile_stable: true, application_stable: true,
    resume_stable: true, isolated_application_rows: 0, network_canary_blocked: true,
    page_errors: 0, server_errors: 0, engine_ps: 0, engine_lsof: 0, ports_refused: 4,
    binaries: [{ pid: 22345, code: 0, signal: null }, { pid: 22346, code: 0, signal: null }],
    fixture: { pid: 22347, code: 0, signal: null },
    requests: { jobs: 25, account: 3, canary_blocked: 1, denied_non_origin: 0,
      denied_popups: 0, denied_downloads: 0, denied_websockets: 0 },
    object_store: { put: 0, get: 0, denied: 0, objects: 0 },
  };
}

test("human hold requires the exact closed schema and absent owned processes", () => {
  const seen = [];
  assertHumanBrowserApiReport(humanReport(), (pid) => { seen.push(pid); return false; });
  assert.deepEqual(seen, [22345, 22346, 22347]);
  for (const change of [
    { version: 4 }, { mode: "automated" }, { cases: [] }, { passed: 7 },
    { ready: false }, { operator_release_received: false }, { hold_ms: 600_001 },
    { account_status_before: 204 }, { account_status_after: 204 },
    { unauthenticated_status: 403 }, { application_rows: 0 }, { application_rows: 2 },
    { prepared_rows: 0 }, { submitted_rows: 1 }, { profile_stable: false },
    { application_stable: false }, { resume_stable: false },
    { network_canary_blocked: false }, { page_errors: 1 }, { server_errors: 1 },
    { engine_ps: 1 }, { engine_lsof: 1 }, { ports_refused: 3 }, { extra: true },
  ]) assert.throws(() => assertHumanBrowserApiReport({ ...humanReport(), ...change }, () => false));
  assert.throws(() => assertHumanBrowserApiReport(humanReport(), () => true), /still exists/);
});

test("human hold rejects every effect, cross-account row and network escape counter", () => {
  for (const field of ["browser_sessions", "workflow_commands", "run_events",
    "irreversible_receipts", "isolated_application_rows"]) {
    assert.throws(() => assertHumanBrowserApiReport({ ...humanReport(), [field]: 1 }, () => false));
  }
  for (const field of ["canary_blocked", "denied_non_origin", "denied_popups",
    "denied_downloads", "denied_websockets"]) {
    const requests = { ...humanReport().requests,
      [field]: field === "canary_blocked" ? 0 : 1 };
    assert.throws(() => assertHumanBrowserApiReport({ ...humanReport(), requests }, () => false));
  }
  for (const object_store of [
    { put: 1, get: 0, denied: 0, objects: 1 },
    { put: 0, get: 1, denied: 0, objects: 0 },
    { put: 0, get: 0, denied: 1, objects: 0 },
  ]) assert.throws(() => assertHumanBrowserApiReport({ ...humanReport(), object_store }, () => false));
});

test("human hold timeout, window-close and incomplete work cannot be represented as passing", () => {
  for (const change of [
    { operator_release_received: false, hold_ms: 1 },
    { ready: false, cases: HUMAN_BROWSER_API_CASES.slice(0, 1), passed: 1 },
    { application_rows: 0, prepared_rows: 0 },
    { binaries: [] }, { fixture: null },
  ]) assert.throws(() => assertHumanBrowserApiReport({ ...humanReport(), ...change }, () => false));
});
