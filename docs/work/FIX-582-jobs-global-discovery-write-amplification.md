# FIX-582: Jobs global discovery write amplification

## Issue

Unchanged global feed manifests and unchanged canonical job candidates were
reprocessed and rewritten repeatedly, while expired candidate bodies remained
in PostgreSQL indefinitely.

## Root Cause

`server/src/db/jobs/global_discovery.rs` treated every refreshed signed
manifest URL/snapshot timestamp as a source revision and reset its completed
schedule. Candidate `content_hash` values were calculated from randomized
encrypted envelopes, so identical plaintext produced a different hash and an
UPDATE on every ingestion.

The global-candidate schema also had no cold-storage lifecycle. Expiration
removed active membership authority but retained the full encrypted candidate
body.

## Fix Summary

- Compare only source family, artifact SHA-256, and expected row count when
  deciding whether a completed source revision must run again.
- Hash deterministic normalized candidate plaintext before encryption.
- Skip canonical candidate UPDATEs when the content hash is unchanged and the
  row is already hot.
- Add an opt-in archive worker that leases only expired, unreferenced candidates
  and writes their encrypted body to R2.
- Require exact R2 read-back bytes and SHA-256 before replacing the PostgreSQL
  body with a compact tombstone.
- Restore archived rows to hot storage when a source rediscovers them.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/global_discovery.rs` | Semantic source revision and deterministic candidate no-op writes |
| `server/src/db/jobs/global_archive.rs` | Archive lease, completion, failure, and retry transactions |
| `server/src/jobs_global_archive.rs` | Verified object-store archive worker |
| `server/src/object_storage.rs` | Content-addressed global candidate object key |
| `infra/postgres/server-runtime/017_jobs_global_candidate_archive.sql` | Additive archive schema |
| `server/src/db/mod.rs` | PostgreSQL and SQLite migration registration |
| `server/src/db/jobs/tests.rs` | Schedule, no-op, archive, failure, and rehydration tests |

## Edge Cases Handled

- Randomized encryption no longer makes unchanged plaintext look changed.
- Metadata-only signed-manifest refreshes do not reset completed schedules.
- A changed artifact revision still schedules ingestion immediately.
- Active source membership or account materialization blocks archival.
- Corrupt legacy payloads fail independently and do not block valid candidates.
- Upload or read-back mismatch preserves the full PostgreSQL body.
- Candidate changes while an archive lease is running prevent stale completion.
- Rediscovery restores a complete hot candidate and clears archive metadata.

## How to Test

```bash
cargo fmt --all --check
cargo test --manifest-path server/Cargo.toml \
  unchanged_global_manifest_preserves_the_completed_run_schedule
cargo test --manifest-path server/Cargo.toml \
  unchanged_global_candidate_does_not_rewrite_the_canonical_payload
cargo test --manifest-path server/Cargo.toml global_candidate_archive
cargo test --manifest-path server/Cargo.toml jobs_global_archive
cargo test --manifest-path server/Cargo.toml \
  rediscovered_archived_global_candidate_restores_the_hot_payload
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- The archive worker is intentionally disabled by default and requires a
  production R2 PUT/GET/read-back preflight before activation.
- Live customer application rows remain in PostgreSQL. Their large immutable
  documents and evidence are already object-storage backed; whole-row
  application archival requires a separate hydration and retention design.
