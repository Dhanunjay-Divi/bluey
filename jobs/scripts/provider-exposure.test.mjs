import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { countObservedVerifiedSubmissions, readOwnedPostgresProviderExposure, readOwnedSqliteProviderExposure,
  summarizeProviderApplicationExposure, summarizeProviderExposure, summarizeProviderExposureWindow,
  summarizeProviderHoldRows } from "./provider-exposure.mjs";

const hold = (key, projectedCents = 9) => ({
  scopeHash: key.repeat(64), state: "held", projectedCents, settledCents: null, usage: null,
});
const settled = (key, settledCents, usage) => ({
  ...hold(key), state: "settled", settledCents, usage,
});

test("counts each exposed attempt once and keeps unresolved and missing attribution visible", () => {
  assert.deepEqual(summarizeProviderExposure([
    hold("a"), settled("b", 2, "exact"), settled("c", 5, "estimated"),
    settled("d", 7, "missing"), settled("e", 4, null),
  ]), {
    heldUnresolvedProjectionCents: 9, settledExactUsagePricedCents: 2,
    settledEstimatedConservativeCents: 5, settledMissingConservativeCents: 7,
    settledUnattributedCents: 4, providerExposureCents: 27, attemptCount: 5,
    scope: "partial_provider_exposure", perVerifiedApplicationCents: null,
  });
});

test("rejects duplicated hold identities even when the amounts or statuses differ", () => {
  assert.throws(() => summarizeProviderExposure([hold("a"), settled("a", 1, "exact")]), /duplicate/);
});

test("rejects malformed amounts, impossible state combinations, and overflow", () => {
  for (const bad of [
    hold("A"), hold("a", -1), hold("a", 0.1), hold("a", Infinity),
    { ...hold("a"), scopeHash: hold("a").scopeHash + "\n" },
    { ...hold("a"), settledCents: 0 }, { ...hold("a"), usage: "exact" },
    settled("a", null, "exact"), settled("a", 4, "unknown"),
  ]) assert.throws(() => summarizeProviderExposure([bad]));
  assert.throws(() => summarizeProviderExposure([hold("a", Number.MAX_SAFE_INTEGER), hold("b", 1)]), /range/);
  assert.throws(() => summarizeProviderExposure([hold("a", Number.MAX_SAFE_INTEGER), settled("b", 1, "exact")]), /range/);
});

test("an empty sample does not imply a free application", () => {
  const result = summarizeProviderExposure([]);
  assert.equal(result.attemptCount, 0);
  assert.equal(result.perVerifiedApplicationCents, null);
});

const rawHold = {
  request_scope_hash: "f0bc585f4e0824dc825e25c499770b02fb6a2222e49281bac71c7eb5e70a046d",
  account_scope_hash: "60b89a07cdaa7fce3463bf6df048088d77622e0f1b51b72a971a9e992e6aa780",
  provider: "fixture-provider", model: "fixture-model", projected_cost_cents: 9,
  status: "settled", settled_cost_cents: 2, usage_provenance: "exact",
};
const rawEvent = {
  account_id: "synthetic-α", request_id: "attempt-1", origin: "server",
  kind: "jobs_resume_generation_attempt", provider: rawHold.provider,
  model: rawHold.model, cost_cents_to_bluey: 2,
};

test("settlement links by UTF-8 account and request hashes without adding usage cost", () => {
  const row = { ...rawHold, usageEvents: [rawEvent] };
  assert.equal(summarizeProviderHoldRows([row]).providerExposureCents, 2);
  assert.equal(summarizeProviderHoldRows([row]).settledExactUsagePricedCents, 2);
  const missing = summarizeProviderHoldRows([{ ...row, usageEvents: [] }]);
  assert.equal(missing.settledExactUsagePricedCents, 0);
  assert.equal(missing.settledUnattributedCents, 2);
  assert.equal(missing.perVerifiedApplicationCents, null);
});

