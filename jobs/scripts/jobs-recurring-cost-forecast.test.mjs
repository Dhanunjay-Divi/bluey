import assert from "node:assert/strict";
import test from "node:test";
import { forecastJobsRecurringCost } from "./jobs-recurring-cost-forecast.mjs";

const attempt = (attemptKey, outcome, costMicros, applicationKey = null, receiptKey = null) =>
  ({ attemptKey, outcome, costMicros, applicationKey, receiptKey });
const sample = (attempts, sharedVariableMicros = 0) =>
  ({ attempts, sharedVariableMicros, expectedAttempts: attempts.length, basis: "scenario_inputs" });
const month = (count = 100, perUserMicros = 200_000, fixedMicros = 4_000_000, projectedVerifiedSubmissions = 5000) =>
  ({ activeUsers: count, perUserMicros, fixedMicros, projectedVerifiedSubmissions });
const months = value => Array.from({ length: 6 }, () => value ?? month());

test("projects 600 user-months, billing fixed once a month and counting all live attempt waste", () => {
  const result = forecastJobsRecurringCost(sample([
    attempt("a1", "verified", 5000, "app-a", "receipt-a"), attempt("a2", "failed", 1000),
    attempt("b1", "verified", 4000, "app-b", "receipt-b"), attempt("c1", "unknown", 3000),
    attempt("d1", "mock", 50_000), attempt("e1", "rejected", 500),
  ], 500), months());
  assert.equal(result.projectionOnly, true);
  assert.equal(result.userMonths, 600);
  assert.deepEqual(result.counts, { verified: 2, failed: 1, rejected: 1, unknown: 1, mock: 1 });
  assert.equal(result.knownAttemptCostMicros.mock, 50_000);
  assert.equal(result.knownVariableSubtotalMicros, 14_000);
  assert.deepEqual(result.variableRate, { numeratorMicros: 14_000, verifiedSubmissions: 2 });
  assert.deepEqual(result.months[0], { month: 1, activeUsers: 100, projectedVerifiedSubmissions: 5000,
    perUserTotalMicros: 20_000_000, sharedFixedMicros: 4_000_000, variableTotalMicros: 35_000_000,
    totalMicros: 59_000_000, averagePerActiveUserMicros: 590_000 });
  assert.equal(result.sixMonthTotalMicros, 354_000_000);
});

test("keeps partial exposure without using missing expense, volume or cost as zero", () => {
  const accepted = attempt("a1", "verified", 17, "a", "r");
  for (const input of [sample([accepted, attempt("a2", "failed", null)]),
    sample([accepted, attempt("a2", "unknown", null)]), sample([accepted], null)]) {
    const output = forecastJobsRecurringCost(input, months());
    assert.equal(output.knownVariableSubtotalMicros, 17);
    assert.equal(output.variableRate, null);
    assert.equal(output.months[0].variableTotalMicros, null);
    assert.equal(output.sixMonthTotalMicros, null);
  }
  for (const period of [month(100, null), month(100, 8, null), month(100, 8, 9, null)]) {
    assert.equal(forecastJobsRecurringCost(sample([accepted]), months(period)).sixMonthTotalMicros, null);
  }
});

test("keeps mock and zero-outcome runs out of the delivery rate", () => {
  const observations = sample([attempt("a1", "unknown", 100), attempt("a2", "mock", null)]);
  const output = forecastJobsRecurringCost(observations, months());
  assert.equal(output.unpricedMockAttempts, 1);
  assert.equal(output.knownVariableSubtotalMicros, 100);
  assert.equal(output.variableRate, null);
  assert.equal(output.sixMonthTotalMicros, null);
  const liveSample = sample([attempt("a", "verified", 1, "a", "r")]);
  assert.equal(forecastJobsRecurringCost(liveSample, months(month(100, 0, 0, 0))).sixMonthTotalMicros, null);
  assert.equal(forecastJobsRecurringCost(observations, months(month(0, null, 100, 0))).sixMonthTotalMicros, null);
  assert.equal(forecastJobsRecurringCost(liveSample, months(month(0, null, 100, 0))).months[0].sharedFixedMicros, 100);
});

test("rejects duplicate receipts, attempts, applications and incomplete or ambiguous inputs", () => {
  const accepted = attempt("a1", "verified", 100, "a", "r");
  for (const row of [accepted, attempt("a2", "verified", 50, "a", "r2"),
    attempt("a2", "verified", 50, "b", "r"), attempt("a2", "mock", 50, "b", "r2"),
    attempt("a2\n", "failed", 50), attempt("a2", "unknown", 1.5),
    { ...attempt("a2", "failed", 20), accountEmail: "dummy@example.test" }]) {
    assert.throws(() => forecastJobsRecurringCost(sample([accepted, row]), months()), /jobs_cost_forecast:/);
  }
  assert.throws(() => forecastJobsRecurringCost({ ...sample([accepted]), expectedAttempts: 2 }, months()), /:sample/);
  assert.throws(() => forecastJobsRecurringCost(sample([accepted]), months().slice(0, -1)), /:period/);
  assert.throws(() => forecastJobsRecurringCost(sample([accepted]), months(month(0))), /:month/);
});

test("handles uneven months with bounded integer arithmetic and conservative rounding", () => {
  const baseline = sample([attempt("a", "verified", 2, "a", "r1"), attempt("b", "verified", 3, "b", "r2")]);
  const period = [month(1, 0, 2, 1), month(2, 0, 3, 2), ...months(month(1, 0, 0, 1)).slice(2)];
  const output = forecastJobsRecurringCost(baseline, period);
  assert.deepEqual(output.months.map(row => row.totalMicros), [5, 8, 3, 3, 3, 3]);
  assert.deepEqual(output.months.map(row => row.averagePerActiveUserMicros), [5, 4, 3, 3, 3, 3]);
  assert.equal(output.sixMonthTotalMicros, 25);
  assert.throws(() => forecastJobsRecurringCost(baseline, months(month(3, Number.MAX_SAFE_INTEGER))), /:overflow/);
});
