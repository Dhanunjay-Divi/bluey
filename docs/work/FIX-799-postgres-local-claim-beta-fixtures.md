# FIX-799: PostgreSQL local-claim public-beta fixtures

> Codex preflight: `bluey-ops`, current code and independent security review.

## Issue / Root Cause

Three isolated configured PostgreSQL regressions failed after the broader
suite became executable. Two positive local-ticket claims returned `None`; a
deletion-intent execution claim returned `Conflict` instead of the older test's
expected `AccountDeleting` storage error.

The local authority fixture inserted an unverified account and no durable beta
cohort/enrollment. `cfg(test)` already enables the master gate, but correctly
does not manufacture those durable admission facts. The positive effect gate
therefore denied both claims. The deletion error expectation was also stale:
both backends intentionally deny an account with a deletion intent at the
public-beta effect gate, before the later explicit write-fence recheck.

## Fix Summary / Files

Only `server/src/db/jobs/postgres_local_authority_tests.rs` changes:

- Verify the synthetic account email before creating its application identity.
- Establish retained `public-v1` cohort/enrollment state using the existing
  canonical test setup pattern. Keep `assigned_count` consistent when fixtures
  are created and after account cascades remove their enrollments.
- Rename the deletion case to describe its real contract, require the exact
  fail-closed `Conflict`, and retain the zero-created-lease assertion.
- Preserve an already-committed `click_started` authorization's exact replay
  after identity deletion. Snapshot the relevant rows after that intentional
  deletion and before replay, then require byte-identical state. Existing
  pre-authorization identity-denial checks remain unchanged.

No master flag, production admission logic, deletion fence, lock order or
application mutation policy is weakened or reordered.

## Independent Diagnosis

An initial suggestion to move the production write fence was rejected after
checking the actual lock queries: the beta gate already acquires the account
`FOR KEY SHARE`; discovery is advisory and the application discovery read is
unlocked; current-authority locking reacquires the parent before child policy
rows. The later strong write fence is a deletion-state revalidation/upgrade,
not the first parent acquisition. No production ordering defect was established.

## Verification / Limits

Run the configured PostgreSQL cases in separate fresh databases using the
Phase 629 harness, then the full server suite and strict Clippy. Actual results
are recorded in the Phase 629 IMPL. The final configured PostgreSQL batch passed
30/30, including the corrected claim/replay/deletion cases; strict Clippy passed.
This fixture does not admit or modify a real user,
enable production beta access, submit an application or send a message.
