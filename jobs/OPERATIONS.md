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
Keep `BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0` until an independently
approved root trust anchor, exact server release ID, signed channel authority,
immutable native packages, and physical macOS/Windows install, launch,
protocol, upgrade, rollback, immutable-host read-back, and portal-download
canaries are complete. Plan entitlement alone must never expose an
undistributed client. Native self-update remains a separate release capability;
the manifest-bound macOS ZIP does not imply that an installed-app updater
exists.
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
Keep `BLUEY_JOBS_MAILBOX_SYNC_ENABLED=0` until Gmail/Outlook OAuth credentials,
reviewed redirect URIs, encrypted provider-token storage, and mailbox-worker
monitoring are configured and verified. The server defaults the read-only
mailbox worker off when the variable is absent. Connecting an inbox does not
authorize Bluey to send mail or infer employer outcomes.
Keep `BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED=0`,
`BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED=0`, and
`BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED=0` as independent release
gates. Read-only inbox consent never implies send or calendar authority. The
server-owned communication worker keeps decrypted OAuth credentials inside the
API process and starts no provider write or lookup loop unless its exact flag is
enabled.

The Jobs API and workflow gateway share `BLUEY_JOBS_WORKFLOW_TOKEN`. The Jobs
API and Temporal worker share `BLUEY_JOBS_WORKER_TOKEN`. The Temporal worker
and browser pool share `BLUEY_JOBS_RUNNER_TOKEN`. Use independently generated
32-byte secrets and rotate them separately.

## Required environment

```text
BLUEY_JOBS_BETA_ENABLED=1
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
# Configure for Browser registry import/verification before enabling local
# Browser distribution. The server release ID must be accepted by the active
# signed activation; the JSON contains public root Ed25519 keys and an
# independently approved threshold.
# BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID=server-603.1
# BLUEY_JOBS_BROWSER_ROOT_TRUST_ANCHOR_JSON='{"threshold":2,"keys":{"root-key-1":"<base64url-public-key>","root-key-2":"<base64url-public-key>"}}'
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
# Keep managed model generation disabled until provider credentials, the
# global spend guard, and usage-ledger monitoring are verified in production.
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_MAILBOX_SYNC_ENABLED=0
# Set to 1 only after the Gmail/Outlook release gates below are complete.
# BLUEY_JOBS_MAILBOX_SYNC_POLL_SECONDS=30
BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED=0
BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED=0
BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED=0
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
# Leave cold archival disabled until the R2 write/read-back preflight below
# succeeds against the production bucket.
BLUEY_JOBS_GLOBAL_ARCHIVE_ENABLED=0
BLUEY_JOBS_GLOBAL_ARCHIVE_RETENTION_DAYS=30
BLUEY_JOBS_GLOBAL_ARCHIVE_POLL_SECONDS=3600
BLUEY_JOBS_GLOBAL_ARCHIVE_BATCH_SIZE=25
BLUEY_JOBS_GLOBAL_ARCHIVE_LEASE_SECONDS=300
BLUEY_JOBS_RUNNER_ORIGIN=https://jobs-runner.internal
BLUEY_JOBS_RUNNER_TOKEN=<random secret>
BLUEY_JOBS_RUNNER_ID=<stable browser-pool replica ID>
BLUEY_JOBS_PROFILE_ENCRYPTION_KEY=<base64 32-byte key>
BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs-runner
BLUEY_JOBS_RUNNER_BUILD_ID=runner-602.1
BLUEY_JOBS_RUNNER_ADMISSION_GRANT_ID=<one-time per-volume grant ID>
BLUEY_JOBS_RUNNER_ADMISSION_GRANT_TOKEN=<one-time per-volume grant token>
BLUEY_JOBS_RUNNER_PROCESS_RUNTIME_GRANT_ID=<one-time per-process runtime grant ID>
BLUEY_JOBS_RUNNER_PROCESS_RUNTIME_GRANT_TOKEN=<one-time per-process runtime grant token>
BLUEY_JOBS_RUNNER_IMAGE_SHA256=<final 64-character OCI image SHA-256>
BLUEY_JOBS_AUTOMATION_BUNDLE_SHA256=<64-character automation bundle SHA-256>
BLUEY_JOBS_PLAYWRIGHT_VERSION=<exact Playwright version>
BLUEY_JOBS_CHROMIUM_REVISION=<exact Chromium revision>
BLUEY_JOBS_CHROMIUM_EXECUTABLE_SHA256=<64-character Chromium executable SHA-256>
BLUEY_JOBS_RUNNER_PROVIDER=<provider ID>
BLUEY_JOBS_RUNNER_PROVIDER_RESOURCE_ID=<managed volume resource ID>
BLUEY_JOBS_RUNNER_RESOURCE_FINGERPRINT=<64 lowercase hex characters>
BLUEY_JOBS_RUNNER_SERVER_COMMAND_KEYS=<JSON key-ID to public-key map>
BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY_ID=<current server key ID>
BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY=<base64url 32-byte server seed>
BLUEY_JOBS_RUNNER_MINIMUM_BUILD_ID=runner-602.1
BLUEY_JOBS_RUNNER_PURGE_VERIFYING_KEYS_JSON=<JSON public-key history>
BLUEY_JOBS_TAKEOVER_ORIGIN=https://jobs-browser.bluey.sh
BLUEY_JOBS_API_ORIGIN=https://bluey.sh
TEMPORAL_ADDRESS=<namespace endpoint>
TEMPORAL_NAMESPACE=<namespace>
TEMPORAL_API_KEY=<Temporal Cloud API key>
TEMPORAL_TLS=true
```

The four purge-policy variables are server-owned and must be valid before the
API starts because account deletion now depends on signed immutable fan-out.
The private signing seed never enters a runner. Each runner instead receives
the public `BLUEY_JOBS_RUNNER_SERVER_COMMAND_KEYS` map plus its own provider-
bound admission grant. Create that grant through
`POST /admin/jobs/runner-volumes/admission-grants` only after resolving the
exact worker ID, provider resource ID, and resource fingerprint; mount the
returned ID and token into one runner and start it before the ten-minute expiry.
The grant is single-use. Use `ops/bluey-jobs-runner.env.example` as the
per-volume template, not the shared API environment.

Create the separate process-runtime grant through
`POST /admin/jobs/runner-volumes/process-runtime-grants` only after deployment
has resolved the final OCI image digest, runner build, platform/architecture,
automation bundle, Playwright and Chromium revisions, and Chromium executable
digest. Deliver its returned ID and one-time token to exactly one new process.
The signed volume instance claim atomically consumes it; neither fleet HMAC,
the volume key, nor lease JSON may self-assert a runtime. Cancel an unused or
leaked grant through the authenticated `.../:grant_id/revocations` route before
its ten-minute expiry.

Do not enable either Browser distribution flag merely because enrollment
succeeds. The fleet status, zero legacy inventory authority, current signed
storage attestations, exact cutover record, container smoke, and separate
authorized device/provider canaries must all pass. Lost or offline volumes stay
in the deletion target set until they acknowledge or an administrator records
provider-bound destruction evidence.

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
The example unit is deliberately independent of `bluey-jobs-api.service`.
API maintenance must not stop discovery indefinitely. The worker waits for the
API listener, retries continuously with `Restart=always`, and remains enabled
across reboots and API binary swaps.

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

