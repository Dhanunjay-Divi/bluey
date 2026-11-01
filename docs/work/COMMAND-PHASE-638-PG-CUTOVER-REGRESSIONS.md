# Phase638 PostgreSQL cutover prerequisite and fixture guards

Prepared, not executed at creation. Expected focused counts must be observed:
1 actual-PG lifecycle test,2 fixture model/schema tests,4 example guard/redaction
tests and1 existing SQLite lifecycle parity test. This is not the full Rust gate.
The nonempty ordinary API reproduction remains independently required.

Use the shared Mac queue/canonical launcher and fresh private PostgreSQL17/vector.
Only the child gets the private HOME/Cargo caches/data. Required-PG mode forbids a
missing-URL skip. Authority rows remain append-only; no destructive reset SQL.
No browser, Temporal, owner account, production, employer or provider effect.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-pg-regression-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node jobs/scripts/run-nonprod-postgres-tests.mjs --pg-bin /opt/homebrew/opt/postgresql@17/bin -- /bin/zsh -f -c 'cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::runner_volume_purge_tests::postgres_runner_volume_fleet_cutover_two_phase_replay_and_stale_snapshot_when_configured -- --exact --nocapture --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::integration_test_runner_control_plane::tests:: -- --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --example jobs-nonprod-runner-control-plane-fixture -- --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::runner_volume_purge_tests::cutover_ready_is_bound_to_exact_snapshot_and_enrollment_invalidates_it -- --exact --test-threads=1'
```

Record terminal result, actual tests/skips, source/HEAD/status and exact temporary
process/PG/root cleanup. P1 FIX819 remains open until this and the normal one-
volume fixture path pass. Run/session mismatch remains unobserved until then.
