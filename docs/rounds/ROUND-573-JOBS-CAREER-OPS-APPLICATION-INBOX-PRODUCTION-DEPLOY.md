# Round 573: Jobs Career Ops And Application Inbox Production Deploy

Date: July 24, 2026

Status: deployed to main and production

## Scope

This round deployed the integrated Bluey Jobs profile-truth, Career Track,
application-inbox, and selective Career Ops source-intelligence work from:

```text
b4c63b5bc18f645dc0d5427954af87ced894caf6
```

The deployment changed only the standalone Jobs API and the Jobs portal. It did
not replace or restart the main Bluey API, Caddy, or the signed Bluey 0.1.104
native release.

## Career Ops Audit

Bluey reviewed `santifer/career-ops` at:

```text
01bf8b469ad5177a9c30230bc00509ead8e006c2
```

The upstream project is MIT licensed. The disposable audit clone was kept
outside the repository and removed after review. Bluey did not ship a copy of
the upstream repository.

The release selectively adapted:

- explicit source-trust reason codes;
- deterministic description fingerprints for possible cross-listings;
- reusable application communication and follow-up concepts;
- compact application-inbox presentation patterns.

Bluey's existing server-owned eligibility, tenant isolation, source
revalidation, immutable packet, receipt, evidence, and metering authority
remain unchanged. Browser impersonation, WAF workarounds, generic provider
loading, and local Markdown/YAML production state were not ported.

Exact provenance is recorded in:

- `jobs/THIRD_PARTY_NOTICES.md`;
- `jobs/THIRD_PARTY_PROVENANCE.md`;
- `docs/rounds/ROUND-572-JOBS-CAREER-OPS-DEEP-AUDIT-AND-SELECTIVE-PORT.md`.

## Product Changes

### Resume and Career Track truth

- Resume import retains user-editable corrections for incorrectly split
  employer, title, and location values.
- Career Tracks use normalized role and experience policies rather than
  treating a parsed employer/title string as a target role.
- Search policy summaries describe server-owned defaults instead of exposing
  internal tuning controls as if they were user guarantees.

### Read-only application inbox

- Gmail and Outlook connections use PKCE.
- Provider credentials and imported message records are encrypted at rest.
- Sync uses durable cursors and leases.
- Requested scopes are read-only; Bluey has no send-mail API or send scope in
  this release.
- Messages can be correlated with known applications and presented for user
  review.
- Bluey does not infer or commit a final employer outcome without user review.
- Calendar connection remains visibly unavailable rather than being presented
  as an implemented feature.

### Source intelligence

- Invalid, insecure, shortened, redirected, and suspiciously mismatched
  application URLs produce typed advisory signals.
- Known ATS hosts avoid false company-domain mismatch warnings.
- Description fingerprints identify possible agency cross-listings without
  silently merging jobs or changing application authority.
- Every lead still requires original-source revalidation before
  employer-facing action.

## Verification Before Deployment

The integrated Jobs matrix passed:

```text
automation: 188
browser:    100
runner:      50
workflows:   51
portal:      75
total:      464
```

Additional gates passed:

- all Jobs TypeScript checks;
- Jobs portal production build;
- generated-asset scan with no source maps, environment files, backups, or
  secret-like files;
- `cargo fmt --all -- --check`;
- all Rust library tests;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- SQLite/Postgres schema parity;
- privacy, provenance/license, CI guard, and client/server boundary checks;
- desktop and mobile onboarding and Settings visual checks;
- route-refresh preservation and horizontal-overflow checks.

The candidate Jobs API was started in isolation and its standalone `/health`
response was verified before the production service swap.

## Backup Evidence

The pre-deployment PostgreSQL backup is:

```text
/var/backups/bluey-api/hourly/bluey-postgres-20260724T090505Z.pgdump
size:   626526799 bytes
sha256: e428805a57971eae16418d441946269ab6327941032e7de4e01884690c9580ef
```

The checksum passed and `pg_restore -l` listed 452 archive entries.

R2 replication was attempted but failed with the existing `AccessDenied`
credential problem. The verified local production-host backup is preserved;
this release does not claim a successful off-host R2 copy.

