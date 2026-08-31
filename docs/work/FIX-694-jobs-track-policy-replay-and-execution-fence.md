# FIX-694: Career Track Policy Replay Could Revive Superseded Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the Phase 613 policy and execution
> boundaries against the current authority worktree. No hosted database, provider, employer,
> credential, deployment, release activation, or production flag was used.

## Issue

Returning taxonomy, account, or Track inputs from `A` to `B` and then back to the same `A` bytes
could make an old policy digest appear current, while PostgreSQL execution could race a
policy-relevant write after validation and before a new employer-facing effect.

## Root Cause

The initial three-table Phase 613 design bound a canonical policy digest to an immutable revision
and receipt, but it had no monotonic activation epoch for the taxonomy/canonicalizer tuple and no
separate monotonic generations for account-wide and Track-local semantic inputs. A content-equal
fast path could therefore confuse a later reversion with a true no-op.

The mutable policy head also lacked an immutable transition chain. Owner export and equal-hash
validation could inspect the current revision/receipt/head without proving every transition that
led to it. Finally, PostgreSQL policy-input writers and final execution validation did not share a
transaction-scoped account fence, leaving a time-of-check/time-of-use window. Auto-submit input
validation and persistence likewise did not derive from one stable account snapshot, and mutable
Auto-submit revocation lacked a per-Track fence shared with final execution.

## Fix Summary

- Replace the three-table design with paired SQLite/PostgreSQL ten-table authority: global
  activation events/head; account-input transitions/head; Track-input transitions/head; immutable
  policy revisions; immutable review receipts; immutable policy-head transitions; and the current
  compare-and-swap policy head.
- Activate the exact taxonomy version/digest and canonicalizer schema/digest at startup. Reusing
  the same tuple is idempotent; every changed tuple, including a return to earlier bytes, advances
  the global epoch and transition chain.
- Derive policy-relevant semantic digests for account inputs and Track inputs. Transport-only
  changes remain no-ops, while `A -> B -> A` advances twice and binds the later `A` to new
  generations and transition digests.
- Bind the activation epoch, canonicalizer authority, both input generations, semantic digests,
  and transition digests into canonical policy JSON, revisions, receipts, head transitions,
  current projections, prepared packets, and application receipts.
- Validate the complete immutable chain before accepting a content-equal no-op. Preserve exact
  predecessor identity, compare-and-swap head movement, tenant ownership, and reviewed parent
  cascades while rejecting direct history mutation, skipped generations, and reparenting.
- Serialize SQLite writes and use exclusive PostgreSQL account locks for policy-relevant writers.
  Hold the matching shared lock through execution authorization and the effect transaction, and
  mint Auto-submit authorization from one locked transaction. Serialize Auto-submit
  authorize/revoke exclusively against the shared final-execution fence for the same Track.
- Verify and export the global activation chain, account and Track input chains, revisions, review
  receipts, policy-head transitions, and current heads rather than only current projections;
  require exact revision/receipt/head-transition bijection and one terminal head per history.
- Validate exact source-resume replay against the current account semantic head, and keep prepared
  evidence stable across Track timestamps and derived match counts without excluding semantic
  Track or reviewed-policy fields.

## Files Modified

| File | Change |
|------|--------|
| `infra/sqlite/server-runtime/056_jobs_canonical_taxonomy_authority.sql` | Define the ten-table SQLite generation and immutable-ledger authority |
| `infra/postgres/server-runtime/034_jobs_canonical_taxonomy_authority.sql` | Mirror the authority, triggers, functions, indexes, and tenant relationships in PostgreSQL |
| `server/src/db/jobs/taxonomy_policy.rs` | Activate, advance, persist, validate, project, encrypt, and export the complete ledger |
| `server/src/db/{mod.rs,jobs.rs}` | Register startup activation and shared authority fields/lock helpers |
| `server/src/db/jobs/{profile_postings,customer_data,resume_assets,evidence}.rs` | Advance/validate semantic generations, persist review reasons, and keep transport-only evidence stable |
| `server/src/db/jobs/{auto_submit,execution_authority}.rs` | Bind Auto-submit and final execution to one current, transaction-fenced ledger snapshot |
| `server/src/db/jobs/{tests,postgres_local_authority_tests}.rs` | Cover replay, corruption, projection drift, cascades, and PostgreSQL authority |
| `jobs/scripts/{check-jobs-schema-parity,ci-guards-self-test}.mjs` | Enforce paired ten-table schema, immutability, generation, tenant, and migration guards |

## Edge Cases Handled

- same taxonomy/canonicalizer tuple at repeated startup versus a later tuple rollback;
- account or Track `A -> B -> A` with an identical final semantic digest;
- timestamps and onboarding transport progress that must not manufacture a semantic generation;
- one Track save that updates relational and JSON projections but advances semantic input once;
- an equal policy digest backed by missing, reordered, cross-tenant, or corrupt evidence;
- equal-millisecond successors with strictly monotonic generations;
- concurrent PostgreSQL profile, preference, fact, identity, resume, or Track writes during final
  execution authorization;
- stale compare-and-swap heads, competing Auto-submit authorization attempts, and revoke/execution
  races;
- exact resume replay after unrecorded semantic drift;
- a semantic no-op Track retry that changes only timestamps or derived match count;
- owner export of full history, orphan-revision rejection, and another account's empty scope; and
- authorized Track/account cascades without making global activation history tenant-owned.

## How to Test

The focused SQLite canonical-policy suite passed 14/14. The finalized local schema-parity guard
passed at 81 tables and 74 indexes in each database. A configured disposable-PostgreSQL run passed
13 authority tests after the normal migration runner completed both passes; within that run the
canonical-policy-ledger and Auto-submit revoke/execution-fence regressions each passed 1/1. The
clean fmt, all-target check/Clippy, and all-target test command passed; Rust ran 1,517 tests with
zero failures or ignored tests, including 1,401 server-library tests.

```bash
cargo fmt --all -- --check
cargo check --manifest-path server/Cargo.toml --all-targets
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path server/Cargo.toml --lib canonical_track_policy_tests
cargo test --manifest-path server/Cargo.toml --lib \
  exact_resume_replay_requires_current_semantic_input_ledger
cargo test --manifest-path server/Cargo.toml --lib \
  execution_lease_excludes_parallel_runs_for_one_browser_profile

# With BLUEY_TEST_POSTGRES_URL set to an isolated, disposable PostgreSQL database:
cargo test --manifest-path server/Cargo.toml --lib \
  postgres_canonical_track_policy_ledger_encrypts_and_rejects_projection_drift \
  -- --nocapture

node jobs/scripts/check-jobs-schema-parity.mjs
# PASS: 81 tables / 74 indexes

node jobs/scripts/ci-guards-self-test.mjs
git -P diff --check
```

Legacy integration execution/export and runner-plan fixtures were updated to install the exact
source resume that Phase 613 requires before approval. The final integration and runner-matrix
targets passed 108/108 and 2/2; hosted PostgreSQL network/interruption and provider-facing evidence
remain separate gates.

## Known Limitations

- Local disposable-PostgreSQL authority evidence is green; hosted concurrency under network or
  interruption failure remains pending.
- This ledger denies stale new effects but does not erase or reinterpret an effect that may already
  have happened; existing `side_effect_unknown` reconciliation remains authoritative.
- Hosted migration, registry read-back, release signing, runner capacity, ATS canaries, cohorts,
  kill switches, rollback rehearsal, and all production flags remain parked external gates.
