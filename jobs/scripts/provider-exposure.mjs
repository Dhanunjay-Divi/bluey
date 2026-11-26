import { createHash } from "node:crypto";

// Offline reporting only. Row inputs and owned SQLite handles must come from
// a trusted caller. This module does not open a database, contact an endpoint,
// or change any admission/billing state.
const validAmount = (value) => Number.isSafeInteger(value) && value >= 0;
const validScope = (value) => typeof value === "string" && /^[a-f0-9]{64}$/.test(value) && value.length === 64;
const provenance = new Set(["exact", "estimated", "missing"]);
const maxHoldCents = 100_000_000;
// Matches UPSTREAM_SPEND_TRUTH_RETENTION_MS; the admission window is separate.
const maxRetentionMs = 31 * 24 * 60 * 60 * 1000;
const maxSnapshotRows = 20_000;
const maxStoredApplicationBytes = 8 * 1024 * 1024;
const defaultReceiptChecks = 8;
const maxReceiptChecksPerReport = 32;

function scopeHash(domain, ...parts) {
  const digest = createHash("sha256").update("bluey-provider-cost-hold-v1\0").update(domain);
  for (const part of parts) {
    if (typeof part !== "string" || part.length === 0) throw new Error("invalid provider usage identity");
    const bytes = Buffer.from(part, "utf8");
    const length = Buffer.alloc(8);
    length.writeBigUInt64BE(BigInt(bytes.length));
    digest.update(length).update(bytes);
  }
  return digest.digest("hex");
}

// One input row per Jobs résumé-generation hold, with usage matches in an array so a
// one-to-many join cannot silently duplicate spend or hide ambiguous linkage.
export function summarizeProviderHoldRows(rows) {
  if (!Array.isArray(rows)) throw new Error("invalid provider hold rows");
  const seen = new Set();
  let releasedCount = 0;
  const attempts = [];
  for (const row of rows) {
    if (!row || !validScope(row.request_scope_hash) || seen.has(row.request_scope_hash)
      || !validScope(row.account_scope_hash) || !validAmount(row.projected_cost_cents)
      || row.projected_cost_cents < 1 || row.projected_cost_cents > maxHoldCents
      || !validAmount(row.settled_cost_cents) || row.settled_cost_cents > maxHoldCents
      || !provenance.has(row.usage_provenance) || !Array.isArray(row.usageEvents)
      || row.usageEvents.length > 1) throw new Error("invalid or duplicate provider hold row");
    seen.add(row.request_scope_hash);
    const base = { scopeHash: row.request_scope_hash, projectedCents: row.projected_cost_cents };
    if (row.status === "held" || row.status === "released") {
      if (row.usageEvents.length !== 0 || row.settled_cost_cents !== 0 || row.usage_provenance !== "missing") {
        throw new Error("invalid unsettled provider hold");
      }
      if (row.status === "released") releasedCount++;
      else attempts.push({ ...base, state: "held", settledCents: null, usage: null });
      continue;
    }
    if (row.status !== "settled" || (row.usage_provenance !== "exact"
      && row.settled_cost_cents < row.projected_cost_cents)) throw new Error("invalid settled provider hold");
    if (row.usageEvents.length === 1) {
      const event = row.usageEvents[0];
      if (!event || event.origin !== "server" || event.kind !== "jobs_resume_generation_attempt"
        || typeof row.provider !== "string" || !row.provider || typeof row.model !== "string" || !row.model
        || event.provider !== row.provider || event.model !== row.model
        || event.cost_cents_to_bluey !== row.settled_cost_cents
        || scopeHash("account", event.account_id) !== row.account_scope_hash
        || scopeHash("request", event.account_id, event.request_id) !== row.request_scope_hash) {
        throw new Error("provider usage does not match its hold");
      }
    }
    attempts.push({ ...base, state: "settled", settledCents: row.settled_cost_cents,
      usage: row.usageEvents.length ? row.usage_provenance : null });
  }
  return { ...summarizeProviderExposure(attempts), releasedCount };
}

