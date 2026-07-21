# Round 560 - Jobs Global Feed Canary Recovery And Resume Invariants

Date: 2026-07-21

## Scope

This round hardens the global candidate-feed path after the first production
canary of the Ashby and Lever source families. It does not broaden unattended
submission. External feed records remain candidate leads until Bluey verifies
the original employer posting and selects a certified application adapter.

The round also extends the evidence-locked resume tests. A job-specific resume
may rewrite, reorder, shorten, and emphasize verified evidence, but it must keep
the candidate's contact details, employers, titles, locations, dates,
education, certifications, projects, and selected visual template intact.

## Canary Evidence

The pinned global manifest contained 49 source families, 47 non-empty source
families, 5,096,589 raw records, and 4,458,802 canonical candidate leads.

The initial production allowlist contained only `ashby` and `lever`:

- Ashby parsed 46,180 rows. The worker stopped after 10,000 uploaded rows when
  a later description contained a NUL/control character that PostgreSQL would
  not accept.
- Lever parsed and uploaded all 68,029 rows in 137 batches. Completion then
  failed because PostgreSQL returns `SUM(BIGINT)` as `NUMERIC`, while the Rust
  completion path expected `i64`.

The canary therefore found two real ingestion defects and a more important
visibility boundary: rows from a running or failed snapshot must never become
customer matches.

## Implementation

### Artifact normalization

`jobs/automation/src/jobhive-artifact.ts` removes database-forbidden C0 control
characters while preserving tabs, carriage returns, and line breaks. Field
length and URL checks still run after normalization.

### PostgreSQL completion accounting

`server/src/db/jobs/global_discovery_completion.rs` casts the aggregate row
count to `BIGINT`, matching the Rust completion contract on PostgreSQL without
changing the SQLite path.

### Completed-snapshot publication

`server/src/db/jobs/global_materialization.rs` now includes a global candidate
only when it has an active source membership whose `last_seen_run_id` belongs
to the same source and points to a completed ingestion run. The same predicate
drives the global index revision, so a partial snapshot cannot wake account
materialization or appear in Matches.

### Replay and recovery

The recovery test covers the production interruption sequence:

1. Worker A leases a source and uploads a batch.
2. The completion response is lost and the lease expires.
3. Worker B receives the same schedule/replay key.
4. Re-uploading the same batch is a no-op replay.
5. Completion succeeds once.
6. Replaying completion returns the stored result.
7. The index still contains exactly one candidate and one membership.

This preserves at-least-once delivery without duplicating candidates or making
an uncertain snapshot visible.

### Resume invariants

`server/src/api/jobs_resume_generation/tests.rs` verifies that a supported
same-role bullet can be improved while the exact contact details, employer,
title, location, dates, education, certifications, projects, and template
metadata remain unchanged. New metrics, unrelated skills, cross-role facts,
or unsupported claims continue to fail validation.

Production model generation remains disabled. Enabling it requires the same
provider-cost reservation, evidence validation, finalization fence, and visual
document QA already required by the Jobs rollout gates.

## Verification

```text
Jobs package tests
  automation: 171 passed
  browser: 100 passed
  runner: 50 passed
  workflows: 49 passed
  portal: 62 passed
  total: 432 passed

Rust server
  unit tests: 745 passed
  HTTP integration tests: 76 passed
  runner plan matrix: passed
  usage schema compatibility: passed

Build and static checks
  all Jobs TypeScript packages: passed
  all Jobs production builds: passed
  Rust Clippy -D warnings: passed
  provenance/license: passed
  privacy: passed
  SQLite/PostgreSQL schema parity: passed
  client/server boundary: passed
  git diff --check: passed
```

## Rollout Plan

1. Build and deploy only the Jobs API and global discovery worker from the
   reviewed source commit. Do not distribute Bluey Browser or enable model,
   local-browser, or cloud-browser flags.
2. Preserve the existing PostgreSQL, API binary, portal, and worker rollback
   artifacts.
3. Restart the Jobs API and global worker, then verify health and zero restart
   loops.
4. Retry only the Ashby and Lever canaries.
5. Require exactly 46,180 Ashby rows and 68,029 Lever rows in completed runs.
6. Verify running/failed snapshots return zero account materializations.
7. Expand source families in small canary groups only after row counts,
   original URLs, freshness, deduplication, and source-specific error rates are
   reviewed.

## Product Truth

Bluey can continuously ingest and rank a broad source catalog, including
public ATS exports and rights-reviewed curated feeds. This is not equivalent
to universal automatic submission. Each lead must still be revalidated against
the original employer page, and each employer-facing Submit action requires a
certified adapter or a visible review/handoff path. OmniParser or another
vision model may help locate unfamiliar controls, but it cannot replace these
truth, identity, receipt, and side-effect guarantees.
