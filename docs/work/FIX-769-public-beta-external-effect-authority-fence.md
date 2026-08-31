# FIX-769: Queued Jobs work could outlive public-beta effect authority

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

An application or communication queued while an account was admitted could
still obtain new employer-facing write authority after account denial, deletion
intent, cohort suspension, or the Jobs master switch closing.

## Root Cause

Customer-route middleware enforced public-beta admission, but authenticated
worker routes and database effect transitions intentionally bypassed customer
routing. Execution-lease claim, managed-effect authorization, fresh managed
irreversible submission, production Browser-release claim, the callable
standalone local claim, fresh local pre-click submission, communication claim,
and communication request-start rechecked their existing effect authorities
but not the current public-beta authority. The local-run HTTP router also put
claim, submit replay, resume, and result reconciliation behind one outer master
middleware. Master-off therefore stopped fresh work but also stranded an exact
issued claim, an exact `click_started` replay, and terminal result evidence. The
claim handler independently checked the combined distribution flag and fleet
readiness before the database could recognize an exact replay. The
communication dispatcher also
prepared its provider credential before the request-start transaction. An
expiring credential could therefore perform OAuth refresh I/O and persist a
rotated token after claim but before a new beta denial, cohort suspension, or
master-off state was observed.

## Fix Summary

Added a transaction-local, read-only public-beta effect check for both database
backends. It requires the master switch, a verified permanent account, a sticky
enrollment, an effect-permitting cohort state, no denial, and no deletion
intent. It never creates an enrollment.

The check now fences execution-lease claim before any lease mutation,
managed-effect authorization, every fresh managed or local irreversible
submission transition, production and standalone local Browser claim,
communication dispatch claim, and provider request-start. The production local
claim checks exact replay first; an already-issued claim response remains
recoverable after public-beta closure or deletion intent, while a fresh claim is
rejected before distribution, release binding, ticket, application, reservation,
session, event, or replay-ledger mutation. Local `click_started` submit replay likewise remains
available before the fresh public-beta and distribution gates for both schema-3 review-first
proofs and schema-4 ATS-certified proofs. Schema 4 recovers the terminal ATS receipt authority;
schema 3 retains the exact stored proof, release, ticket, application, session, and evidence-
capacity checks without inventing ATS authority. Both paths return a stable database-owned
`authorizedAtMs` from the original click transition, so an exact HTTP retry is byte-identical.
An authority loss
before communication request-start returns the action to `needs_input`, clears
its lease/approval authority, and records no provider request-start evidence.
The dispatcher now constructs only its local database-backed provider request,
then records the beta-gated durable request-start decision, and returns on the
closed outcome before credential preparation. Only a successful request-start
may load or refresh a credential and proceed to provider dispatch. A credential
preparation failure after that marker is completed as a definitive no-side-
effect outcome; if preparation consumes the remaining lease, the provider
write is not constructed and the durable marker conservatively sends the
attempt through existing reconciliation. Exact `click_started` replay and
lookup-only `side_effect_unknown` reconciliation remain available after
closure.

The local-run HTTP composition now applies the outer master middleware only to
the fresh mutating resume operation. Claim and submit requests reach their
transactional replay-first authorities, while result remains available to
record terminal evidence and reconcile ambiguity. The claim handler passes the
raw local-distribution flag into the same database transaction; exact replay is
resolved first, current beta authority is checked next, and the flag plus fleet
readiness close only a fresh claim before mutation.

