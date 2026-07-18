# Round 544: Seven-Resume Jobs Manual Production Deploy

Date: 2026-07-18

## Objective

Deploy the Round 543 resume-import, evidence-only tailoring, and truthful packet
review changes to the standalone Bluey Jobs API and `/jobs/` portal. The deployment
was performed manually from the exact reconciled `main` source commit. GitHub
Actions, the main Bluey API, Caddy, Cloudflare, native installers, meeting overlay,
audio, and answer runtime were not changed.

No employer application was submitted. The seven-document parser audit ran locally
without signing into a Bluey account. Signed-in Chrome QA remains a separate browser
check because Chrome was not running and permission to launch it had not been
received at the time this evidence was recorded.

## Source and scope

- Repository: `/Users/uno/Downloads/cue-jobs-universal-main`
- Source branch: `main`
- Implementation commit: `cc1054ec1c1cc4fa6ac054fb26867700e1871840`
- Commit title: `Strengthen Jobs resume import and tailoring`
- Deployed services: `bluey-jobs-api` and the static `/jobs/` portal only
- Unchanged services: main `bluey-api`, Caddy, Cloudflare, and signed native release

The source commit was fetched and compared with `origin/main` before deployment.
Both resolved to the same commit and the worktree was clean.

## Pre-deployment verification

```text
Committed portal tests           10 files, 54 tests passed
Portal TypeScript check          passed
Portal production build          passed, 2,282 modules transformed
Jobs database/policy tests       41 passed
Rust Clippy                      passed with -D warnings
Rust formatting                  passed
git diff --check                 passed
```

The production portal audit found no source maps, local owner paths, supplied
resume names, private resume contents, internal profile-fact identifiers, or corpus
fixtures in the committed/static output.

## Backup and build evidence

### PostgreSQL

- Backup: `/var/backups/bluey-api/hourly/bluey-postgres-20260718T154313Z.pgdump`
- Size: `24,725,903` bytes
- SHA-256: `200952daa639e095a80653231988e1395bd63f8676ce960f2477f0ae8ddc3f75`
- `pg_restore -l` entries: `371`
- Checksum verification: passed

### Exact source archive

- Local archive: `/tmp/bluey-jobs-cc1054ec.tar.gz`
- Size: `33,142,448` bytes
- SHA-256: `ccf10f818729d754a3d9e2b3568b120c1cb611135ca0fa5364bb720b5a0c2d85`
- Production build directory: `/opt/bluey-build-jobs-cc1054ec`
- Build command stamped `BLUEY_GIT_COMMIT=cc1054ec1c1cc4fa6ac054fb26867700e1871840`

Only superseded Jobs build trees were removed to recover build space. The then-live
Jobs build, binary backups, portal backups, database backups, and all unrelated
Bluey artifacts were preserved.

### Jobs API binary

- Installed path: `/usr/local/bin/bluey-jobs-api`
- Size: `20,211,424` bytes
- SHA-256: `c2db137e51e3def6c296064827024e9ee8b5bc7c2429e196ac3a23c968fbd543`
- Service health commit: `cc1054ec1c1cc4fa6ac054fb26867700e1871840`

### Rollback artifacts

- Deployment timestamp: `20260718T155647Z`
- Previous binary: `/var/backups/bluey-api/bin/bluey-jobs-api.before-cc1054ec-20260718T155647Z`
- Previous binary SHA-256: `35957bec97bd776a25d7548c0fd00039e61d0e62c481f91c48de360e9daab96c`
- Previous portal: `/var/www/bluey/backups/jobs-before-cc1054ec-20260718T155647Z`
- Previous portal index SHA-256: `d4bc63861d8fab45bef1b22d437fef9217ebfdb081a1ad06862ea4efb37e43ce`

Rollback consists of restoring the saved binary, restoring the saved portal index
and assets, restarting only `bluey-jobs-api`, and checking its loopback health
before exposing it again. Database restore is not expected for this release, but
the verified dump is available if rollback requires data recovery.

## Portal artifact evidence

The hashed assets were copied before the HTML entry point was atomically replaced.
No delete-based sync was used.

| Artifact | SHA-256 |
| --- | --- |
| `web/jobs/index.html` | `3dd439b1c26a1c3de2c48f863d5766897e062956c2b3ac09341d2b15f295eba6` |
| `index-DQCmM7tL.js` | `41e3bd96627516ebb72995d0e0a8f8c2fdf408b4e7e61a4c4a0785339111d70d` |
| `index-C6juvqyH.css` | `6f048b437aded91cdd289a2e6c21092f7cb2b135a1e5b88888b376e27f69491f` |
| `resume-diff-Bo77Q1LY.js` | `07c12dcc11192b316424cf0fad2d48cbe7168ed18cf3c351a16ec40f733f0154` |
| `us-locations.json` | `6d88220ded2a20734be9905731be2ed134325f9fb595e7ebb3ff5dbbed899df8` |

The public edge serves the same JS, CSS, resume-diff, and location-data hashes.

## Live verification

### Services and listeners

- `bluey-jobs-api`: active, running, zero restarts
- `bluey-api`: active, running, zero restarts
- `caddy`: active, running, zero restarts
- main API: `127.0.0.1:8080`
- Jobs API: `127.0.0.1:8081`
- Jobs API warnings since deploy: none

The loopback Jobs health endpoint reported version `0.1.5` and exact commit
`cc1054ec1c1cc4fa6ac054fb26867700e1871840`. The public `/health` endpoint remains
the intentionally unchanged main Bluey API and therefore reports that service's
separate source commit.

### Public route matrix

| Check | Result |
| --- | --- |
| `/jobs/` | `200` |
| `/auth/captcha/config` | `200` |
| unsigned `/account/me` | `401` |
| unsigned `/api/jobs/workspace` | `401` |
| `/api/jobs/internal/discovery/lease` | `404` |
| `/llms.txt` | `410` |
| missing Jobs source map | `404` |
| GPTBot request to `/jobs/` | `403` |
| native client request to `/health` | `200` |
| direct HTTPS origin bypass | blocked/timed out |
| direct HTTP origin request | redirect only; no application response |

The `/jobs/` response retained CSP, HSTS, `nosniff`, `DENY` framing,
strict-origin referrer policy, and `X-Robots-Tag: noindex, nofollow, noarchive,
nosnippet` behind Cloudflare.

## User-flow boundary

The automated seven-resume corpus test exercised import, parsing, review warnings,
Career Profile construction, job-specific resume generation, factual provenance,
real resume diffs, packet freezing, and Review-first runner guards. It used no Bluey
login and did not submit an application.

The product continues to enforce these boundaries:

- missing source facts are surfaced for review rather than invented;
- every canonical job receives a separate resume version;
- imported contact email is not treated as a verified application identity;
- packet approval freezes identity, resume, answers, job, browser profile, and
  checksum;
- `awaiting_review` work cannot enter a local or cloud runner;
- unknown sites remain Review-only, LinkedIn and Indeed remain handoff surfaces,
  and named ATS families remain beta-review until their exact adapters are certified;
- retries and interventions do not double-meter a committed packet.

Signed-in Chrome QA should verify the visible account label in masked form, import
warnings, route refresh/logout, packet review, and desktop/mobile rendering. It must
stop before employer submission. That browser check is not represented as complete
in this round.

## Outcome

Round 543 is live at `https://bluey.sh/jobs/` from exact reconciled source commit
`cc1054ec1c1cc4fa6ac054fb26867700e1871840`. The deployment changed only the Jobs
API and Jobs portal, retained a verified database backup and complete rollback set,
and passed service, artifact, auth-boundary, crawler, and direct-origin checks.