test("fails closed on duplicated rows, ambiguous joins and mismatched usage evidence", () => {
  const row = { ...rawHold, usageEvents: [rawEvent] };
  assert.throws(() => summarizeProviderHoldRows([row, row]), /duplicate/);
  assert.throws(() => summarizeProviderHoldRows([{ ...row, usageEvents: [rawEvent, rawEvent] }]), /duplicate/);
  for (const fields of [{ account_id: "other" }, { request_id: "attempt-2" },
    { kind: "llm_attempt" }, { origin: "desktop" }, { provider: "other" },
    { model: "other" }, { cost_cents_to_bluey: 3 }]) {
    assert.throws(() => summarizeProviderHoldRows([{ ...row, usageEvents: [{ ...rawEvent, ...fields }] }]),
      /does not match/);
  }
});

test("normalizes storage defaults without counting released holds as settled", () => {
  const unsettled = { ...rawHold, status: "held", settled_cost_cents: 0,
    usage_provenance: "missing", usageEvents: [] };
  assert.equal(summarizeProviderHoldRows([unsettled]).heldUnresolvedProjectionCents, 9);
  const released = summarizeProviderHoldRows([{ ...unsettled, status: "released" }]);
  assert.equal(released.attemptCount, 0);
  assert.equal(released.releasedCount, 1);
  assert.throws(() => summarizeProviderHoldRows([{ ...unsettled, usageEvents: [rawEvent] }]), /unsettled/);
  assert.throws(() => summarizeProviderHoldRows([{ ...rawHold, usage_provenance: "estimated", usageEvents: [] }]),
    /invalid settled/);
});

function fixtureGenerationHash(key) {
  const prefix = Buffer.from("bluey-provider-cost-hold-v1\0generation");
  const account = Buffer.from("synthetic-α");
  return createHash("sha256").update(Buffer.concat([
    prefix, Buffer.from([0, 0, 0, 0, 0, 0, 0, account.length]), account,
    Buffer.from([0, 0, 0, 0, 0, 0, 0, 1]), Buffer.from(key),
  ])).digest("hex");
}

test("keeps spend without a committed application in distinct buckets, including unresolved holds", () => {
  const rows = ["a", "b", "c", "d", "e", "f"].map((key) => ({
    ...rawHold, request_scope_hash: key.repeat(64), generation_scope_hash: fixtureGenerationHash(key),
    status: "held", settled_cost_cents: 0, usage_provenance: "missing", usageEvents: [],
  }));
  const allowance = (key, status, application_id) => ({
    account_id: rawEvent.account_id, generation_key: key, job_id: "job-" + key, status, application_id,
  });
  const allowances = [
    allowance("a", "committed", "application-fixture"), allowance("b", "committed", null),
    allowance("c", "reserved", null), allowance("e", "released", null),
    allowance("f", "committed", "other-fixture"),
  ];
  const meter = { account_id: rawEvent.account_id, job_id: "job-a", application_id: "application-fixture" };
  const result = summarizeProviderApplicationExposure(rows, allowances, [meter]);
  assert.equal(result.providerExposureCents, 54);
  assert.deepEqual(result.applicationAttribution, {
    meteredPacket: { attemptCount: 1, exposureCents: 9 },
    unverifiedApplicationPointer: { attemptCount: 1, exposureCents: 9 },
    missingApplication: { attemptCount: 1, exposureCents: 9 },
    uncommitted: { attemptCount: 1, exposureCents: 9 },
    releasedAllowance: { attemptCount: 1, exposureCents: 9 },
    missingAllowance: { attemptCount: 1, exposureCents: 9 },
  });
  assert.equal(result.perVerifiedApplicationCents, null);
  assert.ok(!JSON.stringify(result).includes("application-fixture"));
  assert.throws(() => summarizeProviderApplicationExposure(rows, [...allowances, allowances[0]]), /duplicate/);
  assert.throws(() => summarizeProviderApplicationExposure([
    { ...rows[0], account_scope_hash: "f".repeat(64) },
  ], allowances), /account mismatch/);
  assert.throws(() => summarizeProviderApplicationExposure(rows, [
    allowance("a", "reserved", "application-fixture"),
  ]), /invalid generation allowance/);
  assert.throws(() => summarizeProviderApplicationExposure(rows, allowances, [meter, meter]), /duplicate/);
  assert.throws(() => summarizeProviderApplicationExposure(rows, allowances, [
    { ...meter, application_id: "mismatch" },
  ]), /metering disagree/);
  const wrongAccount = summarizeProviderApplicationExposure(rows, allowances, [{ ...meter, account_id: "other" }]);
  assert.equal(wrongAccount.applicationAttribution.meteredPacket.attemptCount, 0);
});