The unit-library build keeps the outer gate deterministically enabled. This
avoids process-environment races between parallel Jobs unit tests while they
exercise the database-owned denial, suspension, deletion, replay, and
reconciliation boundaries. Production and integration-test builds still read
`BLUEY_JOBS_BETA_ENABLED` directly; serial HTTP integration coverage remains
the authority for master-off/on behavior.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs_beta_access.rs` | Add the effect check and isolate the unit-only master fixture from process environment |
| `server/src/db/jobs/execution_leases.rs` | Fence lease claim, managed authorization, and fresh irreversible submission |
| `server/src/db/jobs/browser_release_authority.rs` | Fence fresh production local Browser claims after exact replay and preserve PostgreSQL lock order |
| `server/src/db/jobs/local_runner.rs` | Fence standalone local claim and fresh local pre-click submit while retaining schema-3/schema-4 exact click-started replay and its stable authorization timestamp |
| `server/src/db/jobs/communication_actions.rs` | Fence dispatch claim and request-start while retaining reconciliation |
| `server/src/jobs_communication_dispatch/mod.rs` | Order request-start before credential refresh/provider I/O and stop immediately on authority loss |
| `server/src/db/jobs/tests.rs` | Cover a refresh-eligible credential, deny, suspend, replay, and reconciliation without mutating the process-wide beta flag |
| `server/src/api/jobs.rs` | Keep resume behind the outer master boundary; route claim, exact submit replay, and result to their operation-specific authorities; move the raw distribution flag into the claim transaction |
| `server/tests/integration_e2e.rs` | Cover the real master switch, signed worker routes, fresh local claim/submit closure, byte-identical claim/submit replay, accepted result evidence, and the preserved pre-effect lease phase |
| `docs/work/FIX-769-public-beta-external-effect-authority-fence.md` | Record diagnosis and verification requirements |

## Edge Cases Handled

- `closed_to_new` continues to authorize already-admitted accounts.
- `draft` and `suspended` authorize no new effect.
- A closed master switch is checked before and after database authority reads.
- A refresh-eligible credential cannot reach OAuth refresh before the
  transactionally rechecked request-start boundary.
- Credential preparation failure after request-start records a definitive
  no-side-effect completion when its lease is still current.
- Lease expiry during post-marker preparation cannot begin provider dispatch;
  the existing request-start marker preserves conservative reconciliation.
- Exact already-started submission replay remains read-only and is not blocked
  by a later denial, suspension, master-off state, or deletion intent.
- Exact already-issued local Browser claim replay is not mistaken for a new claim.
- Master-off or local-distribution-off cannot strand exact issued claim or
  `click_started` submit responses, or prevent result reconciliation.
- Review-first schema-3 replay does not require or synthesize ATS-certified authority; schema-4
  replay still requires the existing terminal ATS binding.
- Exact submit retries reproduce the original database-owned `authorizedAtMs` instead of sampling
  a new request time.
- Fresh local claim and pre-click denial leave ticket, application, reservation,
  session, evidence capacity, release binding, and final-submit proof unchanged.
- PostgreSQL takes common locks, then the beta cohort/account locks, then the
  discovery/account-specific locks; replay remains ahead of the fresh beta decision.
- Reconciliation remains read-only and available after every authority closes.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  execution_lease_claim_rechecks_public_beta_before_any_claim_mutation
cargo test --manifest-path server/Cargo.toml \
  managed_effect_rechecks_public_beta_before_new_authority_and_submit
cargo test --manifest-path server/Cargo.toml \
  local_claim_rechecks_public_beta_without_stranding_exact_release_replay
cargo test --manifest-path server/Cargo.toml \
  local_submit_rechecks_public_beta_without_stranding_click_started_replay
cargo test --manifest-path server/Cargo.toml \
  postgres_claim_locks_effect_rows_before_final_database_time_and_never_relocks
cargo test --manifest-path server/Cargo.toml \
  local_submit_public_beta_prelock_precedes_discovery_and_fresh_effect_only
cargo test --manifest-path server/Cargo.toml \
  communication_dispatch_rechecks_public_beta_before_refresh_and_keeps_reconciliation
cargo test --manifest-path server/Cargo.toml \
  request_start_authority_loss_returns_before_credential_refresh_or_provider_dispatch
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --test integration_e2e \
  jobs_execution_lease_routes_require_worker_auth_and_fence_submit
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --test integration_e2e \
  jobs_local_submit_resume_recovers_after_consume_before_marker
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --test integration_e2e \
  jobs_legacy_v1_local_result_resume_and_submitted_replay_remain_recovery_only
```

## Known Limitations

- A controlled low-debug run passed the schema-3 current and legacy HTTP journeys, the master-off
  route boundary, and the focused claim/submit replay and source-order tests. Full exact-tip Rust,
  live PostgreSQL concurrency, and hosted verification remain required before release.
- The environment-backed master switch changes with process configuration;
  database cohort suspension is the live transaction-serialized stop control.
- Inline OAuth refresh remains behind the durable request-start marker in this
  bounded fix. A successor may split refresh into its own leased state machine,
  but it must preserve grant CAS, lease deadlines, definitive no-effect
  completion, and ambiguity reconciliation.
- A beta-specific `needs_input` reason belongs in the successor diagnostic
  effect ledger; no user content should be added to operational logs.
