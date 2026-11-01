# FIX-862: Respect PostgreSQL's indexed-value bound in the cost fixture

> **Codex preflight:** `$bluey-ops`; current code and observed execution govern.

## Issue

Run39075 passed formatting, strict all-target Clippy, eight financial SQLite
tests and eight receipt SQLite tests. Its real, required-fresh PostgreSQL case
failed while seeding an oversized `usage_events.request_id`: the existing index
rejected the value before the report could run. The full server suite did not run.

## Root Cause

The shared adversarial helper assumed every large TEXT value accepted by SQLite
could also be stored in PostgreSQL. PostgreSQL correctly rejected this indexed
value with SQLSTATE54000: `index row requires 24048 bytes, maximum size is 8191`.
This is a fixture assumption defect, not evidence the report failed its byte cap.

## Fix Summary

Only the inline test helper changes. PostgreSQL's indexed request-ID case now
requires a real database error, exact PROGRAM_LIMIT_EXCEEDED/54000 and the
observed index-size message prefix. It proves full fixture state unchanged and
both existing usage indexes' definitions, validity and readiness preserved.

The same autocommit client then checks the same bounded SQL projection against
a parameterized derived TEXT row. NULL plus an over-budget integer proves SQL
suppression without trying to store an impossible indexed value. This is
explicitly projection-only evidence, not an actual stored-row report test.

The five other PostgreSQL families still use actual oversized table values,
report rejection, exact restoration and valid-denominator readback. All six
SQLite table paths are unchanged. Final counters require PostgreSQL5+1 versus
SQLite6+0; no index is dropped, no schema/protection changes, and no unexpected
database error is swallowed. Report and production logic are unchanged.

## Files Modified

- `server/src/db/jobs/customer_data.rs`: inline adversarial fixture helper only.

## How to Test / Current Evidence

The original failed run remains preserved, with complete owned process/PG/root
cleanup and unchanged source. Its journal is `phase640-rust-2e465fdbbd4d.jsonl`,
SHA-256 `7d0ce2e673e8006d417b19aea0997f73ae7b062343e89ff5360f30bc4f708846`.
Do not turn its sixteen SQLite passes into a physical-PG or full-suite pass.

Corrected source was formatted and independently reviewed without P0-P3
findings. Customer-data SHA-256 is
`ea7591cbc98aa3c07aaac1857656356722a0213ede4463df62ba2abe0597e07f`.
The778-file successor manifest is
`91ef6fae3f3e49c8a98c3566076abcbc9c8aa84069d02f036847069f3a06cdc0`.
Replacing only that input's hash reconstructs prior5d0a exactly. The reviewed
controller successor changes only its SELF path, manifest and customer pin;
all admission/stage/count/physical-PG/cleanup rules remain unchanged.

Re-execution must pass the corrected physical PostgreSQL case and the complete
strict all-target gate before this batch is runtime-qualified.

## Known Limitations

The report still cannot establish employer delivery, remote object contents,
historical completeness or invoice COGS. Low cost and release readiness are
unproven; no production data, application, email or paid resource is involved.
