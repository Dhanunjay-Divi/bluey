# Round643 — Resume import normalization from observed private testing

Base00617b2c; branch `feat/phase-643-jobs-resume-import-normalization`.
Jobs only. No desktop/overlay/Pinky changes, real submissions, mailbox actions,
deployment, paid resource or provider call. Source resume stays outside Git.

## Observed defects

The real private test import identified the person/history and warned before
mixing two candidates. It nevertheless missed credentials under a combined
`AWARDS AND CERTIFICATION` heading and attached a second noncontiguous employment
period to an extracted job title. Do not put the candidate, resume, employer,
contact, school or exact personal chronology in committed fixtures or docs.

## Bounded acceptance

1. Recognize singular/plural combined awards/certification headings. Preserve
   credential lines while excluding obvious award/scholarship/prize narrative
   from the certification collection. Existing standalone sections still work.
2. A heading with two explicit noncontiguous employment intervals must not put
   the second interval in the title, fabricate continuous tenure across the gap,
   or silently lose the second period. Reuse existing data structures where
   possible; if the type cannot represent it safely, retain it for review and
   emit a clear warning. No schema/authority expansion without parent review.
3. Regression fixtures are newly authored synthetic text. Preserve employer,
   title and date facts; unknown education/location remains unknown. No inference
   from contact details, skill lists or awards into legal/credential authority.
4. Focused parser tests, TypeScript, full relevant portal/build/import gates,
   source/privacy review, then a small reviewed feature checkpoint/push. No
   historical pass or prepared command qualifies changed bytes.

## Ownership

Implementation worker owns the profile parser and its direct synthetic tests;
parent owns docs, integration and live observations. The worker is not alone
and must preserve others' changes. Source/claims remain unconfirmed until the
normal user review/save and server authority paths; import is not verification.
