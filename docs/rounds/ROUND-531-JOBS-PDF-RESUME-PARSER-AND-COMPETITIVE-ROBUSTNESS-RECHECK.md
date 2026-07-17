# Round 531 - Jobs PDF Resume Parser and Competitive Robustness Recheck

Date: 2026-07-16

Status: implemented and verified locally; production deployment follows in a
separate round.

## Scope

This round validates the Bluey Jobs resume importer with an owner-supplied,
one-page, text-native PDF and strengthens the reusable parser without retaining
the private document or any personal data in the repository.

It also rechecks Bluey's current product contract against the five competitor
surfaces audited in Round 528:

- [Tsenta](https://tsenta.com/)
- [Massive](https://usemassive.com/)
- [Sorce](https://www.sorce.jobs/)
- [AIApply](https://aiapply.co/)
- [ApplyBlast](https://applyblast.com/)

## Private Acceptance File

The PDF was rendered and visually inspected before parsing. It contained:

- one contact/location row;
- a summary;
- two education records;
- three employment records;
- three projects; and
- grouped skill categories.

The file was passed through the same production path used by the browser:
`pdfjs-dist` text items, `pdfTextItemsToText`, and
`inferProfileFromResume`. A temporary private acceptance test was removed after
verification. No private filename, contact detail, resume text, screenshot, or
artifact is committed.

## Defects Found

The earlier parser worked for conventional DOCX layouts but the compact PDF
exposed several distinct failures:

| Area | Before |
|---|---|
| Employment | Three records found, but employers were blank and title/company text remained combined |
| Education | Two records found, but the first degree was empty and shifted to the second school |
| Skills | Grouped labels such as `AI/ML:` were treated as new section headings, resulting in zero skills |
| Location | A contact row separated by an em dash did not yield a current location |
| Portfolio | A bare GitHub URL was missed |
| Projects | Three names were found, but summaries were removed when no project URL existed |
| Bullets | PDF line wrapping split sentences and preserved end-of-line hyphenation |

## Implementation

`jobs/portal/src/lib/documents.ts` now:

- recognizes bare GitHub and GitLab portfolio URLs;
- keeps grouped skill labels inside the Skills section;
- separates compact `Title, Company` and `Title, specialty, Company` rows;
- scores a broader set of organization terms without depending on legal suffixes;
- recognizes en/em dashes as contact separators;
- pairs inline school/date rows with the following degree;
- joins wrapped project summaries and preserves summaries without URLs;
- joins wrapped employment bullets; and
- removes PDF line-break hyphenation when reconstructing a word.

`jobs/portal/src/lib/documents.test.ts` adds a fully fictional regression fixture
covering the same layout class. The fixture contains no owner data.

## Acceptance Result

After the fix, the exact private PDF produced:

| Area | Result |
|---|---|
| Employment | 3 of 3 employers, titles, dates, and bullets correctly separated |
| Education | 2 of 2 schools, degrees, and dates correctly paired |
| Projects | 3 of 3 names with summaries and technology lists retained |
| Skills | 26 grouped skills retained |
| Contact | Current location, LinkedIn, and GitHub retained |
| Certifications | 0, matching the source document |

This is a structured baseline, not an irreversible submission. Onboarding keeps
every extracted field editable and requires the user to proceed through the
review steps.

## Visual Verification

The onboarding preview was checked in both dark-theme desktop and a 390 x 844
mobile viewport. The import choices, source filename, extraction summary,
contact fields, and navigation remain readable without overlap or horizontal
overflow.

## Automated Verification

The complete Jobs JavaScript/TypeScript test matrix passed:

| Package | Tests |
|---|---:|
| Automation | 130 |
| Browser | 34 |
| Runner | 40 |
| Workflows | 23 |
| Portal | 22 |
| Total | 249 |

All Jobs packages typechecked, the production portal build passed, generated
assets contain no source maps, and `git diff --check` passed for the scoped
implementation.

## Competitive Verdict

Bluey is currently stronger than the publicly observable competitor surfaces in
several trust and data-contract areas:

- server-authoritative eligibility and hard-filter decisions;
- Review-first as the default boundary;
- job-specific resume versions instead of one generic resume;
- scoped Answer Memory and multiple application identities;
- same-candidate company collision protection across tracks and emails;
- explicit ATS capability labels; and
- typed receipts, evidence hashes, durable checkpoints, and
  `side_effect_unknown` recovery semantics.

It is not yet honest to claim that Bluey is more mature or robust end to end.
Competitors publicly claim broader live inventory, higher application volume,
and wider operating coverage. Bluey still needs production evidence for the
following P0 gates:

1. Freeze the exact approved answers, identity, resume, cover letter, job, and
   capability decision into one immutable execution packet.
2. Add durable dispatch acknowledgement so `queued` and `running` always mean a
   runner owns the work, with automatic recovery or re-credit when launch fails.
3. Operate at least one scheduled, sanctioned discovery source with canonical
   deduplication, stale/closed-job reconciliation, and source-health canaries.
4. Certify Greenhouse and Lever against representative tenant matrices before
   unattended submission, then certify each remaining ATS family separately.
5. Add OCR/layout confidence and claim-level review for scanned or unusually
   complex PDFs.

## Product Position

Bluey Jobs should remain a staged, Review-first beta. The current importer is now
solid for both the previously tested structured DOCX and this compact text-native
PDF class. Scanned PDFs and difficult multi-column layouts should fail clearly or
enter an OCR/confidence workflow rather than silently inventing fields.

The next engineering work should prioritize immutable packets, durable dispatch,
one proven discovery source, and provider certification. Additional visual
expansion or broad automation claims should follow those proofs.
