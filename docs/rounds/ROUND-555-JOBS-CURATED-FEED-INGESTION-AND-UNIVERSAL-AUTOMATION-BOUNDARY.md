# Round 555: Jobs curated-feed ingestion and universal automation boundary

Date: 2026-07-20

## Outcome

Bluey Jobs now has a bounded, typed reader for the four owner-requested public
job lists. It converts table rows into candidate leads, keeps the original
employer application URL, recognizes the five supported public ATS families,
and labels explicit employment and engagement categories without treating any
feed field as application truth.

This is not universal auto-apply. The new reader is a library boundary and is
not yet connected to a global production crawl/index pipeline. Local and cloud
Browser distribution and model generation remain disabled:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Implemented feed contract

`jobs/automation/src/curated-feeds.ts` provides:

- four explicit allowlisted README endpoints;
- HTML and Markdown table parsing without executing third-party scripts;
- 2 MiB response, 20,000-row, and bounded-time limits;
- redirect rejection and ETag/304 support;
- closed-row filtering and canonical original-URL deduplication;
- removal of tracking query parameters;
- rejection of aggregator links when an original employer link is available;
- extraction of Greenhouse, Lever, Ashby, SmartRecruiters, and Workday source
  identities from original URLs;
- `unknown_review` for every unrecognized direct-employer URL; and
- `requiresOriginalRevalidation: true` on every lead.

The parser recognizes these employment types:

```text
full_time, part_time, contract, temporary, internship,
apprenticeship, seasonal, per_diem
```

It separately recognizes:

```text
w2, c2c, 1099, direct_hire
```

Category values inferred from feed text are provisional evidence. Original job
verification and the server-owned Career Track eligibility decision remain
authoritative.

## Live source sample

The four public README files were downloaded and parsed on July 20, 2026. The
result is a time-sensitive sample, not a production availability promise.

| Feed | Open leads | Closed skipped | Five-ATS links | Review-only links |
| --- | ---: | ---: | ---: | ---: |
| Simplify New Grad | 220 | 1,793 | 144 | 76 |
| PrepAI Internships | 497 | 0 | 259 | 238 |
| PrepAI New Grad | 496 | 0 | 274 | 222 |
| Zapply New Grad | 597 | 0 | 393 | 204 |
| **Total** | **1,810** | **1,793** | **1,070** | **740** |

Five-ATS links in this sample route to provider-specific canonical readers;
they do not bypass original-source freshness, eligibility, packet review, or
execution authority. Review-only links require a separate verified provider
adapter or user handoff.

Observed category labels were predominantly full-time and internship. A small
number of rows explicitly indicated contract, part-time, or seasonal work. No
sample row explicitly supplied a reliable W2, C2C, 1099, or direct-hire label,
so Bluey does not invent those values.

## Requested staffing companies

The supplied staffing-company names, aliases, and known domains already live in
`jobs/automation/src/source-catalog.ts`. A brand does not imply a unique form
engine: many listed companies publish through Workday, Greenhouse, Lever,
Ashby, SmartRecruiters, or another underlying ATS. Bluey should route by the
verified application system and adapter version, not by company-name guesses.

Current behavior:

| Source | Discovery | Preparation/submission |
| --- | --- | --- |
| Enrolled Greenhouse, Lever, Ashby, SmartRecruiters, Workday boards | Scheduled production discovery | Beta review only; Browser distribution flags remain off |
| Four curated GitHub lists | Bounded parser implemented; global scheduler/index not implemented | Original-URL revalidation required |
| Jobhive/`ats-scrapers` company directory | Bounded company/source enrollment lead | Original ATS only; no scraper/evasion code imported |
| Listed staffing-company domains | Cataloged | Resolve to a verified ATS or remain review-only |
| LinkedIn and Indeed | Pasted-link preparation/handoff | No unattended submission |
| ZipRecruiter, Dice, CareerBuilder, unknown portals | Review-only lead/handoff | No unattended submission |

## OmniParser boundary

Round 551 introduced a private visual-observation interface for incomplete DOM
surfaces. OmniParser may later provide bounded observations after a separate
runtime and license review, but it is not a substitute for Playwright or a
submission adapter. Visual observations cannot click, cannot claim Submit
success, and cannot authorize an irreversible employer-facing action. A real
DOM/control binding, deterministic validation, durable execution authority, and
receipt evidence are still required.

## Architecture required for broad discovery

The current production discovery scheduler enrolls sources per account and
Career Track. Enrolling thousands of public boards independently for every
account would multiply traffic, duplicate work, and create inconsistent removal
state. Broad discovery therefore requires a shared canonical pipeline:

```text
allowlisted source reader
  -> candidate lead staging
  -> original employer/ATS fetch and open/closed verification
  -> canonical job deduplication and source membership
  -> global searchable posting index
  -> per-account Career Track eligibility and ranking
  -> Review-first application kit
  -> certified adapter or explicit handoff
```

The global source worker must use durable leases, ETags, removal snapshots,
per-source health, bounded retries, provenance, and deny-by-default network
egress. Candidate feeds must never write directly to account matches or create
application authority.

## Verification

Passed for this implementation:

```text
npm run typecheck --prefix jobs
npm test --prefix jobs
node jobs/scripts/check-provenance-licenses.mjs
node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node scripts/check-bluey-jobs-client-boundary.mjs
git diff --check
```

Focused coverage verifies HTML and Markdown formats, inherited company rows,
closed-row filtering, direct-link selection, tracking-parameter removal, URL
deduplication, all five ATS source identities, unknown-review behavior,
employment/engagement classification, response caps, and conditional requests.

The complete Jobs package matrix also passed: 157 automation tests, 100 Browser
tests, 50 runner tests, 38 workflow tests, and 56 portal tests (401 total), plus
typechecking across automation, Browser, runner, workflows, and portal.

## Honest remaining work

Not complete and not represented as live:

- global curated-feed scheduling and a shared canonical job index;
- original-source revalidation/removal persistence for feed candidates;
- direct adapters for every supplied staffing domain;
- certified unattended Submit across every ATS tenant or portal;
- local/cloud Browser distribution and durable cloud-browser operations;
- a production OmniParser service; or
- unattended LinkedIn, Indeed, ZipRecruiter, Dice, or CareerBuilder submission.

The next production slice should build the shared candidate-staging and
canonical-revalidation pipeline, expose source health internally, and only then
surface verified matches to Career Tracks. Employer-facing execution must remain
disabled until the separate adapter, recovery, evidence, and release gates pass.
