import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { readOwnedPostgresProviderYield, readOwnedSqliteProviderYield } from "./provider-exposure.mjs";

const accountId = "provider-yield-synthetic";
const window = {
  accountId,
  startMs: 1_000,
  endMs: 5_000,
  asOfMs: 7_000,
  closeoutLagMs: 1_000,
  safetyMarginMs: 10,
  retentionMs: 10_000,
};

function scopeHash(domain, ...parts) {
  const digest = createHash("sha256").update("bluey-provider-cost-hold-v1\0").update(domain);
  for (const part of parts) {
    const bytes = Buffer.from(part, "utf8");
    const length = Buffer.alloc(8);
    length.writeBigUInt64BE(BigInt(bytes.length));
    digest.update(length).update(bytes);
  }
  return digest.digest("hex");
}

function projectedDdl(table) {
  const migrations = readFileSync(new URL("../../server/src/db/mod.rs", import.meta.url), "utf8");
  const start = migrations.indexOf(`CREATE TABLE IF NOT EXISTS ${table} (`);
  const end = migrations.indexOf("\n    );", start);
  assert(start >= 0 && end > start, `missing ${table} SQLite DDL`);
  return migrations.slice(start, end + "\n    );".length);
}

function fixture(t) {
  const db = new DatabaseSync(":memory:");
  t.after(() => db.close());
  db.exec("PRAGMA foreign_keys = ON");
  // The application projection deliberately contains every relational field
  // read by the yield adapter.  The receipt body stays in SQLite and is never
  // available to the verifier or the report.
  db.exec(`
    CREATE TABLE accounts (id TEXT PRIMARY KEY);
    CREATE TABLE jobs_postings (id TEXT PRIMARY KEY);
    CREATE TABLE jobs_applications (
      id TEXT PRIMARY KEY,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      state TEXT NOT NULL,
      submitted_at_ms INTEGER,
      updated_at_ms INTEGER NOT NULL,
      application_json TEXT NOT NULL
    );
  `);
  for (const table of ["usage_events", "jobs_packet_metering",
    "jobs_generation_allowance_reservations", "jobs_provider_cost_holds"]) {
    db.exec(projectedDdl(table));
  }
  db.prepare("INSERT INTO accounts VALUES (?)").run(accountId);
  db.prepare("INSERT INTO jobs_postings VALUES (?)").run("provider-yield-job");

  const requestId = "provider-yield-request";
  const insertHold = db.prepare(`INSERT INTO jobs_provider_cost_holds (
    request_scope_hash, account_scope_hash, generation_scope_hash, root_scope_hash,
    reservation_token, provider, model, projected_cost_cents, settled_cost_cents,
    usage_provenance, status, created_at_ms, updated_at_ms
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`);
  const addHold = (request, amount, usage, status = "settled") => insertHold.run(
      scopeHash("request", accountId, request), scopeHash("account", accountId),
      scopeHash("generation", accountId, "provider-yield-generation"), "a".repeat(64),
      "provider-yield-reservation", "fixture-provider", "fixture-model", 9, amount, usage,
      status, 1_000, 2_000);
  addHold(requestId, 2, "exact");
  const insertUsage = db.prepare(`INSERT INTO usage_events (
    id, account_id, request_id, ts, origin, kind, provider, model, cost_cents_to_bluey
  ) VALUES (?, ?, ?, ?, 'server', 'jobs_resume_generation_attempt', ?, ?, ?)`);
  const addUsage = (id, request, timestamp, cents) => insertUsage.run(
    id, accountId, request, timestamp, "fixture-provider", "fixture-model", cents);
  addUsage("provider-yield-usage", requestId, "1970-01-01 00:00:05", 2);

  const insertApplication = db.prepare(`INSERT INTO jobs_applications (
    id, account_id, state, submitted_at_ms, updated_at_ms, application_json
  ) VALUES (?, ?, ?, ?, ?, ?)`);
  const add = (id, state, submittedAt, updatedAt, body) =>
    insertApplication.run(id, accountId, state, submittedAt, updatedAt, body);
  add("verified", "submitted", 1_100, 1_200, '{"receipt":"verified"}');
  add("unverified", "submitted", 1_200, 1_300, '{"receipt":"unverified"}');
  add("unavailable", "submitted", 1_300, 1_400, "x".repeat(8 * 1024 * 1024 + 1));
  add("state-only", "failed", 1_400, 1_500, '{"receipt":"state-only"}');
  add("before", "submitted", 999, 1_100, '{"receipt":"before"}');
  add("late", "submitted", 5_000, 5_100, '{"receipt":"late"}');
  add("missing", "submitted", null, 1_600, '{"receipt":"missing"}');
  return { db, addHold, addUsage, addApplication: add, remove: (...ids) => db.prepare(`DELETE FROM jobs_applications
    WHERE id IN (${ids.map(() => "?").join(",")})`).run(...ids) };
}

