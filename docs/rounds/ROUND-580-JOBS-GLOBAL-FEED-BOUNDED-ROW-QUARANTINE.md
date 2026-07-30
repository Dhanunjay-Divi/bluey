# Round 580 - Jobs Global Feed Bounded Row Quarantine

Date: 2026-07-30

## Objective

Recover the otherwise valid production Ashby global-discovery snapshot without
weakening structural source validation or application-truth boundaries.

## Production Finding

The Jobhive Ashby artifact contains 46,378 CSV rows. Of those, 46,377 are
semantically usable and one row, CSV row 45,027, lacks the canonical identity
fields needed to represent a job.

The previous worker aborted the whole source when it reached that row after
processing more than 45,000 valid records. Ashby therefore remained degraded
even though its artifact was overwhelmingly valid.

## Implementation

### Worker

The global-discovery worker now:

1. Keeps the manifest row count immutable.
2. Reads and validates the entire artifact.
3. Quarantines only the typed `missing_identity` semantic defect.
4. Keeps archive, stream, CSV, and manifest defects as hard failures.
5. Uploads accepted rows in contiguous batches.
6. Completes with exact accepted/rejected counts and rejection-reason totals.

### Server

The Jobs server independently requires:

```text
accepted_rows + rejected_rows = expected_rows
```

It permits only known rejection reasons and caps total rejected rows at:

```text
min(100, max(1, ceil(expected_rows / 1000)))
```

Completion also checks accepted database rows and batch counters. Source health
changes to healthy only after all evidence is validated and persisted.
Idempotent replay must present the exact same evidence; conflicting replay
fails.

### Persistence

Fresh and upgraded SQLite/PostgreSQL databases now preserve:

- accepted row count
- rejected row count
- exact rejection summary JSON

No raw rejected row content is stored in the completion record.

## Safety Invariants

- External-feed candidates remain leads.
- Bluey must revalidate the original employer posting before preparation,
  queueing, or submission.
- One malformed semantic row cannot hide valid jobs.
- Structural corruption cannot be silently quarantined.
- An all-invalid source cannot become healthy.
- Rejection counts and reasons are bounded and auditable.
- Model generation remains disabled.
- Local and cloud Browser distribution remain disabled.
- The main API, Caddy, native overlay, audio, STT, and native release artifacts
  are outside this change.

## Verification

All local gates passed:

- 781 server unit tests.
- 76 server integration tests.
- Jobs runner-plan and migration-compatibility tests.
- Rust format and strict Clippy.
- TypeScript checks for all five Jobs packages.
- 469 Jobs tests.
- Jobs production build.
- CI guard self-test.
- Privacy gate.
- SQLite/PostgreSQL schema parity.
- Provenance/license gate.
- Git whitespace validation.

## Manual Deployment Plan

1. Confirm the merged main commit and a clean deployment workspace.
2. Take a fresh PostgreSQL backup, record its SHA-256, and verify its restore
   inventory.
3. Record a rollback snapshot of Jobs API/worker binaries, service definitions,
   environment flags, and source-health rows.
4. Build the Jobs API and discovery workers once from the exact merged commit.
5. Record artifact hashes and deploy those exact artifacts.
6. Run the PostgreSQL migration through the normal server startup path.
7. Keep these flags at zero:

   ```text
   BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
   BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
   BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
   ```

8. Trigger or recover the Ashby global source.
9. Verify exact completion evidence:

   ```text
   expected_rows: 46378
   accepted_rows: 46377
   rejected_rows: 1
   rejection_reasons: {"missing_identity": 1}
   ```

10. Verify Ashby is healthy, candidate rows materialize, the public source
    checker advances, service restart counters remain zero, and logs contain no
    raw quarantined row.
11. Verify R2 replication separately. If it still returns `AccessDenied`,
    report that degradation and do not describe it as healthy.

## Rollback

If schema startup, completion validation, source health, or candidate
materialization fails:

1. Stop only the replaced Jobs API and discovery worker services.
2. Restore the recorded API/worker artifacts and service environment.
3. Restore the database only when forward correction is unsafe and the paired
   schema/binary rollback has been explicitly selected.
4. Restart the prior services and verify protected routes, source health,
   worker liveness, and restart counters.
5. Leave the three Jobs product flags disabled.

## Expected Result

Ashby completes as a 46,378-row source with 46,377 accepted candidates and one
auditable `missing_identity` quarantine. The source becomes healthy without
turning malformed rows into application truth or relaxing any structural
integrity check.
