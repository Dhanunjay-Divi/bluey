# Round 526 - Jobs onboarding manual production deploy

Date: 2026-07-16

Status: deployed and verified

## Outcome

The Round 525 Bluey Jobs onboarding fix is live at
[https://bluey.sh/jobs/](https://bluey.sh/jobs/).

Production now runs:

| Artifact | Revision | SHA-256 |
| --- | --- | --- |
| Jobs API | `e319ef5f8b64d1645ed576a20fff93684981bb71` | `0b7cf23c978c69a3141660733de0e6f3e36b48dd695b2dba310eaf089f08dab4` |
| Jobs portal `index.html` | `e319ef5f8b64d1645ed576a20fff93684981bb71` | `6c6e359498865aef671ee44108d4aecfc98bec28ab72bb109795a9bcd327f011` |
| Portal entry JavaScript | `assets/index-CxZvRSyx.js` | `d84d7e22730aef81971b9feeda47586ad9e3f0b9a0ebdcd7253999823c2088df` |
| Portal CSS | `assets/index-DcOy2kNp.css` | `b16af8342507bf0594dfb1af89c6f7a04e92510c72f09cdee610d375fa76e887` |

This was an owner-requested manual production deployment. GitHub Actions was
not invoked.

The deployment changed only the standalone Jobs API and `/jobs/` static
portal. It did not replace the main Bluey API, Caddy, or signed macOS/Windows
release artifacts.

## Source And Build

The exact source archive was created from clean mainline commit `e319ef5f`:

`/tmp/bluey-jobs-e319ef5f.tar.gz`

Archive SHA-256:

`ef0a40bbb4c7504f9def723c09a3510999d97c98f3b951253f93975884647b22`

Production extracted it to:

`/opt/bluey-build-jobs-e319ef5f`

The Jobs API was compiled there in release mode with
`BLUEY_GIT_COMMIT=e319ef5f8b64d1645ed576a20fff93684981bb71`. The portal was
built locally from the same clean source revision, copied to an off-path
production staging directory, and hash-verified before publication.

## Verification Before Deployment

Round 525's complete verification remains the implementation gate:

- Jobs package tests: 227 passed.
- Server unit tests: 402 passed.
- Server HTTP integration tests: 71 passed.
- Jobs portal typecheck and production build: passed.
- Rust formatting and `git diff --check`: passed.
- Generated Jobs output contains no source maps.

The deploy pass repeated the portal's 17 tests, typecheck, production build,
the Career Track limit-bypass test, the onboarding idempotency test, Rust
formatting, and release hygiene checks.

An isolated second Jobs process could not open another 16-connection pool
alongside the live API under the managed database connection cap. No live
service was affected. The production swap therefore used a health-gated
single-service restart with an automatic restore of the previous binary if the
new process failed to report the exact expected commit.

## Backups And Rollback

Fresh PostgreSQL backup:

`/var/backups/bluey-api/hourly/bluey-postgres-20260716T213527Z.pgdump`

- Size: `24,037,357` bytes.
- SHA-256:
  `4116bbc52c830e3dd5fb3b652d3076cfcfcae27ccab7572c9cc57e4ee164d28d`.
- Checksum verification passed.
- `pg_restore -l` passed with 357 entries.

Previous Jobs API:

`/var/backups/bluey-api/bin/bluey-jobs-api.before-e319ef5f-20260716T213527Z`

- SHA-256:
  `7201dd4f8b9b674c946ab5c301d5a4a15efbfaadc8bd8e3e4644b5cab2b86b84`.

Previous portal:

`/var/www/bluey/backups/jobs-before-e319ef5f-20260716T213527Z`

- Previous `index.html` SHA-256:
  `c0152f506d7c2a9526c586ce5fea6092a475f413beb149836d8f4bf7dda319cf`.
- The snapshot contains the prior index and 27 prior hashed assets.
- Prior hashed assets were also retained in the live asset directory so
  already-open browser tabs continue to load.

API rollback:

1. Stop `bluey-jobs-api`.
2. Atomically install the saved binary at
   `/usr/local/bin/bluey-jobs-api`.
3. Start `bluey-jobs-api`.
4. Require `127.0.0.1:8081/health` to return the previous revision before
   restoring any portal files.

Portal rollback needs only an atomic restoration of the saved `index.html`
because its referenced hashed assets remain present. The complete saved portal
directory is available if a full restoration is required.

## Live Verification

| Check | Result |
| --- | --- |
| `https://bluey.sh/jobs/` | `200`, references the new JS and CSS assets |
| New portal JavaScript | `200`, exact deployed SHA-256 |
| New portal CSS | `200`, exact deployed SHA-256 |
| Local Jobs `/health` | `200`, exact commit `e319ef5f...` |
| Public main `/health` | `200`, unchanged commit `5cf31b9d...` |
| Unauthenticated `/api/jobs/workspace` | `401` |
| Unauthenticated `/api/jobs/onboarding/complete` | `401` |
| Public `/api/jobs/internal/discovery/lease` | `404` |
| `/llms.txt` | `410` |
| Missing Jobs source map | `404` |
| GPTBot request to `/jobs/` | `403` |
| Direct HTTPS origin request | timed out; no Cloudflare bypass |
| Direct HTTP origin request | redirect only; protected content not served |
| Jobs API | active, `NRestarts=0`, no deploy-time warnings/errors |
| Main API | active, `NRestarts=0`, unchanged |
| Caddy | active, `NRestarts=0`, unchanged |
| Jobs listener | loopback only on `127.0.0.1:8081` |
| Root disk | 54% used, 27 GB free |

Cloudflare adds its client-side bot-detection snippet to public HTML. The
origin index hash is therefore the authoritative portal HTML artifact hash.
The referenced JavaScript and CSS are byte-for-byte identical through the
public edge. Native Bluey `/health` traffic remains unchallenged and returns
`200`.

Live unauthenticated desktop (`1440x1000`) and narrow mobile (`390x844`)
screenshots rendered the complete Jobs page without overlap or horizontal
overflow. The owner will exercise the authenticated resume-upload and
completion flow with their live account.

## Scope Preserved

The deployment keeps the existing Jobs safety and product boundaries:

- Review-first Jobs behavior remains intact.
- The main overlay, meetings, audio, sessions, Coach, Workspaces, and local IPC
  were not changed.
- The signed native release was not rebuilt or republished.
- Caddy and Cloudflare rules were not altered.
- Sashreek-owned branches and the dirty shared worktree were not touched.

The Jobs-to-Coach chain remains intentionally gated as documented in Round 523.
Round 525 remains the implementation source of truth for resume parsing,
durable onboarding progress, final-step recovery, and mainline branch
reconciliation.
