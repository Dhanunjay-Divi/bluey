# Bluey Jobs Operations

## Processes

Run these as separate deployable services:

1. `bluey-jobs-api` on the Jobs API origin.
2. `@bluey/jobs-workflows` worker on the `bluey-jobs-applications` Temporal task queue.
3. `@bluey/jobs-workflows` discovery worker via `npm run start:discovery --workspace @bluey/jobs-workflows`.
4. `@bluey/jobs-workflows` gateway for authenticated workflow start and resume requests.
5. `@bluey/jobs-runner` in a Chromium-capable container pool.
6. The static Jobs portal under `/jobs`.

The Jobs API and workflow gateway share `BLUEY_JOBS_WORKFLOW_TOKEN`. The Jobs
API and Temporal worker share `BLUEY_JOBS_WORKER_TOKEN`. The Temporal worker
and browser pool share `BLUEY_JOBS_RUNNER_TOKEN`. Use independently generated
32-byte secrets and rotate them separately.

## Required environment

```text
BLUEY_JOBS_BETA_ENABLED=1
BLUEY_JOBS_WORKFLOW_ORIGIN=https://jobs-workflows.internal
BLUEY_JOBS_WORKFLOW_TOKEN=<random secret>
BLUEY_JOBS_WORKER_TOKEN=<random secret>
BLUEY_JOBS_DISCOVERY_WORKER_ID=<stable deployment replica ID>
BLUEY_JOBS_DISCOVERY_POLL_MS=5000
BLUEY_JOBS_RUNNER_ORIGIN=https://jobs-runner.internal
BLUEY_JOBS_RUNNER_TOKEN=<random secret>
BLUEY_JOBS_RUNNER_ID=<stable browser-pool replica ID>
BLUEY_JOBS_PROFILE_ENCRYPTION_KEY=<base64 32-byte key>
BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs
BLUEY_JOBS_TAKEOVER_ORIGIN=https://jobs-browser.bluey.sh
BLUEY_JOBS_API_ORIGIN=https://bluey.sh
TEMPORAL_ADDRESS=<namespace endpoint>
TEMPORAL_NAMESPACE=<namespace>
TEMPORAL_API_KEY=<Temporal Cloud API key>
TEMPORAL_TLS=true
```

Run the discovery worker as a separate deployment using the workflows image
with command `node workflows/dist/discovery-worker.js`. It leases only
server-configured Greenhouse and Lever sources, sends complete snapshots, and
reports bounded failure codes. Production `BLUEY_JOBS_API_ORIGIN` must use
HTTPS; plaintext origins are accepted only for loopback development.

### Discovery source lifecycle

Discovery is deny-by-default. A Jobs administrator must provision each source
for an account with `POST /admin/jobs/discovery-sources/:account_id`; the only
enabled beta providers are Greenhouse board tokens and Lever sites. New
sources start as `waiting`, become `healthy` only after a complete verified
snapshot, become `degraded` after a failed run, and pause after three
consecutive failures. Paused and stale sources cannot authorize queueing or a
runner start.

Use `PATCH /admin/jobs/discovery-sources/:account_id/:source_id` with
`{"status":"paused"}` as the per-source kill switch. Re-enabling a source
does not manufacture healthy status: it returns to `waiting` when it has never
succeeded and `degraded` otherwise, then must complete a fresh snapshot.

The worker reports only bounded event names, source fingerprints, job counts,
and failure codes. Alert on:

- any source remaining `waiting` for two expected intervals;
- any `degraded` source or three-failure automatic pause;
- lease acquisition without completion before the two-minute lease expires;
- snapshot replay mismatch or commit-fence loss;
- a source-health gate rejecting queue or runner start;
- zero discovered jobs after a previously non-empty snapshot.

Roll out one provider tenant at a time. Confirm canonical deduplication,
freshness, missing-job grace, and portal source health before provisioning the
next tenant. A missing job is expired only after two complete snapshots and a
30-minute grace period; failed or partial snapshots never close jobs.

### Browser execution leases

Every cloud run claims a database execution lease before opening Chromium.
The lease binds account, application, run, application identity, and the
identity-scoped browser profile. The runner heartbeats every ten seconds and
must acquire the irreversible-submit fence before clicking the final submit
control. Only one runner can hold an active application or browser-profile
lease.

