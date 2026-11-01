# FIX-856: Cost receipt validation omitted relational resume authority

> **Codex preflight:** `$bluey-ops`; current source and independent review govern.

## Issue

Round640's unexecuted report draft could classify a stored receipt as
metadata-consistent without reading the application table's resume-version
binding. Independent review identified this before compilation/runtime acceptance.

## Root Cause

The new SQLite/PostgreSQL receipt queries read state/time and the encrypted
application payload but omitted relational `resume_version_id`. Checking the
payload alone cannot establish equality with that separate authoritative column.
The existing execution path rejects a missing or mismatched relational binding.

## Fix Summary

Source implemented, formatted and independently re-reviewed; awaiting execution: both read-only
snapshot paths select the relational binding and require it to be nonempty and
exactly equal to the frozen parsed application resume. SQLite and configured-PG
cases include shared null/mismatched-binding negatives. Do not
change normal preparation/finalization authority or accept a caller success flag.

## Files Modified

- `server/src/db/jobs/customer_data.rs`: report-only stored-receipt adapter and
  inline lifecycle/adversarial tests; correction is source-frozen.
- `docs/work/REVIEW-PHASE-640-JOBS-COST-EVIDENCE.md`: original P1 finding.

## Edge Cases Handled

Required: SQL null/empty, different same-account resume, payload-only match,
valid exact binding, and unchanged application/evidence/object/account state.

## How to Test

Run focused SQLite cases and the required-fresh PostgreSQL parity/adversarial
case through the canonical launcher and existing physical-PG wrapper. No such
execution has occurred yet; format/static review is not a database pass.

## Known Limitations

This is an internal test/integration-only prototype, not a production incident.
The stored-metadata denominator is not employer-delivery/object-byte proof,
invoice COGS or a completed cost/release goal. See Round640 and its review holds.
