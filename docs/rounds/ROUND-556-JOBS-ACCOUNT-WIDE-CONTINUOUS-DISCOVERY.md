# Round 556: Jobs account-wide continuous discovery

Date: 2026-07-20

## Outcome

Bluey Jobs now turns the bounded curated-feed readers from Round 555 into one
managed, account-wide discovery source. A feed is fetched once for the account,
each candidate is assigned to its best matching Career Track, and the Matches
view can progressively render hundreds of relevant results without loading an
unbounded page.

Candidate feeds remain discovery evidence, not application authority. A
candidate must be re-read from its original employer page before Bluey can
prepare an application. Unknown or uncertified application systems remain
Review-only.

The three execution gates remain disabled:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Managed source lifecycle

Every account with at least one Career Track receives one managed source:

```text
provider: curated_feed
provider key: bluey-curated-v1
scope: account-wide
refresh interval: 4 hours with stable account jitter
retry: bounded exponential backoff, capped at 8x the normal interval
```

The source reads the four allowlisted feeds documented in Round 555. The worker
keeps a bounded five-minute shared snapshot, so nearby account leases reuse one
successful external read while retaining independent account matching and
persistence. It deduplicates by canonical original-employer URL and commits one
complete account snapshot. A partial feed failure fails the snapshot so that a
truncated fetch cannot be mistaken for authoritative removals.

Candidate records carry stable external IDs, feed provenance, source
membership, observed timestamps, and expiry state. When the last Career Track
is deleted, the managed source is removed as well.

## Career Track assignment

The discovery worker evaluates each candidate against the account's current
Career Tracks and assigns it to one best track using the same role-family and
token-overlap vocabulary used by current server scoring. Assignment does not
prematurely discard location mismatches: location, employment type,
engagement type, sponsorship, experience, freshness, and company protections
remain server-owned eligibility facts shown to the user.

This prevents account-wide feeds from being fetched separately for every track
while keeping Software Engineering, Data Engineering, Product, clinical, and
other tracks isolated in the portal.

## Revalidation and deduplication

Every curated candidate is persisted as:

```text
availability: unknown
verified_at: absent
requires_original_revalidation: true
capability: provider-derived or unknown_review
```

Selecting **Verify and prepare** first imports the original job URL through the
normal server verification path. Preparation continues only with the returned,
freshly verified canonical posting. Verification does not consume an
application allowance.

If the original posting already exists for the same account and canonical URL,
the verified source upgrades that record in place. It does not create a second
match beside the curated lead. A verified employer source always outranks
curated metadata.

## Matches experience

The Matches view now:

- reports relevant jobs rather than implying every feed row is verified;
- distinguishes **Lead · Verify first** from verified jobs;
- explains managed feeds and original-employer revalidation in Source health;
- shows candidates across their best Career Tracks;
- renders 50 rows initially and adds 50 at a time; and
- preserves layout at 125+ results on desktop and mobile.

A controlled QA scenario creates 125 matches across two tracks and mixes
verified postings with curated leads. It exists only under the preview query
and cannot create production account data.

## Jobhive and repository reuse boundary

The reviewed `kalil0321/ats-scrapers`/Jobhive snapshot contained more than four
million job records across 49 ATS families. Bluey does not download or process
that corpus once per user. Current reuse is limited to the bounded public
company/source directory already documented in provenance, which enrolls
original ATS boards for Bluey's own bounded readers.

If the full corpus is considered later, it must be a separately reviewed global
warehouse with dataset-rights provenance, object storage, incremental manifests,
canonical deduplication, and original-source revalidation before account fanout.
Anti-bot, stealth, proxy-bypass, CAPTCHA-bypass, and visual-only Submit code is
not imported.

## Portal and automation boundary

Current continuous coverage is:

| Source | Continuous discovery | Preparation / execution |
| --- | --- | --- |
| Greenhouse, Lever, Ashby, SmartRecruiters, Workday public boards | Enrolled scheduled sources | Review-first beta; Browser distribution disabled |
| Four curated public job feeds | One managed account-wide source | Original employer-page revalidation required |
| Jobhive company directory | Bounded source-enrollment hints | Original ATS reader only |
| Direct employer link | User import and verification | Certified/beta policy determines next step |
| LinkedIn, Indeed, ZipRecruiter, Dice, CareerBuilder | No access-control bypass | Preparation or user handoff only |
| Unknown ATS | Candidate lead allowed | Review-only until a verified adapter exists |

OmniParser remains an optional future observation aid for incomplete visual
surfaces. It cannot authorize or prove an irreversible employer-facing Submit.

## Verification

Passed during this round:

```text
npm test --prefix jobs
npm test --prefix jobs/portal
npm run typecheck --prefix jobs/portal
npm run build --prefix jobs/portal
cargo test --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml discovery
cargo test --manifest-path server/Cargo.toml concurrent_replay_payloads_commit_once_without_mixing_snapshots
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
git diff --check
```

Observed package coverage after the paging regression was added is 406 Jobs
tests: 157 automation, 100 Browser, 50 runner, 40 workflows, and 59 portal.
The full server assertions passed, including 76 HTTP integration tests. The
integration process required an interrupt after printing its complete passing
summary; it exited zero, so this is recorded as a test-harness shutdown quirk,
not an assertion failure.

Visual proof covered:

- 125 relevant matches at 1440 px with no page-level horizontal overflow;
- 50 to 100 incremental paging on desktop and 390 px mobile;
- 390 px body width equal to scroll width; and
- the mobile candidate dialog's explicit verification copy and
  **Verify and prepare** action.

## Remaining truth

This round does not claim universal portal scraping or universal unattended
submission. A production-wide global job index, licensed aggregators,
provider-specific certification across tenant variations, durable local/cloud
Browser distribution, and employer-facing execution recovery remain separate
gates. Jobs may be shown in the hundreds when they match, but no feed lead can
skip original-source verification, Career Track eligibility, packet review,
metering, or execution policy.