const observationWindow = { accountId: rawEvent.account_id, startMs: 100, endMs: 200, asOfMs: 240, closeoutLagMs: 40,
  safetyMarginMs: 10, retentionMs: 151 };
const timedHold = (key, created_at_ms, updated_at_ms) => ({
  ...rawHold, request_scope_hash: key.repeat(64), generation_scope_hash: fixtureGenerationHash(key),
  created_at_ms, updated_at_ms, usageEvents: [],
});

const submissionCohort = { accountId: "fixture-account", startMs: 100, endMs: 200, asOfMs: 240 };
const candidate = (id, submitted_at_ms, fields = {}) => ({
  id, account_id: submissionCohort.accountId, submitted_at_ms,
  updated_at_ms: submitted_at_ms, state: "submitted", ...fields,
});

test("counts only in-window submissions accepted by the supplied verifier, not status labels", async () => {
  const seen = [];
  const result = await countObservedVerifiedSubmissions([
    candidate("before", 99), candidate("verified", 100), candidate("label-only", 199),
    candidate("end", 200),
  ], submissionCohort, async (accountId, row) => {
    assert.equal(accountId, submissionCohort.accountId);
    seen.push(row.id);
    return row.id === "verified";
  });
  assert.deepEqual(seen, ["verified", "label-only"]);
  assert.deepEqual(result, {
    verifiedSuccesses: 1, unverifiedSubmissions: 1, outsideWindowSubmissions: 2,
    cohortCompletenessVerified: false, perVerifiedApplicationCents: null,
  });
});

test("submission counting refuses duplicate identities, cross-account data and invalid verifier results", async () => {
  const row = candidate("fixture", 110);
  const verify = () => true;
  await assert.rejects(countObservedVerifiedSubmissions([row, row], submissionCohort, verify), /duplicate/);
  await assert.rejects(countObservedVerifiedSubmissions([
    { ...row, account_id: "other" },
  ], submissionCohort, verify), /invalid/);
  await assert.rejects(countObservedVerifiedSubmissions([
    { ...row, updated_at_ms: 241 },
  ], submissionCohort, verify), /newer than report cutoff/);
  await assert.rejects(countObservedVerifiedSubmissions([row], submissionCohort, () => "true"), /outcome/);
  await assert.rejects(countObservedVerifiedSubmissions([row], submissionCohort, () => {
    throw new Error("storage unavailable");
  }), /storage unavailable/);
  const empty = await countObservedVerifiedSubmissions([], submissionCohort, verify);
  assert.equal(empty.verifiedSuccesses, 0);
  assert.equal(empty.cohortCompletenessVerified, false);
  assert.equal(empty.perVerifiedApplicationCents, null);
});

test("invalid cohort rows prevent any receipt verification or evidence reads", async () => {
  const first = candidate("first", 120);
  for (const [invalid, reason] of [
    [candidate("foreign", 130, { account_id: "another-account" }), /invalid or duplicate/],
    [candidate("first", 150), /invalid or duplicate/],
    [candidate("late", 160, { updated_at_ms: 241 }), /newer than report cutoff/],
  ]) {
    let reads = 0;
    await assert.rejects(countObservedVerifiedSubmissions([first, invalid], submissionCohort, () => {
      reads++;
      return true;
    }), reason);
    assert.equal(reads, 0, "reject the complete cohort before reading any receipt");
  }
});