// The caller supplies one consistent cohort. An allowance is only a pointer;
// even a matched packet-meter row is not a successful submission.
export function summarizeProviderApplicationExposure(rows, allowances, meters = []) {
  const total = summarizeProviderHoldRows(rows);
  if (!Array.isArray(allowances)) throw new Error("invalid generation allowances");
  if (!Array.isArray(meters)) throw new Error("invalid packet metering rows");
  const packetKey = (row) => scopeHash("packet-meter", row.account_id, row.job_id);
  const packets = new Map();
  for (const row of meters) {
    if (!row || typeof row.application_id !== "string" || !row.application_id) {
      throw new Error("invalid packet metering row");
    }
    const key = packetKey(row);
    if (packets.has(key)) throw new Error("duplicate packet metering row");
    packets.set(key, row.application_id);
  }
  const generations = new Map();
  for (const row of allowances) {
    if (!row || !["reserved", "released", "committed"].includes(row.status)
      || (row.application_id !== null && (row.status !== "committed"
        || typeof row.application_id !== "string" || !row.application_id))) {
      throw new Error("invalid generation allowance");
    }
    const generation = scopeHash("generation", row.account_id, row.generation_key);
    if (generations.has(generation)) throw new Error("duplicate generation allowance");
    const meter = packets.get(packetKey(row));
    let category = "uncommitted";
    if (row.status === "released") category = "releasedAllowance";
    if (row.status === "committed") {
      if (row.application_id === null) category = "missingApplication";
      else if (meter === undefined) category = "unverifiedApplicationPointer";
      else if (meter === row.application_id) category = "meteredPacket";
      else throw new Error("generation allowance and packet metering disagree");
    }
    generations.set(generation, {
      account: scopeHash("account", row.account_id),
      category,
    });
  }
  const buckets = Object.fromEntries(
    ["meteredPacket", "unverifiedApplicationPointer", "missingApplication",
      "uncommitted", "releasedAllowance", "missingAllowance"]
      .map((key) => [key, { attemptCount: 0, exposureCents: 0 }]),
  );
  for (const row of rows) {
    if (!validScope(row.generation_scope_hash)) throw new Error("invalid provider generation scope");
    const allowance = generations.get(row.generation_scope_hash);
    if (allowance && allowance.account !== row.account_scope_hash) {
      throw new Error("provider generation allowance account mismatch");
    }
    if (row.status === "released") continue;
    const bucket = buckets[allowance?.category ?? "missingAllowance"];
    bucket.attemptCount++;
    bucket.exposureCents += row.status === "held" ? row.projected_cost_cents : row.settled_cost_cents;
  }
  return { ...total, applicationAttribution: buckets };
}

