# Phase638 cloud-entitlement and cutover focused regressions

Prepared, not executed at creation. This extends the reviewed cutover recipe with
one actual-PG integer decoder/schema regression. Expected observed count vector:
[1 decoder PG,1 cutover PG,2 fixture model,4 example guard,1 SQLite parity]=9.
Required-PG mode forbids missing-URL skips. The decoder test performs only
migrations/prepared/type reads before the fresh-fleet lifecycle case; no owner DB
or destructive reset. All output/data/HOME/Cargo stay in the canonical private root.
No browser, Temporal, employer, provider, email or production action.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-pg-regression-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node jobs/scripts/run-nonprod-postgres-tests.mjs --pg-bin /opt/homebrew/opt/postgresql@17/bin -- /bin/zsh -f -c 'cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::queue_admission_tests::postgres_cloud_browser_integer_decoder_is_fallible_when_configured -- --exact --nocapture --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::runner_volume_purge_tests::postgres_runner_volume_fleet_cutover_two_phase_replay_and_stale_snapshot_when_configured -- --exact --nocapture --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::integration_test_runner_control_plane::tests:: -- --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --example jobs-nonprod-runner-control-plane-fixture -- --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::runner_volume_purge_tests::cutover_ready_is_bound_to_exact_snapshot_and_enrollment_invalidates_it -- --exact --test-threads=1'
```

Record actual results, source/HEAD/status and exact process/PG/root cleanup.
This is focused source evidence only, not the full Rust gate or ordinary real-
client lifecycle. FIX821 also requires the separate ordinary API queue/claim pass.
