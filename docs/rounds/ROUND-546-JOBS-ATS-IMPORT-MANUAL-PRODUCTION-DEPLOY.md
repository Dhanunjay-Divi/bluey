# Round 546: Jobs ATS Import Manual Production Deploy

Date: 2026-07-18

## Scope

Deploy the Round 545 public ATS importer and truthful Review-first flow without
changing the native Bluey release, meeting overlay, audio runtime, main Bluey
API, Caddy, or Cloudflare configuration.

This release covers:

- server-owned imports for Lever, Greenhouse, Ashby, SmartRecruiters, and
  Workday public job links;
- structured employment type and posting freshness;
- stale-job and sponsorship rejection before preparation or runner entry;
- URL-first job import with a clearly labeled manual Review-only fallback;
- truthful packet diffs and improved JD term matching; and
- strict approval, metering, and runner eligibility boundaries.

No employer application was submitted. Importing, matching, or preparing a
packet does not send candidate data to an employer.

## Source

```text
Repository: /Users/uno/Downloads/cue-jobs-universal-main
Source commit: d5b9fbd94506d68805c01a95b8a1a121aaadb154
Commit title: Verify public ATS imports before job preparation
Remote target: origin/main
```

The source archive used for the production build was:

```text
/tmp/bluey-jobs-d5b9fbd9.tar.gz
SHA-256: 8c3287ecd9b1312a732a0f6f2dba17c08c6b7600b41be8adabae66a82de2e7a4
Build directory: /opt/bluey-build-jobs-d5b9fbd9
BLUEY_GIT_COMMIT: d5b9fbd94506d68805c01a95b8a1a121aaadb154
```

## Backups And Rollback

Before deployment:

```text
PostgreSQL backup:
  /var/backups/bluey-api/hourly/bluey-postgres-20260718T182101Z.pgdump
  bytes: 24,743,661
  SHA-256: febc5db6a7aeb1ae40b3d19928fcfe1ba0d89610e8b146f8eac6d1d36479e609
  pg_restore list entries: 371

Jobs API binary:
  /var/backups/bluey-api/bin/bluey-jobs-api.before-d5b9fbd9-20260718T183419Z

Jobs portal:
  /var/www/bluey/backups/jobs-before-d5b9fbd9-20260718T183419Z
```

The database checksum, archive readability, and restore list were verified
before replacing either artifact.

Rollback is limited to the Jobs API binary and `/var/www/bluey/jobs`. The signed
native release and unrelated Bluey services must remain untouched.

## Deployed Artifacts

```text
bluey-jobs-api SHA-256:
  395644a4fc7b7ab26437b6db1cbbbfaa7b8618ce367f94476be90c0142450c8a

Jobs portal origin index SHA-256:
  b63055c69cff572a442e94746c98c09992fcaaacaecdefacc379289ac585eacb

Public JavaScript:
  index-CG1Z5sOP.js
  66783605308148540656adde89b4588665a0bbd22454cc6d4c6a7a21a99bd9c8

CSS SHA-256:
  6f048b437aded91cdd289a2e6c21092f7cb2b135a1e5b88888b376e27f69491f

Resume diff bundle SHA-256:
  4673b7dd60d437845ee98e74dd48ed7f6ba0530abc53bf685c9897a114a2715c

Location data SHA-256:
  6d88220ded2a20734be9905731be2ed134325f9fb595e7ebb3ff5dbbed899df8
```

The public HTML body is not byte-identical to the origin index because
Cloudflare injects its client-side detection markup. The edge HTML references
the exact deployed JavaScript and CSS artifacts, and the public JavaScript hash
matches the origin artifact.

## Service And Edge Evidence

Production state after deploy:

```text
bluey-api: active, NRestarts=0
bluey-jobs-api: active, NRestarts=0
caddy: active, NRestarts=0
bluey-api listener: 127.0.0.1:8080
bluey-jobs-api listener: 127.0.0.1:8081
Jobs API loopback health commit: d5b9fbd94506d68805c01a95b8a1a121aaadb154
Jobs API warning/error journal entries in the deploy window: none
```

Live edge checks:

| Check | Result |
| --- | --- |
| `/jobs/` | 200 with `X-Robots-Tag: noindex, nofollow, noarchive, nosnippet` |
| `/auth/captcha/config` | 200 with configured Turnstile site key |
| unsigned `/account/me` | 401 |
| unsigned `/api/jobs/workspace` | 401 |
| public `/api/jobs/internal/discovery/lease` | 404 |
| `/llms.txt` | 410 |
| missing `/jobs/assets/index-CG1Z5sOP.js.map` | 404 |
| GPTBot `/jobs/` | 403 |
| native client `/health` | 200, no blanket JavaScript challenge |
| direct HTTPS origin bypass | blocked/timed out |
| direct HTTP origin | redirect only; application not exposed |

