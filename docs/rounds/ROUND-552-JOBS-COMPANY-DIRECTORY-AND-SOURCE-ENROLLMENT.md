# Round 552: Jobs Company Directory And Source Enrollment

Date: 2026-07-20

## Outcome

Bluey Jobs can now turn a company name into a continuously checked public ATS
source without asking a user to discover or paste the employer's board URL.
The Matches page exposes a compact **Watch companies** flow for Greenhouse,
Lever, Ashby, SmartRecruiters, and Workday. After enrollment, the existing
discovery worker reads the original employer board every four hours, validates
each posting, and applies the existing freshness, hard-filter, deduplication,
and Career Track policy before a match can appear.

This round does not enable model generation, local Browser distribution, cloud
Browser distribution, or employer-facing submission. Those three production
flags remain off. It also does not claim support for arbitrary job portals,
LinkedIn, Indeed, ZipRecruiter, Dice, CAPTCHA bypass, or visual-only Submit.

## External Repository Review

The owner supplied
[`kalil0321/ats-scrapers`](https://github.com/kalil0321/ats-scrapers), which now
redirects to the Jobhive repository. It was reviewed at commit
`d825caefc8e97c3533efe1707b4daddfeed58706`.

Useful material:

- an MIT-licensed company-directory implementation;
- a public, versioned manifest;
- bounded `name,slug,url` company CSVs for five public ATS families.

Material Bluey deliberately did not reuse:

- scraper source code;
- full job snapshots;
- proxy or anti-bot behavior;
- browser-evasion logic;
- selectors or Submit behavior;
- any assertion that an external dataset is current employment truth.

The external directory is a lead only. A selected entry becomes a Bluey
public-ATS source, and all job content is subsequently fetched from and
revalidated against the original employer ATS. Dataset rights remain separate
from the repository's software license. Exact provenance, row counts, and
manifest hashes are recorded in `jobs/THIRD_PARTY_PROVENANCE.md`.

## Server Contract

`server/src/api/jobs_source_directory.rs` adds two authenticated operations,
while `server/src/api/jobs_source_directory_catalog.rs` isolates remote fetch,
checksum, URL grammar, parsing, and cache policy from the HTTP request layer:

- `GET /api/jobs/discovery/catalog`
  - requires an account-owned Career Track;
  - accepts a 2-80 character company query and an optional allowlisted ATS;
  - returns at most 24 ranked entries;
  - returns an opaque catalog ID, provider label, company name, and connected
    state, but never a raw source key or directory URL.
- `POST /api/jobs/discovery/sources`
  - revalidates account ownership of the Career Track;
  - resolves the opaque ID inside the server-held catalog;
  - converts it to the existing `DiscoverySourceInput` contract;
  - enrolls a four-hour source through the same database-enforced account,
    quota, and `(account_id, provider, source_key)` identity rules used by other
    discovery paths.

The directory reader has the following containment:

- fixed HTTPS host and `/jobhive/v1/` path prefix;
- no redirects, credentials, custom ports, query strings, or fragments;
- twelve-second request timeout;
- 256 KiB manifest and 2 MiB CSV streaming limits;
- exact descriptor path, byte length, SHA-256, row count, and CSV-header checks;
- exact URL grammar for each ATS family;
- a six-hour in-process cache and bounded stale-cache fallback;
- no catalog content in logs or error responses.

The fresh PostgreSQL schema now includes the unique source identity index that
the runtime migration already required. Schema parity and its negative fixture
were updated so a clean database cannot silently diverge from an upgraded one.

## Portal Experience

`jobs/portal/src/components/DiscoverySourceDialog.tsx` provides a compact,
keyboard-accessible source picker:

1. choose the Career Track;
2. optionally narrow by ATS;
3. search the company name;
4. select **Watch**;
5. see the connected state immediately.

The empty Matches view now offers **Watch companies** as its primary action and
keeps **Add job link** as the manual path. Copy states that Bluey verifies jobs
on the employer's original application system before showing them as matches.
It does not call the directory a job feed or imply that watching one company
searches the whole Internet.

Visual checks:

- `docs/rounds/assets/ROUND-552-JOBS-COMPANY-DIRECTORY-AND-SOURCE-ENROLLMENT/source-picker-desktop.png`
- `docs/rounds/assets/ROUND-552-JOBS-COMPANY-DIRECTORY-AND-SOURCE-ENROLLMENT/source-picker-mobile.png`

Both viewports had zero horizontal overflow. Search, empty, result, Watch,
connected, and close behavior were exercised in the browser.

## Bluey Browser Review

The existing Bluey Browser assets already include a coherent Bluey icon family
for macOS, Windows, and the controller shell. Its compact controller, tray,
background preference, intervention state, checkpoint recovery, and explicit
Quit behavior are covered by the Browser test suite.

The current copy is intentionally explicit:

- local background work stops when the computer is off;
- cloud execution may continue while the computer is off only when that
  separately operated product is enabled;
- the five ATS families remain beta/review paths;
- LinkedIn and Indeed remain user handoff paths.

No Browser artifact was repackaged or distributed in this round.

## Verification

Passed on the Round 552 worktree:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- complete server test matrix, including 718 unit tests and all HTTP/integration
  suites
- six focused source-directory validation and response-minimization tests
- `npm test` in `jobs/`: 389 tests across automation, Browser, runner,
  workflows, and portal
- Jobs workspace typecheck
- Jobs production build
- Jobs privacy gate
- Jobs PostgreSQL schema-parity gate
- CI guard self-test
- third-party provenance/license gate
- Bluey Jobs client-boundary gate
- Bluey edge-policy gate
- release hygiene scan
- `git diff --check`

## Deployment Boundary

The deployable changes are the server/Jobs API and static Jobs portal. Existing
discovery workers need no new trust or submission authority: a newly enrolled
source uses their current public-ATS lease/snapshot/commit path.

Before deployment:

1. take and verify a fresh PostgreSQL backup;
2. save the current Jobs API binary and Jobs portal entrypoint/assets;
3. build from the exact source commit;
4. confirm all active-work aggregates are zero;
5. confirm the positive upstream spend cap remains present;
6. confirm model generation and both Browser distribution flags are `0`.

After deployment, verify public and loopback health, authenticated workspace
load, unauthorized `401`, public internal-worker `404`, source catalog search,
source enrollment, portal asset hashes, service restart counts, and the three
disabled Jobs flags. Do not publish or replace the signed native `0.1.104`
release.

Rollback uses the paired pre-deploy database backup, Jobs API binary, and Jobs
portal snapshot. Once a new source has been enrolled after public ingress
reopens, prefer a fix-forward unless the database and binary are deliberately
rolled back as one maintenance unit.
