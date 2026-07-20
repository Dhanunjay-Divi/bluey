# Round 551 - Career Track, job category, source, and visual fallback P0

Status: implemented and verified on the scoped Jobs branch; execution flags remain off.

Date: 2026-07-19

## Objective

This round turns the owner's role, experience, employment-category, staffing-source,
public-feed, and OmniParser requests into a fail-closed Bluey Jobs foundation.
It does not claim that every listed site is already certified for unattended
submission. Discovery evidence, candidate eligibility, packet evidence, and final
submission authority remain separate decisions.

The round started from `origin/main` commit
`cbbe83baaf3b7f4562572b77f28a6122e3cfbc2d`. It preserves these production gates:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Implemented

### Career Track and experience policy

- Relevant employment months are calculated within the selected Career Track's role
  family and overlapping periods are counted once.
- Required experience, preferred experience, and title seniority are evaluated as
  separate dimensions.
- Bluey's default search window is one year below through two years above the
  candidate's role-relevant experience, while the independent seniority guard can
  still reject a title such as Staff or Principal.
- Identity and resume selection are frozen to the Career Track/application binding.
- Employment type, engagement type, work authorization, sponsorship, role family,
  location, freshness, company collision, daily allowance, and ATS capability remain
  server-authoritative eligibility inputs.
- Unknown employment or engagement categories require review instead of being
  guessed into Auto-submit eligibility.

Primary code:

- `server/src/db/jobs/candidate_policy.rs`
- `server/src/db/jobs/eligibility.rs`
- `server/src/db/jobs/applications.rs`
- `server/src/db/jobs/execution_authority.rs`
- `server/src/db/jobs/local_runner.rs`

### Candidate evidence and truthful tailoring

- Profile evidence is snapshotted into immutable, versioned evidence revisions.
- Tailored claims store stable claim IDs and evidence revision IDs.
- Base profile fit and tailored packet coverage are separate values.
- Bluey may strengthen, reorder, and align supported material, but unsupported
  employers, titles, dates, years of experience, or skills cannot become confirmed
  claims.
- Eligibility and evidence are rechecked when submission authority is claimed and
  immediately before the irreversible Submit boundary.

Primary code:

- `infra/postgres/server-runtime/011_jobs_candidate_evidence.sql`
- `server/src/db/jobs/evidence.rs`
- `server/src/db/jobs/profile_postings.rs`
- `server/src/db/jobs/execution_leases.rs`

### Job categories

The product now normalizes these employment categories:

```text
full_time, part_time, contract, temporary, internship,
apprenticeship, seasonal, per_diem
```

It independently normalizes these engagement categories:

```text
w2, c2c, 1099, direct_hire
```

That distinction prevents a contract opening from being treated as automatically
acceptable when the candidate selected only W-2, or a full-time track from silently
accepting an internship. The choices appear in onboarding and Settings in both themes.

Primary code:

- `jobs/portal/src/components/JobCategoryChoices.tsx`
- `jobs/portal/src/components/Onboarding.tsx`
- `jobs/portal/src/views/SettingsView.tsx`
- `jobs/automation/src/public-ats.ts`

### Scheduled public ATS discovery

The shared reader and workflow now support five pinned public ATS families:

| ATS family | Scheduled reader | Category normalization | Submission state |
| --- | --- | --- | --- |
| Greenhouse | Implemented | Implemented | Beta review |
| Lever | Implemented | Implemented | Beta review |
| Ashby | Implemented | Implemented | Beta review |
| SmartRecruiters | Implemented | Implemented | Beta review |
| Workday | Implemented with bounded pagination | Implemented | Beta review |

Readers are pinned to provider-owned hosts, enforce size and pagination bounds, do not
follow redirects, reject malformed rows, and fail an incomplete snapshot rather than
publishing false job availability.

Primary code:

- `jobs/automation/src/public-ats.ts`
- `jobs/workflows/src/discovery-provider.ts`
- `jobs/workflows/src/discovery-runtime.ts`
- `server/src/db/jobs/discovery.rs`

### Staffing companies, portals, and curated public feeds

`jobs/automation/src/source-catalog.ts` records the supplied staffing-company names,
aliases, and known domains. It also records the supplied Simplify, PrepAIJobs,
Remote-in-Tech, and Zapply GitHub sources.

These entries are candidate leads, not employer-facing submission authority. A row must
resolve to a current canonical employer or supported ATS page before Bluey can prepare,
meter, queue, or submit it. This prevents stale repository rows, recruiter reposts,
duplicate companies, and fabricated openings from entering automation.

