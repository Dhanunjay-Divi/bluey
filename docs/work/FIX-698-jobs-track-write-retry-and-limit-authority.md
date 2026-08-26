# FIX-698: Career Track Retries Could Duplicate State or Report Partial Failure

> **Codex preflight:** Loaded `$bluey-ops` and reconciled portal Track creation, API routing,
> entitlement limits, relational read-back, and curated-source enrollment against the current
> Phase 613 worktree. No live source, hosted database, external account mutation, deployment, or
> flag was used.

## Issue

A timed-out or late-failing Career Track create could be retried with ambiguous identity, race the
active-Track entitlement limit, or return an error after the Track was already committed because
managed curated-source enrollment failed afterward.

## Root Cause

New Settings Tracks used an empty ID and onboarding used one shared fallback ID, while the portal
selected POST versus PUT from whether `track.id` was nonempty. This did not provide a unique,
stable idempotency key across an ambiguous create retry.

The server checked the active Track count before entering the write transaction. Concurrent
creates could both observe capacity and exceed the plan. The ID-conflict upsert could also affect
zero rows for another account yet appear to return the caller's supplied projection. Read-back
trusted `track_json.active` over the relational `active` column.

Finally, Track create/update and onboarding persisted the Track before ensuring the account-level
managed curated source. Propagating a later enrollment error as HTTP 500 made a successful policy
write look failed and encouraged a duplicate retry. Onboarding completion order could similarly
make a partial request look finished.

## Fix Summary

- Generate a unique client Track UUID once in Settings and once per onboarding component lifetime,
  then reuse it across retries.
- Select create versus update from server-owned persistence evidence (`created_at_ms <= 0` for a
  create), not from whether the stable ID is nonempty.
- Require every Track write to carry a nonempty retry-safe ID and the exact authenticated taxonomy
  binding.
- Enforce the active Track limit inside the same write transaction: SQLite uses an immediate
  transaction and PostgreSQL uses the account transaction lock before recounting and writing.
- Require the upsert to affect exactly one tenant-owned row; reject a cross-account ID collision
  instead of returning a phantom save.
- Project `active` from the relational column during read-back so stale JSON cannot override
  activation authority.
- Treat managed curated-source enrollment after a saved policy as best-effort, log only an opaque
  account fingerprint/category, and retry enrollment on the next safe workspace load.
- Persist onboarding completion last. Earlier profile/preference/Track writes are retry-safe, but a
  partial request cannot make the portal skip onboarding.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/components/Onboarding.tsx` | Generate one stable per-flow Track UUID |
| `jobs/portal/src/views/SettingsView.tsx` | Generate a stable UUID for each new Track draft |
| `jobs/portal/src/api.ts` | Route stable-ID creates by persistence state and send exact taxonomy authority |
| `jobs/portal/src/{api.test,App.test}.ts` | Cover create routing, taxonomy fencing, retry identity, and toast semantics |
| `server/src/api/jobs.rs` | Validate IDs, map atomic-limit errors, preserve saved writes across late enrollment failure, and complete onboarding last |
| `server/src/db/jobs/profile_postings.rs` | Make Track upsert tenant-exact, limit-atomic, retry-safe, and relationally authoritative |
| `server/src/db/jobs/tests.rs` | Cover relational active read-back, cross-account collisions, and concurrent limits |
| `server/tests/integration_e2e.rs` | Inject late curated-enrollment failure and prove stable retry plus workspace repair |

## Edge Cases Handled

- retrying the same POST after timeout or response loss;
- two onboarding sessions versus two retries inside one onboarding session;
- a stable client UUID that has not yet been persisted;
- a Track ID already owned by another account;
- two simultaneous active creates competing for one remaining plan slot;
- stale `track_json.active` disagreeing with the relational `active` authority;
- curated enrollment failing after the Track commit and succeeding on a later workspace load;
- retry after the late enrollment failure without a second Track or policy chain; and
- onboarding failure before the final completion marker.

## How to Test

The latest portal suite passed 349 tests across 28 files and portal typecheck as recorded in
FIX-697. The following Rust unit and fault-injected HTTP integration tests were added and are
included in the final green all-target run.

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  track_readback_uses_the_relational_activation_authority
cargo test --manifest-path server/Cargo.toml --lib \
  track_id_collision_cannot_return_a_cross_account_phantom_save
cargo test --manifest-path server/Cargo.toml --lib \
  concurrent_track_creates_cannot_exceed_the_active_plan_limit
cargo test --manifest-path server/Cargo.toml --test integration_e2e \
  jobs_track_write_is_retry_safe_when_curated_source_enrollment_fails_late \
  -- --exact --nocapture

(cd jobs && npm run test --workspace @bluey/jobs-portal -- src/api.test.ts src/App.test.ts)
(cd jobs && npm run typecheck --workspace @bluey/jobs-portal)
git -P diff --check
```

The configured disposable-PostgreSQL authority run passed 13 tests without self-skip. No dedicated
PostgreSQL concurrent-create result is attributed to this fix, so that exact hosted/dialect race
remains external evidence. The clean all-target Rust command passed 1,517 tests with zero failures
or ignored tests, including 108/108 integration E2E tests and the 2/2 runner plan matrix after its
legacy fixture installed the now-required exact source resume.

## Known Limitations

- Best-effort curated enrollment keeps a committed user policy successful; it does not claim that
  discovery is ready until a later authoritative workspace read confirms the source.
- The API has local disposable-PostgreSQL authority evidence, but the dedicated concurrent-create
  race plus hosted database/network behavior still require production-acceptance evidence.
- This fix does not enable discovery, external source access, Auto-submit, runner distribution, or
  any production flag.
