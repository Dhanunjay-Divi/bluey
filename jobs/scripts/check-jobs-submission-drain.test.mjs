import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  checkSubmissionDrainCensus,
  submissionDrainSql,
} from "./check-jobs-submission-drain.mjs";

const runtime = readFileSync(new URL("../../server/src/db/mod.rs", import.meta.url), "utf8");
const capacityMigration = readFileSync(
  new URL("../../infra/sqlite/server-runtime/044_jobs_submission_evidence_reservations.sql", import.meta.url),
  "utf8",
);
const NOW = Date.now();
const DAY_MS = 24 * 60 * 60 * 1000;
const script = fileURLToPath(new URL("./check-jobs-submission-drain.mjs", import.meta.url));

function table(sql, name) {
  const start = sql.indexOf(`CREATE TABLE IF NOT EXISTS ${name} (`);
  assert.ok(start >= 0, `missing canonical ${name} DDL`);
  let depth = 0;
  for (let index = sql.indexOf("(", start); index < sql.length; index += 1) {
    if (sql[index] === "(") depth += 1;
    else if (sql[index] === ")" && --depth === 0) {
      assert.equal(sql[index + 1], ";", `canonical ${name} DDL must terminate`);
      return sql.slice(start, index + 2);
    }
  }
  assert.fail(`unterminated canonical ${name} DDL`);
}

function database(t) {
  const db = new DatabaseSync(":memory:");
  t.after(() => db.close());
  db.exec(`
    CREATE TABLE accounts(id TEXT PRIMARY KEY);
    CREATE TABLE jobs_postings(id TEXT PRIMARY KEY);
    CREATE TABLE jobs_resume_versions(id TEXT PRIMARY KEY);
    ${table(runtime, "jobs_applications")}
    ${table(runtime, "jobs_local_run_tickets")}
    ${table(runtime, "jobs_attempt_reservations")}
    ${table(runtime, "jobs_execution_leases")}
    ${capacityMigration}
    INSERT INTO accounts(id) VALUES ('synthetic-a'), ('synthetic-b');
  `);
  return db;
}

function application(db, { id, account = "synthetic-a", state = "awaiting_review", submittedAt = null }) {
  db.prepare("INSERT INTO jobs_postings(id) VALUES (?)").run(`job-${id}`);
  db.prepare(`INSERT INTO jobs_applications
    (id, account_id, job_id, state, application_json, created_at_ms, updated_at_ms, submitted_at_ms)
    VALUES (?, ?, ?, ?, '{}', ?, ?, ?)`)
    .run(id, account, `job-${id}`, state, NOW - 2, NOW - 1, submittedAt);
}

function localTicket(db, { id, applicationId, status, updatedAt = NOW - 1 }) {
  db.prepare(`INSERT INTO jobs_local_run_tickets
    (id, account_id, application_id, ticket_hash, ticket_secret, payload_json, status,
      expires_at_ms, created_at_ms, updated_at_ms)
    VALUES (?, 'synthetic-a', ?, ?, 'synthetic-secret', '{}', ?, ?, ?, ?)`)
    .run(id, applicationId, `hash-${id}`, status, NOW + DAY_MS, NOW - 2, updatedAt);
}

function cloudLease(db, { id, applicationId, phase, updatedAt = NOW - 1, finishedAt = null }) {
  db.prepare(`INSERT INTO jobs_execution_leases
    (run_id, account_id, application_id, browser_profile_id, owner_id, lease_token_sha256, fence, phase,
      lease_expires_at_ms, created_at_ms, updated_at_ms, finished_at_ms)
    VALUES (?, 'synthetic-a', ?, ?, 'synthetic-owner', ?, 1, ?, ?, ?, ?, ?)`)
    .run(id, applicationId, `profile-${id}`, "a".repeat(64), phase, NOW + DAY_MS, NOW - 2, updatedAt, finishedAt);
}

function capacity(db, { applicationId, runId, runner = "cloud", state = "active", completedAt = null }) {
  db.prepare(`INSERT INTO jobs_submission_evidence_capacity
    (account_id, application_id, run_id, runner, reserved_bytes, reserved_objects, consumed_bytes,
      consumed_objects, expires_at_ms, state, created_at_ms, updated_at_ms, completed_at_ms)
    VALUES ('synthetic-a', ?, ?, ?, 1, 1, 0, 0, ?, ?, ?, ?, ?)`)
    .run(applicationId, runId, runner, NOW + DAY_MS, state, NOW - 2, NOW - 1, completedAt);
}

