# FIX-753 — Jobs PostgreSQL lock order and post-lock time

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its
> memory against Round 614B and the current repository state.

**Severity:** P1/P2

**Status:** Implemented in source; the static and configured-PostgreSQL Auto Reload lock-order
regressions are green on the current source. The broader exact-tip focused/configured manifest,
aggregate, and strict-Clippy evidence remains pending.

## Issue

Effect-capable Jobs transactions did not share one PostgreSQL row-lock order. Application saves
could wait for an application row after resolving signed authority, while reservation paths could
hold entitlement or reservation rows before reaching the same application. Running-status,
workflow, browser, local-runner, managed-cloud, and final-receipt paths also acquired overlapping
effect rows in different orders. Those paths could form application/reservation, entitlement/
reservation, or effect-row deadlock cycles.

Several paths also sampled process or database time before their last potentially blocking lock.
A transaction could wait behind a publisher, reservation, lease, ticket, session, runner-volume,
or capacity writer and then authorize a mutation using a time at which the authority was still
valid even though it had expired by the time the effect was committed.

The Stripe Auto Reload reversal path also selected and locked refundable `credit_batches` before
locking the owning account. Account metering uses the opposite dependency, `Account ->
credit_batches`, so concurrent reversal and metering could form a separate account/batch deadlock
cycle outside the Jobs application rows.

## Root Cause

The affected paths evolved around separate application, reservation, workflow, local-browser,
managed-cloud, and submission-finalization boundaries. Each boundary validated its own subset of
state, but no common contract stated:

1. the order in which overlapping PostgreSQL authority and effect rows must be locked; or
2. that temporal authorization must be evaluated only after the final blocking effect lock.

Some compatibility helpers also sampled time internally or reacquired authority locks. Other
paths used a preliminary runner-volume or capacity result as if it were final. This made a locally
correct check unsafe when composed with a concurrent publisher or effect transaction.

## Canonical Order

The shared notation in this fix is:

- `H`: operational-hold authority;
- `M`: managed-cloud release/admission registry;
- `ATS`: ATS-certification authority;
- `D`: discovery/account source authority;
- `Account`: account write/deletion fence;
- `A`: exact application and its posting;
- `E`: account runner entitlement; and
- `R`: exact attempt reservation.

Fresh PostgreSQL effect paths now enter through `H -> M -> ATS -> D`, acquire the account fence,
and then use `A -> E -> R` whenever those three row families are involved. Effect-specific rows
follow that prefix in a stable order. A path that does not touch one of `A`, `E`, or `R` omits it;
it does not acquire a later row and then come back to an earlier row.

## Fix Summary

### Application persistence and prepared finalization

- Queued/running `save_application` uses
  `H -> M -> ATS -> D -> Account -> A/posting -> E -> R -> current composed authority -> hold ->
  application CAS`.
- The exact expected application revision is locked with `FOR UPDATE`, while its posting is held
  `FOR SHARE`, before queue admission can wait on entitlement or reservation state.
- PostgreSQL prepared finalization uses READ COMMITTED and
  `H -> M -> ATS -> D -> Account -> profile/posting -> expected A -> E -> R -> Auto-submit
  authority -> refreshed composed authority -> hold -> evidence/resume/application CAS`.
- The queued approval timestamp comes from the refreshed composed original-source result. The
  refresh occurs after entitlement, reservation, and exact Auto-submit locks, so a wait cannot
  carry approval past source, ATS, or signed-integrity expiry.
- Approval bytes and the queued application transition remain in the same transaction.

### Reservation and running admission

- `reserve_application_attempt` uses
  `H -> M -> ATS -> D -> Account -> A/posting -> E FOR UPDATE -> exact R FOR UPDATE -> signed
  source/integrity and runner capability -> R mutation`.
- `update_attempt_reservation_status` uses the same `A -> E -> R` order for `reserved` and
  `running`, including existing-reservation revalidation. Release and terminal bookkeeping paths
  that do not require running capability do not acquire unrelated authority rows.
- Entitlement, exact runner assignment, current composed source/ATS/integrity, signed employer
  domain, and operational hold checks all precede reservation DML.

### Workflow start and resume

- Fresh cloud start uses
  `H -> M -> ATS/fleet -> D -> Account -> A/posting -> E -> R -> generation allowance -> browser
  session -> final current authority -> reservation/meter/session/command/binding mutations`.