Rollback artifacts:

```text
/var/backups/bluey-api/bin/bluey-jobs-api.before-b4c63b5b-20260724T092945Z
124da4c7b7a2ca47fd4952aa3fc7471bb9d61f40034b50c329cc25bbc2f605a5

/var/backups/bluey-api/env/bluey-jobs.env.before-b4c63b5b-20260724T092945Z
2db8730603026b88d6576983ef2ef4d913b260760189e417b80b942dadad4741

/var/www/bluey/backups/jobs-before-b4c63b5b-20260724T092945Z.tar.gz
9f87a44f8dfb08c54a3843f84a990a67ecfe4a04ae1c16f484c79f3558c4034f
```

## Deployed Artifacts

The production source archive and portal archive used for the deployment were:

```text
source archive:
87793ca3929a5aa12354e42e8054da6604d254394fdc4ed6203259ef886bd4f7

portal archive:
664dca76897f261fbd0521a116658501cfed6bf98f187fef3d33aa0d4930203a
```

The final deployed artifacts are:

```text
/usr/local/bin/bluey-jobs-api
4193b82c02f0dde2f2a01f83b76a2b3ca31530aad395fc21191c05fd781c5cb8

/var/www/bluey/jobs/index.html
1fa55eb584c2c1229125e0d14ac81fa1d57788b1bacd0fa638c3463de67f4381
```

The portal contains 27 files and no `.map` files.

Standalone Jobs health reports the exact deployed source:

```text
GET http://127.0.0.1:8081/health
200
{"status":"ok","version":"0.1.5","commit":"b4c63b5bc18f645dc0d5427954af87ced894caf6","platform":"linux-x86_64",...}
```

## Protected Capability State

Unfinished or undistributed capabilities remained disabled:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_MAILBOX_SYNC_ENABLED=0
```

Both discovery workers are inactive:

```text
bluey-jobs-discovery.service=inactive
bluey-jobs-global-discovery.service=inactive
```

This means the deployed UI and API do not falsely claim that model resume
generation, local/cloud Browser execution, continuous discovery, or background
mailbox sync are active.

## Production Service State

```text
bluey-jobs-api.service MainPID=2510519 NRestarts=0 active/running
bluey-api.service      MainPID=2257183 NRestarts=0 active/running
caddy.service          MainPID=2217438 NRestarts=0 active/running
```

No Jobs warning, error, panic, or failed-event log was emitted after the
deployment. The root filesystem remained at 79% usage with approximately 13 GB
free.

## Live Verification

Final edge checks:

```text
GET /jobs/                                      200  ttfb=0.177s
GET /api/jobs/workspace                        401  ttfb=0.073s
GET /api/jobs/internal/discovery/lease         404  ttfb=0.064s
GET /jobs/assets/not-a-real-source.map         404  ttfb=0.072s
GET /api/jobs/mailbox-oauth/config             401  ttfb=0.084s
GET /assets/favicon-512.png                    200
GET /jobs/assets/index-h3ha1Q1W.js             200
GET /jobs/assets/index-B5MkWviE.css            200
GET /jobs/ with GPTBot user agent              403
```

The signed-in Settings view showed the truthful Application Inbox state:

- Connect Gmail or Outlook;
- zero of one inboxes connected;
- read-only and never sends email;
- no connected application inbox;
- employer activity checking;
- calendar unavailable.

A later Chrome-control bridge click stalled, but the application API did not:
the signed-in mailbox configuration request completed with `200` in 28 ms,
anonymous protected requests returned immediately, and the Jobs service emitted
no timeout, pool, warning, or error event.

## Cleanup

Temporary source archives, portal archives, candidate binaries, and the
temporary previous-portal directory were removed after final verification.
The database and explicit rollback artifacts above were preserved.

## Rollback

For a portal-only regression, restore the preserved Jobs portal archive.

For a Jobs API regression, restore the paired previous binary and environment
snapshot, then verify standalone health, protected routes, capability flags,
service restart counts, and portal loading.

Prefer a fix-forward deployment unless the Jobs API is unavailable or a
protected-route, tenant-isolation, or evidence-authority invariant regresses.