function attempt(db, { id, applicationId, status }) {
  db.prepare(`INSERT INTO jobs_attempt_reservations
    (id, account_id, application_id, company_key, period_key, runner, status, reserved_at_ms, updated_at_ms)
    VALUES (?, 'synthetic-a', ?, ?, 'fixture-period', 'cloud', ?, ?, ?)`)
    .run(id, applicationId, `company-${id}`, status, NOW - 2, NOW - 1);
}

function census(db) {
  db.exec("PRAGMA query_only = ON;");
  const row = db.prepare(submissionDrainSql("sqlite")).get();
  return checkSubmissionDrainCensus(`${row.census}\n`);
}

function assertCensus(observed, fields) {
  assert.equal(observed.schema, "bluey-jobs-submission-drain-v1");
  assert(observed.observedAtMs >= NOW - 1_000 && observed.observedAtMs <= Date.now() + 1_000);
  assert.deepEqual(
    { ...observed, observedAtMs: undefined },
    { schema: "bluey-jobs-submission-drain-v1", observedAtMs: undefined, ...fields },
  );
}

test("counts each durable storage type without treating a completed cloud submission as pending", (t) => {
  const db = database(t);
  application(db, { id: "local-click" });
  application(db, { id: "local-unknown", state: "side_effect_unknown" });
  application(db, { id: "cloud-click" });
  application(db, { id: "cloud-unknown", state: "side_effect_unknown" });
  application(db, { id: "cloud-pending" });
  application(db, { id: "cloud-complete", state: "submitted", submittedAt: NOW - 1 });
  application(db, { id: "cloud-wrong-run", state: "submitted", submittedAt: NOW - 1 });
  application(db, { id: "cloud-no-completion", state: "submitted", submittedAt: NOW - 1 });
  application(db, { id: "cloud-no-submitted-at", state: "submitted" });
  application(db, { id: "capacity-only" });
  localTicket(db, { id: "local-click", applicationId: "local-click", status: "click_started" });
  localTicket(db, { id: "local-unknown", applicationId: "local-unknown", status: "side_effect_unknown" });
  localTicket(db, { id: "local-failed", applicationId: "local-click", status: "failed" });
  cloudLease(db, { id: "cloud-click", applicationId: "cloud-click", phase: "click_started" });
  cloudLease(db, { id: "cloud-unknown", applicationId: "cloud-unknown", phase: "side_effect_unknown" });
  cloudLease(db, { id: "cloud-pending", applicationId: "cloud-pending", phase: "submitted" });
  cloudLease(db, { id: "cloud-complete", applicationId: "cloud-complete", phase: "submitted", finishedAt: NOW - 1 });
  cloudLease(db, { id: "cloud-wrong-run", applicationId: "cloud-wrong-run", phase: "submitted" });
  cloudLease(db, { id: "cloud-no-completion", applicationId: "cloud-no-completion", phase: "submitted" });
  cloudLease(db, { id: "cloud-no-submitted-at", applicationId: "cloud-no-submitted-at", phase: "submitted" });
  capacity(db, { applicationId: "cloud-complete", runId: "cloud-complete", state: "committed", completedAt: NOW - 1 });
  capacity(db, { applicationId: "cloud-wrong-run", runId: "other-run", state: "committed", completedAt: NOW - 1 });
  capacity(db, { applicationId: "cloud-no-completion", runId: "cloud-no-completion", state: "committed" });
  capacity(db, { applicationId: "cloud-no-submitted-at", runId: "cloud-no-submitted-at", state: "committed", completedAt: NOW - 1 });
  capacity(db, { applicationId: "capacity-only", runId: "capacity-only" });
  attempt(db, { id: "attempt-unknown", applicationId: "cloud-unknown", status: "side_effect_unknown" });

  assertCensus(census(db), {
    localClickStarted: 1,
    localUnknown: 1,
    cloudClickStarted: 1,
    cloudUnknown: 1,
    cloudAwaitingPublication: 4,
    activeEvidenceReservations: 1,
    unknownApplications: 2,
    unknownAttemptReservations: 1,
    agedExecutionRows: 0,
    invalidExecutionClocks: 0,
    status: "pending",
    releaseAuthorized: false,
  });
});

