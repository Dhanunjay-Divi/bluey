# Round 559: Global Ingestion And Complete Resume Export

Date: 2026-07-21

## Goal

Move Bluey Jobs from per-account feed duplication toward one shared candidate
index, and make every exported resume a complete, job-specific document rather
than a partial preview. Preserve the submission boundary: a broad feed can
suggest a job, but it cannot authorize an employer-facing action.

## Shared Candidate Ingestion

The new ingestion path reads the pinned Jobhive manifest once for the platform,
downloads immutable source-family artifacts into private staging, verifies each
artifact's declared byte length and SHA-256, and streams bounded row batches to
the private Jobs API.

The server stores candidate leads in a shared canonical index. A separate
account projection then applies Career Track role, location, engagement,
freshness, and experience constraints before creating an account-visible match.
It does not copy millions of rows into every account.

Implemented modules:

- `jobs/automation/src/jobhive-manifest.ts`
- `jobs/automation/src/jobhive-artifact.ts`
- `jobs/workflows/src/global-discovery-api.ts`
- `jobs/workflows/src/global-discovery-runtime.ts`
- `jobs/workflows/src/global-discovery-worker.ts`
- `server/src/db/jobs/global_discovery.rs`
- `server/src/db/jobs/global_discovery_completion.rs`
- `server/src/db/jobs/global_materialization.rs`
- `infra/postgres/server-runtime/012_jobs_global_candidate_index.sql`
- `infra/sqlite/server-runtime/035_jobs_global_candidate_index.sql`
- `ops/bluey-jobs-global-discovery.service.example`

## Verified Feed Scale

The manifest was fetched from its pinned HTTPS origin on 2026-07-21. Its
generated timestamp is 2026-07-20T14:30:05Z and reports:

| Measure | Count |
| --- | ---: |
| Source families | 49 |
| Non-empty source families | 47 |
| Canonical candidate leads | 4,458,802 |
| Raw source rows | 5,096,589 |
| Companies | 63,487 |

The largest source families include EURES, Bundesagentur, Workday,
SuccessFactors, Oracle, Beisen, SmartRecruiters, Greenhouse, Phenom, iCIMS,
Welcome to the Jungle, and Workable. These counts describe candidate leads,
not verified open applications.

## Source And Submission Matrix

| Source class | Discovery state | Submission state |
| --- | --- | --- |
| Greenhouse, Lever, Ashby, SmartRecruiters, Workday | Scheduled public ATS | Beta, Review first |
| Shared Jobhive source families | Shared ingestion foundation | Unknown/Review first until original-source verification |
| Simplify, PrepAI, Remote in Tech, Zapply | Curated candidate leads | Original employer page required |
| LinkedIn, Indeed, Glassdoor, Wellfound | Pasted-link handoff | User handoff |
| ZipRecruiter, Dice, CareerBuilder | Pasted-link review | Unknown/Review first |
| Staffing-company catalog | Candidate leads | Original employer or canonical ATS required |
| Unknown portal | No automatic authority | Review/handoff only |

Source metadata never grants submission authority. Before packet preparation,
queueing, or final Submit, Bluey must revalidate the canonical employer URL,
availability, source freshness, Career Track eligibility, application identity,
and certified adapter capability.

This lets Bluey keep a large stream of matching opportunities current without
pretending every source has the same application form. A source family expands
discovery coverage; only the original application system determines whether
Bluey may use a certified adapter, open a reviewed browser handoff, or ask the
user to finish the last step.

## Account Materialization

The server projection is bounded, idempotent, and account scoped. It:

- selects only candidate rows that can plausibly match a Career Track;
- excludes stale or closed rows;
- keeps external-feed rows unverified until the employer source succeeds;
- preserves an account's verified import rather than overwriting it with feed
  metadata;
- records the candidate source and canonical job identity for deduplication;
- never uses shared-feed inclusion to bypass Review first; and
- leaves unknown ATS families outside unattended submission.

