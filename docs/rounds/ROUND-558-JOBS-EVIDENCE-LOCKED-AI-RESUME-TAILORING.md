# Round 558: Evidence-Locked AI Resume Tailoring

Date: 2026-07-21

## Goal

Let Bluey tailor a resume to a specific job description without changing the
candidate's identity or history. The model may improve wording, ordering, and
emphasis, but the final packet must remain grounded in imported or confirmed
candidate evidence.

## Implemented Contract

- Each job receives a separate resume version.
- The model may reorder verified skills, employment entries, projects, and
  existing bullets for relevance.
- The model may rewrite an employment bullet only when it cites the original
  bullet first and may cite up to three additional bullets from the same role.
- A rewritten bullet is checked against its cited source text before it is
  accepted.
- Employers, titles, locations, dates, current-role state, education,
  certifications, projects, contact details, and target-job identity remain
  server-owned facts.
- Skills may be selected from the candidate profile, but an unsupported skill
  cannot be added.
- The final packet records a real before/after diff and the source bullet IDs
  used by each rewritten claim.
- Finalization revalidates the complete generated document before persistence.
  A model response cannot bypass the truth guard by returning plausible
  provenance metadata with altered content.
- The model-plan validator and persistence boundary now share the same semantic
  claim guard. Citing a real bullet is not enough: the rewritten text must also
  avoid new metrics, tools, skills, ownership, outcomes, or cross-role facts.

## Persistence Guard

`server/src/db/jobs/evidence.rs` now rejects generated documents that:

- change the candidate name, application email, phone, location, or links;
- change the target job, company, title, or location;
- add, remove, duplicate, or modify employment records;
- change an employer, title, role location, date, or current-role flag;
- add an unsupported skill;
- change education, certification, or project facts;
- rewrite a bullet without an exact same-role source mapping;
- omit or reuse an original bullet in a way that breaks one-to-one evidence
  coverage; or
- add unsupported top-level or nested target, contact, or employment fields.

The guard runs inside `finalize_prepared_application` before checksumming,
claim-evidence persistence, metering, or queue-state selection.

## Model Boundary

Model generation remains opt-in and defaults off. The production flag must
remain disabled until provider, spend, packet, and acceptance gates authorize a
separate rollout:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
```

Deterministic packet generation remains the fallback.

## Original Template Status

The current import pipeline extracts text from PDF and DOCX files and stores the
source file name, but it does not retain the original document bytes or layout
anchors. The current export pipeline creates a Bluey-owned DOCX/PDF layout.
Therefore this round preserves candidate facts and resume structure, but it does
not yet preserve the exact original visual template.

Exact DOCX template preservation needs a separate document pipeline:

1. Store the encrypted original DOCX object with a content hash.
2. Parse OOXML paragraphs, tables, styles, runs, headers, and bullet anchors.
3. Associate each editable bullet with a stable evidence and OOXML anchor.
4. Patch only approved bullet text while retaining styles, spacing, tables,
   margins, headers, and section ordering.
5. Render the result and reject overflow, clipping, missing fonts, or page-count
   regressions before exposing it to the user.
6. Keep the original, generated version, diff, and source evidence together in
   the application receipt.

Arbitrary PDFs cannot be edited losslessly as templates. PDF imports should be
converted once into an editable Bluey template, shown to the user for visual
approval, and then versioned. Bluey should never claim pixel-identical PDF
preservation without a verified editable source.

## Verification

The following checks pass:

```text
cargo test jobs_resume_generation --lib
  23 passed

cargo test evidence_tests --lib
  8 passed

cargo test resume_generation_finalization --lib
  5 passed

cargo test resume_generation --lib
  30 passed

cargo clippy --lib --tests -- -D warnings
  passed

git diff --check
  passed
```

Adversarial tests cover forged identity, employer, title, start date,
unsupported skill, uncited bullet changes, a cited rewrite that invents a new
technology and 99% metric, unknown nested fields, cross-role evidence, and
source IDs attached to the wrong rendered role. A valid same-role
evidence-backed rewrite continues to pass.

## Deployment

No production deployment, feature-flag change, commit, or push was performed in
this round.
