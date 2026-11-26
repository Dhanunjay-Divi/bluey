import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  checkGlobalDiscoveryCensus,
  GLOBAL_DISCOVERY_CENSUS_SQL,
} from "./check-global-discovery-quiescence.mjs";

const migration = readFileSync(
  new URL("../../infra/sqlite/server-runtime/035_jobs_global_candidate_index.sql", import.meta.url),
  "utf8",
);
const script = fileURLToPath(new URL("./check-global-discovery-quiescence.mjs", import.meta.url));

function database(t) {
  const db = new DatabaseSync(":memory:");
  t.after(() => db.close());
  db.exec(migration);
  return db;
}

function insertSource(db, id, status = "active") {
  db.prepare(`INSERT INTO jobs_global_discovery_sources
      (id, provider, source_key, source_json, status, next_run_at_ms, created_at_ms, updated_at_ms)
    VALUES (?, 'jobhive', ?, '{}', ?, 1, 1, 1)`).run(id, id, status);
}

function insertRun(db, id, source, status, completed) {
  db.prepare(`INSERT INTO jobs_global_ingestion_runs
      (id, source_id, replay_key, status, started_at_ms, completed_at_ms)
    VALUES (?, ?, ?, ?, 10, ?)`).run(id, source, id, status, completed);
}

function census(db) {
  db.exec("PRAGMA query_only = ON;");
  return checkGlobalDiscoveryCensus(`${db.prepare(GLOBAL_DISCOVERY_CENSUS_SQL).get().census}\n`);
}

test("a source state can hold the rollout even without a lease or any runs", (t) => {
  const db = database(t);
  for (const [id, status, health] of [
    ["fixture-running", "active", "running"],
    ["fixture-disabled-running", "disabled", "running"],
    ["fixture-unknown-health", "active", "future"],
    ["fixture-unknown-status", "future", "waiting"],
    ["fixture-paused", "active", "paused"],
    ["fixture-healthy", "active", "healthy"],
    ["fixture-degraded", "disabled", "degraded"],
  ]) {
    insertSource(db, id, status);
    db.prepare("UPDATE jobs_global_discovery_sources SET health = ? WHERE id = ?").run(health, id);
  }
  const observed = census(db);
  assert.equal(observed.status, "hold");
  assert.equal(observed.sourcesWithLease, 0);
  assert.equal(observed.sourcesWithUnclearState, 4);
  assert.equal(observed.runs, 0);
});

test("an empty database does not report a cleared rollout", (t) => {
  const observed = census(database(t));
  assert.equal(observed.status, "no_sources");
  assert.equal(observed.activationAuthorized, false);
});

test("reports only counts for closed runs, without granting activation", (t) => {
  const db = database(t);
  insertSource(db, "fixture-source-a");
  insertSource(db, "fixture-source-b", "disabled");
  insertRun(db, "fixture-a", "fixture-source-a", "completed", 20);
  insertRun(db, "fixture-b", "fixture-source-b", "failed", 21);
  assert.deepEqual(census(db), {
    schema: "bluey-jobs-global-quiescence-v2",
    status: "no_inflight_rows",
    sources: 2,
    sourcesWithLease: 0,
    sourcesWithUnclearState: 0,
    runs: 2,
    running: 0,
    unknownStatus: 0,
    invalidTerminal: 0,
    activationAuthorized: false,
  });
});