If the final click may have happened but confirmation is unavailable, finish
the lease as `side_effect_unknown`, preserve evidence, and reconcile manually.
Never retry that employer-facing action automatically. Use
`BLUEY_JOBS_RUNNER_ID` to identify a replica without logging account, resume,
answer, cookie, or OTP data.

Mount `BLUEY_JOBS_RUNNER_DATA` on encrypted persistent storage. The runner
keeps a profile plaintext only while Chromium owns an active run. Between runs,
the profile is sealed in a `BLUEYJP2` AES-256-GCM envelope and the active
directory is removed. The configured 32-byte key is HKDF-SHA256 input, not a
direct AES key: the runner derives separate keys for every hashed tenant/profile
scope and durable request scope. Durable results bind both the 40-hex
tenant/profile digest and the SHA-256 request digest in HKDF context and AAD;
resume calls send only the tenant/profile digest, never raw account or identity
values. Versioned purpose and scope values are also authenticated as AAD.
Browser-step result records accept only the exact current committed/staged
payload envelope; a staged record is never returned as a completed result. The
runner fsyncs each `0600` ciphertext staging file before atomic replacement and
syncs the parent directory where the platform supports directory fsync. These
records make Temporal activity retries idempotent even when a submit response
is lost in transit.
Production should replicate the encrypted snapshot and receipt directory to
R2/S3 with lifecycle and tenant-deletion jobs.

`BLUEYJP1`, plaintext, malformed, unknown-version, wrong-scope, and invalid
result payload files are rejected. There is no legacy migration flag. Before a
deployment first reads `BLUEYJP2`, deploy the workflow worker that sends the
hashed `profileScope` on resume, then drain the runner and quarantine the
existing `snapshots/` and `step-results/` directories outside
`BLUEY_JOBS_RUNNER_DATA`. The old runner ignores the additional resume field,
so this order avoids a contract gap. Legacy snapshots cannot be reused, so
affected browser profiles must sign in again. Before removing a legacy result
from quarantine, reconcile its workflow, receipt, and execution-lease state and
retry only work known not to have crossed the irreversible-submit boundary.
Never decrypt arbitrary legacy result JSON and relabel it committed.

The runner resolves every top-level navigation and blocks private, loopback,
link-local, carrier-grade NAT, and credential-bearing URLs. Keep egress policy
at the container/network layer as a second boundary; application-level checks
do not replace a deny-by-default production network policy.

## Desktop releases

`npm run package --workspace @bluey/jobs-browser` downloads the matching
Playwright Chromium and builds macOS, Windows, and Linux artifacts. Public
artifacts require Apple signing/notarization and Windows Authenticode signing.
Unsigned beta artifacts must not be presented as production installers.

Run direct Electron Builder validation from `jobs/browser` so package-relative
icons, Chromium resources, and the `dist/main.js` entry resolve correctly:

```sh
CSC_IDENTITY_AUTO_DISCOVERY=false npx electron-builder --dir --config electron-builder.yml
```

The package build cleans every production `dist` directory before compiling.
Release validation must confirm `app.asar` contains `dist/main.js` and the
compiled `@bluey/jobs-automation/dist/index.js`, contains no Bluey `src` or
`tests` directories or test configuration, and includes the matching Chromium
bundle under the app's `Resources/playwright` directory.

The authenticated portal starts a local application by creating a 24-hour
capability in `jobs_local_run_tickets`, then opening
`bluey-jobs://run/<run-id>?ticket=<random-ticket>`. Only the ticket enters the
custom-protocol URL. Its secret and frozen packet are encrypted in the Jobs
database, the database stores a lookup hash separately, and terminal results
are idempotent. The desktop claims and reports the run through
`BLUEY_JOBS_API_ORIGIN`; production must keep that origin on HTTPS.

## External release gates

- Gmail and Outlook OAuth applications, redirect URIs, webhook subscriptions,
  and encrypted refresh-token storage.
- Licensed discovery-provider contracts and API credentials.
- Browser takeover streaming and short-lived authorization URLs.
- R2/S3 upload credentials for final PDFs, screenshots, and receipt bundles.
- Live sandbox certification for representative tenants of every supported ATS.
- Regional Temporal, Postgres, Valkey, OpenSearch, and browser-pool monitoring.

No source change can manufacture provider approvals, signing certificates, or
production credentials. Keep the Jobs beta flag off until these gates pass.