test("selects the half-open creation window and retains exposure updated during closeout", () => {
  const report = summarizeProviderExposureWindow([
    timedHold("a", 99, 130), timedHold("b", 100, 100),
    timedHold("c", 199, 240), timedHold("d", 200, 205),
  ], [], observationWindow);
  assert.equal(report.providerExposureCents, 4);
  assert.equal(report.attemptCount, 2);
  assert.equal(report.observation.outsideWindowRows, 2);
  assert.equal(report.observation.updatedAfterWindowRows, 1);
  assert.equal(report.observation.cohortCompletenessVerified, false);
  assert.equal(report.perVerifiedApplicationCents, null);
});

test("does not use linked usage beyond the report cutoff to upgrade provider exposure provenance", () => {
  const row = {
    ...timedHold("a", 100, 200),
    request_scope_hash: rawHold.request_scope_hash,
    usageEvents: [{ ...rawEvent, ts_upper_bound_ms: observationWindow.asOfMs }],
  };
  const exact = summarizeProviderExposureWindow([row], [], observationWindow);
  assert.equal(exact.settledExactUsagePricedCents, 2);
  for (const ts_upper_bound_ms of [observationWindow.asOfMs + 1, undefined, -1, 200.5]) {
    assert.throws(() => summarizeProviderExposureWindow([
      { ...row, usageEvents: [{ ...rawEvent, ts_upper_bound_ms }] },
    ], [], observationWindow), /provider usage timestamp/);
  }
  const missing = summarizeProviderExposureWindow([{ ...row, usageEvents: [] }], [], observationWindow);
  assert.equal(missing.settledUnattributedCents, 2);
  assert.equal(missing.perVerifiedApplicationCents, null);
});

test("refuses stale windows and rows whose current state cannot represent the cutoff", () => {
  for (const change of [{ retentionMs: 150 }, { closeoutLagMs: 41 }, { safetyMarginMs: 0 },
    { startMs: 200 }, { asOfMs: 199 }, { retentionMs: 32 * 24 * 60 * 60 * 1000 }]) {
    assert.throws(() => summarizeProviderExposureWindow([], [], { ...observationWindow, ...change }), /bounds/);
  }
  assert.throws(() => summarizeProviderExposureWindow([
    timedHold("a", 100, 241),
  ], [], observationWindow), /newer than report cutoff/);
  assert.throws(() => summarizeProviderExposureWindow([
    timedHold("a", 100, 99),
  ], [], observationWindow), /timestamps/);
  const allowance = { account_id: rawEvent.account_id, generation_key: "a",
    job_id: "job-a", status: "committed", application_id: "fixture", created_at_ms: 100, updated_at_ms: 241 };
  const meter = { account_id: rawEvent.account_id, job_id: "job-a", application_id: "fixture", created_at_ms: 240 };
  assert.throws(() => summarizeProviderExposureWindow([
    timedHold("a", 100, 200),
  ], [allowance], observationWindow), /allowance is newer than report cutoff/);
  const atCutoff = summarizeProviderExposureWindow([
    timedHold("a", 100, 200),
  ], [{ ...allowance, updated_at_ms: 240 }], observationWindow, [meter]);
  assert.equal(atCutoff.applicationAttribution.meteredPacket.exposureCents, 2);
  assert.throws(() => summarizeProviderExposureWindow([
    timedHold("a", 100, 200),
  ], [{ ...allowance, updated_at_ms: 240 }], observationWindow, [{ ...meter, created_at_ms: 241 }]),
  /metering is newer than report cutoff/);
});