// Fence the single-account observation before selecting attempts by immutable
// creation time. A current row updated after asOfMs cannot reconstruct prior state.
export function summarizeProviderExposureWindow(rows, allowances, window, meters = []) {
  if (!Array.isArray(rows) || !Array.isArray(allowances) || !Array.isArray(meters)
    || !window || typeof window !== "object" || typeof window.accountId !== "string" || !window.accountId) {
    throw new Error("invalid provider exposure window");
  }
  const { accountId, startMs, endMs, asOfMs, closeoutLagMs, safetyMarginMs, retentionMs } = window;
  if (![startMs, endMs, asOfMs, closeoutLagMs, safetyMarginMs, retentionMs].every(validAmount)
    || startMs >= endMs || endMs > asOfMs || asOfMs - endMs < closeoutLagMs
    || safetyMarginMs < 1 || retentionMs < 1 || retentionMs > maxRetentionMs
    || safetyMarginMs >= retentionMs || asOfMs - startMs >= retentionMs - safetyMarginMs) {
    throw new Error("provider exposure window outside closeout or retention bounds");
  }
  const selected = [];
  const accountScope = scopeHash("account", accountId);
  let outsideWindowRows = 0;
  let updatedAfterWindowRows = 0;
  for (const row of rows) {
    if (!row || !validAmount(row.created_at_ms) || !validAmount(row.updated_at_ms)
      || row.updated_at_ms < row.created_at_ms) throw new Error("invalid provider hold timestamps");
    if (row.account_scope_hash !== accountScope) throw new Error("provider observation account mismatch");
    if (row.created_at_ms < startMs || row.created_at_ms >= endMs) {
      outsideWindowRows++;
      continue;
    }
    if (row.updated_at_ms > asOfMs) throw new Error("provider hold state is newer than report cutoff");
    // The reader must project an inclusive upper bound for usage_events.ts,
    // never a timestamp rounded down across a precision boundary.
    // Do not let a later linked event improve the cutoff report's provenance.
    // The database clocks/precision can differ; do not infer sequence against
    // the hold timestamp from two fields written by the same transaction.
    if (Array.isArray(row.usageEvents) && row.usageEvents.some((event) =>
      !validAmount(event?.ts_upper_bound_ms) || event.ts_upper_bound_ms > asOfMs)) {
      throw new Error("provider usage timestamp is invalid or newer than report cutoff");
    }
    if (row.updated_at_ms >= endMs) updatedAfterWindowRows++;
    selected.push(row);
  }
  for (const row of allowances) {
    if (!row || !validAmount(row.created_at_ms) || !validAmount(row.updated_at_ms)
      || row.updated_at_ms < row.created_at_ms) throw new Error("invalid generation allowance timestamps");
    if (row.account_id !== accountId) throw new Error("provider observation account mismatch");
    if (row.updated_at_ms > asOfMs) throw new Error("generation allowance is newer than report cutoff");
  }
  for (const row of meters) {
    if (!row || !validAmount(row.created_at_ms)) throw new Error("invalid packet metering timestamp");
    if (row.account_id !== accountId) throw new Error("provider observation account mismatch");
    if (row.created_at_ms > asOfMs) throw new Error("packet metering is newer than report cutoff");
  }
  return { ...summarizeProviderApplicationExposure(selected, allowances, meters),
    observation: { startMs, endMs, asOfMs, closeoutLagMs, safetyMarginMs, retentionMs,
      outsideWindowRows, updatedAfterWindowRows, cohortCompletenessVerified: false } };
}

// Read one bounded, account-scoped snapshot from a caller-owned SQLite handle.
// This is an offline adapter; it does not reconstruct an older MVCC snapshot,
// attest retention, or establish the verified-submission denominator.
export function readOwnedSqliteProviderExposure(database, window) {
  return summarizeSnapshot(readOwnedSqliteSnapshot(database, window, false), window);
}

