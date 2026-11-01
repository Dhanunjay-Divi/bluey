# Phase638 active session-binding focused regressions

Prepared, not executed at creation. Use only after independent source review and
complete prospective-index whitespace/privacy. This uses the reviewed shared Mac
queue, canonical temporary launcher and fresh private PostgreSQL. It runs the
eight pure resolver tests, three SQLite lifecycle tests and existing managed submit
fixtures, then actual ordinary API/real-client prepared claim, observed heartbeat,
safe release and a new signed HTTP terminal replay. These tests never call an
employer, start a browser/VM/Temporal engine or qualify native runtime measurement.
The owner/production account and database are not used. Every child and private
Cargo/HOME/PG/data/output path must be cleaned even on failure.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-session-binding-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node jobs/scripts/run-nonprod-postgres-tests.mjs --pg-bin /opt/homebrew/opt/postgresql@17/bin -- /bin/zsh -f -c 'node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --preflight-client && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::execution_session_authority -- --nocapture --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::tests::managed_cloud_final_submit_boundary_denies_omission_and_wrong_worker_without_mutation -- --exact --nocapture --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::tests::managed_effect_rechecks_public_beta_before_new_authority_and_submit -- --exact --nocapture --test-threads=1 && cargo build --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --bin bluey-jobs-api --example jobs-nonprod-runner-control-plane-fixture && node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --prepared-lifecycle'
```

Record exact source/HEAD/status and binary identities, actual tests/counts, failures
and process/listener/root cleanup. Never relabel unrelated fixture/setup rejection
as successful session binding. This is not the full D0.3 negative matrix or D0.4
server/portal qualification. Retained ordinary checkpoint recovery is separately
unqualified and must not depend on active START state when it is corrected.

## Failure-diagnostic rerun only

After84098 passed all13 focused Rust tests, only the Node failure diagnostic
changed. Reuse the same private build/API reproduction without rerunning those
unchanged Rust tests solely to collect the closed PG error category. This does
not replace any full-source or regression gate after a product fix. No result
from this diagnostic run qualifies source by itself.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-session-binding-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node jobs/scripts/run-nonprod-postgres-tests.mjs --pg-bin /opt/homebrew/opt/postgresql@17/bin -- /bin/zsh -f -c 'node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --preflight-client && cargo build --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --bin bluey-jobs-api --example jobs-nonprod-runner-control-plane-fixture && node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --prepared-lifecycle'
```

## FIX826 actual PostgreSQL finalizer plus focused regressions

Prepared, not executed at addition. Expect14 observed tests:1 migrated PostgreSQL
subject finalizer creation/rollback/replay,8 pure and3 SQLite session cases,2 existing
managed-authority cases. The wrapper discovers nonempty exact filters, allocates
each case a fresh private database, requires PG tests, and shares one compiler root
without resetting append-only authority. This avoids contaminating the ordinary
API reproduction with regression fixture state. Run that API recipe separately
only after this focused pass, with fresh source/privacy/review gates.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-subject-regression-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node jobs/scripts/run-nonprod-postgres-tests.mjs --pg-bin /opt/homebrew/opt/postgresql@17/bin --case-filter db::jobs::runner_volume_purge_tests::postgres_runner_volume_finalizer_creates_smallint_subject_and_replays_when_configured --case-filter db::jobs::execution_session_authority --case-filter db::jobs::tests::managed_cloud_final_submit_boundary_denies_omission_and_wrong_worker_without_mutation --case-filter db::jobs::tests::managed_effect_rechecks_public_beta_before_new_authority_and_submit -- cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support
```

The finalizer case uses an actual prepared lease through the existing debug legacy
context, then the production prepare/finalize transaction and public binding wrapper.
It is not the separate ordinary managed HTTP claim or cloud execution evidence.