test("owned SQLite yield snapshots bind receipt digests without exporting receipt content", async (t) => {
  const { db } = fixture(t);
  const seen = [];
  const report = await readOwnedSqliteProviderYield(db, window, async (receivedAccountId, candidate) => {
    assert.equal(receivedAccountId, accountId);
    assert.deepEqual(Object.keys(candidate).sort(), [
      "account_id", "id", "storedApplicationSha256", "submitted_at_ms", "updated_at_ms",
    ]);
    assert.equal(candidate.account_id, accountId);
    assert.match(candidate.storedApplicationSha256, /^[a-f0-9]{64}$/);
    assert.equal("application_json" in candidate, false);
    seen.push(candidate.id);
    return {
      verdict: candidate.id === "verified" ? "verified" : "unverified",
      storedApplicationSha256: candidate.storedApplicationSha256,
    };
  });
  assert.deepEqual(seen, ["verified", "unverified"]);
  assert.deepEqual(report.verifiedSubmissions, {
    verifiedSuccesses: 1,
    unverifiedSubmissions: 2,
    unavailableSubmissions: 1,
    outsideWindowSubmissions: 2,
    lateObservedSubmissions: 1,
    missingSubmissionTimestamp: 1,
  });
  assert.equal(report.perVerifiedApplicationCents, null);
  assert.equal(report.providerExposurePerObservedVerifiedSubmission, null);
  const rendered = JSON.stringify(report);
  for (const receipt of ["verified", "unverified", "state-only", "before", "late", "missing"]) {
    assert.equal(rendered.includes(`\\"receipt\\":\\"${receipt}\\"`), false);
  }
});

test("yield ratio requires a receipt-bound verified submission and nonzero provider exposure", async (t) => {
  const { db, remove } = fixture(t);
  remove("unverified", "unavailable", "state-only", "before", "late", "missing");
  const report = await readOwnedSqliteProviderYield(db, window, async (_accountId, candidate) => ({
    verdict: "verified",
    storedApplicationSha256: candidate.storedApplicationSha256,
  }));
  assert.deepEqual(report.providerExposurePerObservedVerifiedSubmission, {
    numeratorCents: 2,
    denominator: 1,
  });
  assert.equal(report.perVerifiedApplicationCents, null);
  assert.equal(report.verifiedSubmissions.unavailableSubmissions, 0);
  assert.equal(report.verifiedSubmissions.missingSubmissionTimestamp, 0);
});

