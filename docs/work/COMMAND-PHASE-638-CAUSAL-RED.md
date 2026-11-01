# Phase638 ordinary API-to-runner causal-red command

Recipe prepared, NOT executed at creation. The intended outcome is a verified
404 causal reproduction with sourceAccepted:false, not a passing product gate.
Run only after independent static review and complete prospective-index privacy/
whitespace checks include every new file. No source fix precedes the actual red.

This uses the shared Mac queue, canonical launcher, a fresh password-authenticated
private PostgreSQL17/pgvector cluster and a closed child environment. The private
PG harness supplies isolated HOME/Cargo source-cache links and removes owner URLs,
provider credentials and flags. All build output/keys/data are under the owned
root and removed by canonical cleanup after child/group shutdown. No VM, browser,
Temporal service, employer connection, owner account, email or submission.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-causal-red-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node jobs/scripts/run-nonprod-postgres-tests.mjs --pg-bin /opt/homebrew/opt/postgresql@17/bin -- /bin/zsh -f -c 'node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --preflight-client && cargo build --offline --locked --manifest-path server/Cargo.toml --features integration-test-support --bin bluey-jobs-api --example jobs-nonprod-runner-control-plane-fixture && node jobs/scripts/run-nonprod-runner-control-plane-tests.mjs --expect-target-missing'
```

Record actual exit/counts/stage/failure, source/binary identities, root and terminal
process/listener/root cleanup. A setup failure is not the intended causal red.
Retain its dated result and fix only the proven fixture cause; never relabel it
or silently restart a still-live command. Production remains unchanged.