test("refuses cross-account hold, allowance and meter rows in a single-account observation", () => {
  const row = timedHold("a", 100, 200);
  const allowance = { account_id: rawEvent.account_id, generation_key: "a",
    job_id: "job-a", status: "committed", application_id: "fixture", created_at_ms: 100, updated_at_ms: 200 };
  const meter = { account_id: rawEvent.account_id, job_id: "job-a", application_id: "fixture", created_at_ms: 200 };
  const report = summarizeProviderExposureWindow([row], [allowance], observationWindow, [meter]);
  assert.equal(report.applicationAttribution.meteredPacket.exposureCents, 2);
  assert.ok(!JSON.stringify(report).includes(observationWindow.accountId));
  assert.ok(!JSON.stringify(report).includes(row.account_scope_hash));
  for (const [rows, allowances, meters] of [
    [[{ ...row, account_scope_hash: "f".repeat(64) }], [], []],
    [[{ ...row, account_scope_hash: "f".repeat(64), created_at_ms: 99 }], [allowance], [meter]],
    [[row], [{ ...allowance, account_id: "other", generation_key: "unused" }], [meter]],
    [[row], [allowance], [{ ...meter, account_id: "other", job_id: "unrelated" }]],
  ]) {
    assert.throws(() => summarizeProviderExposureWindow(rows, allowances, observationWindow, meters),
      /observation account mismatch/);
  }
  assert.throws(() => summarizeProviderExposureWindow([row], [], { ...observationWindow, accountId: "" }),
    /invalid provider exposure window/);
});

const sqliteWindow = { ...observationWindow, startMs: 1000, endMs: 5000, asOfMs: 6999,
  closeoutLagMs: 1000, retentionMs: 10_000 };

function sqliteCostFixture(t) {
  const db = new DatabaseSync(":memory:");
  t.after(() => db.close());
  const migrations = readFileSync(new URL("../../server/src/db/mod.rs", import.meta.url), "utf8");
  // Only these supporting identities are synthetic; the four reporting
  // tables use the current checked-in SQLite CREATE definitions verbatim.
  db.exec(`
    CREATE TABLE accounts (id TEXT PRIMARY KEY);
    CREATE TABLE jobs_postings (id TEXT PRIMARY KEY);
    CREATE TABLE jobs_applications (id TEXT PRIMARY KEY);
    INSERT INTO jobs_postings VALUES ('fixture-job');
    INSERT INTO jobs_applications VALUES ('fixture-application');
  `);
  for (const table of ["jobs_provider_cost_holds", "usage_events",
    "jobs_generation_allowance_reservations", "jobs_packet_metering"]) {
    const start = migrations.indexOf("CREATE TABLE IF NOT EXISTS " + table + " (");
    const end = migrations.indexOf("\n    );", start);
    assert(start >= 0 && end > start);
    db.exec(migrations.slice(start, end + "\n    );".length));
  }
  db.prepare("INSERT INTO accounts VALUES (?), (?)").run(rawEvent.account_id, "other-fixture");
  const holds = db.prepare(`INSERT INTO jobs_provider_cost_holds (
    request_scope_hash,account_scope_hash,generation_scope_hash,root_scope_hash,reservation_token,
    provider,model,projected_cost_cents,settled_cost_cents,usage_provenance,status,created_at_ms,updated_at_ms
  ) VALUES (?,?,?,?,'fixture',?,?,9,?,?,?,?,?)`);
  const insertHold = (request, generation, actual, usage, status, created, updated, account = rawHold.account_scope_hash) =>
    holds.run(request, account, fixtureGenerationHash(generation), "e".repeat(64),
      rawHold.provider, rawHold.model, actual, usage, status, created, updated);
  insertHold(rawHold.request_scope_hash, "a", 2, "exact", "settled", 1000, 4999);
  insertHold("b".repeat(64), "a", 9, "estimated", "settled", 1200, 5200);
  insertHold("c".repeat(64), "c", 0, "missing", "held", 1300, 1300);
  insertHold("d".repeat(64), "a", 0, "missing", "released", 1300, 1400);
  insertHold("f".repeat(64), "f", 0, "missing", "held", 5000, 5000);
  insertHold("a".repeat(64), "a", 0, "missing", "held", 1100, 1200, "f".repeat(64));
  const addUsage = (id, timestamp, account = rawEvent.account_id) => db.prepare(`
    INSERT INTO usage_events (id,account_id,request_id,ts,origin,kind,provider,model,cost_cents_to_bluey)
    VALUES (?,?,?,?, 'server','jobs_resume_generation_attempt',?,?,2)
  `).run(id, account, rawEvent.request_id, timestamp, rawHold.provider, rawHold.model);
  addUsage("authoritative-fixture", "1970-01-01 00:00:05");
  addUsage("other-account-fixture", "1970-01-01 00:00:06", "other-fixture");
  db.prepare(`INSERT INTO jobs_generation_allowance_reservations VALUES
    (?,'fixture-job','a','fixture','committed',1000,'fixture-application',1000,2000)`).run(rawEvent.account_id);
  db.prepare(`INSERT INTO jobs_packet_metering VALUES
    (?,'fixture-job','fixture-application','meter-fixture',1,0,3000)`).run(rawEvent.account_id);
  return { db, addUsage };
}

