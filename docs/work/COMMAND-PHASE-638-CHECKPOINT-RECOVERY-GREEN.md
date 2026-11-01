# Phase638 corrected checkpoint recovery qualification

Prepared, NOT run at creation. Parent owns all execution. Require independent
backend/inspector/Node/FIX833 review and prospective privacy/whitespace before
source freeze. Shared mac-build lock/canonical launcher/fresh private PG only.

First command uses --checkpoint-prepared-recovery (review-first). Inside the same
private launcher/PG command, run client preflight, these focused filters with
cargo test --offline --locked --manifest-path server/Cargo.toml --lib
--features integration-test-support FILTER -- --nocapture --test-threads=1:

- db::jobs::execution_session_authority_tests:: (expected13)
- db::jobs::execution_session_authority_db_tests:: (expected5)
- db::jobs::integration_test_runner_control_plane::tests::checkpoint_ (expected3)

Then cargo test with the same offline/locked/feature/manifest flags,
--example jobs-nonprod-runner-control-plane-fixture checkpoint_
-- --nocapture --test-threads=1 (expected2), build the exact API/fixture, and run
node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --checkpoint-prepared-recovery.
Counts must be observed, not assumed. Zero-matched filters do not qualify a gate.

Actual base wrapper, private parent
/private/tmp/bluey-test-runs-phase638-checkpoint-green-20261028:

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-checkpoint-green-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node jobs/scripts/run-nonprod-postgres-tests.mjs --pg-bin /opt/homebrew/opt/postgresql@17/bin -- /bin/zsh -f -c 'node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --preflight-client && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::execution_session_authority_tests:: -- --nocapture --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::execution_session_authority_db_tests:: -- --nocapture --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --lib --features integration-test-support db::jobs::integration_test_runner_control_plane::tests::checkpoint_ -- --nocapture --test-threads=1 && cargo test --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --example jobs-nonprod-runner-control-plane-fixture checkpoint_ -- --nocapture --test-threads=1 && cargo build --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --bin bluey-jobs-api --example jobs-nonprod-runner-control-plane-fixture && node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --checkpoint-prepared-recovery'
```

Auto-submit needs its separate fresh private cluster/root, build and
--auto-submit-checkpoint-recovery; never reuse review-first output as auto proof.
Both inner/outer validators bind expected mode to argv (FIX833). Auto requires
ordinary Track consent/auto packet and initial production-decoded ATS with whole
row unchanged; review-first requires zero ATS.

Both require observed heartbeat then stopped heartbeat; typed READ ONLY before,
after and exact-replay inspectors; actual real-client recovery200 and fresh real
recovery replay200; canonical frozen app/session invariance;16 unchanged families,
seven immutable families, unchanged stable lease/attempt authority; intended
failed application/session, released lease/attempt and exact cloud_recovery marker.
The subsequent real active-lease finish204 is a TERMINAL replay, not a second
release; prove20-family equality, then fresh signed finish204 replay unchanged.
Require zero effects and6 clean child records for review-first /7 for auto,
listeners/API/PG/root cleanup and exact source/HEAD/status/binary identities.

No browser/employer/Temporal/VM/provider, production or owner DB action. This
does not qualify unsafe/unknown/submitted recovery, broader negative/concurrency/
key-rotation/head-change matrix, full source/native/cloud gates or release.

Actual execution:6759 failedsharedfixture (FIX834),77890 passed23focused then
failedPGtyping (FIX835). AftercorrectingonlyPGprojection, build-only86077 and20772
passed review-first/auto respectively. FIX832/FIX835 record exact current source,
artifacts, observedstate and cleanup; do not count the earlier failed API runs green.
