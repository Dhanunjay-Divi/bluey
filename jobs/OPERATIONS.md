# Bluey Jobs Operations

## Processes

Run these as separate deployable services:

1. `bluey-jobs-api` on the Jobs API origin.
2. `@bluey/jobs-workflows` worker on the `bluey-jobs-applications` Temporal task queue.
3. `@bluey/jobs-workflows` discovery worker via `npm run start:discovery --workspace @bluey/jobs-workflows`.
4. `@bluey/jobs-workflows` global candidate-feed worker via `npm run start:global-discovery --workspace @bluey/jobs-workflows`.
5. `@bluey/jobs-workflows` gateway for authenticated workflow start and resume requests.
6. `@bluey/jobs-runner` in a Chromium-capable container pool.
7. The static Jobs portal under `/jobs`.

On a Bluey production host, install `ops/bluey-jobs.env.example` as
`/etc/bluey-api/bluey-jobs.env`, replace every placeholder with an independent
secret, and set mode `0640` with owner `root:bluey`. The Jobs systemd unit also
loads the shared API, Valkey, and Postgres environment files. Keep
`BLUEY_JOBS_BETA_ENABLED=0` until the restricted beta is intentionally opened.
Keep `BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0` until versioned Bluey
Browser packages and updater metadata are published and physical macOS/Windows
install, launch, protocol, and rollback canaries pass. Plan entitlement alone
must never expose an undistributed client.
Keep `BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0` until the authenticated
workflow gateway, Temporal workers, and isolated Chromium pool are deployed and
their start, intervention, restart-recovery, receipt, and rollback canaries pass.
The Jobs workspace masks Cloud plan runner access unless both that explicit
release gate and a non-empty workflow credential are present. A plan entitlement
must never be presented as runtime availability.
Install `ops/bluey-api-jobs-env.conf.example` as the main API service drop-in so
account export, account deletion, and Jobs admin routes use the same data key.
The standalone Jobs API binds to loopback by default; container deployments
must opt into another IP with `BLUEY_JOBS_API_HOST` and enforce private ingress.

The Jobs API and workflow gateway share `BLUEY_JOBS_WORKFLOW_TOKEN`. The Jobs
API and Temporal worker share `BLUEY_JOBS_WORKER_TOKEN`. The Temporal worker
and browser pool share `BLUEY_JOBS_RUNNER_TOKEN`. Use independently generated
32-byte secrets and rotate them separately.

## Required environment