The main `/health` commit remains the separately deployed main Bluey API commit;
the Jobs-specific commit is verified through the loopback Jobs health endpoint.

## Real Listing Canary

The public canary is a current Ashby listing:

```text
Employer: OpenAI
Role: Software Engineer, Codex - Enterprise Controls
URL: https://jobs.ashbyhq.com/openai/fff02c39-1185-427c-bf89-70d7eaa5e3db
ATS identifier: fff02c39-1185-427c-bf89-70d7eaa5e3db
Location: San Francisco
Workplace: Hybrid
Employment type: Full time
Published: 2026-07-13
Listed at verification time: true
```

The public Ashby posting API supplied the structured job facts. The public page
title supplied the employer display name. No candidate data was sent to Ashby
or the employer during this verification.

Two Lever listings exercised stale negative cases:

- Kestrel Intelligence, Software Engineer, published 2026-01-15, with an
  explicit US-citizenship requirement; and
- IntraFi, Full Stack Software Engineer, published 2026-05-07.

Both fail the candidate account's 14-day freshness limit before packet
preparation. The citizenship requirement also fails the sponsorship policy.

## Candidate Boundary

The signed-in production account is masked as
`internal-admin-...3943@bluey.sh`. Its current candidate is ADITHYA REDDY
KOPPULA, imported from `Adithya_Reddy_Koppula_Resume_Rivian.docx`.

Review first is enabled. Sponsorship is required. Candidate identity, answers,
and a job-specific resume version remain inside Bluey until the user explicitly
approves an application and a runner is allowed to execute it.

## Runner And Metering Truth

The release does not claim a production runner submission.

- Runner pickers include only `queued` applications.
- The queue API rejects `awaiting_review`.
- Import, match, and packet preview do not consume allowance or balance.
- The unique packet is metered once after explicit approval/commit.
- Local execution requires a local-run entitlement and an available signed
  Bluey Browser distribution.
- Cloud execution requires a cloud entitlement and an available cloud gateway
  and browser pool.
- Stale, blocked, unknown, handoff-only, or otherwise ineligible jobs cannot
  enter either runner.
- If an employer-side submit may have happened but its response was lost, the
  run becomes `side_effect_unknown`; Bluey does not retry the submit blindly.

The production account used for this canary is on the Free Jobs allowance. Its
local and cloud runner controls truthfully remain unavailable. Changing that
entitlement or charging the account was outside this deploy.

## Focused Post-Deploy Tests

```text
cargo test --manifest-path server/Cargo.toml jobs_import -q
  9 passed

cargo test --manifest-path server/Cargo.toml \
  packet_metering_only_counts_a_job_once -q
  1 passed

cargo test --manifest-path server/Cargo.toml \
  stale_imported_job_cannot_prepare_or_enter_a_runner -q
  1 passed
```

These are in addition to the complete Round 545 matrix:

```text
Server unit tests: 585
Server HTTP integration tests: 75
Jobs package tests: 307
Rust formatting: pass
Rust clippy -D warnings: pass
Portal typecheck: pass
Portal build: pass
git diff --check: pass
Production source maps: absent
```

## Signed-In Browser QA Status

Chrome can be launched with the user's Profile 2 and the Bluey Jobs production
tab is present, but the installed Chrome control plugin cannot currently attach
to any tab. It still fails after the supported reconnect procedure and a fresh
profile window. This is a browser-control connection failure, not evidence of a
Bluey page failure.

Because the signed-in tab could not be controlled, the following UI actions are
still unverified in a real signed-in production session:

1. importing the Ashby link through the Add job dialog;
2. inspecting the generated application kit and actual resume diff;
3. confirming the awaiting-review application is absent from runner pickers;
4. approving the packet and observing one-time metering; and
5. observing the account-specific local/cloud runner lock explanation.

Chrome must be reconnected or the Chrome control plugin reinstalled before this
interactive canary can be completed. A different browser was not substituted
because the owner explicitly requested Chrome and the signed-in Chrome state.

## Submission Hold

Employer submission remains deliberately held. Before the final Submit action,
the owner must confirm the exact employer, role, candidate identity, application
email, phone number, resume version, answers, destination, and runner mode.

That confirmation must happen after the final application packet is visible. It
cannot be inferred from permission to test the product generally.
