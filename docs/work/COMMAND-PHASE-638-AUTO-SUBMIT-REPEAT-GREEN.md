# Phase638 ordinary auto-submit prepared retry qualification

Prepared recipe, NOT an observed pass. Root owns all runtime. Run only after
independent source review and complete prospective-index privacy/whitespace.
Use canonical isolation/private PostgreSQL and the shared mac-build lock.
No browser, Temporal engine, real ATS, owner account or employer effect is allowed.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-auto-green-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node jobs/scripts/run-nonprod-postgres-tests.mjs --pg-bin /opt/homebrew/opt/postgresql@17/bin -- /bin/zsh -f -c 'node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --preflight-client && cargo build --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --bin bluey-jobs-api --example jobs-nonprod-runner-control-plane-fixture && node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --auto-submit-prepared-repeat'
```

Focused production regression names (two new, two extended):

- db::jobs::managed_cloud_prepared_ats_retry_requires_same_owner_live_exact_lease
- db::jobs::ats_certification_authority_tests::managed_cloud_ats_retry_rejects_incompatible_record_and_runtime
- db::jobs::ats_certification_authority_tests::phase_a_nonce_scope_packet_and_exact_replay_fail_closed
- db::jobs::ats_certification_authority_tests::certified_cloud_context_runs_phase_a_then_atomic_phase_b

Run each with cargo test --offline --locked --manifest-path server/Cargo.toml
--lib --features integration-test-support FILTER -- --exact --nocapture
--test-threads=1, inside the same private launcher/PG command before the API build.
Assert each gate exits0 before dispatch.
Do not run the old expected409 case to qualify the corrected path.

The new separate scenario requires ordinary Track consent/auto packet preparation,
first actual claim200 and production-decoded exact PhaseA, observed heartbeat,
fresh readiness replay and same-input fresh-proof claim200/new token/higher fence.
Compare the complete ATS row, not only its summary: frozen bytes/nonce/expiry stay
unchanged. Recheck exact current lease/volume/process-runtime binding, immutable
families, attempt and account subject. Fresh signed old-token/fence heartbeat and
safe-finish must409 without changing20 protected families; current heartbeat200
must extend its own lease. Safe release204 and fresh signed terminal replay204,
zero effect/capacity/canary/receipt state and complete4-child/listener/PG/root
cleanup are mandatory. The old red/review-first scenarios remain distinct.

Record exact source/HEAD/status/binaries and actual results. This protocol case
does not exercise PhaseB, expired/takeover recovery or full native/cloud/source/
release gates. sourceAccepted remainsfalse, even on a fully passing narrow run.

Actual parent run55819 on2026-10-28 exited0 after the4 exact focused tests (4passes,
zeroignored) and the command's build/green case. The mandatory current-source
review-first rerun is separate and not counted here. FIX831 records actual source,
binary, protected-state and cleanup evidence; this recipe is not release approval.
