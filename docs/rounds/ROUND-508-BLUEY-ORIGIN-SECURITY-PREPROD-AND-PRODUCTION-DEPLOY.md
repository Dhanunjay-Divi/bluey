# Round 508 - Bluey Origin Security Preprod And Production Deploy

Date: 2026-07-12
Repository: `/Users/uno/Downloads/cue-bluey-jobs`
Branch: `codex/bluey-jobs-20260710`

## Outcome

Round 506's source-verifiable origin protections were isolated from the shared
dirty worktree, tested in preprod, and promoted to `https://bluey.sh`.

This release materially reduces self-identifying AI crawling, closes public
Jobs worker and admin surfaces, retires `llms.txt`, makes missing static assets
real 404 responses, adds shared Jobs API throttling, and binds both application
servers to loopback.

It does **not** claim that a public website is unscrapable. Cloudflare is not
yet authoritative for Bluey's DNS, so bot scoring, AI Crawl Control, managed
challenges, and direct-origin shielding remain an external deployment gate.

## Release Identity

- Release source SHA-256:
  `f9b8ba3dcda94ed55d02f5e9485f75d8b858462ef38a8e87d8910f813ca95c90`
- Embedded release ID: `round506-f9b8ba3dcda9`
- Web archive SHA-256:
  `7fb9d115a1c0407755c940f95edd951a39a2bfb7520a0749fe307fc8a67eac07`
- Main API binary SHA-256:
  `87f092675d74a9af88a2c68145283622ad1ce3e201fa57f98926e25eb23edaa0`
- Jobs API binary SHA-256:
  `1eacf9a6a349e366fcc0c23fdaa0965287590a63ead41c4aa057773bb248af25`
- Caddyfile SHA-256:
  `2b36267138ea1254746e8fd202800929b196bb3fec73f46b49c1a50ca05bf0c5`

## Additional Hardening

The release adds one defense-in-depth improvement beyond the initial Round 506
candidate:

- `BLUEY_API_HOST` now defaults to `127.0.0.1`; production explicitly sets it.
- The Jobs listener remains explicitly bound to `127.0.0.1`.
- `/admin/*` and `/api/jobs/internal/*` return 404 on the customer hostname.
- Caddy blocks self-identifying Training, Search, and Agent crawler user agents
  as a fallback while the managed edge is pending.
- Conventional Googlebot remains reachable for the current SEO-preserving
  posture.

The crawler user-agent rule is not treated as the authoritative defense because
a hostile client can spoof a normal browser.

## Preprod Evidence

An isolated stack ran on production-host loopback ports without replacing live
services:

- main API: `127.0.0.1:18080`
- Jobs API: `127.0.0.1:18081`
- Caddy: `127.0.0.1:18090`

Verification passed:

- 326 server library tests
- 66 server HTTP integration tests
- clippy with warnings denied
- 15 portal tests, typecheck, and production build
- edge-policy and Jobs client-boundary source checks
- authenticated Redis-backed Jobs throttling returned 429 with `Retry-After`
- a valid signed worker request reached its handler; exact replay returned 401
- desktop 1280 px and mobile 390 px visual checks had no body overflow
- browser console contained no errors

Preprod was stopped after production promotion.

## Production Backup

The following backups were completed before mutation:

- PostgreSQL:
  `/var/backups/bluey-api/hourly/bluey-postgres-20260712T203301Z.pgdump`
- Release snapshot:
  `/var/backups/bluey-api/releases/round506-f9b8ba3dcda9-20260712T203301Z`

The release snapshot contains the previous API binaries, Caddyfile, API and
Jobs environment files, web root archive, and a checksum manifest.

## Production Evidence

All three services are active, enabled, and report zero restarts:

- `bluey-api`
- `bluey-jobs-api`
- `caddy`

Listeners after promotion:

- main API: `127.0.0.1:8080`
- Jobs API: `127.0.0.1:8081`
- Caddy: public 80/443

The firewall exposes only SSH and HTTP/HTTPS. A direct request to public port
8080 timed out.

Live HTTP checks:

| Check | Result |
| --- | --- |
| `/health` | 200, exact release ID |
| `/jobs/` and active hashed asset | 200 |
| `/llms.txt` | 410 |
| missing Jobs source map | 404 |
| public Jobs worker route | 404 |
| public admin route | 404 |
| unauthenticated Jobs workspace | 401 |
| invalid Square webhook signature | 401 |
| GPTBot / OAI Search / Claude / Perplexity user agents | 403 |
| conventional Googlebot | 200 |
| `robots.txt` | 200 `text/plain` with Content-Signal policy |
| Jobs `X-Robots-Tag` | `noindex, nofollow, noarchive, nosnippet` |
| public `Server` header | absent |
| installers and release metadata | 200 |
| signed worker request | reached handler; exact replay 401 |
| production source maps | none present |
| `.env`, backup, private-key, and Git metadata files in web root | none present |
| Terms and Privacy | 200; automated-access restrictions present |

The deployed web tree has no drift from the tested preprod candidate. The live
browser passed desktop and mobile visual checks with no horizontal overflow and
no console errors.

## Turnstile Production Enablement

After the initial origin deployment, a managed Cloudflare Turnstile widget named
`Bluey Production` was created for `bluey.sh` and `www.bluey.sh`.

The site and secret keys were written directly to the root-owned production
environment without printing them into deployment logs:

- `BLUEY_TURNSTILE_SITE_KEY`
- `BLUEY_TURNSTILE_SECRET_KEY`
- `BLUEY_REQUIRE_TURNSTILE=1`

The existing `BLUEY_LOG_STORAGE=r2` and
`BLUEY_SQUARE_APPLICATION_ID_EXPECTED` settings were explicitly checked before
the API restart and remained intact. Strict cloud preflight passed with zero
warnings. Live `/auth/captcha/config` returned 200 with
`provider=turnstile` and a present public site key.

## Remaining External Gate

`bluey.sh` still resolves directly to `165.227.77.152`, and direct HTTPS to the
historical origin returns 200. Name servers remain at the registrar. Therefore:

1. Sign in to Cloudflare and add/activate the zone.
2. Move DNS to Cloudflare and proxy `bluey.sh` and `www.bluey.sh`.
3. Enable Full (strict) TLS, AI Crawl Control for Training/Search/Agent,
   managed bot controls, and staged rate-limit rules.
4. Restrict origin 80/443 to Cloudflare ranges or use Cloudflare Tunnel.
5. Prove direct-origin bypass fails before marking Round 506 complete.

The broader passkey, trusted-device, BFF cookie-session, extraction-budget, and
account-farming controls from the replacement handoff remain a separate
authentication architecture migration. They were intentionally not mixed into
this origin-security production release.

## Rollback

If rollback is required:

1. Stop `bluey-api`, `bluey-jobs-api`, and Caddy.
2. Restore binaries, Caddyfile, and environment files from the release snapshot.
3. Restore the web root from `web.before.tar.gz`.
4. Remove the Round 506 Caddy environment drop-in if reverting the Caddyfile.
5. Run `systemctl daemon-reload` and restart all three services.
6. Verify the previous `/health` release ID and signed-in browser flows.

The PostgreSQL backup is a last-resort data rollback and was not needed during
this deployment.