function readOwnedSqliteSnapshot(database, window, includeSubmissions, receiptCheckLimit) {
  summarizeProviderExposureWindow([], [], window);
  const { accountId } = window;
  const accountScope = scopeHash("account", accountId);
  const select = (sql, identity) => database.prepare(sql + " LIMIT ?").all(identity, maxSnapshotRows + 1);
  database.exec("BEGIN DEFERRED");
  try {
    const holds = select(
      `SELECT request_scope_hash, account_scope_hash, generation_scope_hash, provider, model,
              projected_cost_cents, settled_cost_cents, usage_provenance, status,
              created_at_ms, updated_at_ms
       FROM jobs_provider_cost_holds WHERE account_scope_hash = ?`, accountScope);
    const events = select(
      `SELECT account_id, request_id, origin, kind, provider, model, cost_cents_to_bluey,
              CASE WHEN strftime('%Y-%m-%d %H:%M:%S', ts) = ts
                   THEN (CAST(strftime('%s', ts) AS INTEGER) + 1) * 1000
              END AS ts_upper_bound_ms
       FROM usage_events WHERE account_id = ? AND kind = 'jobs_resume_generation_attempt'`, accountId);
    const allowances = select(
      `SELECT account_id, job_id, generation_key, status, application_id, created_at_ms, updated_at_ms
       FROM jobs_generation_allowance_reservations WHERE account_id = ?`, accountId);
    const meters = select(
      `SELECT account_id, job_id, application_id, created_at_ms
       FROM jobs_packet_metering WHERE account_id = ?`, accountId);
    const snapshot = { holds, events, allowances, meters };
    if (includeSubmissions) {
      const submissions = [];
      const { startMs, endMs, asOfMs } = window;
      // Materialize routing metadata only. Do not fetch stored bodies for an
      // over-budget cohort, even though the report still needs its counts.
      const stream = database.prepare(`WITH candidates AS MATERIALIZED (
          SELECT id, account_id, state, submitted_at_ms, updated_at_ms,
            CASE WHEN state = 'submitted' AND submitted_at_ms >= ? AND submitted_at_ms < ? AND updated_at_ms <= ?
                   AND length(CAST(application_json AS BLOB)) <= ? THEN 1 ELSE 0 END AS digest_candidate
          FROM jobs_applications WHERE account_id = ? AND (state = 'submitted' OR submitted_at_ms IS NOT NULL)
          LIMIT ?
        )
        SELECT c.*, CASE WHEN c.digest_candidate = 1 AND budget.candidate_count <= ? THEN (
          SELECT application_json FROM jobs_applications
          WHERE id = c.id AND account_id = c.account_id
        ) END AS raw
        FROM candidates c CROSS JOIN (SELECT sum(digest_candidate) AS candidate_count FROM candidates) budget`
      ).iterate(startMs, endMs, asOfMs, maxStoredApplicationBytes, accountId, maxSnapshotRows + 1, receiptCheckLimit);
      for (const { raw, ...row } of stream) {
        if (submissions.length === maxSnapshotRows) throw new Error("provider snapshot exceeds row limit");
        submissions.push({ ...row, stored_application_sha256: raw === null
          ? null : createHash("sha256").update(raw, "utf8").digest("hex") });
      }
      snapshot.submissions = submissions;
    }
    return snapshot;
  } finally {
    database.exec("ROLLBACK");
  }
}

// Keep the four sources in a single PostgreSQL statement snapshot. The
// caller supplies an authorized, read-only connection; this code never
// obtains credentials or opens a connection. An exact/native timestamp
// rounded upward cannot falsely place sub-millisecond data before cutoff.
export const OWNED_POSTGRES_PROVIDER_SNAPSHOT_SQL = `
SELECT json_build_object(
  'holds', (SELECT coalesce(json_agg(h), '[]'::json) FROM (
    SELECT request_scope_hash, account_scope_hash, generation_scope_hash, provider, model,
           projected_cost_cents, settled_cost_cents, usage_provenance, status,
           created_at_ms, updated_at_ms
    FROM jobs_provider_cost_holds WHERE account_scope_hash = $1 LIMIT $3) h),
  'events', (SELECT coalesce(json_agg(e), '[]'::json) FROM (
    SELECT account_id, request_id, origin, kind, provider, model, cost_cents_to_bluey,
           CASE WHEN isfinite(ts) THEN ceil(extract(epoch FROM ts) * 1000)::bigint
           END AS ts_upper_bound_ms
    FROM usage_events WHERE account_id = $2 AND kind = 'jobs_resume_generation_attempt' LIMIT $3) e),
  'allowances', (SELECT coalesce(json_agg(a), '[]'::json) FROM (
    SELECT account_id, job_id, generation_key, status, application_id, created_at_ms, updated_at_ms
    FROM jobs_generation_allowance_reservations WHERE account_id = $2 LIMIT $3) a),
  'meters', (SELECT coalesce(json_agg(m), '[]'::json) FROM (
    SELECT account_id, job_id, application_id, created_at_ms
    FROM jobs_packet_metering WHERE account_id = $2 LIMIT $3) m)
) AS snapshot`;

