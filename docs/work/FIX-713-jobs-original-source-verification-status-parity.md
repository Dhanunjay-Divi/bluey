# FIX-713: Original-Source Verification Status Vocabulary Diverged

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against the current Phase 614
> source and Round 614. The SSD archive was not used.

**Status:** Implemented; focused and final-parser Jobs checkpoints green, post-lifecycle final
aggregate pending

## Issue

The TypeScript discovery classifier, Rust posting projection, and Round 612 receipt contract used
different names and status coverage for original-source outcomes. Without a closed translation, one
result could be treated differently across retrieval, receipt publication, eligibility, queueing,
and final effect paths.

## Root Cause

The earlier discovery-quality vocabulary predated immutable managed verifier receipts:

- TypeScript used `verified_open`, `verified_closed`, `mismatch`, `unreachable`, and `unknown`;
- the Rust mutable posting projection primarily recognized `verified_open`, `closed`, and
  `mismatch`; and
- Round 612 defined a broader receipt vocabulary for redirect, identity, material-change, trust,
  expiry, challenge, parser, rate-limit, and provider-availability results.

Those values lacked one server-validated observation/result translation and paired storage
constraints.

## Fix

The immutable receipt vocabulary is now closed to:

```text
verified_open
closed
redirected_to_unknown
identity_mismatch
materially_changed
source_untrusted
expired
unknown
unreachable
rate_limited
auth_required
captcha_required
parse_ambiguous
provider_unavailable
```

- Only `verified_open` is potentially positive, and only with the complete current receipt/head,
  consumable assignment, independent employer/risk authority, and all existing execution checks.
- Closed, redirect, identity mismatch, material change, source-untrusted, and expired results are
  hard non-authority.
- Unknown, unreachable, rate-limited, authentication/CAPTCHA-required, parse-ambiguous, and
  provider-unavailable results are indeterminate non-authority. Bounded retry never converts them
  into success or revives an old head.
- Worker completion/failure result codes remain distinct from receipt statuses. Rust validates
  exact result, retrieval status, HTTP status, material presence, and error-code combinations before
  deriving a receipt.
- Paired SQLite/PostgreSQL `CHECK` constraints close receipt and transition status storage to the
  same canonical vocabulary.
- The mutable compatibility boundary still recognizes legacy `verified_closed` and `mismatch` only
  as nonpositive outcomes. Unknown/future values fail closed and cannot broaden preparation, queue,
  or effect authority.
- Projection writes only the server-derived `verified_open` status for a validated current positive
  receipt; caller-supplied projection status is not receipt authority.

## Files

- `jobs/automation/src/original-source-verification.ts`
- `jobs/automation/tests/original-source-verification.test.ts`
- `server/src/db/jobs.rs`
- `server/src/db/jobs/original_source_verification.rs`
- `server/src/db/jobs/eligibility.rs`
- `server/src/db/jobs/tests.rs`
- `infra/sqlite/server-runtime/057_jobs_original_source_verification_authority.sql`
- `infra/postgres/server-runtime/035_jobs_original_source_verification_authority.sql`
- `jobs/scripts/check-jobs-schema-parity.mjs`

## Verification

Observed focused evidence:

```text
Provider verification matrix                    27 / 27 post-fix
Rust original-source authority                  25 / 25; normal-parallel twice
Schema parity                                   95 tables / 79 indexes per dialect
Rust all-target check and strict Clippy          passed
```

The 25/25 Rust suite includes server validation of contradictory observation/result/HTTP tuples,
bounded lease fairness, persisted hold backoff, expired-attempt audit, public strict-v2
composition, and the public heartbeat/completion/failure/replay/fencing/authority-loss lifecycle.
The post-FIX-716 provider suite passed with duplicate/conflicting raw members, malformed UTF-8,
raw-octet hashing, and alias conflicts included. The final-parser Jobs checkpoint passed 1,884
tests with one skipped test; final Rust tests and all post-lifecycle final-diff guards remain
pending.

## Known Limitations

- Status parity does not prove live provider behavior, provider signatures, or arbitrary snapshot
  stability.
- Public ATS cursor evidence remains acquisition evidence, not a verifier receipt.
- Raw duplicate-key, malformed-byte, and alias-conflict hardening is recorded under FIX-716.
- No authenticated provider action, deployment, activation, or flag change is authorized.
