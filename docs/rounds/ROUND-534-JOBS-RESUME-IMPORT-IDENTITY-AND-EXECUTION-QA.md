# Round 534 - Jobs resume import, identity, and execution QA

Date: 2026-07-16

Status: implementation and local verification complete; ready for scoped mainline
reconciliation and manual deployment

## Objective

Finish the interrupted Bluey Jobs candidate-data and execution work as one
coherent release. The release must import real-world DOCX and PDF resumes without
mixing candidate facts, make onboarding easy to correct, keep the verified
application identity separate from resume contact text, and guarantee that a
runner submits the exact packet the user approved.

This round does not change the Bluey meeting overlay, audio runtime, native
session behavior, or signed desktop installers.

## Resume import and onboarding

The portal now supports staged PDF, DOCX, and TXT import with a 10 MB file limit,
bounded page/text extraction, file-signature checks, and a review dialog before
candidate facts are applied.

The review offers two deliberate actions:

- replace the Career Profile with the imported candidate; or
- fill only blank fields when the import is the same person.

Bluey rejects a different-person merge. Replacing a different person clears old
candidate-specific address, authorization, sponsorship, salary, notice-period,
and reusable-answer data while preserving account-level controls.

The six-step onboarding flow has complete editors for identity, work history,
education, projects, qualifications, roles, locations, exclusions, work types,
and Career Tracks. A no-resume path reaches the same factual profile without
requiring a document.

Accessible typeahead suggestions cover roles, locations, companies, skills,
certifications, education locations, target roles and locations, exclusions,
employment types, and Career Tracks. Imported values augment the suggestion
catalog, and free-form answers remain supported.

## Parser hardening

DOCX conversion preserves paragraphs, bullets, manual line breaks, and table
cells. It retains contact details placed in table headers while dropping category
headings that would otherwise become skills. Employment parsing recognizes
parenthetical subheadings, keeps employer/title/location boundaries, and uses a
current role location as the current-location fallback without inventing missing
locations.

Two owner-authorized documents were exercised through the production parser and
actual browser upload UI. Neither document nor its personal contents are stored
in the repository.

- The healthcare DOCX produced five distinct work-history entries, one education
  entry, 41 skills, and five certifications. US and international employers,
  titles, locations, and dates remained separate.
- The engineering PDF produced three distinct work-history entries, two education
  entries, three projects, and 26 skills. Role locations stayed blank where the
  source did not provide them.

Temporary exact-file regression tests were deleted after verification. Sanitized
fixtures retain the structural cases in the committed test suite.

## Candidate identity boundary

The Career Profile email is now explicitly the contact address printed on a
resume. It is not silently treated as the identity used to submit an application.

The server creates a primary application identity from the authenticated,
verified Bluey account email. Additional application emails remain separately
verified identities with isolated browser profiles. Preparing an application
therefore cannot turn editable resume text into a trusted submission identity.

The email explanation is attached directly to its field in onboarding and the
profile editor, including at mobile widths.

## Candidate feedback and outcomes

Candidate decisions are stored as encrypted, tenant-scoped, append-only events:

- match approval, pass reason, and restore;
- application issue category and note; and
- interview, offer, rejection, or withdrawal outcome.

The Matches and Applications views render these events without replacing the
canonical application state or pretending that an inbox integration inferred an
employer outcome.

## Approved execution and receipts

Approval freezes a versioned application packet containing the selected identity,
job-specific resume, cover-letter state, final answers, verified claims, browser
profile, and canonical job evidence. A stable SHA-256 checksum binds that packet
to local and cloud execution.

Dispatch now rejects mutable, missing, unapproved, or mismatched packets. Answer-
bearing interventions require packet regeneration and approval; a content-neutral
final-review approval can resume the original frozen packet. Checkpoint restore
and workflow resume both verify the checksum.

Final receipt persistence verifies the exact identity, job, resume, answers,
claims, approved checksum, terminal lease, and evidence objects. Retries remain
idempotent and do not double-meter a committed packet.

## Verification

Automated Jobs packages:

| Package | Tests |
|---|---:|
| Automation | 133 |
| Browser | 34 |
| Runner | 50 |
| Workflows | 34 |
| Portal | 37 |
| Total | 288 |

All five TypeScript packages typechecked and built. The portal production bundle
completed successfully.

Server verification:

- 10 Jobs library tests passed;
- 15 Jobs HTTP integration tests passed;
- receipt partial-failure cleanup and atomic idempotency passed;
- cross-tenant identifiers remain indistinguishable from missing records;
- approval, worker replay defense, rate limits, leases, and side-effect-unknown
  recovery passed.

Browser and responsive QA:

- the owner-authorized DOCX and PDF were uploaded through real Chrome;
- staged review counts and extracted fields matched parser output;
- light and dark mobile layouts at 390 by 844 had zero horizontal overflow;
- the resume-email explanation sits directly below its input; and
- the onboarding card retained stable dimensions and readable controls.

## Files

Primary implementation areas:

- `jobs/portal/src/components/Onboarding.tsx`
- `jobs/portal/src/components/ResumeImportReview.tsx`
- `jobs/portal/src/components/CareerFields.tsx`
- `jobs/portal/src/lib/documents.ts`
- `jobs/portal/src/lib/profile-validation.ts`
- `jobs/portal/src/lib/candidate-events.ts`
- `jobs/portal/src/views/MatchesView.tsx`
- `jobs/portal/src/views/ApplicationsView.tsx`
- `jobs/portal/src/views/ResumeView.tsx`
- `jobs/portal/src/views/SettingsView.tsx`
- `jobs/automation/src/approved-execution.ts`
- `jobs/automation/src/receipts.ts`
- `jobs/runner/src/intervention-policy.ts`
- `jobs/workflows/src/intervention-policy.ts`
- `server/src/api/jobs.rs`
- `server/src/db/jobs.rs`
- `infra/postgres/server-runtime/005_jobs_candidate_events.sql`

## Launch boundary

This release is suitable for the staged, review-first Bluey Jobs beta. It does not
claim unattended public automation is certified on every employer site. Live ATS
certification, production cloud-browser operations, signed local browser delivery,
and Gmail/Outlook outcome workers remain operational launch gates and must stay
truthfully labeled in the product.