### Durable discovery worker release

Build one relocatable artifact containing both discovery workers:

```bash
ops/build-bluey-jobs-workers.sh /tmp/bluey-jobs-workers
```

Promote the exact archive and checksum together:

```bash
sudo ops/install-bluey-jobs-workers.sh \
  /tmp/bluey-jobs-workers/jobs-workers-<commit>.tar.gz \
  /tmp/bluey-jobs-workers/jobs-workers-<commit>.tar.gz.sha256
```

The installer verifies the checksum and archive paths, extracts one immutable
release under `/opt/bluey-jobs-workers/releases`, points both worker services
at that retained release through atomic `current` links, installs the service
and health-check units, and rolls back both links if either service cannot
start. It keeps three releases by default. Never point a production unit at a
temporary build directory.

Both workers use independent `Restart=always` services. They are ordered after
the Jobs API but are not `PartOf` or `Requires` that service. The
`bluey-jobs-discovery-health.timer` checks every 15 minutes that:

- both retained release links resolve;
- both worker services are active;
- every active direct and global source has completed a successful sync within
  12 hours.

Run the same check during incident response:

```bash
sudo /usr/local/sbin/check-bluey-jobs-discovery.sh
```

The health host must have Python 3 and `psql` installed. Provision a dedicated,
random 32-byte hex `BLUEY_JOBS_DIAGNOSTIC_KEY` through the host secret manager
or the root-owned environment file read by the check:

```text
BLUEY_JOBS_DIAGNOSTIC_KEY=<64 hexadecimal characters>
```

By default the check sources `/etc/bluey-api/bluey-api.env` and
`/etc/bluey-api/bluey-postgres.env`; the health unit does not load
`bluey-jobs.env` directly. A reviewed systemd drop-in may instead set
`BLUEY_JOBS_API_ENV_FILE` to a dedicated root-owned secret file. Do not reuse
`BLUEY_JOBS_DATA_KEY`: the diagnostic key is only for domain-separated opaque
source correlation and never enters SQL, a process argument, or alert output.
The check resolves exact Python and `psql` paths, starts each helper with a
minimal clean environment, and passes the diagnostic key or database URL only
through an inherited file descriptor. The `psql` child receives the database
URL as `PGDATABASE`; the renderer receives no database, data-encryption,
diagnostic, or provider key in its environment. Use
`BLUEY_JOBS_PYTHON_BIN` only when the reviewed Python 3 executable is not named
`python3`, and use `BLUEY_JOBS_PSQL_BIN` only for a reviewed `psql` executable.
A missing or malformed key, missing dependency, unknown diagnostic dimension,
or invalid diagnostic row fails the check. Provisioning this key and those
host dependencies plus process-table inspection on the target host is an
external deployment gate; source tests do not prove it.

An overdue-source alert has the bounded form
`direct|global:<provider>:ref-<64-lowercase-hex>`. It must never contain the raw
source key. Treat any raw source, account, employer, URL, token, or secret in
this diagnostic as a privacy incident and stop the rollout.

The 12-hour limit is twice the slowest normal six-hour global cadence. A source
older than that is delayed even when its last stored health value says
`healthy`. The Jobs portal uses the same threshold and shows `Updates delayed`
instead of presenting stale data as current.

Rollback is an exact release-link switch:

```bash
sudo ln -sfn /opt/bluey-jobs-workers/releases/<previous-release> \
  /opt/bluey-jobs-discovery/current
sudo ln -sfn /opt/bluey-jobs-workers/releases/<previous-release> \
  /opt/bluey-jobs-global-discovery/current
sudo systemctl restart bluey-jobs-discovery.service
sudo systemctl restart bluey-jobs-global-discovery.service
sudo /usr/local/sbin/check-bluey-jobs-discovery.sh
```

Do not roll back database contents merely because one feed is delayed.
Candidate-feed rows remain leads and still require original-employer
revalidation before packet creation or submission.

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

### Operational Jobs holds and readiness

Run the `bluey-ops` preflight before creating or releasing an operational hold
or changing its deployment contract.

Phase 606 adds one durable, fail-closed control plane for stopping new Jobs
authority before a side effect. It does not enable a feature and it is not a
replacement for a feature flag, discovery-source pause, signed ATS circuit,
account-deletion fence, provider grant, Browser release, or runner authority.
The same protected routes are mounted on the full Bluey server and the
standalone Jobs API; both surfaces must use the same Jobs database and data-key
configuration.

The closed capability set is `discovery`, `generation`, `application_queue`,
`runner_claim`, `final_submit`, `mailbox_sync`, and
`communication_dispatch`. An operator-only `all` hold applies alongside the
requested concrete capability. The closed scope set is `global`,
`discovery_source`, `ats_provider`, `ats_adapter`, `employer_domain`, `account`,
`career_track`, `region`, `runner_kind`, `mailbox_provider`, `model_provider`,
and `model`.

The protected administrator routes are:

| Route | Contract |
|-------|----------|
| `POST /admin/jobs/operational-holds/events` | Append or exactly replay one hold/release event. |
| `GET /admin/jobs/operational-holds` | List redacted heads with encrypted keyset pagination. |
| `GET /admin/jobs/readiness` | Read aggregate generic and native blockers by capability. |

Every route requires a bearer access token for an active administrator account.
They reject normal accounts, apply a 64 KiB request-body limit, and treat
unknown mutation fields as invalid. Every success, validation error,
authentication error, conflict, and storage error is private and non-storable:

```text
Cache-Control: private, no-store
Pragma: no-cache
```

Do not put request bodies, bearer tokens, raw scope IDs, event IDs, reason
references, or responses into shared terminals, tickets, URLs, or logs.

#### Create the first hold

The first event for one capability/scope pair must be `held`, must expect
revision zero, and must not name a predecessor. Use the raw route only after
resolving the exact server-owned scope. For example:

```http
POST /admin/jobs/operational-holds/events
Authorization: Bearer <administrator-access-token>
Content-Type: application/json

{
  "eventId": "<unique-operator-event-id>",
  "capability": "generation",
  "scopeKind": "account",
  "scopeId": "<exact-account-id>",
  "transition": "held",
  "reasonCode": "incident",
  "reasonRef": "INC-606",
  "expectedHeadRevision": 0,
  "expectedCurrentEventId": null
}
```

`global` accepts only `scopeId: "*"`. Account, Career Track, and discovery
source IDs remain exact identifiers. Region and Unicode employer-domain scopes
are lowercased and NFC-normalized; verified employer-domain context includes
both its normalized Unicode form and its ASCII IDNA form. Other categorical
scope values are lowercase ASCII. Unknown-like regions such as `unknown`,
`n/a`, `not specified`, and `unspecified` are omitted from derived admission
context rather than becoming scope authority.

The closed reason-code set is `incident`, `security_review`, `privacy_review`,
`compliance_review`, `quality_regression`, `provider_outage`, `capacity_guard`,
`maintenance`, `account_request`, `certification_guard`, `rollout_guard`, and
`manual_release`. A reason reference is optional, at most 120 ASCII characters,
starts with an alphanumeric character, and otherwise contains only letters,
digits, `.`, `_`, or `-`.