- Fresh resume uses
  `H -> M -> ATS/fleet -> D -> Account -> A/posting -> E -> start-command authority -> exact open
  intervention -> final current authority -> intervention/command/binding mutations`.
- Resume does not mutate a reservation and therefore does not acquire `R` merely for symmetry.
- Exact command replays remain read-only and return the already frozen authority.

### Local-browser claim and FinalSubmit

- Standalone and browser-bound fresh claims use
  `H -> M -> ATS -> D -> Account -> A -> E -> R -> ticket -> session`.
  Browser-bound claim then locks the browser-release registry before sampling database time.
- The post-lock database scalar drives ticket expiry, current signed authority, browser-release
  validity, hold admission, and the claim CAS. The after-prelock authority helper does not reacquire
  `H`, `M`, `ATS`, or `D`.
- Initial local FinalSubmit extends the order through the browser-release registry and a
  deterministic lock of the complete account submission-capacity set. It samples one database
  time only after those locks, then revalidates signed execution, browser release, hold, ATS,
  ticket, and capacity before proof binding, capacity reservation, or `click_started` mutation.
- A `click_started` replay locks its exact ticket, browser release, terminal ATS binding,
  application, browser session, and exact active capacity row without an expiry predicate. It
  then samples database time and checks capacity expiry. The replay returns authenticated stored
  authority and performs no new employer effect.

### Managed-cloud authority and execution leases

- Managed effect resolution makes application-before-entitlement an explicit precondition, then
  locks `R`, the exact command/binding, and the exact execution lease.
- Its first readiness pass is read-only and exists only to acquire all managed readiness/runtime
  rows. A second database-time sample is taken after that pass, and current signed authority,
  operational holds, recovery admission, runtime instance, and worker identity are all repeated at
  that final scalar. The returned resolution carries that scalar to the caller.
- Managed effect authorization first acquires the canonical authority/effect rows, performs a
  read-only runner-volume pass, and then repeats managed resolution. The final returned scalar is
  reused for lease expiry and runner-volume revalidation.
- PostgreSQL heartbeat performs a read-only runner-volume prepass, locks the account and exact
  lease, samples database time, and repeats runner-volume and lease-expiry validation before the
  renewal CAS. A stale preliminary time cannot resurrect an expired lease.
- Fresh cloud FinalSubmit uses
  `H -> M -> ATS -> D -> Account -> A -> E -> R -> managed/runner prepass -> lease -> browser
  session -> account capacity set -> final database time/current authority -> ATS/proof/capacity/
  lease mutations`.
- Preliminary time values in these paths are explicitly non-authoritative. No receipt acceptance,
  capacity change, lease CAS, or employer-facing effect is derived from them.

### Final receipt and object publication

- PostgreSQL final receipt reconciliation uses
  `runner-volume prepass -> Account -> A -> R -> lease/ticket -> browser session -> pending object
  uploads -> object outbox -> exact capacity -> final database time`.
- The final scalar is reused for runner-volume identity, execution grace/expiry, receipt evidence,
  object capacity, terminal session timestamps, and application submission timestamps.
- Pending uploads and outbox rows are locked in deterministic order. The exact capacity row is
  locked before any temporal acceptance or publication mutation.
- The separate PostgreSQL application-evidence upload reservation locks its application, existing
  upload, PUT outbox, and exact submission-capacity row before sampling database time. That scalar
  drives capacity expiry/CAS, upload and outbox timestamps, replay, and daily usage.
- Expiry during a capacity-row wait denies the upload with no capacity, upload, outbox, or usage
  mutation. The complete residual capacity-window repair is recorded in FIX-755.

### Account metering and Auto Reload reversal

- `reverse_and_restrict` now locks the exact account row before selecting or updating refundable
  Auto Reload credit batches.
- That makes reversal use the same `Account -> credit_batches` dependency as account metering and
  removes the reachable reverse edge.
- A static source-contract test pins account-first ordering. A configured PostgreSQL contention
  test runs real reversal and metering transactions under bounded statement/lock timeouts, checks
  blocking state, rejects SQLSTATE `40P01`, and verifies the final account, batch, attempt, and
  ledger state.
- The configured test identifies the sole blocker through `pg_blocking_pids`; it does not depend
  on brittle `pg_stat_activity` query-text matching.

