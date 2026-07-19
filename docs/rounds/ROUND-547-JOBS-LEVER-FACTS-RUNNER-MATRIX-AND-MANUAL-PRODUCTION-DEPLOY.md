# Round 547: Jobs Lever Facts, Runner Matrix, And Manual Production Deploy

Date: 2026-07-18

## Scope

Correct the public Lever import and preview behavior discovered while checking
the Institute of Foundation Models listing, prove Free/Pro/Cloud runner policy
with isolated test accounts, and deploy only the Bluey Jobs API and Jobs portal
after the complete pre-deploy test matrix passes.

This release does not change or redeploy the native Bluey overlay, audio or
meeting runtime, signed desktop installers, main Bluey API, Caddy, or Cloudflare
configuration. No employer application is submitted.

## Real Lever Listing Evidence

The listing checked is:

```text
Employer: Institute of Foundation Models
Role: Full Stack Software Engineer
Location: Sunnyvale, CA
URL: https://jobs.lever.co/ifm-us/1454349c-eb2b-480b-9a57-edfbb2aeeffe
ATS identifier: 1454349c-eb2b-480b-9a57-edfbb2aeeffe
Commitment: Full-time
Workplace: On-site
Compensation: USD 150,000-450,000 per year
Published by Lever: 2025-04-09
Sponsorship: "This position is eligible for visa sponsorship."
```

These facts came from Lever's public posting response. Bluey does not submit
candidate data when reading this public response.

The listing is still returned by Lever, but it is far older than the candidate
track's 14-day freshness limit. Bluey must therefore reject it as stale before
packet preparation or runner entry. It is also outside the selected candidate
locations unless the location policy is changed.

## Bugs Corrected

### Posting date

The preview portal used Bluey's local import timestamp when the employer
posting date was unavailable. This made a pasted listing appear as "Posted
today." The portal now uses only the employer-provided posting timestamp.
Unknown dates render as "Posting date not listed."

Preview mode also states that it cannot verify live job facts and asks the user
to sign in before importing and checking the listing.

### Lever description and compensation

Lever stores material facts across several response fields. Discovery and
server import now preserve:

- the primary description;
- every structured list heading and body;
- the additional disclosure section, including HTML fallback; and
- the structured salary range.

This prevents sponsorship disclosures and compensation from disappearing
before eligibility evaluation.

### Sponsorship eligibility

Server-owned eligibility still checks explicit blockers first. It now also
recognizes a narrow set of explicit positive sponsorship statements. A listing
that says it is eligible for visa sponsorship no longer receives an incorrect
"sponsorship must be confirmed" review reason.

## Resume Generation Truth

Bluey Jobs does not currently call an LLM to create the tailored resume. The
current implementation in `server/src/db/jobs_tailoring.rs` deterministically
builds a job-specific resume version from stored candidate facts and the job
description. The exact version and diff remain part of packet review.

This is intentionally described as deterministic tailoring rather than naming
an AI model that is not in the execution path.

## Runner And Account Truth

The Free/Pro/Cloud release policy is now covered by an end-to-end HTTP matrix
using isolated temporary fixture accounts and a mocked workflow gateway:

- `awaiting_review` cannot enter either runner and consumes no packet;
- Free can approve a reviewed packet but includes neither browser runner;
- Pro includes local only when the signed local distribution gate is enabled;
- Pro cannot enter cloud execution;
- Cloud includes local and cloud entitlements;
- Cloud execution returns unavailable when its workflow gateway is absent;
- a mocked authenticated gateway receives exactly one cloud workflow request;
- packet approval meters once, and commit replay does not meter again.

The production environment still has no running Temporal worker, cloud browser
pool, Chromium runner service, or enabled local Bluey Browser distribution.
The plan matrix proves API policy; it does not pretend those production runners
exist. No paid production account was fabricated for this verification.

## Pre-Deploy Verification

All gates completed before deployment:

```text
Jobs package tests: 308 passed
  automation: 133
  browser: 34
  runner: 50
  workflows: 35
  portal: 56

Server unit tests: 585 passed
Server HTTP integration tests: 75 passed
Runner plan matrix, debug: passed
Runner plan matrix, optimized release: passed
Usage reservation schema test: passed
Rust formatting: passed
Rust Clippy with -D warnings: passed
Portal typecheck: passed
Portal production build: passed
git diff --check: passed
Production source maps: absent
```