A new append returns `201 Created`; an exact replay of the same event and actor
returns `200 OK` with `replayed: true`. Changed bytes or actor under an existing
event ID, stale compare-and-swap state, an unresolved opaque reference, or an
account-deletion fence returns a bounded conflict. Invalid input is rejected,
and storage or canonical-state corruption returns an unavailable response.
Never interpret a timeout, transport loss, or unavailable response as release
authority: read the current head before deciding whether an exact replay is
appropriate.

The response exposes only `capability`, `scopeKind`, `scopeRef`, `headRevision`,
`currentEventRef`, `eventSha256`, `state`, `reasonCode`, `recordedAtMs`, and
`replayed`. It never exposes raw scope ID, raw event ID, reason reference,
administrator identity, or canonical event bytes.

#### List and transition a hold by opaque reference

List active heads with:

```http
GET /admin/jobs/operational-holds?activeOnly=true&limit=50
Authorization: Bearer <administrator-access-token>
```

`activeOnly` defaults to `true`; set it to `false` when released heads are also
needed. `limit` defaults to 50 and must be from 1 through 100. When
`nextCursor` is present, pass it unchanged as `cursor` with the same
`activeOnly` value. The cursor is encrypted, purpose-bound, bounded to 2,048
bytes, and ordered by the internal capability/scope key. Do not decode, edit,
or reuse it with another filter. Follow pages until `nextCursor` is absent.

The list deliberately returns no raw scope or event identity. Use its exact
`scopeRef`, `headRevision`, and `currentEventRef` for a later transition:

```http
POST /admin/jobs/operational-holds/events
Authorization: Bearer <administrator-access-token>
Content-Type: application/json

{
  "eventId": "<new-unique-operator-event-id>",
  "capability": "generation",
  "scopeKind": "account",
  "scopeRef": "scope-<64-lowercase-hex>",
  "transition": "released",
  "reasonCode": "manual_release",
  "reasonRef": "INC-606",
  "expectedHeadRevision": 1,
  "expectedCurrentEventRef": "event-<64-lowercase-hex>"
}
```

The server resolves both refs against one indexed current head, verifies them
in constant time, then applies the same canonical compare-and-swap and replay
rules as a raw transition. Never release by guessing a ref, reconstructing a
raw identifier from an alert, changing only one expected field, deleting a
row, or updating a head directly. A release has no TTL and is itself an
append-only event. `held -> held`, `held -> released`, and
`released -> held` each require the exact current predecessor; a second
consecutive release and a first-event release are rejected.

#### Interpret readiness without weakening native authority

`GET /admin/jobs/readiness` returns schema version 1, aggregate readiness, one
entry for every concrete capability, the paused discovery-source count, the
open ATS-circuit count, and the evaluation timestamp. Each capability entry
contains `ready`, `blockerCount`, `operationalHoldCount`, and
`nativeBlockerCount`. An active `all` hold is counted for every concrete
capability in addition to its capability-specific holds.

Native authority remains independent:

- paused discovery sources contribute native blockers only to `discovery`;
- open signed ATS circuits contribute native blockers only to `final_submit`;
- releasing a generic hold does not resume a source or close a circuit; and
- resuming a source or reviewing a circuit does not release a generic hold.

Readiness fails closed when storage is unavailable or a count contains an
unknown/private dimension. A zero generic-hold count does not grant provider,
tenant, account, runner, model, or submission authority. Verify every relevant
native status and disabled-by-default release flag separately. The protected
metrics endpoint uses only the closed capability and scope-kind labels; it must
not contain raw tenant, source, employer, model, URL, token, hash, cursor,
reason reference, actor, event ID, or error text.

Holds stop only new authority at these boundaries:

- direct and global discovery lease acquisition;
- paid managed-generation provider reservation;
- application-attempt reservation, including an active-reservation replay;
- local and cloud runner claim or reissue;
- local and cloud final pre-click authorization;
- mailbox-sync lease claim; and
- communication-dispatch claim and its durable request-start marker.

Those boundaries validate their complete server-owned projection before they
evaluate a hold. Mailbox sync requires the connection, relational sync state,
and encrypted sync state to name the same provider. A `curated_feed:*` posting
requires the account's exact managed `curated_feed` / `bluey-curated-v1`
membership before application- or job-scoped authority is admitted. Discovery
leases freeze relevant Career Track insert, update, and delete operations while
the lease is unexpired; successful completion, recorded failure, or lease
expiry releases that freeze. Account-wide curated discovery validates every
Track's relational/JSON ID and active projection and selects only active
Tracks; a directly bound inactive Track remains in Career Track and Region hold
scope. Treat any projection mismatch as storage corruption: stop admission and
repair the canonical rows rather than releasing a hold or editing a head.

An API-created `unassigned` application reservation has no `runner_kind` scope;
use `application_queue` and the other applicable account, Track, employer, ATS,
region, or model scopes to stop that reservation. A later cloud or local runner
claim independently evaluates its concrete `runner_kind` before atomically
persisting the binding.

Do not use a hold to suppress evidence after a possible side effect. Heartbeat,
Browser-profile sealing, exact click-started replay, worker result and receipt
persistence, submission checkpoint recovery, mailbox completion,
communication completion, and read-only reconciliation remain available so
Bluey can reduce uncertainty safely.

For local `click_started` recovery, "exact" binds the ticket and encrypted
payload, account/application/run, running session, final-submit proof, terminal
ATS authority, active evidence capacity, and the complete Browser
build/release binding frozen onto the run. It does not freeze the current server
process deployment ID from the first authorization call. If the immutable
activation frozen onto that Browser binding accepted server releases `A` and
`B`, a possible side effect started under `A` may be recovered after the server
deploys `B`. An ID outside that same frozen accepted set is denied. The replay
must return the already durable ATS receipt authority without another marker,
canary reservation, capacity reservation, or final click. This accepted
`A -> B` rule is recovery continuity, not permission to switch the frozen
Browser manifest, build, channel, activation, or trust authority.

The HTTP exception is equally narrow. An expired signed v2 submit capability
may reach recovery only for a durable `click_started` ticket and only inside
the reconciliation grace. A disabled local-distribution flag blocks claimed or
new submit work but permits that exact ticket to reach database reconstruction.
If the object-storage maximum changed after the marker, recovery uses the
already active durable reserved bytes/object count and the current upload
limits; it never reserves replacement capacity. Signed v1 or v2 result
reconciliation uses that durable capacity when ticket state is
`click_started` or `side_effect_unknown`; submit recovery remains signed
v2-only. Claimed, `needs_input`, and other new/pre-side-effect paths derive
capacity from current storage configuration. Missing, inactive, expired,
non-local, or wrong-scope durable capacity is denied.

During an incident pause, set only
`BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0`. Keep the current
`BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID`, imported root/activation/manifest
authority, Jobs data key, and evidence object-storage configuration available
until every post-marker run is submitted, reconciled, or otherwise terminal.
The server release ID is still required to prove that the current deployment is
in the frozen activation's accepted set, and object-storage configuration is
still required to retain current upload limits. Removing either dependency is
not a safe kill switch: it strands evidence/recovery while possible employer
side effects remain unresolved.