test("holds on an expired, partial, or active lease and on running rows for disabled sources", (t) => {
  const db = database(t);
  insertSource(db, "fixture-expired");
  insertSource(db, "fixture-partial");
  insertSource(db, "fixture-active");
  insertSource(db, "fixture-disabled", "disabled");
  db.exec(`UPDATE jobs_global_discovery_sources SET lease_expires_at_ms = 1 WHERE id = 'fixture-expired';
    UPDATE jobs_global_discovery_sources SET lease_owner = 'fixture-worker' WHERE id = 'fixture-partial';
    UPDATE jobs_global_discovery_sources SET lease_owner = 'fixture-worker', lease_token = 'fixture-token',
      lease_expires_at_ms = 1800000000000 WHERE id = 'fixture-active';`);
  insertRun(db, "fixture-running", "fixture-disabled", "running", null);
  const observed = census(db);
  assert.equal(observed.status, "hold");
  assert.equal(observed.sourcesWithLease, 3);
  assert.equal(observed.running, 1);
  assert.doesNotMatch(JSON.stringify(observed), /fixture/);
  assert.equal(db.prepare("SELECT status FROM jobs_global_ingestion_runs WHERE id = ?")
    .get("fixture-running").status, "running");
});

test("does not interpret unknown or incomplete run states as closed", (t) => {
  const db = database(t);
  insertSource(db, "fixture-source");
  insertRun(db, "fixture-unknown", "fixture-source", "stopped", 20);
  insertRun(db, "fixture-missing-terminal", "fixture-source", "failed", null);
  insertRun(db, "fixture-early-terminal", "fixture-source", "completed", 5);
  const observed = census(db);
  assert.equal(observed.status, "hold");
  assert.equal(observed.unknownStatus, 1);
  assert.equal(observed.invalidTerminal, 2);
});

test("the SQL refuses a database without the required tables", (t) => {
  const db = new DatabaseSync(":memory:");
  t.after(() => db.close());
  assert.throws(() => census(db), /no such table/);
});

test("rejects stale formats, surplus rows, malformed counts, and inconsistent totals", () => {
  for (const line of [
    "",
    "bluey-jobs-global-quiescence-v1 1 0 0 0 0 0\n",
    "bluey-jobs-global-quiescence-v2 1 0 0 0 0 0\n",
    "bluey-jobs-global-quiescence-v2 1 0 0 0 0 0 0\n\n",
    "bluey-jobs-global-quiescence-v2 01 0 0 0 0 0 0\n",
    "bluey-jobs-global-quiescence-v2 1 -1 0 0 0 0 0\n",
    "bluey-jobs-global-quiescence-v2 1 0 0 9007199254740992 0 0 0\n",
    "bluey-jobs-global-quiescence-v2 0 0 0 1 1 0 0\n",
    "bluey-jobs-global-quiescence-v2 1 2 0 0 0 0 0\n",
    "bluey-jobs-global-quiescence-v2 1 0 2 0 0 0 0\n",
    "bluey-jobs-global-quiescence-v2 1 0 0 2 1 1 1\n",
  ]) assert.throws(() => checkGlobalDiscoveryCensus(line), /global_discovery_census/);
});

test("CLI emits read-only SQL, distinguishes hold from clear, and never echoes bad input", () => {
  const sql = spawnSync(process.execPath, [script, "--sql"], { encoding: "utf8" });
  assert.equal(sql.status, 0);
  assert.equal(sql.stdout, GLOBAL_DISCOVERY_CENSUS_SQL);
  const run = (input) => spawnSync(process.execPath, [script, "--check"], { input, encoding: "utf8" });
  const clear = run("bluey-jobs-global-quiescence-v2 1 0 0 2 0 0 0\n");
  assert.equal(clear.status, 0);
  assert.equal(JSON.parse(clear.stdout).activationAuthorized, false);
  assert.equal(run("bluey-jobs-global-quiescence-v2 0 0 0 0 0 0 0\n").status, 2);
  assert.equal(run("bluey-jobs-global-quiescence-v2 1 0 0 1 1 0 0\n").status, 2);
  assert.equal(run("bluey-jobs-global-quiescence-v2 1 0 1 0 0 0 0\n").status, 2);
  const malformed = run("fixture-private-input".repeat(100));
  assert.equal(malformed.status, 65);
  assert.equal(malformed.stdout, "");
  assert.doesNotMatch(malformed.stderr, /fixture-private-input/);
});
