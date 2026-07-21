# Round 562 - Jobs All-Source Rollout And Exact-Template AI Resumes

Date: 2026-07-21

## Scope

This round closes two product gaps without broadening employer-facing authority:

1. Bluey can plan a bounded ingestion rollout for every currently known Jobhive
   source family instead of relying on an unsafe implicit all-source start.
2. An imported DOCX remains the visual and factual authority while Bluey proposes
   evidence-backed, job-specific experience bullet rewrites.

This is discovery and document-generation hardening. It does not certify every
source for automatic submission, distribute Bluey Browser, or enable model
generation in production.

## Live Source Evidence

The live manifest was read on 2026-07-21 and reported:

| Measure | Result |
| --- | ---: |
| Source families | 49 |
| Non-empty source families planned | 47 |
| Empty source families deferred | 2 (`meta`, `wellfound`) |
| Unknown families quarantined | 0 |
| Raw rows represented | 4,418,578 |
| Bounded rollout waves | 10 |

`jobs/automation/src/jobhive-rollout.ts` classifies all 49 families into typed
ATS, ATS feed, direct employer, public service, or job board sources. The class
controls rollout order only. It never upgrades a source's submission capability
and never removes original-employer revalidation.

The planner keeps each normal wave at or below 500,000 rows and 2 GiB by
default. Workday and EURES exceed those limits and are isolated into dedicated
capacity-review waves. Unknown future source names are quarantined rather than
silently ingested.

The global worker now refuses to start unless
`BLUEY_JOBS_GLOBAL_DISCOVERY_SOURCE_FAMILIES` contains an explicit approved
allowlist. This prevents an operator typo from starting a multi-million-row
rollout. The systemd example documents the same requirement.

### Coverage boundary

The broad feed can provide candidate leads from 47 non-empty source families.
It does not mean Bluey can submit to 47 ATS families. Every lead still requires:

1. canonical deduplication;
2. original URL, open-state, freshness, employer, and job-fact revalidation;
3. account and Career Track eligibility evaluation;
4. a certified application adapter or visible review/handoff route; and
5. an immutable packet and side-effect-safe receipt before Submit.

OmniParser can parse a screenshot into candidate interactive regions and may
be useful as a review-mode observation aid. It is not submission authority and
cannot prove that a job is real, a field is correct, or Submit was committed
exactly once. Bluey therefore keeps deterministic adapters and transactional
guards authoritative.

Primary references reviewed:

- https://github.com/kalil0321/ats-scrapers
- https://github.com/microsoft/OmniParser
- https://github.com/SimplifyJobs/New-Grad-Positions
- https://github.com/PrepAIJobs/Summer2026-Internships
- https://github.com/PrepAIJobs/New-Grad-2026
- https://github.com/remoteintech/remote-jobs
- https://github.com/zapplyjobs/New-Grad-Jobs-2027

The curated GitHub lists remain candidate feeds with source provenance and
original-employer revalidation. A repository row is never treated as employer
submission truth.

## Employment And Engagement Categories

The existing Bluey contract covers:

- employment: full-time, part-time, contract, temporary, and internship;
- engagement: W-2, C2C, 1099, and direct hire.

Discovery normalizes explicit source text to canonical values. Preferences and
Career Tracks enforce those values server-side. A contract with unknown
engagement terms is review-only when the user selected a specific engagement
type; Bluey does not guess.

Relevant implementation:

- `jobs/portal/src/components/JobCategoryChoices.tsx`
- `jobs/automation/src/curated-feeds.ts`
- `jobs/automation/src/public-ats.ts`
- `jobs/workflows/src/global-discovery-runtime.ts`
- `server/src/db/jobs/eligibility.rs`
- `server/src/db/jobs/candidate_policy.rs`

## Exact-Template AI Resume Policy

For a Career Profile with `source_resume_template_status = exact_docx`, the
server now treats the imported Word document as the layout authority.

Bluey preserves:

- headline and summary;
- contact details;
- all employers, titles, locations, and dates;
- section, employer, bullet, project, and education order;
- the complete original skills and certifications; and
- the original DOCX template and styling.

The model may propose only same-role experience bullet rewrites. Each accepted
rewrite must cite the original bullet evidence ID first and can cite only other
highlights from the same employment entry. Existing truth guards still reject
new metrics, unrelated skills, cross-role facts, and unsupported claims.

The server ignores model-requested reordering or skill selection in exact-DOCX
mode before validation and materialization. The visible diff reports only real
experience rewrites plus the layout policy; it no longer labels an unchanged
source order as AI emphasis.

PDF imports continue to use Bluey's ATS layout because a PDF is not an editable
Word template. The factual invariants remain the same.

## Implementation

### Discovery rollout

- `jobs/automation/src/jobhive-rollout.ts`
  - classifies all known manifest sources;
  - defers empty sources;
  - quarantines unknown sources;
  - creates deterministic bounded waves;
  - preserves submission capability and revalidation requirements.
- `jobs/workflows/src/global-discovery-worker.ts`
  - fails closed without an explicit source allowlist.
- `ops/bluey-jobs-global-discovery.service.example`
  - documents the reviewed-wave requirement.

### Resume generation

- `server/src/api/jobs_resume_generation.rs`
  - adds `layout_policy` to model input;
  - locks exact-DOCX plans to source order and complete skills;
  - keeps same-role evidence-backed rewrites;
  - reports truthful layout provenance and diff output.

## Verification

Verification completed:

```text
Server library tests             754 passed
Jobs package tests               440 passed
  Automation                     177 passed
  Browser                        100 passed
  Runner                          50 passed
  Workflows                       51 passed
  Portal                          62 passed
All Jobs TypeScript checks        passed
All Jobs production builds        passed
Live manifest                    49 total / 47 planned / 0 unknown
Scoped git diff check             passed
```

The resume policy in this round is intentionally delivered with the source-DOCX
asset storage, integrity checks, exact-template export path, and portal wiring.
That complete feature slice passes the full server library and Jobs package
test matrices. Production activation remains separate from source integration:
the model and local/cloud Browser distribution flags stay disabled until their
independent rollout gates pass.

## Rollout And Release Gates

No production runtime is deployed by this round. A correct release sequence is:

1. commit and verify the source-DOCX asset pipeline and this policy together;
2. run the complete Rust, portal, automation, workflow, privacy, schema-parity,
   provenance, and client/server-boundary suites;
3. run a pre-production wave with row-count, hash, URL, freshness, dedupe,
   revalidation, projection, latency, and error evidence;
4. expand one reviewed wave at a time;
5. keep unknown and uncertified application systems review-only; and
6. keep model, local Browser distribution, and cloud Browser distribution
   flags disabled until their independent gates pass.

## Product Truth

Bluey can continuously surface hundreds of ranked matches without copying a
multi-million-row corpus into each account. Broad discovery is now explicitly
plannable for every known non-empty source family. Universal unattended
submission is not complete: site discovery, application preparation, and a
certified employer-facing Submit action are separate capabilities and remain
separately gated.
