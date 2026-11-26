// Offline planning math only. The caller must supply an already verified
// cohort from one observation cutoff. This is not receipt or invoice authority.
const MAX_ROWS = 10_000;
const OUTCOMES = new Set(["verified", "failed", "rejected", "unknown", "mock"]);
const KEY = /^[a-z0-9][a-z0-9_-]{0,127}$/;
const amount = value => Number.isSafeInteger(value) && value >= 0;
const money = value => value === null || amount(value);
const boundedKey = value => typeof value === "string" && value.length <= 128 && KEY.exec(value)?.[0] === value;
const shape = (value, keys) => value !== null && typeof value === "object"
  && Object.getPrototypeOf(value) === Object.prototype && Object.keys(value).sort().join(",") === keys;

function need(ok, reason) { if (!ok) throw new Error(`jobs_cost_forecast:${reason}`); }
function asNumber(value) {
  need(value <= BigInt(Number.MAX_SAFE_INTEGER), "overflow");
  return Number(value);
}
function multiply(a, b) { return asNumber(BigInt(a) * BigInt(b)); }
function add(values) { return asNumber(values.reduce((sum, value) => sum + BigInt(value), 0n)); }
function divideUp(a, b) { return asNumber((a + b - 1n) / b); }

/**
 * The variable sample contains every attempt, including retries, at one
 * cutoff. Attempt amounts include only variable service cost; shared/per-user
 * monthly allocation must not be duplicated here. Dollar inputs are integer
 * USD micros, never parsed from monetary text. `null` means unmeasured.
 */
export function forecastJobsRecurringCost(sample, months) {
  need(shape(sample, "attempts,basis,expectedAttempts,sharedVariableMicros")
    && ["observed_inputs", "scenario_inputs"].includes(sample.basis)
    && Array.isArray(sample.attempts) && sample.attempts.length <= MAX_ROWS
    && amount(sample.expectedAttempts) && sample.expectedAttempts <= MAX_ROWS
    && sample.attempts.length === sample.expectedAttempts && money(sample.sharedVariableMicros), "sample");
  need(Array.isArray(months) && months.length === 6, "period");

  const attempts = new Set();
  const verified = new Set();
  const receipts = new Set();
  const counts = { verified: 0, failed: 0, rejected: 0, unknown: 0, mock: 0 };
  const amounts = { verified: 0n, failed: 0n, rejected: 0n, unknown: 0n, mock: 0n };
  let unpricedLiveAttempts = 0;
  let unpricedMockAttempts = 0;
  for (const row of sample.attempts) {
    need(shape(row, "applicationKey,attemptKey,costMicros,outcome,receiptKey") && boundedKey(row.attemptKey)
      && !attempts.has(row.attemptKey) && OUTCOMES.has(row.outcome) && money(row.costMicros), "attempt");
    attempts.add(row.attemptKey);
    counts[row.outcome]++;
    if (row.outcome === "verified") {
      need(boundedKey(row.applicationKey) && boundedKey(row.receiptKey)
        && !verified.has(row.applicationKey) && !receipts.has(row.receiptKey), "duplicate_or_invalid_receipt");
      verified.add(row.applicationKey);
      receipts.add(row.receiptKey);
    } else {
      need(row.applicationKey === null && row.receiptKey === null, "nonverified_receipt");
    }
    if (row.costMicros !== null) amounts[row.outcome] += BigInt(row.costMicros);
    else if (row.outcome === "mock") unpricedMockAttempts++;
    else unpricedLiveAttempts++;
  }
  const knownAttemptCostMicros = Object.fromEntries(Object.entries(amounts).map(([key, value]) => [key, asNumber(value)]));
  const knownVariableSubtotalMicros = asNumber(BigInt(sample.sharedVariableMicros ?? 0)
    + amounts.verified + amounts.failed + amounts.rejected + amounts.unknown);
  const variableRate = verified.size > 0 && unpricedLiveAttempts === 0 && sample.sharedVariableMicros !== null
    ? { numeratorMicros: knownVariableSubtotalMicros, verifiedSubmissions: verified.size } : null;

  const rows = [];
  for (let index = 0; index < months.length; index++) {
    const row = months[index];
    need(shape(row, "activeUsers,fixedMicros,perUserMicros,projectedVerifiedSubmissions")
      && amount(row.activeUsers) && money(row.fixedMicros) && money(row.perUserMicros)
      && (row.projectedVerifiedSubmissions === null || amount(row.projectedVerifiedSubmissions))
      && (row.activeUsers > 0 || !row.projectedVerifiedSubmissions), "month");
    const perUserTotalMicros = row.activeUsers === 0 ? 0
      : row.perUserMicros === null ? null : multiply(row.activeUsers, row.perUserMicros);
    const n = row.projectedVerifiedSubmissions;
    // Zero completions cannot establish zero work; previous cohorts can leave
    // pending or failed attempts even when no active user remains this month.
    const variableTotalMicros = !n || variableRate === null ? null
      : divideUp(BigInt(n) * BigInt(variableRate.numeratorMicros), BigInt(variableRate.verifiedSubmissions));
    const parts = [perUserTotalMicros, row.fixedMicros, variableTotalMicros];
    const totalMicros = parts.includes(null) ? null : add(parts);
    rows.push({ month: index + 1, activeUsers: row.activeUsers, projectedVerifiedSubmissions: n,
      perUserTotalMicros, sharedFixedMicros: row.fixedMicros, variableTotalMicros, totalMicros,
      averagePerActiveUserMicros: totalMicros === null || row.activeUsers === 0 ? null
        : divideUp(BigInt(totalMicros), BigInt(row.activeUsers)) });
  }
  const complete = rows.every(row => row.totalMicros !== null);
  return {
    projectionOnly: true, basis: sample.basis, counts, knownAttemptCostMicros, knownVariableSubtotalMicros,
    unpricedLiveAttempts, unpricedMockAttempts, variableRate, months: rows,
    userMonths: add(rows.map(row => row.activeUsers)),
    sixMonthTotalMicros: complete ? add(rows.map(row => row.totalMicros)) : null,
  };
}