Before production operation, complete the external gates that source work
cannot prove: authorized live PostgreSQL migration and two-connection lock
rehearsal, backup/restore evidence, exact artifact promotion, shared database
and data-key configuration across both router surfaces, administrator access
and audit ownership, alert thresholds, incident/release drills, and an approved
canary that proves a committed hold wins against concurrent new admission while
post-marker recovery still completes. Also provision the separate diagnostic
key and Python 3 dependency described above. Keep every Jobs, model, Browser,
mailbox, communication, provider, and tenant flag at its existing parked value
until its own launch gate passes; an operational release is never an enabling
action.

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

## Global candidate cold storage

Run the `bluey-ops` preflight before changing any archive setting. PostgreSQL
remains the authoritative live search and relationship index. R2 stores only an
encrypted, immutable copy of the heavy normalized candidate body for rows that
are all of the following:

- expired and older than `BLUEY_JOBS_GLOBAL_ARCHIVE_RETENTION_DAYS`;
- absent from every non-expired source membership;
- not materialized into any account match or application;
- not already archived or leased by another archive worker.

The worker is fail-closed and off by default. It writes the archive object,
reads the exact object back, checks both byte equality and SHA-256, and only
then replaces the heavy PostgreSQL candidate body with a small encrypted
tombstone. An upload, read-back, checksum, lease, or transaction failure leaves
the complete PostgreSQL payload intact and schedules a bounded retry.
Rediscovery with the same or changed content restores the complete hot payload
and clears the archive metadata.

This lifecycle is not a database backup. Keep the normal PostgreSQL backup and
restore proof, R2 replication, lifecycle, deletion, and object-inventory
procedures independently operational. Do not delete archived objects during
the first rollout. A verified content-addressed object may remain unreferenced
if the candidate changes after read-back but before the guarded PostgreSQL
completion. Do not delete that object in the worker: a newer lease may be using
the same deterministic key. Reconcile unreferenced objects through the bounded
object-inventory process after confirming no database row references them.

Before enabling production archival:

1. Verify the Jobs API is running the migration that adds the archive columns.
2. Verify the configured R2 credentials can PUT, GET, and byte-compare a test
   object under the configured private prefix.
3. Confirm the preflight query below returns only expired candidates without
   active memberships or account materializations.
4. Enable the worker with a small batch size and monitor one complete pass.
5. Verify object hashes and sizes against `archive_sha256` and
   `archive_size_bytes`.
6. Confirm Jobs API latency, source freshness, PostgreSQL CPU, dead tuples, and
   archive retry counts remain healthy before increasing throughput.

```sql
SELECT c.id, c.canonical_key, c.updated_at_ms
FROM jobs_global_candidates c
WHERE c.availability_status = 'expired'
  AND c.archive_state = 'hot'
  AND c.updated_at_ms < (
      EXTRACT(EPOCH FROM NOW() - INTERVAL '30 days') * 1000
  )::BIGINT
  AND NOT EXISTS (
      SELECT 1
      FROM jobs_global_candidate_memberships m
      WHERE m.candidate_id = c.id
        AND m.expired_at_ms IS NULL
  )
  AND NOT EXISTS (
      SELECT 1
      FROM jobs_global_candidate_materializations m
      WHERE m.candidate_id = c.id
  )
ORDER BY c.updated_at_ms
LIMIT 100;
```

Monitor archive state and retries:

```sql
SELECT archive_state, COUNT(*) AS candidates,
       SUM(archive_attempt_count) AS attempts
FROM jobs_global_candidates
GROUP BY archive_state
ORDER BY archive_state;

SELECT id, archive_attempt_count, archive_next_attempt_at_ms
FROM jobs_global_candidates
WHERE archive_state = 'hot'
  AND archive_attempt_count > 0
ORDER BY archive_next_attempt_at_ms
LIMIT 100;
```

To stop archival, set `BLUEY_JOBS_GLOBAL_ARCHIVE_ENABLED=0` and restart only the
Jobs API candidate that has passed the normal release gates. Already archived
rows remain valid searchable tombstones. Do not restore an old binary against
the migrated authority schema; fix forward. If a hot body is required again,
normal source rediscovery rehydrates it.

## Jobs artifact storage boundary

Keep queryable authority in PostgreSQL:

- account and tenant ownership;
- job/application state and canonical IDs;
- Career Track and identity binding;
- claim provenance and verification state;
- searchable timestamps, hashes, receipt references, and metering authority;
- compact structured resume/application metadata needed for authorization.

Keep immutable or large artifacts in private R2 and reference them by key,
SHA-256, media type, and size:

- original imported resumes;
- rendered job-specific PDF/DOCX documents and cover letters;
- frozen job descriptions and answer bundles;
- screenshots, confirmation evidence, and final receipt documents;
- encrypted cold global-candidate bodies.

Do not move live application rows wholesale to R2 without a tested hydration,
authorization, export, deletion, and interview-prep retrieval path. A future
terminal-application archive may use the same verified read-back pattern after
those paths exist.

Historical customer artifacts may be used for private, same-account retrieval,
evaluation, resume improvement, and interview preparation. Do not pool customer
artifacts for cross-account model training without explicit opt-in,
de-identification, versioned dataset manifests, deletion propagation, and
reviewed Terms and Privacy disclosures.

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

Browser release packaging is target-explicit and native-host-only:

```sh
cd jobs
npm run package:darwin-arm64 --workspace @bluey/jobs-browser
npm run package:darwin-x64 --workspace @bluey/jobs-browser
npm run package:windows-x64 --workspace @bluey/jobs-browser
```

These are low-level target entry points. They require the already compiled
automation and Browser trees, target-matching Chromium, exact descriptor inputs,
and native signing environment; they are not a substitute for the protected
release workflow and must not be used alone to label an artifact releasable.

The generic `package` and `prepare:chromium` entry points intentionally fail
without one exact target. Linux and universal-macOS artifacts are not supported
by this release contract. Use the protected `Bluey Browser Release Authority
Gate` workflow for candidates: it checks out one exact clean default-branch
commit, materializes build-signing inputs outside the repository, packages once
on the matching native host, and preserves those same bytes through independent
threshold authorization and promotion. Do not run Electron Builder directly
for a releasable artifact and do not rebuild between candidate verification and
promotion.

Each candidate must prove the exact approved native signer. macOS validation
verifies signatures, notarization, and existing staples for the ZIP application,
DMG, and mounted DMG application, then byte-compares their canonical Resources
and bundle-metadata inventories. Windows validation checks
the signed/timestamped NSIS installer, extracts its exact `app-64.7z` payload,
checks the signed/timestamped application executable, and inventories those
extracted Resources. The gate also proves `app.asar`, the compiled automation
dependency, matching headed Chromium revision/architecture, embedded build
authority, excluded source/test/config files, immutable artifact hashes and
sizes, and all five native packages. Promotion validates a canonical external
canary-evidence record with the exact required stable check IDs; it does not run
those physical canaries itself.
The protected candidate environment must supply
`BLUEY_BROWSER_MACOS_SIGNER_IDENTITY` and
`BLUEY_BROWSER_WINDOWS_SIGNER_IDENTITY` alongside the corresponding native
signing/notarization credentials; an absent or non-exact identity fails before
candidate evidence can be emitted.

On macOS, post-signing `Info.plist` inspection proves the exact bundle ID,
product name, and custom-protocol declaration in both package forms. On Windows,
the source gate proves the locked trusted NSIS construction inputs and the
packaged runtime's exact protocol-registration call without executing candidate
application code on the signing runner. Actual Windows registry registration is
therefore a mandatory fresh, credential-free clean-install canary, together with
installer/AUMID identity and `bluey-jobs` protocol launch. Outer/inner
Authenticode signatures, timestamps, ASAR inventory, and the compiled
`setAsDefaultProtocolClient("bluey-jobs")` call are construction proof only.

