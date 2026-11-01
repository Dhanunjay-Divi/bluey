# Test Bluey Jobs — November 2, 2026

Two local tests are available. The saved-account test uses a disposable
non-production PostgreSQL database and proves that one prepared application
survives an actual UI reload during the same run. The preview is a separate,
in-memory product tour.

Neither test connects to an employer, job provider, production account, email
inbox, payment service, or real application. Use synthetic data only.

## Saved non-production account test

Use the current qualified checkout only. From Terminal, copy and paste this
whole block:

```sh
cd /Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831

/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py \
  with-machine-lock --machine mac-build -- \
  env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-saved-account-human-20261102 \
  scripts/run-bluey-tests.sh -- \
  env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node \
  jobs/scripts/run-nonprod-postgres-tests.mjs \
  --pg-bin /opt/homebrew/opt/postgresql@17/bin -- \
  /bin/zsh -f -c 'set -euo pipefail
cargo build --offline --locked --manifest-path server/Cargo.toml --bins
cargo build --offline --locked --manifest-path server/Cargo.toml \
  --features integration-test-support --example jobs-nonprod-browser-fixture
node jobs/scripts/run-nonprod-browser-api-tests.mjs --human-hold'
```

Startup includes private offline builds and can take several minutes. That is
setup time, not a product-latency measurement. When the isolated browser opens,
the synthetic session has at most 10 minutes from token issuance; it is not a
password login.

In the browser:

1. Open **Matches**, then select **Refresh Jobs** to load the seeded synthetic posting.
2. Open **Synthetic Qualification Employer** only.
3. Keep **Factual** and **Review First**, then select **Prepare**.
4. Open **Applications** and use the browser's **Reload** toolbar control.
5. Confirm the application says **Awaiting Review** and
   **Prepared · Review First**.
6. Select **Finish synthetic test** in the compact test panel.

Do not close the browser or stop Terminal before selecting Finish. File imports,
downloads, popups, and nonlocal browsing are blocked in this test. Do not enter
real profile data, follow a real job link, approve or submit an application, or
perform any mail or inbox action.

The account and prepared application persist only across reloads inside this
single run. Cleanup removes them. This does not provide a password account or
cross-restart durability. A cancelled or timed-out run is not a pass. Separate
SIGTERM tests during API startup and after headed readiness both passed their
expected-failure, absent-report/process and complete outer-cleanup checks.

### Evidence already observed

- The unchanged default journey passed its ordered v4 `9/9` checks with terminal
  exit `0` (run `33796`; journal SHA-256
  `35130f465cd2157faaf8d928a45a4d0876a8ebeeb90d1f0699ad8785852bae54`).
- The headed saved-account journey passed its closed v1 `8/8` checks with
  terminal exit `0` (run `93177`; journal SHA-256
  `308be50b1d0cbed1c0ced7a22b4ad7b6ac634e21174743a28d30f3716793df08`).
- The observed flow used only the owned synthetic application path. The actual
  Applications reload preserved the prepared Review First state; API and opaque
  PostgreSQL readbacks agreed. Account isolation returned the expected `401`,
  and effect, egress, page-error, and server-error counts remained zero.
- The focused Prepare control was visibly unobscured by the compact test panel
  in this run. This observation is not a general accessibility-conformance
  claim.
- Both runs used source manifest
  `9b1ee47e1deeb796af1d771513f6c2dd60362f832f1f4c5711bd8d9b2990c764`,
  HEAD `409498589b8d2e62b5ef54ebdee1372aeb8d005f`, and scoped-status SHA-256
  `932ee3fd34f0e460355ecf5ef07063c2c940cd431cacf8e2fc22da5588722fdc`.
  Final source, PostgreSQL, PID, listener, run-root, parent, and index cleanup
  checks passed.
- Startup cancellation `5512` and ready-state cancellation `10800` each exited
  `0` in their negative-test controller only after the interrupted wrapper was
  non-passing, both report paths were absent, and process/PG/root cleanup passed.
  Their journal SHA-256 values are
  `9430914ea7fec1f5c2da18ab0955b55a77d68c6e9bdf5671f41f0a9d1b002594` and
  `f7c017633ee6886e4bed568576d8a4dc163d8d77676c5ae32c4939b461bb31fb`.
  Four malformed mode invocations also failed before any resource startup.

## In-memory preview

For a lighter product tour, from Terminal:

```sh
cd /Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831/jobs
/opt/homebrew/bin/npm run dev --workspace @bluey/jobs-portal
```

Open `http://127.0.0.1:5187/jobs/?preview=1` and confirm the visible **Preview**
label. Keep `preview=1` in the address. Press `Ctrl-C` in Terminal when finished.
If port 5187 is already in use, do not stop an unknown process.

Preview changes are fixture-only and in memory. Refreshing resets them. Preview
does not prove sign-in, PostgreSQL persistence, provider discovery, browser
automation, employer submission, email delivery, or a verified receipt. Do not
use real credentials, resumes, or personal data.

## Current boundary

These are local synthetic qualifications, not release approval or proof of full
autonomy. Bluey Jobs is not ready for real applications. Cloud-runner startup,
real provider/employer effects, production deployment, cost and latency targets
remain outside this qualification or unverified. The two cancellation tests
cover their exact SIGTERM points, not every possible interruption timing.