### SQLite equivalence

SQLite retains `BEGIN IMMEDIATE`/`TransactionBehavior::Immediate` for effect transactions. Its
single writer boundary prevents the PostgreSQL row-wait cycle. SQLite paths still revalidate the
same current signed authority, holds, exact runner/ticket/lease state, and capacity before effect
DML; the PostgreSQL-only row-order helpers are not emulated with advisory locks.

## Denied Paths And Zero-Effect Behavior

- Missing or changed exact application revision, posting, entitlement, reservation, runner,
  ticket, lease, session, browser release, managed runtime, or capacity fails before protected
  application/reservation/lease/capacity DML.
- Expired or changed original-source, ATS, signed job-integrity, Auto-submit, ticket, lease,
  runner-volume, browser-release, or capacity authority fails at the final post-lock scalar.
- Operational holds and missing signed employer-domain authority fail before the effect mutation.
- Optimistic updates retain exact state/revision/fence/token predicates. A failed CAS rolls back
  all preceding writes in the transaction.
- Preliminary runner-volume/readiness checks may reject early but cannot authorize or mutate.
- Exact idempotent replays return authenticated stored state and do not remint current authority.
- Defensive ATS layout-quarantine bookkeeping remains an intentional authority-side safety effect;
  it does not commit an application, reservation, runner, or employer-facing mutation.

## Files Modified

| File | FIX-753 change |
| ---- | -------------- |
| `server/src/db/jobs/applications.rs` | Canonical application-first save/finalization order, post-lock composed refresh, CAS/static tests, and configured PostgreSQL reserve/save contention fixture. |
| `server/src/db/jobs/eligibility.rs` | Canonical application/entitlement/reservation order for reserve and running-status admission. |
| `server/src/db/jobs/workflow_commands.rs` | Canonical fresh start/resume effect-row prelocks before final composed authority. |
| `server/src/db/jobs/browser_release_authority.rs` | Browser claim uses the local-run prelock/at-ms authority split and samples time after effect and release-registry locks. |
| `server/src/db/jobs/local_runner.rs` | Canonical local claim/submit order, capacity prelock, post-lock database time, and hardened click-started replay. |
| `server/src/db/jobs/customer_data.rs` | Final receipt locks execution, object-publication, outbox, and capacity rows before its authoritative time. |
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Explicit application/entitlement prelock contract and final managed readiness/current-authority revalidation. |
| `server/src/db/jobs/execution_leases.rs` | Post-lock heartbeat, managed-effect, and irreversible-submit temporal validation with read-only preliminary passes. |
| `server/src/db/object_uploads.rs` | Application-evidence upload locks every effect row before one database-time capacity decision. |
| `server/src/db/stripe_auto_reload.rs` | Lock the account before refundable credit batches and add static/configured-PostgreSQL regressions for the shared metering order. |
| `docs/work/FIX-753-jobs-postgres-lock-order-and-post-lock-time.md` | This implementation and evidence record. |

## Regression And Static Coverage

The source contains focused contracts for the repaired boundaries, including:

- `postgres_finalization_uses_lock_first_read_committed_authority`;
- `application_queue_persistence_prelocks_complete_authority_before_reads`;
- `postgres_application_first_row_order_prevents_reserve_save_cycle`;
- `reservation_and_running_mutations_follow_complete_current_authority`;
- `postgres_workflow_effect_rows_lock_in_canonical_order_before_final_authority`;
- `postgres_claim_locks_effect_rows_before_final_database_time_and_never_relocks`;
- `standalone_local_claim_uses_signed_domain_after_one_postgres_prelock`;
- `initial_local_submit_uses_one_post_lock_database_time`;
- `click_started_submit_replay_samples_database_time_after_exact_capacity_lock`;
- `postgres_finalization_uses_one_authoritative_time_after_all_effect_locks`;
- `postgres_application_object_upload_uses_one_post_lock_database_clock`;
- `postgres_application_upload_waiting_past_capacity_expiry_mutates_nothing`;
- `submission_evidence_capacity_uses_the_final_database_time_window`;
- `postgres_auto_reload_reversal_locks_account_before_credit_batch`;
- `postgres_auto_reload_reversal_and_account_metering_share_lock_order_when_configured`;
- `managed_cloud_execution_effect_is_fresh_without_rewriting_claim_tuple`; and
- the execution-lease managed-effect/claim/FinalSubmit source-contract tests.