// Only a budget-admitted cohort reaches the digest expression. The candidates
// CTE retains no bodies. PostgreSQL's SHA-256 is core; no extension or migration.
export const OWNED_POSTGRES_PROVIDER_YIELD_SNAPSHOT_SQL = `
WITH costs AS (${OWNED_POSTGRES_PROVIDER_SNAPSHOT_SQL}), candidates AS MATERIALIZED (
  SELECT id, account_id, state, submitted_at_ms, updated_at_ms,
         CASE WHEN state = 'submitted' AND submitted_at_ms >= $5 AND submitted_at_ms < $6 AND updated_at_ms <= $7
                AND octet_length(application_json) <= $4 THEN 1 ELSE 0 END AS digest_candidate
  FROM jobs_applications
  WHERE account_id = $2 AND (state = 'submitted' OR submitted_at_ms IS NOT NULL) LIMIT $3
)
SELECT costs.snapshot::jsonb || jsonb_build_object('submissions', (
  SELECT coalesce(jsonb_agg(a), '[]'::jsonb) FROM (
    SELECT c.*, CASE WHEN c.digest_candidate = 1 AND budget.candidate_count <= $8 THEN (
      SELECT encode(sha256(convert_to(application_json, 'UTF8')), 'hex')
      FROM jobs_applications WHERE id = c.id AND account_id = c.account_id
    ) END AS stored_application_sha256
    FROM candidates c CROSS JOIN (SELECT sum(digest_candidate) AS candidate_count FROM candidates) budget
  ) a)) AS snapshot FROM costs`;

export async function readOwnedPostgresProviderExposure(query, window) {
  const observation = { ...window };
  summarizeProviderExposureWindow([], [], observation);
  if (typeof query !== "function") throw new Error("invalid provider snapshot reader");
  const { accountId } = observation;
  const result = await query(OWNED_POSTGRES_PROVIDER_SNAPSHOT_SQL,
    [scopeHash("account", accountId), accountId, maxSnapshotRows + 1]);
  if (!Array.isArray(result?.rows) || result.rows.length !== 1) {
    throw new Error("invalid provider snapshot result");
  }
  return summarizeSnapshot(result.rows[0]?.snapshot, observation);
}

function checkReceiptBudget(verifyStoredSubmission, receiptCheckLimit) {
  if (typeof verifyStoredSubmission !== "function" || !validAmount(receiptCheckLimit)
    || receiptCheckLimit < 1 || receiptCheckLimit > maxReceiptChecksPerReport) {
    throw new Error("invalid submission verifier or receipt check limit");
  }
}

export async function readOwnedSqliteProviderYield(
  database, window, verifyStoredSubmission, receiptCheckLimit = defaultReceiptChecks,
) {
  checkReceiptBudget(verifyStoredSubmission, receiptCheckLimit);
  const observation = { ...window };
  return summarizeProviderYieldSnapshot(
    readOwnedSqliteSnapshot(database, observation, true, receiptCheckLimit), observation,
    verifyStoredSubmission, receiptCheckLimit,
  );
}

export async function readOwnedPostgresProviderYield(
  query, window, verifyStoredSubmission, receiptCheckLimit = defaultReceiptChecks,
) {
  checkReceiptBudget(verifyStoredSubmission, receiptCheckLimit);
  const observation = { ...window };
  summarizeProviderExposureWindow([], [], observation);
  if (typeof query !== "function") throw new Error("invalid provider snapshot reader");
  const { accountId, startMs, endMs, asOfMs } = observation;
  const result = await query(OWNED_POSTGRES_PROVIDER_YIELD_SNAPSHOT_SQL,
    [scopeHash("account", accountId), accountId, maxSnapshotRows + 1, maxStoredApplicationBytes,
      startMs, endMs, asOfMs, receiptCheckLimit]);
  if (!Array.isArray(result?.rows) || result.rows.length !== 1) {
    throw new Error("invalid provider snapshot result");
  }
  return summarizeProviderYieldSnapshot(result.rows[0]?.snapshot, observation, verifyStoredSubmission,
    receiptCheckLimit);
}

