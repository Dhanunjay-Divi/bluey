# Round 564: Jobs Workspace Load Timeout Fix

Date: 2026-07-22

Status: deployed to main and production

## Problem

A live Jobs visit produced a Cloudflare 524 page while opening the signed-in
Jobs portal. Public `/jobs/` and `/health` were fast, and unauthenticated
`/api/jobs/workspace` returned quickly with `401`. That points to the signed-in
workspace payload path rather than static hosting.

The expensive part of that path was account-level projection of shared global
discovery candidates. The workspace endpoint synchronously attempted to
materialize up to the shared candidate scan before returning the portal data.
When global candidate projection was slow, locked, or backed up, the first
signed-in render could wait long enough for Cloudflare to time out.

## Fix

`GET /api/jobs/workspace` and onboarding completion now schedule global
candidate materialization as best-effort background work instead of blocking the
request. The workspace payload returns the current account state immediately;
newly projected matches appear on the next refresh after the background pass
finishes.

The portal API client now uses bounded fetches:

- short timeouts for authenticated reads and token refresh;
- longer timeouts for writes/downloads;
- a clear retryable Jobs message instead of an indefinite blank loading screen.

This preserves the safety boundary: global feed entries remain discovery leads,
and employer-facing action still requires original-source revalidation and the
existing eligibility checks.

## Files

- `server/src/api/jobs.rs`
- `jobs/portal/src/api.ts`

## Verification

Local verification before deployment:

```text
cargo fmt --check
cargo check
cargo test jobs::
npm run typecheck --workspace @bluey/jobs-portal
npm run test --workspace @bluey/jobs-portal
npm run build --workspace @bluey/jobs-portal
```

Observed results:

- server `cargo check` passed;
- focused server Jobs tests passed: 115 passed;
- portal typecheck passed;
- portal tests passed: 10 files, 62 tests;
- portal production build passed.

The Vite build emitted only the existing large-chunk advisory and did not emit
source maps.

## Deployment Evidence

Implementation source commit used for the production Jobs API build:

```text
02a8cce224eb40124d5151deedd1deaaaf4e0407
```

Final mainline/documentation commit:

```text
ae4c733981690e22b8da37b641234c9bcc178dd2
```

Production backups captured before rollout:

```text
/var/backups/bluey-api/round564-20260723T040005Z/bin/bluey-jobs-api.before-02a8cce224eb
c84eb4c3b7e4673698d39b7543bb1d4f29f0ee7f2f2efcc466f2c6f975b2e53f

/var/www/bluey/backups/jobs-before-round564-20260723T040005Z.tar.gz
71daf8240c1dadebddc843c14189d113f90b3b8e30c3185ddb0b27dc2ad6194b
```

Deployment note: the first API swap accidentally used a local macOS binary on
the Linux production host. Systemd rejected it with `Exec format error`; the
Jobs API was immediately rolled back to the verified previous Linux binary
before the final deployment continued. The final production binary was then
built on the Linux host from the exact source archive for commit `02a8cce2`.

Final deployed Jobs API binary:

```text
/usr/local/bin/bluey-jobs-api
beab5dedb46854dbad9547edb6f46e062048dbdbacd2b70c004d6ce99a8bceeb
ELF 64-bit LSB pie executable, x86-64
```

The Jobs API standalone loopback health reports the intended source commit:

```text
GET http://127.0.0.1:8081/health
200
{"status":"ok","version":"0.1.5","commit":"02a8cce224eb40124d5151deedd1deaaaf4e0407","platform":"linux-x86_64",...}
```

The rebuilt Jobs portal is deployed:

```text
/var/www/bluey/jobs/index.html
e44a1b750bfd282e3912edfc6eef3a425d662f1ba22038c6df804f7ea9ff0335
```

Protected Jobs flags remained disabled:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

Final service state:

```text
bluey-jobs-api.service ActiveState=active MainPID=2425193 NRestarts=0
caddy.service ActiveState=active MainPID=2217438 NRestarts=0
```

Final live checks after the documentation commit:

```text
GET https://bluey.sh/jobs/                              200 0.070s
GET https://bluey.sh/jobs/assets/index-*.js             200 0.409s
GET https://bluey.sh/jobs/assets/index-*.css            200 0.134s
GET https://bluey.sh/health                             200
GET https://bluey.sh/api/jobs/workspace                 401 0.078s
GET https://bluey.sh/api/jobs/internal/discovery/lease  404
GET https://bluey.sh/jobs/assets/index-*.js.map         404
GET https://bluey.sh/jobs/ with GPTBot UA               403 0.157s
GET http://127.0.0.1:8081/api/jobs/workspace            401 0.001s
```

The observed Cloudflare 524 path is fixed at the server boundary: signed-in
workspace loads no longer block on global candidate materialization, and the
portal has bounded fetches if any future read stalls.

## Rollback

For a portal-only regression, restore the saved `/var/www/bluey/jobs` directory.

For an API regression, restore the paired previous Jobs API/server binary and
repeat route, flag, health, and portal checks. Prefer fix-forward unless the
workspace route is unavailable or protected route behavior regresses.
