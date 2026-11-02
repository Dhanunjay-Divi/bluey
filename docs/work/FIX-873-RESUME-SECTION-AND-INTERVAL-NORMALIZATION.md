# FIX-873: Resume section and noncontiguous interval normalization

Preflight: `bluey-ops`; current code and Round643 authoritative.

## Issue and root cause

An observed private import missed certifications under a combined singular
awards/certification heading. The section recognizer admitted only standalone
credential headings. Employment parsing extracted only the first explicit
date range on a line and left a subsequent range in the title.

## Fix summary / files

`jobs/portal/src/lib/documents/parser.ts` recognizes combined heading forms,
excludes award narrative before label removal, extracts all explicit ranges,
and emits separate existing employment records with shared employer/title
headers. Highlights belong to the final interval only; no gap is counted as
continuous tenure. Original source text remains available for user review.
`jobs/portal/src/lib/documents.test.ts` uses newly authored synthetic examples.

## Edge cases

Independent review found and requested correction of labelled awards losing
their labels before filtering and employer-below-title formatting losing the
employer on the first interval. Synthetic regressions cover both corrections,
plus employer-above formatting. Unknown education/location stays unknown.

## Verification

Actual current-source gate38567 terminated0:33 focused document tests and the
full Jobs regression passed, including both Chromium fixtures with zero skips.
All workspace typechecks/builds passed. Independent final review accepted the
corrected source. Scope was narrowed to employment multi-interval records;
education keeps first-range record behavior with its own regression. A mixed
credential/award list also retains the valid credential. See IMPL643 for pins.

## Limitations

Import proposes reviewable facts, not credential verification or server
application authority. No schema, employer-facing automation, authorization or
source resume is changed. No personal resume/contact/history enters fixtures.
