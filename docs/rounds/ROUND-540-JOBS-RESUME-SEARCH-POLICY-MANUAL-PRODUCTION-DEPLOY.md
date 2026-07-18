# Round 540 - Jobs resume and search policy production deploy

Date: 2026-07-17

Status: deployed and live verification complete

Repository: `/Users/uno/Downloads/cue-round536-mainline-refactor`

Source commit: `6a5501ade108325b2b1e2136c7396d40fe31eaf2`

Live portal: `https://bluey.sh/jobs/`

## Objective

Deploy Round 539 from one exact mainline source revision and verify the real
signed-in experience. This release fixes resume parsing, nationwide location
suggestions, canonical role selection, experience-aware search policy, compact
skills and certification editing, truthful Matches activation, and onboarding
completion without changing Bluey's meeting overlay, native audio, answer runtime,
Caddy configuration, Cloudflare configuration, or signed desktop release.

## Source settlement

The implementation was rebased onto the then-current `origin/main`, reverified,
committed as `6a5501ade108325b2b1e2136c7396d40fe31eaf2`, and pushed directly to
`origin/main`. The candidate contained only the reviewed Jobs portal, Jobs policy
and persistence, generated Jobs location data, tests, static bundle, and Round 539
documentation.

The two owner-authorized resumes used for browser QA remained local. No resume,
candidate contact data, browser credential, or session token was added to Git or
deployment evidence.

## Predeployment verification

```text
Jobs portal tests             10 files, 45 tests passed
Jobs portal typecheck         passed
Jobs portal production build  passed, 2,281 modules transformed
Jobs database tests           39 passed
Jobs server strict Clippy      passed with -D warnings
Location data                 32,238 records and national spot checks passed
git diff --check              passed
Published source maps         none
```

The production build emitted one non-blocking bundle-size warning for a 502.51 kB
chunk. It did not emit source maps.

## Backup and rollback evidence

A fresh PostgreSQL backup was created before swapping any artifact:

```text
Path       /var/backups/bluey-api/hourly/bluey-postgres-20260718T002858Z.pgdump
Bytes      24,266,508
Inventory  371 pg_restore entries
Result     checksum and restore-list verification passed
```

The previous production artifacts were preserved here:

```text
Jobs API  /var/backups/bluey-api/bin/bluey-jobs-api.before-6a5501ad-20260718T004327Z
SHA-256   f63745421f0200f76e32d099d4ed516d0f2098fc13fcd32184b0c9111c089688

Portal    /var/www/bluey/backups/jobs-before-6a5501ad-20260718T004327Z
Old index 4036255d17e51b359005ecdac9af67fec3e0210ae19767bdff2a3aa369091393
```

Rollback is intentionally narrow: restore the saved Jobs binary, restart only
`bluey-jobs-api`, restore the saved Jobs portal directory/index, and rerun the
health/auth/static checks below. A database restore is not part of routine artifact
rollback and must be an explicit operator decision.

## Build and deployed artifacts

The exact Git revision was archived and verified before the production build:

```text
Archive  /tmp/bluey-jobs-6a5501ad-20260718T0029Z.tar.gz
SHA-256  47ca266db8b8d40e7a77cf78553ac81724e8e63daee50f761fea98a08e900866
```

The deployed Jobs API identifies the same source commit:

```text
Binary   /opt/bluey-build-jobs-6a5501ad/server/target/release/bluey-jobs-api
Commit   6a5501ade108325b2b1e2136c7396d40fe31eaf2
SHA-256  35957bec97bd776a25d7548c0fd00039e61d0e62c481f91c48de360e9daab96c
Bytes    20,020,008
Health   version 0.1.5, linux-x86_64, status ok
```

The portal was published by copying content-addressed assets first and replacing
the location data and index only after those assets were available:

```text
index.html              d4bc63861d8fab45bef1b22d437fef9217ebfdb081a1ad06862ea4efb37e43ce
us-locations.json       6d88220ded2a20734be9905731be2ed134325f9fb595e7ebb3ff5dbbed899df8
assets/index-DTq0e6u6.js  2589b1353dec407168e513fb06e19950576efd2a89e8a271a27620745bea11f2
assets/index-B6okI5-Y.css a20b29014df5ec82530d465b0f86e4fa31bb81a5c6b3e42980a372336e7251b1
```

Edge response bodies matched the deployed asset hashes.

## Live service verification

Only `bluey-jobs-api` and the Jobs static portal changed. The main API, Caddy,
Cloudflare edge policy, and native release remained untouched.

```text
bluey-jobs-api                    active, enabled, zero restarts
bluey-api                         active, enabled, zero restarts
caddy                             active, enabled, zero restarts
Jobs listener                     127.0.0.1:8081 only
/jobs/                            200
/jobs/us-locations.json           200, exact hash
/jobs/assets/index-DTq0e6u6.js    200, exact hash
/jobs/assets/index-B6okI5-Y.css   200, exact hash
missing Jobs source map           404
/auth/captcha/config              200
/account/me                       401 when unsigned
/api/jobs/workspace               401 when unsigned
/api/jobs/internal/discovery/lease 404 on the public edge
/llms.txt                         410
GPTBot on /jobs/                  403
direct-origin HTTPS bypass        blocked/timed out
```

The Jobs route retained CSP, HSTS, frame denial, MIME sniffing protection, strict
referrer policy, and `X-Robots-Tag` noindex/nofollow/noarchive/nosnippet headers.
No new service warning was observed after deployment.

## Signed-in Chrome verification

Production was tested through the owner's existing signed-in Chrome session without
reading cookies, storage, passwords, or tokens.

- `/jobs/` redirected to and rendered `/jobs/matches`.
- Refresh preserved both Matches and Settings routes.
- Browser console logs were empty.
- Matches truthfully reported that automatic discovery was not connected and gave
  the immediate `Add job link` path.
- The activation sequence rendered as verify posting, check hard filters, rank fit,
  and review application kit.
- Settings rendered Bluey-owned policy instead of editable pace/threshold fields:
  14-day freshness, experience fit, up to 10 applications daily, and Review first.
- Typing `PM` offered the full canonical roles Product Manager, Project Manager,
  and Program Manager.
- Typing `Truth or Conseq` in the live location field returned
  `Truth or Consequences, NM` from the nationwide data set.
- Existing signed-in profile data was not changed or saved during these checks.

Desktop and mobile responsive visuals were already verified with synthetic data in
Round 539. The live production desktop rendering matched that reviewed layout.

## Product boundary

This release makes the current review-first Jobs beta more reliable and honest. It
does not claim that automatic discovery credentials, every ATS adapter, local/cloud
browser runners, mailbox ingestion, or unattended universal submission are active.
Those capabilities remain gated until their separate operational and certification
requirements are complete.
