# FIX-829: Review-first protocol harness demands an auto-submit ATS binding

## Issue

Private run69688 passed the new native request-ID validator test and received a
real managed-client claim200 (1291ms), then failed exact_prepared_bindings. The
combined assertion discarded individual counts; no full lifecycle pass occurred.

## Root Cause

The ordinary harness explicitly creates a review_first packet and approves it
through normal APIs, but demands one preflight ATS binding. Both production
SQLite/PostgreSQL application_requires_ats_phase_a predicates require auto_submit
plus frozen certification. The harness therefore expects an auto-submit policy
effect for a review-first run. Do not create fake ATS rows, switch consent modes,
or relax production predicates to make the assertion pass. Other individual count
values from69688 were not retained and must not be invented.

## Fix Summary

Read back persisted submission_mode and require review_first. Report explicit
lease/volume/ATS/alias counts instead of a generic exactBindings boolean. Require
one lease, exactly one total volume binding and one exact volume/epoch/process
match, no run-ID session alias, and zero
ATS rows independently across the account, application and run. ATS counts have
no phase/profile/session filters that could hide a malformed binding. No product
code, authority, approval mode or ATS guard changes.

## Files Modified

- `jobs/scripts/nonprod-runner-control-plane.mjs`
- `jobs/scripts/nonprod-runner-control-plane.test.mjs`
- Phase638 qualification records.

## Edge Cases Handled

Mode drift, missing lease, duplicate volume binding, any account/application/run
ATS row, and an alias session each fail report validation. A review-first prepared
lifecycle cannot masquerade as auto-submit ATS qualification.

## How to Test

65 Node report/diagnostic/cleanup tests pass, zero skipped. Actual review-first
claim/heartbeat/safe-release/fresh terminal replay remains pending the reviewed
private API rerun. Auto-submit ATS admission remains a separate required gate.

Subsequent actual80446 passed that bounded review-first lifecycle after FIX830
corrected encrypted-storage mode readback. FIX830 records exact source/artifacts,
observed state and cleanup. This supersedes the pending runtime note above, not
the separate auto-submit ATS or broader negative/full release gates.

Run69688 was exit1: Rust test compile2m38s, new256-vector validator test1/1 passed;
API/fixture build2.84s. No PGerror/panic/blockers observed. APIcleanuptrue/PGexited,
exact root removed/parentempty.774-source manifest
9e928d8c3a34d16bd78d5740254d1d99869735ceccec98bd1c94e1e72f00e4e8
and HEAD/status matched. API SHA198c507a0ab29b551987ae4492a50f1794a1a5f3468766c611dc37c9ef163159;
fixture SHA7e010e8f6568eec15923f0175bb466a77e6226094d64737bd8e478cf5e9c13aa.

## Known Limitations

Claim200 is not complete prepared protocol acceptance. Neither this correction
nor a later review-first pass establishes automatic employer submissions, native
runtime measurement, cloud capacity, provider coverage or production readiness.