The macOS ZIP is retained as a manifest-classified updater artifact only. No
installed-app feed, downloader, installer coordinator, or automatic rollback is
shipped by this batch, so it must not be presented as self-update readiness.

### Browser release workflow handoff

The `Bluey Browser Release Authority Gate` workflow has four explicit
operations. `contract` also runs for pull requests; manually dispatched
`candidate`, `authorize`, and `promote` operations must run from the repository
default branch. Every later operation must name the exact prior run, artifact,
source commit, release ID, and authority digests printed by the earlier run:

1. `contract` exercises the release scripts and fail-closed workflow contract.
2. `candidate` prepares one exact clean commit without credentials on each
   native target, packages it once inside the protected
   `bluey-browser-release-signing` environment, verifies native evidence, and
   assembles the three target parts into one five-artifact candidate set.
3. `authorize` downloads that exact candidate and joins it to an independently
   produced threshold manifest signature set. It neither rebuilds nor signs a
   native package.
4. `promote` downloads that exact authorization and joins it to the canonical
   activation, promotion signature set, and separately collected physical
   canary-evidence record. `require_production_ready=true` validates the record
   and its required check IDs; it is not evidence that this source checkout ran
   the devices.

GitHub artifacts retained by this workflow are handoff evidence, not an
immutable public download origin. Before registry activation, copy the exact
verified bytes to the manifest's immutable HTTPS URLs and perform a complete
public read-back with matching sizes and SHA-256 digests. Never rebuild or
rename an artifact between candidate, authorization, read-back, and promotion.

### Browser release authority inventory

The protected `bluey-browser-release-signing` native packaging environment must
configure the build and native-signing values below. An empty, wrong, or extra
credential must fail the candidate rather than fall back to an unsigned
package:

- Build identity: `BLUEY_BROWSER_BUILD_PRIVATE_KEY_PKCS8_BASE64`,
  `BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_BASE64`, and
  `BLUEY_BROWSER_BUILD_PUBLIC_KEYRING_SHA256`.
- macOS: `BLUEY_BROWSER_APPLE_API_KEY_BASE64`,
  `BLUEY_BROWSER_APPLE_API_KEY_ID`, `BLUEY_BROWSER_APPLE_API_ISSUER`,
  `BLUEY_BROWSER_MACOS_CSC_LINK`, `BLUEY_BROWSER_MACOS_CSC_KEY_PASSWORD`, and
  `BLUEY_BROWSER_MACOS_SIGNER_IDENTITY`.
- Windows: `BLUEY_BROWSER_WINDOWS_CSC_LINK`,
  `BLUEY_BROWSER_WINDOWS_CSC_KEY_PASSWORD`, and
  `BLUEY_BROWSER_WINDOWS_SIGNER_IDENTITY`.

Candidate assembly must also receive
`BLUEY_BROWSER_RELEASE_TRUST_POLICY_BASE64` and
`BLUEY_BROWSER_RELEASE_TRUST_POLICY_SHA256`. They are public authority material,
not private signing keys, and must identify the exact approved policy used to
assemble the candidate.

The workflow's manifest signature set, activation, activation signature set,
and canary evidence are canonical public authorities passed between offline
ceremony and exact stored-byte verification. Root, release, promotion, and
incident private keys must never be stored in the repository, candidate
artifacts, Browser package, API environment, or workflow outputs. The API host
receives only `BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID` and the independently
approved public `BLUEY_JOBS_BROWSER_ROOT_TRUST_ANCHOR_JSON`.

`BLUEY_JOBS_BROWSER_DEVELOPMENT_RELEASE_DIRECTORY` is deliberately absent from
the production environment example. It is accepted only by an unpackaged
Browser process whose Jobs API origin is credential-free loopback HTTP; a
packaged process ignores it. Never place it in a production service, package,
or customer environment.

### Browser release registry ceremony

The administrator-authenticated registry exposes eight routes. Import and
movement requests are strict JSON with unknown fields rejected:

| Route | Request authority | Purpose |
|-------|-------------------|---------|
| `POST /admin/jobs/browser-releases/trust-policies` | `{canonicalBase64url, signatureSetBase64url}` | Import or byte-replay a root-authorized trust policy. |
| `POST /admin/jobs/browser-releases/manifests` | `{canonicalBase64url, signatureSetBase64url, buildProofs}` | Import the threshold-authorized manifest and every exact packaged build proof. |
| `POST /admin/jobs/browser-releases/activations` | `{canonicalBase64url, signatureSetBase64url}` | Import a promotion-authorized channel activation without moving the head. |
| `POST /admin/jobs/browser-releases/activations/apply` | `{activationSha256, expectedHeadRevision, expectedTransitionSha256}` | Compare-and-swap the channel head to an imported activation. |
| `POST /admin/jobs/browser-releases/rollbacks` | `{canonicalBase64url, signatureSetBase64url}` | Apply an exact signed higher-sequence rollback transition. |
| `POST /admin/jobs/browser-releases/revocations` | `{canonicalBase64url, signatureSetBase64url}` | Append an irreversible incident-authorized revocation. |
| `POST /admin/jobs/browser-releases/accounts/:account_id/channel` | `{assignmentGeneration, predecessorAssignmentSha256, channel, reasonRef, assignedAtMs}` | Assign one account to one channel with a monotonic generation and predecessor digest. |
| `GET /admin/jobs/browser-releases/channels/:channel/status` | none | Read current CAS fields, authority digests, expiry, and availability. |

Use this order for a new channel head: configure the public root anchor and
server release ID; import the trust policy; import the manifest and exact build
proofs from the stored promotion set; import the activation; read channel
status; apply the activation with that exact `headRevision` and
`transitionSha256`; assign only intended accounts with the next exact assignment
generation; then read channel and account-facing availability again. A replay
must return the same digests with `replayed=true`; any conflicting replay,
sequence regression, stale compare-and-swap, incomplete artifact set, obsolete
origin, expiry, or revocation stops the ceremony.

Rollback and revocation are separate incident paths. A rollback requires a new
higher-sequence signed authority that binds the current activation and exact
target manifest. A revocation is append-only and must be followed by channel
status and account-facing availability checks. Keep
`BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0` throughout rehearsal; enabling
it is a distinct approved production change after immutable hosting, native
credentials, and every physical canary have independently passed.

The authenticated portal starts a local application by creating a bounded
capability in `jobs_local_run_tickets`, then opening
`bluey-jobs://run/<run-id>?ticket=<random-ticket>`. Only the ticket enters the
custom-protocol URL. Its secret and frozen packet are encrypted in the Jobs
database, the database stores a lookup hash separately, and terminal results
are idempotent. Claim sends the exact packaged canonical build descriptor,
detached signature, and request-bound nonce. The server consumes the ticket
only after the current account assignment, signed channel head, server-runtime
compatibility, full artifact set, and revocation state pass in one transaction,
then freezes those bindings into operation-scoped capabilities. Pre-click
authorization rechecks the current assignment, activation compatibility, and
revocation state without weakening result/resume or trusted-receipt recovery.
The desktop claims and reports the run through `BLUEY_JOBS_API_ORIGIN`;
production must keep that origin on HTTPS.