test("yield stops checking receipts on verifier unavailability without reporting a partial ratio", async (t) => {
  const { db, remove, addApplication } = fixture(t);
  remove("verified", "unverified", "unavailable", "state-only", "before", "late", "missing");
  // The insertion order differs from the check order, including a time tie.
  for (const [id, at] of [["later", 1_300], ["other-same-time", 1_200], ["blocked", 1_200], ["first", 1_100]]) {
    addApplication(id, "submitted", at, at + 10, '{"receipt":"synthetic"}');
  }
  const seen = [];
  const observe = (failed) => readOwnedSqliteProviderYield(db, window, async (_account, candidate) => {
    seen.push(candidate.id);
    const verdict = candidate.id === failed ? "unavailable"
      : candidate.id === "blocked" ? "unverified" : "verified";
    return { verdict, storedApplicationSha256: candidate.storedApplicationSha256 };
  });

  const partial = await observe("blocked");
  assert.deepEqual(seen.splice(0), ["first", "blocked"]);
  assert.equal(partial.verifiedSubmissions.verifiedSuccesses, 1);
  assert.equal(partial.verifiedSubmissions.unavailableSubmissions, 1);
  assert.equal(partial.verifiedSubmissions.unverifiedSubmissions, 0);
  assert.deepEqual(partial.receiptVerification, { checkLimit: 8, eligibleCandidates: 4, deferredCandidates: 2 });
  assert.equal(partial.providerExposureCents, 2);
  assert.equal(partial.providerExposurePerObservedVerifiedSubmission, null);

  const complete = await observe(null);
  assert.deepEqual(seen.splice(0), ["first", "blocked", "other-same-time", "later"]);
  assert.equal(complete.verifiedSubmissions.unverifiedSubmissions, 1);
  assert.equal(complete.receiptVerification.deferredCandidates, 0);
  assert.deepEqual(complete.providerExposurePerObservedVerifiedSubmission, { numeratorCents: 2, denominator: 3 });

  const unavailableLast = await observe("later");
  assert.deepEqual(seen, ["first", "blocked", "other-same-time", "later"]);
  assert.equal(unavailableLast.verifiedSubmissions.unverifiedSubmissions, 1);
  assert.equal(unavailableLast.verifiedSubmissions.unavailableSubmissions, 1);
  assert.equal(unavailableLast.receiptVerification.deferredCandidates, 0);
  assert.equal(unavailableLast.providerExposurePerObservedVerifiedSubmission, null);
});

test("keeps failed attempts and unresolved retries in the partial numerator, not the verified denominator", async (t) => {
  const { db, remove, addHold, addUsage } = fixture(t);
  remove("unverified", "unavailable", "before", "late", "missing");
  addHold("rejected-provider", 11, "estimated");
  addUsage("rejected-provider-event", "rejected-provider", "1970-01-01 00:00:04", 11);
  addHold("missing-usage-retry", 9, "missing");
  addHold("unresolved-retry", 0, "missing", "held");
  addHold("unused-reservation", 0, "missing", "released");
  const verified = [];
  const report = await readOwnedSqliteProviderYield(db, window, async (_id, candidate) => {
    verified.push(candidate.id);
    return { verdict: "verified", storedApplicationSha256: candidate.storedApplicationSha256 };
  });
  assert.deepEqual(verified, ["verified"]);
  assert.equal(report.verifiedSubmissions.verifiedSuccesses, 1);
  assert.equal(report.verifiedSubmissions.unverifiedSubmissions, 1);
  assert.equal(report.settledExactUsagePricedCents, 2);
  assert.equal(report.settledEstimatedConservativeCents, 11);
  assert.equal(report.settledUnattributedCents, 9);
  assert.equal(report.heldUnresolvedProjectionCents, 9);
  assert.equal(report.releasedCount, 1);
  assert.deepEqual(report.providerExposurePerObservedVerifiedSubmission, { numeratorCents: 31, denominator: 1 });
  assert.equal(report.scope, "partial_provider_exposure");
  assert.equal(report.observation.cohortCompletenessVerified, false);
  assert.equal(report.perVerifiedApplicationCents, null);
});