```text
BLUEY_JOBS_BETA_ENABLED=1
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
# Keep managed model generation disabled until provider credentials, the
# global spend guard, and usage-ledger monitoring are verified in production.
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_UPSTREAM_SPEND_LIMIT_CENTS=1000
BLUEY_UPSTREAM_SPEND_WINDOW_HOURS=24
BLUEY_JOBS_WORKFLOW_ORIGIN=https://jobs-workflows.internal
BLUEY_JOBS_WORKFLOW_TOKEN=<random secret>
BLUEY_JOBS_WORKER_TOKEN=<random secret>
BLUEY_JOBS_DISCOVERY_WORKER_ID=<stable deployment replica ID>
BLUEY_JOBS_DISCOVERY_POLL_MS=5000
BLUEY_JOBS_GLOBAL_DISCOVERY_WORKER_ID=<stable global-ingestion replica ID>
BLUEY_JOBS_GLOBAL_DISCOVERY_POLL_MS=5000
BLUEY_JOBS_GLOBAL_DISCOVERY_RUN_INTERVAL_MS=21600000
BLUEY_JOBS_GLOBAL_DISCOVERY_MANIFEST_REFRESH_MS=900000
BLUEY_JOBS_GLOBAL_DISCOVERY_ARTIFACT_TIMEOUT_MS=1800000
BLUEY_JOBS_GLOBAL_DISCOVERY_MAX_ARTIFACT_BYTES=4294967296
BLUEY_JOBS_GLOBAL_DISCOVERY_STAGING_DIR=/var/lib/bluey-jobs-global-discovery
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
with command `node workflows/dist/discovery-worker.js`, or install
`ops/bluey-jobs-discovery.service.example` for a co-located systemd deployment.
The co-located unit loads `/etc/bluey-api/bluey-jobs-discovery.env` after the
shared Jobs environment. Set the private worker values there so signed worker
routes never cross the public edge:

```env
BLUEY_JOBS_API_ORIGIN=http://127.0.0.1:8081
BLUEY_JOBS_DISCOVERY_WORKER_ID=production-discovery-1
BLUEY_JOBS_DISCOVERY_POLL_MS=5000
```

Keep this file root-owned and mode `0640`, with group access limited to the
service account. Do not put the signing key in this override; it remains in the
shared root-managed Jobs environment.
The example unit is part of `bluey-jobs-api.service`, so API maintenance also
restarts the co-located worker after the listener is available. Confirm both
units are active after deployment; do not leave the worker stopped after an API
binary swap.

It leases only
server-configured, host-pinned Greenhouse, Lever, Ashby, SmartRecruiters, and
Workday sources, sends complete snapshots, and reports bounded failure codes.
Networked production workers must use HTTPS. Plaintext is accepted only for a
co-located worker connecting to a loopback-only Jobs listener.

Run the shared candidate-feed worker separately with
`node workflows/dist/global-discovery-worker.js`, or install
`ops/bluey-jobs-global-discovery.service.example`. Its root-owned override file
at `/etc/bluey-api/bluey-jobs-global-discovery.env` should contain only the
loopback API origin, stable worker ID, an explicit comma-separated
`BLUEY_JOBS_GLOBAL_DISCOVERY_SOURCE_FAMILIES` canary allowlist, and bounded
timing overrides. The shared Jobs environment supplies the signing key. An
unset source-family allowlist means every nonempty manifest family, so do not
leave it unset during the initial production rollout. Expand the allowlist only
after each family's source completion, disk headroom, row counts, and original-
source revalidation metrics pass. The worker reads a pinned HTTPS
manifest, downloads each immutable CSV to private `0700` staging, verifies its
exact byte length and SHA-256, streams bounded batches to the shared candidate
index, and removes the staged artifact after the run. A completed snapshot is
accepted only when the server observes the exact manifest row and batch counts.

The artifact ceiling is 4 GiB because current Workday, EURES,
Bundesagentur, and SuccessFactors snapshots exceed the former 1 GiB ceiling.
Artifacts are streamed to disk and then parsed as bounded batches; they are
never loaded into memory as one buffer. Provision at least 8 GiB of free space
in the private staging filesystem so the largest current snapshot, its partial
download, and normal filesystem overhead fit safely. Alert below that headroom,
and keep the service stopped rather than silently omitting a source family.
The 30-minute download timeout and 4 GiB byte ceiling are explicit environment
controls, not permission to follow redirects to an unpinned host or accept an
artifact whose declared size, digest, schema, or row count differs from the
signed manifest.

These shared-feed records are candidate leads, not employer application truth.
Every account projection remains Review first, and the original employer URL
must pass a fresh availability and ATS-capability check before packet creation,
queueing, or submission. Unknown portals, public lists, LinkedIn, Indeed,
ZipRecruiter, Dice, and similar aggregators never gain submission authority from
feed inclusion. Disable the systemd unit to stop shared-feed refresh without
affecting direct account imports or the account-specific ATS discovery worker.

### Discovery source lifecycle

Discovery is deny-by-default. A Jobs administrator can provision a source for
an account with `POST /admin/jobs/discovery-sources/:account_id`. In addition,
a verified direct import from the account's own public Greenhouse, Lever,
Ashby, SmartRecruiters, or Workday job page can enroll that exact provider
board for the imported Career Track. Manual links and LinkedIn, Indeed,
ZipRecruiter, Dice, unknown, and private targets never enroll discovery.
Source keys are the official provider identifiers: a Greenhouse board token,
Lever site, Ashby board name, SmartRecruiters company identifier, or a Workday
`tenant~instance~site` tuple. The worker derives the provider host and path;
neither administrators nor imports can supply an arbitrary scheduled URL. New
sources start as `waiting`, run once immediately, then use a four-hour
baseline interval with stable per-source jitter and failure backoff. Discovery
is capped at 8 sources per Career Track and 24 per account. A provider board
is bound to exactly one Career Track in an account; reject a conflicting
import/source instead of allowing two tracks to overwrite the same job. A
future multi-track product must use an explicit association table. Sources
become `healthy` only after a complete verified snapshot, become `degraded`
after a failed run, and pause after three consecutive failures. Paused and
stale sources cannot authorize queueing or a runner start.

Verified public imports validate their board and Career Track enrollment before
the match is accepted. A quota or conflicting board binding is returned to the
caller; it is never silently converted into an unscheduled import. Workspace
repair remains for legacy rows only, not as a client-side replay mechanism.

Each authenticated workspace load also performs a bounded, idempotent repair
for existing verified public-ATS imports. It considers only allowlisted import
sources with a verified timestamp and a valid account Career Track; manual,
restricted, unknown, forged, and unverified postings remain ineligible.
The repair scans up to 250 eligible imports in deterministic workspace order,
skips bindings already known or over quota, and never treats a client replay
as the repair mechanism.

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

Nonterminal browser work is also checkpointed in authenticated `BLUEYJP2`
`run-checkpoint` envelopes under `run-checkpoints/<profile-scope>/`. The AAD and
HKDF context bind both the hashed profile scope and hashed browser-session
scope. A checkpoint contains the frozen request, bounded events, safe workflow
phase, provider-review state, and non-secret lease fence/expiry/owner metadata;
it never serializes the lease token or worker signing key. On startup the
runner seals every valid crash-left `active/<profile-scope>` directory into its
encrypted profile snapshot (or removes it if sealing fails) before opening the
HTTP listener. It rehydrates only unexpired `prepared`, `needs_input`, and
`provider_review` checkpoints. `final_submit_started`, activated, uncertain,
and otherwise ambiguous checkpoints remain `side_effect_unknown` and are never
automatically replayed. Configure a stable `BLUEY_JOBS_RUNNER_ID` so a restarted
replica can rotate its still-prepared server lease immediately; without one,
recovery waits for the old prepared lease to expire.

Bluey Browser uses the same conservative phase policy for local runs. Its
operation-scoped result/resume capabilities and frozen request are held only in
an AES-256-GCM checkpoint under Electron's per-user data directory; the root
claim ticket is never persisted. On macOS/Linux the random installation key is
an owner-only `0600` file under a `0700` recovery directory, deliberately
avoiding Keychain prompts. On Windows, the secure-store-disabled path uses the
same random file key and removes ACL inheritance before granting only the
current account access. Electron `safeStorage`/DPAPI is loaded only when an
operator explicitly enables `BLUEY_USE_OS_KEYCHAIN` or
`BLUEY_USE_SECURE_STORE`; the release test environment sets both to `0` and
proves zero secure-store calls. A durable local final-submit marker always
overrides a stale safe checkpoint and forces manual reconciliation.

Immediately before every local final click, Bluey sends only the scoped submit
capability to `POST /api/jobs/local-runs/:run_id/authorize-submit`. The server
atomically rechecks current local-run entitlement, verified application
identity, ticket/session/application/profile binding, and the stored
provider-final-review approval. The Browser rejects redirects, credentials,
cache, non-success responses, malformed payloads, and requests that exceed ten
seconds; it rechecks local capability expiry after the response. No denial path
may write the durable submit marker or click. A crash after the marker or click
is `side_effect_unknown` and is never automatically retried.
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