## ATS certification authority

Run the `bluey-ops` preflight before importing or changing any ATS
certification authority. Discovery coverage, review-fill coverage, ATS
certification, Career Track Auto authorization, and Browser distribution are
independent gates. A provider URL, source catalog entry, adapter result,
synthetic fixture, portal label, or successful reviewed submission never grants
`certified` by itself.

The safe default is zero active certification. A fresh migration must create no
active head, seed no tenant, and grant no unattended capability. Greenhouse and
Lever remain `beta_review`; Workday, Ashby, and SmartRecruiters remain
review-fill only; semantic and protected portals remain Review, Takeover, or
Handoff. An absent root anchor, incomplete authority, expired record, unknown
layout, open circuit, stale compare-and-swap, or runtime mismatch preserves that
default.

Keep all four independent production gates at `0` throughout local source work,
schema rehearsal, signed-authority import rehearsal, and shadow observation:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_MAILBOX_SYNC_ENABLED=0
```

An imported or applied ATS activation cannot override either Browser
distribution flag. Conversely, an available Browser cannot manufacture an ATS
certification.

### Certification preflight

Before any authority import or movement:

1. Resolve the exact reviewed source commit and confirm the worktree is clean.
2. Verify the paired SQLite/PostgreSQL ATS-certification migration and schema
   parity from that commit. An older Jobs binary must not run against a newer
   authority schema.
3. Confirm a new database contains zero certification heads, zero canary
   reservations, and zero application certification bindings.
4. Resolve the exact provider, tenant, variant, surface digest, adapter version
   and bundle digest, layout-contract digest, suite version, and runner target.
   The target comes from canonical job and original-source authority, never an
   operator-supplied arbitrary URL.
5. For a local target, resolve the exact active Browser release manifest,
   platform/architecture artifact, build descriptor, Playwright version, and
   Chromium revision. For cloud, resolve the immutable runner image digest,
   runner build ID, automation bundle, Playwright version, and Chromium
   revision.
6. Verify every referenced evidence object by complete immutable read-back,
   exact byte size, SHA-256, media type, provenance, authorization reference,
   sanitizer version, capture time, and expiry.
7. Confirm signed layout observations contain no candidate values, labels with
   candidate data, page text, HTML, screenshots, documents, cookies, tokens,
   OTPs, credentials, full query strings, or selectors containing user data.
8. Confirm manifest, evidence, activation, revocation, and layout-observation
   signing roles use disjoint approved public keys and required independent
   thresholds.
9. Confirm the approved canary account set, eligible plan, daily submission
   cap, concurrency cap, named support owner, incident owner, and rollback
   authority are recorded outside the repository.
10. Read current target status, quarantine/circuit status, head revision,
    transition digest, expiry, and revocation state before attempting a change.

The API host receives only the independently approved public ATS-certification
root anchor, for example through
`BLUEY_JOBS_ATS_CERTIFICATION_ROOT_TRUST_ANCHOR_JSON`. Manifest, evidence,
activation, revocation, and layout-observation private keys must not enter the
repository, Jobs API environment, Browser package, runner image, database, CI
artifact, or workflow output. If the deployed implementation does not recognize
and validate the exact root-anchor setting, stop; an environment variable alone
is not authority.

### Shadow observations and zero-authority rehearsal

`LayoutObservationV1` is a signed, PII-free structural record. It binds one
provider-target fingerprint, adapter version, exact runner target, page variant,
control shapes, effective submit target, unique submit-control identity,
challenge categories, observation time, expiry, and predecessor digest.

Evidence-object source kind is classified explicitly:

- `synthetic` exercises fixtures, parity vectors, validators, drift handling,
  and failure paths;
- `fault_injection` proves crash, replay, ambiguity, and circuit behavior in an
  authorized non-production environment;
- `authorized_sandbox` proves the exact approved sandbox target; and
- `authorized_canary` proves a separately authorized live test vacancy.

The separately signed layout-observation class is exactly `synthetic`,
`authorized_sandbox`, or `authorized_live`. A non-shadow activation requires a
complete authorized-sandbox and authorized-live check/result and layout pair for
every exact runtime target. Fault-injection objects do not become a production
layout class.

Synthetic and fault-injection evidence are shadow-only. They may support an
`observe_only` shadow manifest and activation, but they cannot authorize a
canary or general unattended head. A source test that accepts a synthetic
manifest proves only that the authority validator works.

Import observations before the manifest that names them. A byte-identical
replay must return the same digest. A conflicting replay, PII-bearing record,
unapproved evidence class, wrong target, wrong runner, expired observation, or
predecessor conflict stops the ceremony and creates no authority.

### Signed import and activation ceremony

Administrative import and movement requests use strict JSON with unknown
fields rejected. Use the exact routes implemented by the reviewed Jobs API;
the intended registry shape is:

| Route | Request authority | Purpose |
|-------|-------------------|---------|
| `POST /admin/jobs/ats-certifications/trust-policies` | canonical root-authorized trust policy and signature set | Import or byte-replay public signing policy. |
| `POST /admin/jobs/ats-certifications/layout-observations` | canonical `LayoutObservationV1` and independent signature set | Import one PII-free exact structural observation. |
| `POST /admin/jobs/ats-certifications/manifests` | aggregate of a canonical independently signed `ManifestV1` plus independently signed immutable evidence envelopes | Atomically import one exact provider/target/adapter/layout/runner candidate and its evidence metadata. |
| `POST /admin/jobs/ats-certifications/activations` | canonical `ActivationV1` and independent promotion signature set | Import a shadow, canary, or general activation without moving a head. |
| `POST /admin/jobs/ats-certifications/activations/apply` | activation digest, expected head revision, expected transition digest | Compare-and-swap one exact scope/channel head. |
| `POST /admin/jobs/ats-certifications/canary-allowlists` | exact bounded account set, validity window, and approval reference | Import or byte-replay the server-owned allowlist referenced by a canary activation. |
| `POST /admin/jobs/ats-certifications/canary-allowlists/revoke` | exact allowlist digest and revocation reference | Irreversibly stop new authority from that canary account set. |
| `POST /admin/jobs/ats-certifications/circuits` | exact scope, transition, trigger, and authority reference | Open, hold, or reviewed-close a circuit. |
| `POST /admin/jobs/ats-certifications/revocations` | canonical `RevocationV1` and independent incident signature set | Append an irreversible revocation. |
| `GET /admin/jobs/ats-certifications/targets/:target_key/status` | none beyond administrator authentication | Read current head, manifest, expiry, revocation, quarantine, circuit, and rollout state. |

If the reviewed release exposes different route names or request fields, update
this runbook and its contract tests before operating it. Never infer a route or
send signed authority to an unreviewed endpoint.

Use this order:

1. Configure and verify the independently approved public root anchor.
2. Import or byte-replay the current trust policy; verify its digest, sequence,
   predecessor, time window, disjoint key roles, and thresholds.
3. Import every signed PII-free layout observation and verify its exact
   provider-target, adapter, runner, evidence class, sanitizer, and expiry.
4. Import the immutable evidence metadata only after full object read-back
   matches the recorded bytes, size, and SHA-256.
5. Import `ManifestV1`. Verify the exact tenant, variant, surface, scope,
   adapter version and bundle, layout set, suite and stable check IDs, source
   commit, maximum capability, evidence set, and runtime targets.
6. Before importing a canary activation, import and read back its exact bounded
   allowlist. Verify the digest, members, validity window, approval reference,
   and absence of revocation; a client-supplied account list is not authority.
7. Import `ActivationV1` with a separate promotion signature. Verify the
   manifest digest, scope, channel, capability ceiling, sequence, time window,
   account allowlist digest, activation-wide total/distinct-account/concurrency
   caps, and server-owned UTC daily side-effect cap. For a canary, the
   `canaryEvidenceManifestSha256` field is not a second artifact: it must equal
   the exact canonical `ManifestV1` digest that already binds the independently
   signed evidence/layout objects and complete suite results.
8. Read target status. Do not rely on the import response as current-head
   evidence.
9. Apply the activation using that exact `headRevision` and
   `transitionSha256`. A stale revision, sequence regression, wrong predecessor,
   expired object, open circuit, quarantine, revocation, incomplete evidence,
   synthetic evidence in a non-shadow channel, or runtime mismatch stops the
   compare-and-swap.
10. Read status again and verify the expected manifest, activation, channel,
   capability, validity, runner targets, transition, and head revision.
11. Re-read customer-facing availability. With both Browser distribution flags
    still `0`, it must remain unavailable even when a signed certification head
    is valid.

Use `shadow` first. Move to `canary` only with authorized sandbox/live evidence,
an exact account allowlist digest, a positive submission cap, a named support
owner, and separately approved runner distribution. `general` requires the full
external matrix, independent production canary evidence, reviewed launch
thresholds, and an explicit production approval. No source-complete Round 604
checkout may perform that movement by assumption.

### Two-phase irreversible-submit boundary

Certification is checked twice and bound once. It is not a reusable bearer
permission.

Phase A is server-side preflight and single-use binding. Before a certified Auto
run becomes employer-facing, the server atomically rechecks the exact job and
original-source evidence, eligibility and hard filters, confirmed claims,
current Track Auto authorization, verified identity, source resume, approved
packet checksum, attempt and allowance, runner distribution, release/image,
current activation head, manifest, target, adapter, layout set, expiry,
revocations, quarantine, circuit, and rollout scope. It freezes those digests
with the application, run, attempt, browser session/profile, random nonce hash,
expiry, and fence. One attempt may hold only one live binding.

Phase B begins immediately before the durable irreversible marker. The runner
presents only its operation-scoped single-use capability, exact provider proof,
and current PII-free layout digest. One transaction locks and rechecks the
application, attempt, local ticket or cloud lease, binding, activation head,
revocations, circuit, target, adapter, layout, runner, Track authority, packet,
documents, discovery evidence, and eligibility. It then reserves exact canary
capacity, consumes the binding, and advances the fence once.

Only a successful Phase B response permits the runner to write its durable
marker and activate the one provider-scoped submit control. A bounded 4xx
authorization denial writes no marker or click. HTTP 5xx, transport loss,
timeout, malformed success, or any response that may conceal a committed Phase
B transaction is terminal `side_effect_unknown` and is never retried.

Review-first and `beta_review` paths keep explicit packet and provider-final
approval. An active signed provider-target certification does not force a
Review Career Track into Auto. Only a current `track_auto_submit` admission plus
an exact active certification and distributed runner may omit per-application
final approval. Any changed answer, document, identity, resume, Track policy,
packet, layout, or build invalidates the preflight binding before the marker.

### Quarantine, circuits, and revocation

An unexpected layout, target mismatch, evidence failure, confirmation
ambiguity, false-state risk, repeated side-effect uncertainty, or configured
error threshold opens the narrowest safe circuit and blocks new preflight and
Phase B authority. Record every quarantine/circuit command and transition
append-only. A successful later run does not auto-close a circuit or delete a
quarantine record.

Incident order:

1. Open or hold the exact provider, tenant, surface, adapter, activation, or
   runtime circuit.
2. Verify new preflights and Phase B requests fail before the marker.
3. Preserve active browser, result, receipt, evidence, binding, canary
   reservation, and reconciliation records.
4. Read the affected head and exact frozen bindings before importing incident
   authority.
5. Import and verify the independently signed append-only `RevocationV1`.
6. Read target and customer-facing status again; new unattended authority must
   be unavailable.
7. Reconcile every run already at or beyond the irreversible marker through its
   frozen recovery authority.

Revocation, expiry, head replacement, or circuit opening after the marker must
not deny the exact trusted result or receipt. It blocks new side effects while
allowing only:

- replay of the exact bound trusted result and immutable evidence;
- completion of a valid provider-confirmed submitted receipt;
- recovery-only result/resume capabilities that cannot reach Submit; or
- the existing owner-confirmed-not-submitted reconciliation path.

Never revoke by deleting rows, rewriting canonical bytes, moving a head
backward, restoring an older database, or rejecting recovery until an operator
is tempted to retry. Submitted state remains irreversible. Provider, target,
adapter, and runtime circuits require reviewed release. Only an exact predecessor
activation circuit may close through an applied newer activation whose complete
trust, manifest, runtime, revocation, quarantine, and canary authority remains
current after the database authority lock is acquired. A backdated event cannot
restore stale authority. Changed layout, adapter, Browser release, cloud image,
Chromium build, or suite requires new signed evidence and a new manifest.

### Monitoring and canary stop conditions

Monitor and alert by provider, tenant/surface, adapter version and bundle,
manifest, activation, channel, and runner target:

- certification preflight allows and typed denials;
- layout-observation age, mismatch, and quarantine rate;
- form fill/read-back and exact document-upload failures;
- challenge and intervention rates by type;
- Phase B authorization, canary reservation, and fence conflicts;
- submit activations, explicit confirmations, and negative-result vetoes;
- `side_effect_unknown` count, age, and reconciliation outcome;
- complete receipt/evidence acceptance and rejection;
- duplicate-prevention conflicts and workflow replay;
- activation-wide canary total, distinct-account, live-concurrency, and UTC-day
  side-effect reservation use;
- circuit state and transition age;
- authority, observation, manifest, and activation expiry; and
- current revocations and affected in-flight recovery bindings.

Stop the canary and open the circuit on any hard-filter violation, unsupported
candidate claim, duplicate submit activation, false Submitted state,
incomplete/mismatched receipt, PII-bearing observation or telemetry, unexpected
layout, unexplained evidence loss, or recovery path that attempts another
click. Do not average zero-tolerance failures into a success rate.

Before expanding an approved canary, confirm the policy-defined success,
intervention, recovery, and latency thresholds; zero unresolved
`side_effect_unknown` outside the bounded reconciliation window; complete
typed receipts; healthy source and runner state; allowance correctness; and
available support capacity. Expansion always uses a separately reviewed signed
activation and compare-and-swap transition.

### External certification gates and launch truth

Source-complete evidence may prove the registry, canonical validation, target
matching, shadow observations, lifecycle, two-phase fence, quarantine, circuit,
and recovery behavior. It cannot prove a real provider tenant or production
runner.

Before any Greenhouse or Lever target is activated beyond shadow, require:

- independently managed signing roles and approved public root anchor;
- two or three owner-authorized sandbox/live test vacancies for that exact
  provider target and layout variants;
- every required stable suite check, including custom/dynamic fields, exact PDF
  upload/read-back, sensitive/legal handling, CAPTCHA/2FA/assessment pause,
  unique submit control, positive and negative confirmation, crash-after-click,
  duplicate delivery, drift, privacy, and complete receipt evidence;
- an exact local signed Browser release with immutable public read-back and
  physical platform canaries, or an exact cloud image/Chromium build with
  authenticated Temporal, PostgreSQL, R2/S3, network, takeover, crash, and
  capacity canaries;
- approved canary accounts and plan, activation-wide distinct-account and total
  caps, UTC daily side-effect cap, live-concurrency cap, support owner, incident
  owner, and rollback authority;
- provider/data-rights approval where required; and
- an explicit production change authorizing the selected Browser distribution
  flag.

Workday, Ashby, and SmartRecruiters cannot receive unattended activation until
their own provider-specific state machines and the same complete external
matrix pass. Semantic, unknown, and protected portals cannot inherit another
provider's certification.

Do not write or approve launch claims that say an ATS is certified, unattended
submission is available, a canary passed, or a runner launched from migrations,
fixtures, synthetic/fault evidence, signed-object parser tests, imported shadow
authority, or a source-complete review. Until the Round 604 verification matrix
is fully green, say only:

> Signed ATS certification authority implementation is in progress; every
> provider remains Review first.

After every source-completable Round 604 gate is proven, the strongest honest
source-only wording is:

> Signed ATS certification authority is source-ready; every provider remains
> Review first pending authorized tenant and runner evidence.

## Reviewed communication release authority

Recruiter replies and interview-calendar actions use an authority boundary that
is separate from read-only mailbox synchronization. A connected inbox is not
send authority. The user must inspect the exact immutable draft, and the server
must verify an explicit provider write grant before approval can become
dispatchable.

The provider worker runs in-process rather than behind a credential-bearing HTTP
lease, so OAuth access and refresh tokens never leave the owning server process
or appear in portal responses. Both `bluey-jobs-api` and `bluey-server` embed the
worker. The production Jobs routes run under `bluey-jobs-api.service` on port
8081, which loads `/etc/bluey-api/bluey-jobs.env`; `bluey-api.service` also loads
that file when `bluey-api-jobs-env.conf` is installed. Before a provider write,
the worker revalidates the account, application, mailbox connection, exact
stored reply target and source message where required, payload digest, approval
revision, granted scope revision, release flag, and fenced attempt. It commits
an append-only request-start marker before network I/O. Calendar creates use an
exact IANA time zone and request attendee invitations from the provider.

Keep these gates independent:

```text
BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED=0
BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED=0
BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED=0
```

The dispatch and reconciliation loops are selected at process startup. The
OAuth write gate is checked on each upgrade and callback request, but a systemd
environment-file change still does not reach an already running process. After
an independently approved flag change, restart `bluey-jobs-api.service` and
every other running worker-capable service that loads `bluey-jobs.env`, including
`bluey-api.service` when its Jobs environment drop-in is installed. Enabling one
gate never enables either of the other two.

Do not enable write-scope OAuth until read-only Gmail/Outlook canaries, approved
Google/Microsoft applications, exact redirect URIs, consent-screen review, token
rotation/revocation, retention, deletion, and monitoring are complete. Required
write grants are Gmail send, Google Calendar events, Microsoft Mail.Send, and
Microsoft Calendars.ReadWrite; capabilities must be derived from the grants the
provider actually returns.

Do not enable dispatch until fixture and live authorized sandbox matrices prove
recipient/thread binding, exact provider identity, duplicate prevention,
disconnect and account-deletion fencing, token expiry/rotation, outage handling,
and receipt/evidence completeness for all four transports.

Before enabling any communication gate, exercise authenticated list, detail,
approval, cancellation, and account-export requests with an authorized canary.
Verify every private communication JSON response sends `Cache-Control: private,
no-store` and `Pragma: no-cache`, the portal requests use client-side
`cache: "no-store"`, and no sensitive response is retained in browser or service
worker caches. The owning customer export intentionally contains the immutable
reviewed payload; confirm it omits OAuth tokens and grants, provider
object/thread/conversation IDs, request and idempotency markers, private evidence
and reconciliation fingerprints, lease or worker identity, and raw provider
errors. Inspect structured server logs separately and confirm they omit all of
those fields plus payload text.

Provider timeout, connection loss, 5xx, or malformed success after a possible
write is `side_effect_unknown`. Never retry it. Reconciliation is a separately
gated provider lookup that may prove the exact sent message/event, leave the
result unknown, or prove bounded absence and return the immutable draft to fresh
user review. A worker cycle claims at most 25 eligible actions, and each action
has a separate ceiling of 20 reconciliation claims. Exact found or inconclusive
lookup may run immediately; an authoritative absence counts only after at least
15 minutes from dispatch, and three counted absences return the action to
`needs_input`. Persisted unknown observations back off for at least five minutes.
An inconclusive or conflicting result remains unknown and requires manual
operator investigation; it never becomes retry authority. Disabling dispatch
must not convert unknown outcomes into retries.

If account deletion or mailbox disconnect enters communication drain:

1. Treat the persisted drain as a write fence. Do not clear it, reconnect under
   another identifier, delete the source message, or issue a replacement action.
2. Allow only exact completion of the already-started attempt and, when its
   separate flag and provider-read gate are approved, read-only reconciliation.
3. Preserve the action, attempt, request-start, provider evidence, and encrypted
   credential boundary while the result remains unknown. Never copy operation
   keys, provider objects, raw errors, grants, or tokens into tickets or logs.
4. If exact provider evidence proves success, retain the terminal customer
   result; if three bounded authoritative absences prove no side effect, cancel
   the resulting reviewable draft as part of the pending lifecycle rather than
   approving it again.
5. If evidence conflicts or stays inconclusive, keep the drain and escalate to
   the named privacy/incident owner. Retry deletion or disconnect only after the
   unresolved irreversible authority is terminally settled.

Until the complete Round 605 local matrix and independent review pass, say only:

> Reviewed communication execution authority implementation is in progress;
> connected inboxes remain read-only and no provider delivery is enabled.

After every source-completable Round 605 gate passes and independent review
accepts the source, but before authorized provider sandboxes and production
canaries pass, the strongest honest wording is:

> Reviewed communication execution is source-ready; connected inboxes remain
> read-only and no reply or calendar action is enabled for provider delivery.

## External release gates

- Apple Developer ID/notarization and Windows Authenticode/timestamp credentials,
  exact approved signer identities, and immutable artifact-host access.
- Physical macOS arm64/x64 and Windows x64 clean-install, protocol-claim,
  upgrade, rollback, immutable-readback, and portal-download canary evidence.
- Gmail and Outlook OAuth applications, redirect URIs, webhook subscriptions,
  and encrypted refresh-token storage.
- Licensed discovery-provider contracts and API credentials.
- Browser takeover streaming and short-lived authorization URLs.
- R2/S3 upload credentials for final PDFs, screenshots, and receipt bundles.
- Live sandbox certification for representative tenants of every supported ATS.
- Regional Temporal, Postgres, Valkey, OpenSearch, and browser-pool monitoring.

No source change can manufacture provider approvals, signing certificates, or
production credentials. Keep the Jobs beta flag off until these gates pass.