test("reads a coherent SQLite cohort including unmetered and retried attempts without counting a receipt", (t) => {
  const { db } = sqliteCostFixture(t);
  db.exec("PRAGMA query_only=ON");
  const report = readOwnedSqliteProviderExposure(db, sqliteWindow);
  assert.equal(report.providerExposureCents, 20);
  assert.equal(report.attemptCount, 3);
  assert.equal(report.releasedCount, 1);
  assert.equal(report.settledExactUsagePricedCents, 2);
  assert.equal(report.settledUnattributedCents, 9);
  assert.equal(report.heldUnresolvedProjectionCents, 9);
  assert.deepEqual(report.applicationAttribution.meteredPacket, { attemptCount: 2, exposureCents: 11 });
  assert.deepEqual(report.applicationAttribution.missingAllowance, { attemptCount: 1, exposureCents: 9 });
  assert.equal(report.observation.outsideWindowRows, 1);
  assert.equal(report.observation.updatedAfterWindowRows, 1);
  assert.equal(report.observation.cohortCompletenessVerified, false);
  assert.equal(report.perVerifiedApplicationCents, null);
  assert(!JSON.stringify(report).includes(rawEvent.account_id));
  db.exec("BEGIN DEFERRED; ROLLBACK");
});

test("fences the end of a coarse SQLite usage timestamp and rejects ambiguous linked usage", (t) => {
  const { db, addUsage } = sqliteCostFixture(t);
  db.exec("UPDATE usage_events SET ts='1970-01-01 00:00:06' WHERE id='authoritative-fixture'");
  assert.throws(() => readOwnedSqliteProviderExposure(db, sqliteWindow), /provider usage timestamp/);
  assert.equal(readOwnedSqliteProviderExposure(db, { ...sqliteWindow, asOfMs: 7000 })
    .settledExactUsagePricedCents, 2);
  db.exec("UPDATE usage_events SET ts='bad' WHERE id='authoritative-fixture'");
  assert.throws(() => readOwnedSqliteProviderExposure(db, sqliteWindow), /provider usage timestamp/);
  db.exec("UPDATE usage_events SET ts='1970-01-01 00:00:05' WHERE id='authoritative-fixture'");
  addUsage("duplicate-fixture", "1970-01-01 00:00:05");
  assert.throws(() => readOwnedSqliteProviderExposure(db, sqliteWindow), /duplicate/);
  db.exec("BEGIN DEFERRED; ROLLBACK");
});

test("the PostgreSQL acquisition adapter refuses overflow or out-of-scope rows", async () => {
  const snapshot = { holds: [], events: [], allowances: [], meters: [] };
  const query = (value) => async () => ({ rows: [{ snapshot: value }] });
  for (const key of Object.keys(snapshot)) {
    await assert.rejects(readOwnedPostgresProviderExposure(
      query({ ...snapshot, [key]: Array(20_001).fill(null) }), sqliteWindow),
    /provider snapshot exceeds row limit/);
  }
  await assert.rejects(readOwnedPostgresProviderExposure(query({
    ...snapshot, events: [{ ...rawEvent, account_id: "foreign" }],
  }), sqliteWindow), /invalid usage scope/);
  let called = 0;
  await assert.rejects(readOwnedPostgresProviderExposure(async () => { called++; },
    { ...sqliteWindow, closeoutLagMs: 10_000 }), /bounds/);
  assert.equal(called, 0);
});