test("never subtracts old or invalid post-marker records from the drain", (t) => {
  const db = database(t);
  application(db, { id: "old-local" });
  application(db, { id: "invalid-cloud", state: "side_effect_unknown" });
  application(db, { id: "old-publication" });
  localTicket(db, { id: "old-local", applicationId: "old-local", status: "click_started", updatedAt: NOW - DAY_MS });
  cloudLease(db, { id: "invalid-cloud", applicationId: "invalid-cloud", phase: "side_effect_unknown", updatedAt: -1 });
  cloudLease(db, { id: "old-publication", applicationId: "old-publication", phase: "submitted", updatedAt: NOW - DAY_MS - 1 });
  assertCensus(census(db), {
    localClickStarted: 1,
    localUnknown: 0,
    cloudClickStarted: 0,
    cloudUnknown: 1,
    cloudAwaitingPublication: 1,
    activeEvidenceReservations: 0,
    unknownApplications: 1,
    unknownAttemptReservations: 0,
    agedExecutionRows: 2,
    invalidExecutionClocks: 1,
    status: "pending",
    releaseAuthorized: false,
  });
});

test("empty storage is explicitly not release authority", (t) => {
  assertCensus(census(database(t)), {
    localClickStarted: 0,
    localUnknown: 0,
    cloudClickStarted: 0,
    cloudUnknown: 0,
    cloudAwaitingPublication: 0,
    activeEvidenceReservations: 0,
    unknownApplications: 0,
    unknownAttemptReservations: 0,
    agedExecutionRows: 0,
    invalidExecutionClocks: 0,
    status: "no_unresolved_rows",
    releaseAuthorized: false,
  });
});

test("parser rejects malformed framing, counts, status, and inconsistent subsets", () => {
  for (const raw of [
    "",
    "bluey-jobs-submission-drain-v0 1 0 0 0 0 0 0 0 0 0 0\n",
    "bluey-jobs-submission-drain-v1 1 0 0 0 0 0 0 0 0 0\n\n",
    "bluey-jobs-submission-drain-v1 01 0 0 0 0 0 0 0 0 0\n",
    "bluey-jobs-submission-drain-v1 1 -1 0 0 0 0 0 0 0 0\n",
    "bluey-jobs-submission-drain-v1 1 0 0 0 0 0 0 0 0 1\n",
    "bluey-jobs-submission-drain-v1 1 1 0 0 0 0 0 0 2 0\n",
    "bluey-jobs-submission-drain-v1 1 0 0 0 0 0 0 0 0 0 extra\n",
  ]) assert.throws(() => checkSubmissionDrainCensus(raw), /submission_drain/i);
});

test("Postgres projection is present and keeps the same durable categories", () => {
  const postgres = submissionDrainSql("postgres");
  for (const fragment of [
    "jobs_local_run_tickets",
    "jobs_execution_leases",
    "jobs_submission_evidence_capacity",
    "jobs_applications",
    "jobs_attempt_reservations",
    "click_started",
    "side_effect_unknown",
    "committed",
    "completed_at_ms",
  ]) assert.match(postgres, new RegExp(fragment));
  assert.throws(() => submissionDrainSql("mysql"), /submission_drain/i);
});

test("CLI treats a pending census as a hold and never echoes untrusted input", () => {
  const run = (input) => spawnSync(process.execPath, [script, "--check"], { input, encoding: "utf8" });
  const clear = run("bluey-jobs-submission-drain-v1 1 0 0 0 0 0 0 0 0 0 0\n");
  assert.equal(clear.status, 0);
  assert.equal(JSON.parse(clear.stdout).releaseAuthorized, false);
  const held = run("bluey-jobs-submission-drain-v1 1 1 0 0 0 0 0 0 0 0 0\n");
  assert.equal(held.status, 2);
  assert.equal(JSON.parse(held.stdout).status, "pending");
  const marker = "synthetic-secret-marker-never-echo";
  for (const input of [marker, marker.repeat(32)]) {
    const result = run(input);
    assert.equal(result.status, 65);
    assert.equal(result.stdout, "");
    assert.doesNotMatch(result.stderr, new RegExp(marker));
  }
});