| Source class | Current behavior |
| --- | --- |
| Greenhouse, Lever, Ashby, SmartRecruiters, Workday | Scheduled typed public reader |
| LinkedIn, Indeed | Pasted-link preparation and user handoff |
| ZipRecruiter, Dice, CareerBuilder | Pasted-link/unknown-review |
| Supplied staffing-company sites | Cataloged candidate leads; canonical revalidation required |
| Supplied public GitHub lists | Cataloged candidate leads; original application URL revalidation required |
| Unknown source | Review-only; never assumed Auto-submit capable |

The source catalog intentionally does not import anti-bot evasion, stealth browsing,
proxy bypass, or third-party scraping code.

### Private visual observation seam

`jobs/automation/src/visual-observation.ts` provides a Bluey-owned, optional boundary for
screen parsers such as OmniParser when a form's DOM is incomplete.

The boundary:

- is disabled unless explicitly enabled;
- accepts only a fixed loopback HTTP or authenticated HTTPS endpoint;
- strips URL query strings, fragments, and credentials;
- bounds screenshot, response, timeout, and observation counts;
- rejects redirects and malformed schemas;
- accepts only high-confidence observations that uniquely bind back to a real DOM
  control; and
- has no click primitive and no Submit phase.

No OmniParser code, weights, or runtime were copied. The reviewed OmniParser repository
currently has licensing/runtime ambiguity: the root license is CC BY 4.0, some newer
detector material is described as MIT, older Ultralytics detector dependencies can be
AGPL, and the public project exposes a demo rather than a stable production service
contract. Bluey can connect a separately reviewed private service later without making
vision output the authority for irreversible actions.

## Portal verification

The settings and category controls were checked in the actual Vite portal at desktop
and mobile sizes, in dark and light themes.

- Desktop dark settings: [settings-dark-desktop.png](ROUND-551-JOBS-CAREER-TRACK-CATEGORY-SOURCE-AND-VISUAL-FALLBACK-assets/settings-dark-desktop.png)
- Desktop dark categories: [categories-dark-desktop.png](ROUND-551-JOBS-CAREER-TRACK-CATEGORY-SOURCE-AND-VISUAL-FALLBACK-assets/categories-dark-desktop.png)
- Mobile light settings: [settings-light-mobile.png](ROUND-551-JOBS-CAREER-TRACK-CATEGORY-SOURCE-AND-VISUAL-FALLBACK-assets/settings-light-mobile.png)
- Mobile light categories: [categories-light-mobile.png](ROUND-551-JOBS-CAREER-TRACK-CATEGORY-SOURCE-AND-VISUAL-FALLBACK-assets/categories-light-mobile.png)

At 390 by 844 CSS pixels, `scrollWidth` remained 390. No horizontal overflow or browser
console errors were observed. Category controls wrap without changing the page width.

## Verification

Passed:

```text
npm ci
npm test
npm run typecheck
npm run build
cargo test --workspace
cargo test --manifest-path server/Cargo.toml
cargo clippy --workspace --all-targets -- -D warnings
```

The Jobs package matrix passed 389 tests:

```text
automation  150
browser      95
runner       50
workflows    38
portal       56
```

The server integration suite passed 76 tests, followed by the free/Pro/Cloud runner
entitlement matrix and PostgreSQL usage-reservation schema test. The broader workspace
and server unit/doc suites also passed.

Focused coverage proves:

- role-family experience excludes unrelated work and merges overlaps;
- required and preferred experience do not bypass title seniority;
- known category mismatches block and unknown categories require review;
- all five scheduled ATS sources map only to official provider endpoints;
- malformed, capped, redirected, oversized, and incomplete source snapshots fail closed;
- source aliases/domains deduplicate the supplied catalog;
- visual observations cannot expose a Submit phase or click primitive; and
- profile evidence and claim bindings remain tenant-scoped and immutable.

## Honest completion boundary

Completed in this round:

- Career Track role/experience/category policy;
- evidence-bound tailoring authority;
- five scheduled public ATS readers;
- the supplied staffing/GitHub source catalog;
- canonical revalidation boundaries;
- private visual-observation interface;
- responsive Jobs settings UX; and
- full local test/build verification.

Not represented as complete:

- a production OmniParser service or bundled model;
- continuous ingestion adapters for every curated GitHub/staffing source;
- certified unattended Submit behavior for every listed company or portal;
- distributed local/cloud Browser execution; or
- LinkedIn, Indeed, ZipRecruiter, Dice, and CareerBuilder unattended submission.

Those require provider-specific certification, durable browser recovery, direct posting
revalidation, source rights review, and live sandbox evidence. Until those gates pass,
Bluey keeps unknown and uncertified paths in review or handoff mode rather than risking
duplicate, stale, fake, or misrepresented applications.