// `verifyStoredSubmission` is trusted application code. A positive verdict
// must use the full server receipt validators and object readback against the
// exact captured application digest, not status, metering, or the digest alone.
async function summarizeProviderYieldSnapshot(snapshot, window, verifyStoredSubmission, receiptCheckLimit) {
  const exposure = summarizeSnapshot(snapshot, window);
  if (!Array.isArray(snapshot.submissions) || snapshot.submissions.length > maxSnapshotRows
    || typeof verifyStoredSubmission !== "function") throw new Error("invalid submission observation");
  const { accountId, startMs, endMs, asOfMs } = window;
  const counts = { verifiedSuccesses: 0, unverifiedSubmissions: 0, unavailableSubmissions: 0,
    outsideWindowSubmissions: 0, lateObservedSubmissions: 0, missingSubmissionTimestamp: 0 };
  const seen = new Set();
  const selected = [];
  for (const row of snapshot.submissions) {
    if (!row || row.account_id !== accountId || typeof row.id !== "string" || !row.id
      || row.id.length > 256 || seen.has(row.id) || typeof row.state !== "string" || !row.state
      || !validAmount(row.updated_at_ms) || ![0, 1].includes(row.digest_candidate)
      || (row.stored_application_sha256 !== null && (row.digest_candidate !== 1
        || !validScope(row.stored_application_sha256)))) throw new Error("invalid or duplicate submission observation");
    seen.add(row.id);
    const time = row.submitted_at_ms;
    if (row.digest_candidate === 1 && (row.state !== "submitted" || !validAmount(time)
      || time < startMs || time >= endMs || row.updated_at_ms > asOfMs)) {
      throw new Error("submission digest candidate is outside the observation");
    }
    if (time === null && row.state === "submitted") {
      counts.missingSubmissionTimestamp++;
      continue;
    }
    if (!validAmount(time) || time === 0 || row.updated_at_ms < time) {
      throw new Error("invalid submission timestamp");
    }
    if (time < startMs || time >= endMs) {
      counts.outsideWindowSubmissions++;
      if (time >= endMs && row.updated_at_ms <= asOfMs && row.state === "submitted") {
        counts.lateObservedSubmissions++;
      }
      continue;
    }
    if (row.updated_at_ms > asOfMs) throw new Error("submission observation is newer than report cutoff");
    if (row.state !== "submitted") { counts.unverifiedSubmissions++; continue; }
    if (row.digest_candidate === 0) { counts.unavailableSubmissions++; continue; }
    selected.push(Object.freeze({ id: row.id, account_id: accountId, submitted_at_ms: time,
      updated_at_ms: row.updated_at_ms, storedApplicationSha256: row.stored_application_sha256 }));
  }
  // Do not verify just the first N receipts. A partial scan would give the
  // full period's exposure an arbitrary denominator. A production host still
  // needs its own aggregate byte/time policy and an authenticated verifier.
  const overBudget = selected.length > receiptCheckLimit;
  if (selected.some((row) => (row.storedApplicationSha256 === null) !== overBudget)) {
    throw new Error("submission digest work does not match receipt budget");
  }
  const receiptVerification = { checkLimit: receiptCheckLimit, eligibleCandidates: selected.length,
    deferredCandidates: overBudget ? selected.length : 0 };
  if (overBudget) {
    return { ...exposure, verifiedSubmissions: counts, receiptVerification,
      providerExposurePerObservedVerifiedSubmission: null };
  }
  // Validate the complete cohort first. Both readers may return rows in a
  // different order; use the same ordered, bounded plan before starting I/O.
  selected.sort((a, b) => a.submitted_at_ms - b.submitted_at_ms
    || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  for (const [index, candidate] of selected.entries()) {
    const result = await verifyStoredSubmission(accountId, candidate);
    if (!result || Object.keys(result).sort().join(",") !== "storedApplicationSha256,verdict"
      || result.storedApplicationSha256 !== candidate.storedApplicationSha256) {
      throw new Error("submission verification does not bind the observed application");
    }
    if (result.verdict === "verified") counts.verifiedSuccesses++;
    else if (result.verdict === "unverified") counts.unverifiedSubmissions++;
    else if (result.verdict === "unavailable") {
      // This period's denominator is incomplete. Preserve the outcomes we
      // observed and do not ask the same unavailable verifier to read more.
      counts.unavailableSubmissions++;
      receiptVerification.deferredCandidates = selected.length - index - 1;
      break;
    } else throw new Error("invalid submission verification outcome");
  }
  return { ...exposure, verifiedSubmissions: counts, receiptVerification,
    providerExposurePerObservedVerifiedSubmission: counts.verifiedSuccesses > 0
      && counts.unavailableSubmissions === 0 && counts.missingSubmissionTimestamp === 0
      && receiptVerification.deferredCandidates === 0
      && exposure.usageLinkage.orphanUsageRowsWithinObservation === 0
      && exposure.usageLinkage.orphanUsageRowsCutoffUnknown === 0
      && exposure.providerExposureCents > 0
      ? { numeratorCents: exposure.providerExposureCents, denominator: counts.verifiedSuccesses }
      : null };
}

function summarizeSnapshot(snapshot, window) {
  if (!snapshot) throw new Error("invalid provider snapshot result");
  const { holds, events, allowances, meters } = snapshot;
  for (const rows of [holds, events, allowances, meters]) {
    if (!Array.isArray(rows)) throw new Error("invalid provider snapshot result");
    if (rows.length > maxSnapshotRows) throw new Error("provider snapshot exceeds row limit");
  }
  const linked = new Map(holds.map((row) => [row.request_scope_hash, []]));
  const usageLinkage = { orphanUsageRowsBeforeWindow: 0, orphanUsageRowsWithinObservation: 0,
    orphanUsageRowsCutoffUnknown: 0 };
  for (const event of events) {
    if (!event || event.account_id !== window.accountId || event.kind !== "jobs_resume_generation_attempt") {
      throw new Error("invalid usage scope in provider snapshot");
    }
    const matches = linked.get(scopeHash("request", event.account_id, event.request_id));
    if (matches) {
      matches.push(event);
    } else {
      // The amount comes from holds, never from usage. An unlinked event
      // can expose a gap, but its timestamp cannot supply attempt creation
      // time or make an ambiguous charge attributable to this period.
      const timestamp = event.ts_upper_bound_ms;
      if (event.origin !== "server" || !validAmount(timestamp) || timestamp > window.asOfMs) {
        usageLinkage.orphanUsageRowsCutoffUnknown++;
      } else if (timestamp < window.startMs) {
        usageLinkage.orphanUsageRowsBeforeWindow++;
      } else {
        usageLinkage.orphanUsageRowsWithinObservation++;
      }
    }
  }
  const exposure = summarizeProviderExposureWindow(
    holds.map((row) => ({ ...row, usageEvents: linked.get(row.request_scope_hash) })),
    allowances, window, meters);
  return { ...exposure, usageLinkage };
}

// Only a trusted caller using the server receipt/evidence validators may
// supply verifyStoredSubmission. This counts observed successes, never a
// complete cohort or a cost-per-success figure.
export async function countObservedVerifiedSubmissions(candidates, cohort, verifyStoredSubmission) {
  if (!Array.isArray(candidates) || !cohort || typeof verifyStoredSubmission !== "function"
    || typeof cohort.accountId !== "string" || !cohort.accountId
    || ![cohort.startMs, cohort.endMs, cohort.asOfMs].every(validAmount)
    || cohort.startMs >= cohort.endMs || cohort.endMs > cohort.asOfMs) {
    throw new Error("invalid submission cohort");
  }
  const { accountId, startMs, endMs, asOfMs } = cohort;
  const seen = new Set();
  const selected = [];
  let verifiedSuccesses = 0;
  let unverifiedSubmissions = 0;
  let outsideWindowSubmissions = 0;
  for (const candidate of candidates) {
    if (!candidate || candidate.account_id !== accountId || typeof candidate.id !== "string"
      || !candidate.id || seen.has(candidate.id) || !validAmount(candidate.submitted_at_ms)
      || candidate.submitted_at_ms === 0 || !validAmount(candidate.updated_at_ms)
      || candidate.updated_at_ms < candidate.submitted_at_ms) {
      throw new Error("invalid or duplicate submission candidate");
    }
    seen.add(candidate.id);
    if (candidate.submitted_at_ms < startMs || candidate.submitted_at_ms >= endMs) {
      outsideWindowSubmissions++;
      continue;
    }
    if (candidate.updated_at_ms > asOfMs) throw new Error("submission candidate is newer than report cutoff");
    selected.push(candidate);
  }
  // Validate the whole cohort before the trusted verifier reads any evidence.
  for (const candidate of selected) {
    const verified = await verifyStoredSubmission(accountId, candidate);
    if (verified === true) verifiedSuccesses++;
    else if (verified === false) unverifiedSubmissions++;
    else throw new Error("invalid submission verification outcome");
  }
  return { verifiedSuccesses, unverifiedSubmissions, outsideWindowSubmissions,
    cohortCompletenessVerified: false, perVerifiedApplicationCents: null };
}

export function summarizeProviderExposure(attempts) {
  const totals = {
    heldUnresolvedProjectionCents: 0,
    settledExactUsagePricedCents: 0,
    settledEstimatedConservativeCents: 0,
    settledMissingConservativeCents: 0,
    settledUnattributedCents: 0,
  };
  const seen = new Set();
  if (!Array.isArray(attempts)) throw new Error("invalid provider attempts");
  for (const attempt of attempts) {
    if (!attempt || typeof attempt !== "object"
      || !validScope(attempt.scopeHash)
      || seen.has(attempt.scopeHash) || !validAmount(attempt.projectedCents)) {
      throw new Error("invalid or duplicate provider attempt");
    }
    seen.add(attempt.scopeHash);
    let key;
    let amount;
    if (attempt.state === "held" && attempt.settledCents === null && attempt.usage === null) {
      key = "heldUnresolvedProjectionCents";
      amount = attempt.projectedCents;
    } else if (attempt.state === "settled" && validAmount(attempt.settledCents)) {
      switch (attempt.usage) {
        case "exact": key = "settledExactUsagePricedCents"; break;
        case "estimated": key = "settledEstimatedConservativeCents"; break;
        case "missing": key = "settledMissingConservativeCents"; break;
        case null: key = "settledUnattributedCents"; break;
        default: throw new Error("invalid provider usage attribution");
      }
      amount = attempt.settledCents;
    } else {
      throw new Error("invalid provider attempt state");
    }
    totals[key] += amount;
    if (!validAmount(totals[key])) throw new Error("provider exposure exceeds safe range");
  }
  const providerExposureCents = Object.values(totals).reduce((sum, amount) => sum + amount, 0);
  if (!validAmount(providerExposureCents)) throw new Error("provider exposure exceeds safe range");
  return { ...totals, providerExposureCents, attemptCount: seen.size,
    scope: "partial_provider_exposure", perVerifiedApplicationCents: null };
}
