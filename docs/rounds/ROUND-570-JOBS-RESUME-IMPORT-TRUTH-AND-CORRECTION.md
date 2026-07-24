# Round 570 - Jobs Resume Import Truth And Correction

Date: 2026-07-24

## Objective

Close the first launch gate from Round 569: make Bluey Jobs resume import
reliable enough that users can review a truthful Career Profile before job
matching, packet generation, or runner eligibility begins.

This round is intentionally limited to the browser-side PDF/DOCX import and
correction path. It does not enable model generation, local-browser
distribution, cloud-browser distribution, or employer-facing submission.

## Private Fixture Audit

Eight owner-provided resumes spanning PDF and DOCX were exercised through the
real portal parser. The fixtures covered:

- software engineering;
- data engineering;
- AI and machine learning;
- clinical research and clinical operations;
- multi-employer histories;
- projects, education, skills, and certifications;
- United States and international locations.

Private filenames, paths, candidate details, and extracted resume content were
used only for local verification. They were not copied into source control,
tests, screenshots, or this document. The temporary private-fixture audit
harness was removed after verification.

## Confirmed Defect

One DOCX exposed a deterministic employment-header error. After parsing a valid
employer and title, the header augmentation loop continued into the
responsibility prose and could promote a sentence into the company or title
field.

Example failure shape:

```text
Employer
Title, location, dates
Supported clinical teams across ...
```

The third line could previously be considered another header candidate.

## Implementation

### Narrative boundary

`jobs/portal/src/lib/documents/parser.ts` now uses one shared employment
narrative detector for:

- common resume action verbs;
- bullet lines;
- long sentence-like lines ending in punctuation.

Once employment responsibility prose begins, header augmentation stops. This
preserves the valid employer/title/location header and leaves narrative lines
as highlights.

The parser deliberately stops rather than skips. Skipping a narrative line and
continuing to scan could consume later internal project or subsection headings
as employer metadata.

### Correction warning

`jobs/portal/src/lib/documents/profile.ts` now flags sentence-like company or
title values in the existing resume review step. The user can correct the field
before importing the Career Profile.

This is a warning, not silent mutation. Bluey does not guess a replacement
employer or title from unrelated text.

### Sanitized regression coverage

`jobs/portal/src/lib/documents.test.ts` includes fictional regression fixtures
that verify:

- responsibility sentences remain highlights;
- company, title, location, and dates remain correctly separated;
- sentence-like company/title values produce a correction warning.

No owner resume content appears in the committed tests.

## Browser Verification

The actual Vite portal and browser import flow were exercised, not only parser
unit tests:

1. A text PDF produced the expected identity, location, three employment
   entries, two education entries, and three projects.
2. The affected DOCX produced all five expected employment entries after the
   fix, with employer, title, location, and dates in their correct fields.
3. The existing different-person warning remained visible when an imported
   resume identity differed from the active Career Profile.
4. The modal remained usable in the light theme with editable extracted fields
   and clear replacement behavior.

## Verification

```text
npm --prefix jobs/portal test -- src/lib/documents.test.ts
  29 passed

npm --prefix jobs/portal test
  73 passed across 11 files

npm --prefix jobs/portal run typecheck
  passed

npm --prefix jobs/portal run build
  passed

cargo test --manifest-path server/Cargo.toml jobs --lib
  223 passed

git diff --check
  passed
```

The portal build retained the existing large-chunk warning. This round did not
introduce a new runtime dependency or change the server payload contract.

## Truthful Boundary

This round verifies text-bearing PDF and DOCX resumes. It does not add OCR for
scanned or image-only documents. When no usable text can be extracted, Bluey
must ask the user for another file or manual entry rather than presenting an
empty profile as a successful import.

Extraction remains a draft until the user reviews it. No imported fact should
enter Auto eligibility merely because a parser produced a value.

## Deployment State

No production deployment, runtime flag change, service restart, commit, or
push was performed in this round.

The production safety flags remain unchanged:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Next Gate

The next launch gate should make Career Track setup authoritative and compact:

1. canonical role-family and alias selection;
2. complete location suggestions;
3. normalized certifications and skill review;
4. relevant non-overlapping experience calculation;
5. server-owned search pace and Auto eligibility defaults.

Matching, tailoring, and runner testing should begin only after those values
are visibly correct for the imported profile.