## Evidence-Locked Resume Tailoring

The resume pipeline now supports a two-pass job-specific flow:

1. Extract required and preferred job-description responsibilities.
2. Build an evidence map from imported or user-confirmed profile facts.
3. Let the model reorder verified skills, roles, projects, and existing bullets.
4. Let the model rewrite a bullet only from cited evidence in the same role.
5. Reject new employers, titles, dates, metrics, tools, skills, seniority,
   ownership, or outcomes that are not supported by the evidence.
6. Render and persist the real before/after diff with source claim IDs.
7. Re-run the server truth guard immediately before final persistence.

The model instruction explicitly preserves candidate identity, employment,
education, projects, certifications, and chronology. It prioritizes required
responsibilities before preferred ones, keeps every source bullet represented
exactly once, and rejects keyword stuffing or copied job-description language.

Implemented modules:

- `server/src/api/jobs_resume_generation.rs`
- `server/src/api/jobs_resume_generation/tests.rs`
- `server/src/db/jobs/evidence.rs`
- `server/src/db/jobs/resume_truth.rs`
- `jobs/portal/src/lib/documents/export.ts`
- `jobs/portal/src/views/ResumeView.tsx`

Managed model generation remains opt-in and off by default:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
```

The deterministic evidence-backed packet generator remains the fallback.

## Complete Resume Export

DOCX and PDF export now include:

- name, contact details, and links;
- professional headline and summary;
- selected skills;
- complete employment history with location, dates, and bullets;
- projects;
- education; and
- certifications.

The portal keeps the resume usable on desktop and mobile. The mobile layout no
longer relies on a fixed-width paper surface and does not horizontally overflow
the 390-pixel verification viewport.

## Original Template Boundary

The current import stores extracted text and document metadata, not the original
DOCX/PDF bytes and layout anchors. Export therefore uses a Bluey-owned ATS-safe
template. It does not yet preserve an imported resume's exact visual template.

Exact DOCX template preservation needs encrypted original-object storage,
stable OOXML paragraph/table/style anchors, bullet-level patching, and render
validation. Arbitrary PDFs cannot be losslessly edited; they need a one-time
conversion to an editable approved template. Bluey must not claim exact visual
template preservation until that separate pipeline passes visual regression
tests.

The current job-specific export still preserves the candidate's fixed identity,
employers, titles, dates, education, certifications, projects, and chronology.
It changes ordering and evidence-backed bullet wording inside one stable Bluey
ATS template; it does not create a different employment history for each job.

## Verification

The completed source batch passes:

```text
Jobs package tests
  automation: 170 passed
  browser: 100 passed
  runner: 50 passed
  workflows: 46 passed
  portal: 62 passed
  total: 428 passed

Rust server
  743 passed

Portal
  TypeScript: passed
  production build: passed

Rust Clippy
  -D warnings: passed

Repository gates
  provenance/license: passed
  privacy: passed
  schema parity: passed
  client/server boundary: passed
  git diff --check: passed
```

Visual evidence:

- `docs/rounds/assets/round-559-resume-desktop.png`
- `docs/rounds/assets/round-559-resume-mobile.png`

The mobile viewport measured 390 CSS pixels for both body width and scroll
width. Resume heading, contact, summary, and document surfaces stayed within
the viewport, and browser console output was empty.

## Rollout Status

The account-wide curated-feed deployment from Round 557 remains the current
production discovery path. It exposes 805 canonical candidate leads from four
curated sources.

This round's multi-million-row shared ingestion foundation is not deployed. It
still needs the production staging filesystem, private signed worker service,
first-run migration, source-by-source canaries, account materialization
monitoring, and original-source revalidation metrics. Deploying only the reader
would not make global jobs safely searchable.

The local and cloud browser distribution flags also remain off. Broad discovery
does not mean universal auto-submit; each employer-facing action still requires
a certified deterministic adapter or a visible Review/handoff flow.