test("keeps unmatched usage visible and suppresses a misleading ratio without adding its cost twice", async (t) => {
  const { db, remove, addUsage } = fixture(t);
  remove("unverified", "unavailable", "state-only", "before", "late", "missing");
  db.exec("UPDATE jobs_provider_cost_holds SET created_at_ms = 1100");
  const cohort = { ...window, startMs: 1_001 };
  const observe = () => readOwnedSqliteProviderYield(db, cohort, async (_account, candidate) => ({
    verdict: "verified",
    storedApplicationSha256: candidate.storedApplicationSha256,
  }));

  addUsage("old-usage-only", "older-unmatched-request", "1970-01-01 00:00:00", 123);
  const before = await observe();
  assert.deepEqual(before.usageLinkage, {
    orphanUsageRowsBeforeWindow: 1, orphanUsageRowsWithinObservation: 0, orphanUsageRowsCutoffUnknown: 0,
  });
  assert.deepEqual(before.providerExposurePerObservedVerifiedSubmission, { numeratorCents: 2, denominator: 1 });

  addUsage("in-window-usage-only", "unmatched-request", "1970-01-01 00:00:01", 97);
  addUsage("cutoff-unknown-usage-only", "unknown-request", "bad-timestamp", 99);
  addUsage("later-usage-only", "later-request", "1970-01-01 00:00:07", 20);
  const incomplete = await observe();
  assert.equal(incomplete.verifiedSubmissions.verifiedSuccesses, 1);
  assert.equal(incomplete.providerExposureCents, 2);
  assert.equal(incomplete.settledExactUsagePricedCents, 2);
  assert.deepEqual(incomplete.usageLinkage, {
    orphanUsageRowsBeforeWindow: 1, orphanUsageRowsWithinObservation: 1, orphanUsageRowsCutoffUnknown: 2,
  });
  assert.equal(incomplete.providerExposurePerObservedVerifiedSubmission, null);
  assert.equal(incomplete.perVerifiedApplicationCents, null);
  const rendered = JSON.stringify(incomplete);
  assert(!rendered.includes("unmatched-request") && !rendered.includes(accountId));
});

test("keeps an orphan at any inclusive cutoff boundary unresolved", async (t) => {
  const { db, remove, addUsage } = fixture(t);
  remove("unverified", "unavailable", "state-only", "before", "late", "missing");
  // The SQLite adapter sets the upper bound at the end of the recorded second.
  for (const timestamp of ["1970-01-01 00:00:00", "1970-01-01 00:00:04", "1970-01-01 00:00:06"]) {
    addUsage("boundary-usage", "boundary-request", timestamp, 97);
    const report = await readOwnedSqliteProviderYield(db, window, async (_account, candidate) => ({
      verdict: "verified",
      storedApplicationSha256: candidate.storedApplicationSha256,
    }));
    assert.deepEqual(report.usageLinkage, {
      orphanUsageRowsBeforeWindow: 0, orphanUsageRowsWithinObservation: 1, orphanUsageRowsCutoffUnknown: 0,
    }, timestamp);
    assert.equal(report.providerExposurePerObservedVerifiedSubmission, null, timestamp);
    db.exec("DELETE FROM usage_events WHERE id='boundary-usage'");
  }
});

test("defers the entire verification cohort before hashing bodies or verifying receipts", async (t) => {
  const { db, remove, addApplication } = fixture(t);
  remove("unverified", "state-only", "before", "late", "missing");
  for (let i = 0; i < 8; i++) {
    addApplication(`private-budget-app-${i}`, "submitted", 1_100, 1_200, '{"receipt":"private-budget-witness"}');
  }
  let calls = 0;
  let materializedBodies = 0;
  const observedDb = {
    exec: db.exec.bind(db),
    prepare(sql) {
      const statement = db.prepare(sql);
      if (!sql.includes("FROM jobs_applications")) return statement;
      return {
        *iterate(...parameters) {
          for (const row of statement.iterate(...parameters)) {
            if (row.raw !== null) materializedBodies++;
            yield row;
          }
        },
      };
    },
  };
  const verifier = async (_accountId, candidate) => {
    calls++;
    return { verdict: "verified", storedApplicationSha256: candidate.storedApplicationSha256 };
  };
  const deferred = await readOwnedSqliteProviderYield(observedDb, window, verifier);
  assert.equal(calls, 0);
  assert.equal(materializedBodies, 0);
  assert.deepEqual(deferred.receiptVerification, { checkLimit: 8, eligibleCandidates: 9, deferredCandidates: 9 });
  assert.equal(deferred.verifiedSubmissions.verifiedSuccesses, 0);
  assert.equal(deferred.verifiedSubmissions.unavailableSubmissions, 1);
  assert.equal(deferred.providerExposurePerObservedVerifiedSubmission, null);
  assert.equal(deferred.providerExposureCents, 2);
  assert(!JSON.stringify(deferred).includes(accountId) && !JSON.stringify(deferred).includes("private-budget"));
  remove("unavailable");
  const verified = await readOwnedSqliteProviderYield(observedDb, window, verifier, 9);
  assert.equal(calls, 9);
  assert.equal(materializedBodies, 9);
  assert.deepEqual(verified.receiptVerification, { checkLimit: 9, eligibleCandidates: 9, deferredCandidates: 0 });
  assert.deepEqual(verified.providerExposurePerObservedVerifiedSubmission, { numeratorCents: 2, denominator: 9 });
});

