# Round 576: Jobs Discovery Freshness And Durable Workers

Date: July 29, 2026

Status: implementation verified; production deployment pending

## User-Visible Failure

The Matches page contained 1,140 relevant jobs, but source rows showed their
last successful sync five or six days earlier. Refreshing the page did not
produce newer jobs.

The Refresh action was not the writer. It only reloaded current account state
from PostgreSQL. The discovery workers that publish new snapshots were stopped,
so the database correctly returned old rows on every refresh.

## Production Root Cause

The direct worker was inactive after Jobs API maintenance. The global worker
also referenced a deleted temporary build directory. Existing service units
were coupled to the Jobs API lifecycle and did not guarantee a restart after a
clean stop.

The portal compounded the problem by rendering a stored `healthy` state without
considering `last_success_at_ms`. Historical success looked like current health.

## Durable Runtime Contract

Round 576 creates one relocatable runtime for both discovery workers:

```text
jobs-workers-<commit>.tar.gz
jobs-workers-<commit>.tar.gz.sha256
```

The production installer:

1. verifies the exact SHA-256 sidecar;
2. rejects unsafe archive paths;
3. extracts one immutable release under
   `/opt/bluey-jobs-workers/releases/<release-id>`;
4. atomically points direct and global worker links at the same release;
5. installs and enables independent systemd services;
6. verifies both workers are active;
7. restores both old links if activation fails;
8. retains three releases for exact rollback.

Production units never point at disposable build directories.

## Freshness Contract

The worker health timer runs every 15 minutes and fails when:

- either immutable release link is missing;
- either worker service is inactive;
- an active direct or global source has no successful sync in 12 hours.

Twelve hours is twice the slowest normal six-hour global cadence. The portal
uses the same threshold. A historically healthy but overdue source now renders
as `Degraded`, and the track summary says `Updates delayed`.

## Submission Safety

This round changes discovery availability only.

- Managed and curated feeds provide candidate leads.
- Every lead still requires current original-employer availability and ATS
  capability checks.
- Unknown, protected, and uncertified portals do not gain automatic submission
  authority.
- Jobs model generation and both Browser-distribution flags remain disabled.
- No native overlay, audio, STT, or meeting-runtime file changes.

## Verification

Verified on the exact staged tree:

- strict TypeScript and production portal build;
- 466 Jobs tests across automation, Browser, runner, workflows, and portal;
- 943 Rust workspace library tests passed, 0 failed, 5 ignored;
- strict Rust formatting and all-target/all-feature Clippy;
- worker unit policy, health failure, immutable install, idempotency, retention,
  and activation rollback shell suites;
- privacy, schema parity, provenance/license, CI guard, client/server boundary,
  and SQLite boundary gates;
- `git diff --check`;
- stale-source layouts in light and dark themes on desktop and 390-by-844
  mobile viewports.

Exact production artifact hashes, source catch-up, live edge checks, disabled
execution flags, and rollback proof remain deployment acceptance criteria and
must be appended after deployment.

## Deployment Sequence

1. Merge reviewed source through a feature branch and pull request.
2. Build the exact merged worker archive and portal bundle.
3. Create and verify a fresh PostgreSQL backup.
4. Install the worker archive using its SHA-256 sidecar.
5. Deploy only the portal files changed by this round.
6. Confirm both workers and the health timer are active and enabled.
7. Wait for direct and global source catch-up.
8. Confirm source timestamps advance and stale rows no longer appear healthy.
9. Verify public Jobs, protected APIs, internal-route hiding, and disabled
   capability flags.

## Rollback

Switch both worker links to the same prior retained release, restart both
workers, and rerun the discovery checker. Restore the preserved portal archive
for a portal-only regression. Do not roll back candidate data solely because a
source is delayed.

## Live Evidence

Pending deployment.
