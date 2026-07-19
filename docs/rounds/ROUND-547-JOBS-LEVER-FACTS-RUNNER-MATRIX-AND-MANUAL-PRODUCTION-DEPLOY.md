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

Deployment evidence is appended after the exact tested source commit is
created and the manual production deployment completes.

## Rollback Boundary

Rollback is limited to:

1. the `bluey-jobs-api` binary; and
2. `/var/www/bluey/jobs`.

The main Bluey API, native installers, meeting runtime, Caddy, and Cloudflare
must remain unchanged.