test("rejects inconsistent PostgreSQL candidate metadata before the verifier can run", async () => {
  const row = { id: "synthetic-submitted", account_id: accountId, state: "submitted",
    submitted_at_ms: 1_100, updated_at_ms: 1_200, digest_candidate: 1,
    stored_application_sha256: "a".repeat(64) };
  const emptyCosts = { holds: [], events: [], allowances: [], meters: [] };
  let attempts = 0;
  const verify = () => { attempts++; };
  for (const [submission, expected] of [
    [{ ...row, stored_application_sha256: null }, /does not match receipt budget/],
    [{ ...row, digest_candidate: 0 }, /invalid or duplicate submission observation/],
    [{ ...row, submitted_at_ms: window.endMs }, /outside the observation/],
  ]) {
    const query = async () => ({ rows: [{ snapshot: { ...emptyCosts, submissions: [submission] } }] });
    await assert.rejects(readOwnedPostgresProviderYield(query, window, verify, 1), expected);
  }
  const query = async () => ({ rows: [{ snapshot: { ...emptyCosts,
    submissions: [row, { ...row, id: "synthetic-second", stored_application_sha256: null }] } }] });
  await assert.rejects(readOwnedPostgresProviderYield(query, window, verify, 1), /does not match receipt budget/);
  assert.equal(attempts, 0);
});

test("rejects corrupt receipt limits before any PostgreSQL acquisition", async () => {
  let queried = 0;
  const query = async () => { queried++; };
  for (const limit of [0, -1, 2.5, 33, Number.NaN, "8"]) {
    await assert.rejects(readOwnedPostgresProviderYield(query, window, async () => {}, limit),
      /invalid submission verifier or receipt check limit/);
  }
  await assert.rejects(readOwnedPostgresProviderYield(query, window, null),
    /invalid submission verifier or receipt check limit/);
  assert.equal(queried, 0);
});

test("yield rejects a verifier receipt bound to another stored application", async (t) => {
  const { db, remove } = fixture(t);
  remove("unverified", "unavailable", "state-only", "before", "late", "missing");
  await assert.rejects(readOwnedSqliteProviderYield(db, window, async () => ({
    verdict: "verified",
    storedApplicationSha256: "f".repeat(64),
  })), /does not bind/);
});

test("zero observed receipts never manufacture a cost-per-success ratio", async (t) => {
  const { db, remove } = fixture(t);
  remove("verified", "unverified", "unavailable", "state-only", "before", "late", "missing");
  let calls = 0;
  const report = await readOwnedSqliteProviderYield(db, window, async () => {
    calls++;
    return { verdict: "verified", storedApplicationSha256: "0".repeat(64) };
  });
  assert.equal(calls, 0);
  assert.deepEqual(report.verifiedSubmissions, {
    verifiedSuccesses: 0,
    unverifiedSubmissions: 0,
    unavailableSubmissions: 0,
    outsideWindowSubmissions: 0,
    lateObservedSubmissions: 0,
    missingSubmissionTimestamp: 0,
  });
  assert.equal(report.providerExposurePerObservedVerifiedSubmission, null);
  assert.equal(report.perVerifiedApplicationCents, null);
});
