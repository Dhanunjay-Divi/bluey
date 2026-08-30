# FIX-751 — Original-source encrypted posting recheck

## Status

Implemented; focused SQLite lifecycle evidence is green. An earlier local PostgreSQL 17 lifecycle
checkpoint was green but is superseded by later source changes; fresh frozen-source PostgreSQL
evidence remains pending.

## Defect

The original-source lease and completion rechecks read `jobs_postings.posting_json` and passed
the stored bytes directly to `serde_json`. Jobs posting documents are encrypted at rest, so every
production-format posting failed before the verifier could lease or complete its assignment. Older
tests that inserted plaintext JSON did not exercise the deployed storage representation.

## Fix

Both SQLite and PostgreSQL subject rechecks now use the shared authenticated `parse_json` storage
boundary. It decrypts current encrypted envelopes while retaining the intentional legacy-plaintext
read compatibility used by migration tests. The canonical subject and trusted membership checks
remain unchanged and still run after decoding.

## Evidence

- The shared production-positive fixture stores the job through
  `save_verified_import_posting_with_source`, then leases and completes the public managed verifier
  lifecycle. Its first run reproduced this defect against encrypted `posting_json`.
- `cargo test --manifest-path server/Cargo.toml
  public_lifecycles_resolve_one_production_positive_composition -- --nocapture` — **green, 1
  passed / 0 failed**. This stores the posting through the encrypted production persistence path
  before the public verifier lease and completion.
- A local PostgreSQL 17 run of
  `postgres_positive_lifecycle_when_configured` passed on the earlier `r5` test binary. Later
  fixture and FinalSubmit source edits superseded that binary, so this is diagnostic context only,
  not final-source release evidence.
- Fresh PostgreSQL lifecycle parity remains part of the pending exact-name `r6` manifest.