`postgres_application_first_row_order_prevents_reserve_save_cycle` is guarded by
`BLUEY_TEST_POSTGRES_URL` and uses an isolated disposable schema. The application-upload expiry
test uses the same configured disposable PostgreSQL lane. Their exact-tip behavioral execution
remains pending. Static tests are compiled by `cargo check --tests`, but compilation does not count
as executing their assertions.

## Evidence

- **PASS — integrated test compilation (root-observed checkpoint):** `cargo check --tests`.
  This compiled the integrated test target; it did not execute unit, static-contract, contention,
  or integration tests.
- **PASS — historical local replay source hygiene:**
  `rustfmt --edition 2021 server/src/db/jobs/local_runner.rs` and
  `git diff --check -- server/src/db/jobs/local_runner.rs`.
- **PENDING — exact-tip integrated compilation after the later click-replay, capacity-window,
  application-upload, and runner-specific fixture edits.**
- **PENDING — remaining focused/static test execution.**
- **PENDING — remaining configured PostgreSQL contention execution.**
- **PASS — current source:**
  `db::stripe_auto_reload::tests::postgres_auto_reload_reversal_locks_account_before_credit_batch`,
  1 passed and 0 failed.
- **PASS — configured PostgreSQL 17 `r6`, current source:**
  `db::stripe_auto_reload::tests::postgres_auto_reload_reversal_and_account_metering_share_lock_order_when_configured`,
  1 passed and 0 failed. The first attempt exposed a test-observer defect; after the test-only
  blocker lookup was corrected to use `pg_blocking_pids`, the exact behavioral case passed.
- **PENDING — full Rust tests and strict Clippy.**
- **PENDING — hosted PostgreSQL interruption/expiry evidence.**

No test command is reported as passing merely because its source compiled.

## How To Test

```bash
# Integrated compilation.
CARGO_INCREMENTAL=0 cargo check --manifest-path server/Cargo.toml --tests

# Representative source-contract tests.
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  application_queue_persistence_prelocks_complete_authority_before_reads -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  reservation_and_running_mutations_follow_complete_current_authority -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  postgres_workflow_effect_rows_lock_in_canonical_order_before_final_authority -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  initial_local_submit_uses_one_post_lock_database_time -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  click_started_submit_replay_samples_database_time_after_exact_capacity_lock -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  postgres_finalization_uses_one_authoritative_time_after_all_effect_locks -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  postgres_application_object_upload_uses_one_post_lock_database_clock -- --nocapture

# Configured PostgreSQL contention test. Use only an isolated disposable database.
BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> CARGO_INCREMENTAL=0 \
  cargo test --manifest-path server/Cargo.toml --lib \
  postgres_application_first_row_order_prevents_reserve_save_cycle -- --nocapture
BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> CARGO_INCREMENTAL=0 \
  cargo test --manifest-path server/Cargo.toml --lib \
  postgres_application_upload_waiting_past_capacity_expiry_mutates_nothing -- \
  --nocapture --test-threads=1

# Auto Reload/account-metering dependency order.
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  postgres_auto_reload_reversal_locks_account_before_credit_batch -- --nocapture
BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> CARGO_INCREMENTAL=0 \
  cargo test --manifest-path server/Cargo.toml --lib \
  db::stripe_auto_reload::tests::postgres_auto_reload_reversal_and_account_metering_share_lock_order_when_configured \
  -- --exact --nocapture --test-threads=1

# Aggregate gates after focused tests pass.
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml
CARGO_INCREMENTAL=0 cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- Remaining focused/static assertions, the rest of the configured PostgreSQL contention set, full
  tests, and strict Clippy have not yet been run against the final frozen FIX-753 source.
- The recorded integrated `cargo check --tests` checkpoint predates the final bounded
  click-started replay, capacity-window, application-upload, and runner-specific fixture edits; an
  exact-tip rerun is still required.
- The isolated contention fixture proves the canonical application-first wait/no-deadlock shape;
  broader hosted PostgreSQL publisher, expiry-during-wait, interruption, and rollback matrices
  remain Phase 614B release evidence.
- This fix changes transaction ordering and temporal admission only. It does not enable Jobs
  production flags, deploy, submit an application, or perform any external side effect.