The Vite build reports a non-fatal chunk-size advisory for the main portal
bundle. It does not produce source maps or fail the production build.

## Source And Deployment

The exact tested source was committed and pushed directly to `main`:

```text
Source commit: f3a0a04360febb36363c27f869e954f2d61f32e0
Source archive: /tmp/bluey-jobs-f3a0a043.tar.gz
Source archive SHA-256: 36b99d118bc9b35f5689940f1b2f18a9879e67881b36a2a758172f8b7f8fb401
```

The deployment was manual. GitHub Actions was not used. A first remote build
attempt stopped before deployment because the host's system Cargo 1.75 could
not read the repository's version-4 lockfile. The clean archive was then built
successfully with `/root/.cargo/bin/cargo` 1.95 and the exact source commit was
embedded through `BLUEY_GIT_COMMIT`.

Before replacement, production received a fresh PostgreSQL backup and scoped
Jobs rollback copies:

```text
PostgreSQL backup:
  /var/backups/bluey-api/hourly/bluey-postgres-20260719T020959Z.pgdump
  SHA-256: 6650bbc5b5a8bee3544ab28b5252c88e72ce30d96a2db81be20e53f3a701cba7
  pg_restore listing: 371 lines

Previous Jobs API:
  /var/backups/bluey-api/bin/bluey-jobs-api.before-f3a0a043-20260719T020959Z
  SHA-256: 395644a4fc7b7ab26437b6db1cbbbfaa7b8618ce367f94476be90c0142450c8a

Previous Jobs portal:
  /var/www/bluey/backups/jobs-before-f3a0a043-20260719T020959Z
```

Only the Jobs API binary and Jobs portal were replaced. The deployed artifact
identities are:

```text
Jobs API: /usr/local/bin/bluey-jobs-api
Jobs API SHA-256: 07e12cb5d5668c4c8cd24129a9fbccc3f512e873d2400867b0fde55be288ac96

Portal index SHA-256:
  0536f97d36782a830dc7148596bac5712b25bc57cabc8e353f43f7986be6b755
Portal JavaScript SHA-256:
  f0bf6af9744c7d40c6f52ce7738527f8f1e159dc2310b1f3c533f075002dbde2
Portal CSS SHA-256:
  6f048b437aded91cdd289a2e6c21092f7cb2b135a1e5b88888b376e27f69491f
US location data SHA-256:
  6d88220ded2a20734be9905731be2ed134325f9fb595e7ebb3ff5dbbed899df8
```

The portal files were copied before the index swap. The new index and location
data were moved atomically, then stale origin assets were pruned. No production
source maps are present.

## Live Verification

Production verification passed after the scoped restart:

- `bluey-api`, `bluey-jobs-api`, and `caddy` are active with zero restarts;
- Jobs listens only on `127.0.0.1:8081`;
- loopback Jobs health reports commit
  `f3a0a04360febb36363c27f869e954f2d61f32e0`;
- `https://bluey.sh/jobs/` returns `200` and the expected robot policy;
- the public portal references the new JavaScript hash and its bytes match the
  origin artifact;
- an unsigned Jobs workspace request returns `401`;
- the public internal discovery lease path returns `404`;
- GPTBot receives `403` on `/jobs/`;
- the native `bluey-cloud-client/0.1.100` user agent receives `200` on `/health`;
- `/auth/captcha/config` returns `200`;
- the new bundle's missing `.map` path returns `404`;
- direct HTTPS access to the historical origin is blocked; and
- the Jobs service emitted no warning or error journal entries after deploy.

An authenticated Chrome session loaded the live Settings workspace with the
shared balance, Career Track, application-email limits, managed search rules,
Answer Memory, and Free/Pro/Cloud gates. Separate headless Chromium checks then
loaded the live preview Matches route at desktop (`1440x1000`) and mobile
(`390x844`) viewports. Both returned `200`, rendered the Matches workspace, and
reported no application runtime or page exceptions. Cloudflare's optional
analytics beacon is blocked by Bluey's stricter content-security policy; that
non-product telemetry notice does not affect portal behavior.

## Rollback Boundary

Rollback is limited to:

1. the `bluey-jobs-api` binary; and
2. `/var/www/bluey/jobs`.

The main Bluey API, native installers, meeting runtime, Caddy, and Cloudflare
must remain unchanged.
