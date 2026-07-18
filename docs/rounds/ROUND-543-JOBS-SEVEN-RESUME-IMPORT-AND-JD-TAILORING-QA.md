# Round 543: Seven-Resume Import And JD Tailoring QA

Date: 2026-07-18

## Objective

Exercise Bluey Jobs with seven owner-authorized PDF and DOCX resumes spanning
software engineering, data engineering, analytics, clinical research, cloud,
quality engineering, and AI work. Fix the import failures found in that corpus,
then carry a job through the real packet-preparation boundary to prove that Bluey
creates a separate, reviewable resume version for each job without inventing
candidate facts or allowing Review-first work to enter a runner.

The private source documents and their contents are not stored in this repository.
This document records only aggregate results and the product behavior needed to
reproduce the checks with synthetic fixtures.

## Resume import findings

The previous parser was too dependent on one heading order. It could treat a
university as a title, leave a company blank, split wrapped certification levels
into separate credentials, or mistake a school location for a candidate location.
It also lost useful contact preambles from some DOCX files because HTML conversion
and raw-text extraction preserve different parts of a document.

This round changes the import path so that:

- DOCX extraction merges a normalized contact preamble from raw text with the
  structured HTML body;
- contact inference reads the preamble instead of scanning education or experience
  headings for a name and location;
- company-first, title-first, and title/company-on-separate-line work histories are
  normalized into separate Company, Title, Location, and Date fields;
- repeated employer blocks and nested client assignments are merged without
  duplicating the candidate's employment;
- education parsing separates school, degree, field, location, and graduation date,
  including comma-style and international layouts;
- skill lists preserve commas inside parentheses, remove category-only labels, and
  repair common PDF line wraps;
- wrapped certification levels remain attached to their credential; and
- any missing name, location, role, company/title pair, or school/degree pair is
  presented as a review warning instead of being fabricated.

## Seven-document result matrix

| Corpus shape | Employment | Education | Skills/certifications | Review behavior |
| --- | --- | --- | --- | --- |
| PDF, Java/cloud engineering with repeated client blocks | deduplicated and separated | two schools separated | wrapped skills and certifications repaired | ready for review |
| DOCX, frontend/backend engineering | two roles separated | graduate degree separated | long skill and certification lists preserved | source contains no location; location warning shown |
| DOCX, data engineering with title-first blocks | two roles separated | graduate degree separated | category labels removed | source contains no detectable name; name warning shown |
| DOCX, clinical research with US and India roles | five roles separated | health-informatics degree separated | five certifications preserved | ready for review |
| DOCX, cloud/data/AI engineering | five roles separated | three schools separated | credentials preserved | source location retained exactly for user review |
| PDF, software/quality engineering with projects | four roles separated | two schools separated | projects and technology lists preserved | ready for review |
| PDF, analytics/AI with wrapped lists | four roles separated | two schools and locations separated | wrapped skill phrases repaired | ready for review |

All seven files preserve the source facts needed to build an editable Career
Profile. Bluey does not guess the two source fields that are genuinely absent.

## Job-specific resume behavior

Packet preparation now delegates to `server/src/db/jobs_tailoring.rs` instead of
growing the already-large Jobs database module. The tailoring contract is:

1. Normalize meaningful JD terms and common role/technology aliases.
2. Score only the candidate's existing skills, experience bullets, and projects.
3. Move the strongest matching evidence first while preserving the complete set of
   source facts.
4. In Factual mode, do not add claims or rewrite facts.
5. In Enhance mode, the headline or summary may emphasize only skills already in
   the Career Profile. Empty fields stay empty.
6. Store a new ResumeVersion for the canonical job and expose the real structural
   diff used by packet review.

The diff records skill emphasis, the experience evidence moved to the top, project
ordering, the evidence policy, and an empty `claims_added` list. It no longer leaks
internal fact identifiers into the public packet view, and the web renderer handles
nested before/after values instead of displaying `[object Object]`.

Tests prove that an AWS/PostgreSQL JD and a React JD produce different resume IDs
and move different existing bullets/projects to the top. They also prove that an
empty headline or summary is never filled with a fabricated employer or role claim.

## Review, identity, and Auto Apply boundary

- An imported resume email is contact data, not a verified application identity.
- Additional application identities require email verification and cannot belong to
  a different Bluey candidate.
- Approval freezes the identity, email, resume version, answer set, job, browser
  profile, and packet checksum.
- One candidate cannot bypass the active-company collision guard by choosing a
  different email, Career Track, or resume.
- `awaiting_review` applications never appear in local or cloud runner pickers.
- Unknown sites remain Review-only; LinkedIn and Indeed remain handoff surfaces;
  known ATS families remain beta-review unless their exact adapter version is
  certified.
- Metering remains idempotent and starts only at the existing committed-packet
  boundary.

No employer application was submitted during this round.

## Verification

```text
Jobs portal tests                 11 files, 61 tests passed with local corpus QA
Committed portal tests           10 files, 54 tests passed after corpus fixture removal
Jobs portal TypeScript check      passed
Jobs portal production build      passed, 2,282 modules transformed
Jobs database/policy tests        41 passed
Tailoring module tests            3 passed
Rust formatting                   passed
git diff --check                  passed
```

The local seven-resume audit fixture contained owner paths and parsed private data,
so it was used only for QA and removed before commit. Permanent parser tests use
synthetic resumes that reproduce every corrected layout.

## Deployment boundary

This is a Jobs portal and Jobs API release. It does not change the Bluey meeting
overlay, native audio, signed desktop installers, or answer runtime. Deployment
must build and publish the portal and `bluey-jobs-api` from the same reconciled main
commit, retain the existing crawler/origin protections, and verify the signed-in
import, packet review, route refresh, logout, and unauthenticated API boundary before
claiming completion.
