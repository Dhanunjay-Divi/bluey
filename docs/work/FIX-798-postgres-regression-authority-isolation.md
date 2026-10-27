# FIX-798: PostgreSQL regression authority isolation

> Codex preflight: `bluey-ops`, security review and Round 629.

## Issue

The configured PostgreSQL rerun passed concurrent startup after FIX-795 and the
real leaseable probe/replay/cascade case. Across the expanded selection, 15 of
27 cases passed and 12 failed at the same signed v2 fixture import with
`IdentityConflict`. Its private cluster shut down cleanly and its root was removed.

## Root Cause

Independent test invocations shared one database. Each fixture models a first
release with generation/sequence 1 and historical test identities. Reusing those
identities for different v2/v3 signed bytes correctly fails. A live v3 activation
head, scoped v2 authority and legacy default-v2 fixture also intentionally cannot
overwrite or silently coexist as another scenario's authority. Serialization
alone does not make independent global fixtures compatible.

Neither failure permits relaxing production identity, activation, scope or
head-generation fences.

## Fix Summary / Files

- `jobs/scripts/run-nonprod-postgres-tests.mjs`: repeated `--case-filter` options
  discover nonempty, disjoint exact Cargo test names with bounded output and at
  most 128 cases. Allocate a random fresh database and pgvector extension for
  each exact case inside the one task-owned cluster. Keep the compiler/home
  stable, inject only the generated per-case URL, collect every failed case,
  and require an all-pass result. The entire cluster is removed afterward.
- `server/src/api/jobs_managed_cloud_releases.rs`: add a configured real
  Axum-handler → PostgreSQL → response regression covering registration,
  premature/unbound readiness rejection, real startup, persisted canonical bytes
  and exact replay. It injects an already-authenticated worker identity; it does
  **not** claim socket or fleet-HMAC middleware coverage.

An attempted test-only v3 namespace with generation/sequence 3 was statically
reviewed but failed executable first-import validation with `SequenceRegression`.
That attempted branch and its assertion were fully reverted: fresh scenarios
must start at generation/sequence 1. The final correction isolates their
databases rather than weakening the sequencing fence or changing historical
fixture bytes. The first isolated run passed 22/28 cases: three failed because
of that now-reverted attempt, while three older local-claim regressions remain
under separate diagnosis. No all-pass result is claimed at this checkpoint.

## Edge Cases / Verification

- Empty/overlapping filters cannot silently produce a passing zero-test gate.
- Wrong version, interrupted database, signal death or nonclean shutdown still
  fail validation. Database identifiers are generated, not taken from test text;
  passwords/URLs are never emitted to logs or process arguments.
- Each case can create several connections/threads to its own database, so real
  concurrency and locking remain exercised without cross-case state pollution.
- The API case rejects client-supplied probe results without mutation and checks
  exact replay against snapshots of all six runtime row families.

Run through the canonical launcher and shared Mac-heavy lock, with installed
PostgreSQL 17/pgvector and `--case-filter` plus common `cargo ... test --lib`
arguments. The Phase 629 IMPL records actual commands/results; the corrected
final configured run passed all 30 exact cases in separate fresh databases,
including the API path; clean PostgreSQL shutdown was verified before Clippy.

## Known Limitations

These are synthetic local database/handler tests. They do not certify hosted
PostgreSQL, real provider/ATS/mailbox behavior, signed production activation,
capacity, deployment or rollout approval.
