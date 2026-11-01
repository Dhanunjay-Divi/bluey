# FIX-857: Cost-report text bounds were enforced after materialization

> **Codex preflight:** `$bluey-ops`; current source and independent review govern.

## Issue

The unexecuted Round640 draft imposed row/8-MiB limits after database text values
had been fetched and copied. A row limit alone does not bound allocated text.
Independent review identified the P2 source defect before acceptance.

## Root Cause

Queries returned unrestricted TEXT into strings/vectors before the cumulative
check. The new report's bounded-work claim was therefore stronger than its
implementation for oversized synthetic or legacy values.

## Fix Summary

Source implemented, formatted and independently re-reviewed; awaiting execution: SQL CASE
projections suppress oversized text before transport. SQLite checks byte length,
not character count; PostgreSQL uses octet length. Streamed rows charge their
conservative byte bound before owned string copies, within one shared8-MiB
budget. Bounded serialization avoids whole-vector JSON buffers. Strict row caps,
tenant predicates and closed failures remain; errors omit rejected content.

## Files Modified

- `server/src/db/jobs_provider_cost_holds.rs`: report cohort/usage input limits.
- `server/src/db/jobs/customer_data.rs`: application/evidence/object input limits.
- `docs/work/REVIEW-PHASE-640-JOBS-COST-EVIDENCE.md`: original P2 finding.

## Edge Cases Handled

Required: one oversized value, many individually valid values exceeding the
combined bound, multibyte text, null/sentinel detection, both backends, and
unchanged source database state after rejection. Do not silently truncate rows
or substitute zero cost for rejected input.

## How to Test

Inspect source-level SQL pre-allocation checks and execute bounded synthetic
SQLite/required-fresh PostgreSQL negative cases. Existing post-load rejection
does not prove pre-allocation safety. No runtime pass is claimed yet.

## Known Limitations

No public report route or production exposure was introduced by the draft.
Whole-report, database/immutability, full-source and capacity gates remain open.
