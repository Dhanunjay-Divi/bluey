# Round 597 - Jobs Durable Encrypted Browser Profile Recovery

**Date:** 2026-08-03

**Branch:** `feat/phase-jobs-full-autonomy-20260802`

**Status:** Implemented and verified; Browser distribution remains off

## Outcome

Bluey can now move a cloud Browser run to a replacement runner without losing
the application identity's authenticated browser profile or accepting stale
profile state.

The Browser profile is encrypted locally before upload. Encrypted bytes live in
account-scoped R2/S3-compatible object storage, while PostgreSQL or SQLite owns
the current generation, exact writer run and execution-lease fence. Restore
checks the stored byte count and SHA-256 before the worker may decrypt or launch
Chromium.

## Authority Boundary

Every restore or store request must carry the exact:

- account, application and run;
- opaque browser profile ID;
- active execution-lease token and fence;
- expected profile generation; and
- encrypted envelope metadata.

Private worker authentication signs the body and operation scope. The server
rejects expired leases, stale fences, cross-profile access, generation races,
invalid hashes, non-canonical payloads and oversized snapshots.

## Recovery Lifecycle

```text
claim fenced execution lease
  -> restore latest encrypted generation
  -> verify hash and size
  -> install atomically
  -> decrypt into the scoped runtime directory
  -> launch Chromium
  -> close Chromium
  -> seal the profile locally
  -> upload and read back encrypted bytes
  -> commit generation with the same live lease
  -> finalize or reconcile the lease
```

Normal completion, abort, intervention and restart-recovery paths all store the
profile before invalidating the execution lease. A replacement runner refuses
an older generation and an equal-generation payload with different bytes.

## Storage Semantics

Object keys hash the opaque browser profile ID and include generation plus
content hash. Raw profile IDs and decrypted browser data never appear in object
paths.

The object upload deliberately occurs before the metadata compare-and-swap.
When a writer loses that race, its immutable content-addressed object may remain
for bounded lifecycle cleanup. Deleting it in the request path could remove an
object concurrently adopted by the winning writer.

## Verification

```text
Runner full test suite:                    63 passed
Runner strict TypeScript:                  passed
Server unit tests:                        828 passed
Server integration/schema tests:           87 passed
Server strict Clippy:                       passed
Rust formatting and compile checks:        passed
git diff --check:                           passed
```

The integration path verifies initial 204 restore, upload and read-back,
metadata commit, idempotent replay, replacement restore, forged-token rejection
and cross-profile rejection.

The full suite also caught and closed an operator-path defect: the new
PostgreSQL migration now carries the required target marker, so
`scripts/bluey-postgres-migrate.sh` will apply it instead of silently skipping
it.

## Production Boundary

No production deployment or feature-flag change is part of this round. Model
generation, local Browser distribution, cloud Browser distribution and mailbox
sync remain disabled. A real cloud Browser pool still requires deployment,
monitoring, capacity tests and authorized ATS certification before distribution
can be enabled.
