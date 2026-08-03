# Round 595 - Jobs Exact DOCX Package Fidelity

**Date:** 2026-08-03

**Branch:** `feat/phase-jobs-full-autonomy-20260802`

**Status:** Implemented and verified; managed generation remains disabled in production

## Outcome

Bluey now treats a tailored DOCX as a document package that must be proven
safe, not merely a ZIP file whose main XML happened to parse.

The source package is read once, validated, cloned, and patched only in
`word/document.xml`. Before Bluey returns the result, it reopens the generated
file and verifies that every non-document package part remains byte-identical
and retains its original order, compression mode, directory status, Unix
permission bits, and ZIP timestamp.

## Existing Application-Kit Contract

Round 590 already established the managed-generation boundary:

- bounded provider routing and deadlines;
- per-attempt cost holds and account/provider rate limits;
- allowance and upstream-spend reservations;
- job-specific idempotent output caching;
- evidence IDs on generated claims;
- rejection of unsupported factual claims;
- deterministic fallback; and
- full cover-letter review.

Bluey also stores `base_profile_fit` separately from
`tailored_packet_coverage`. Tailoring may improve packet coverage, but it does
not rewrite the candidate's underlying fit score.

## DOCX Contract

The exact-template patcher now:

1. rejects duplicate package entry names;
2. requires `[Content_Types].xml` and `word/document.xml`;
3. rejects rewrites that normalize to the same text as the source;
4. requires one unambiguous source paragraph per evidence-grounded rewrite;
5. preserves all source package metadata and non-document bytes;
6. reopens and validates the generated package;
7. proves paragraph count is unchanged; and
8. proves every unrelated paragraph remains unchanged while each selected
   source paragraph is replaced exactly once.

The realistic regression fixture includes relationships, core properties,
styles, numbering, header, footer, media, mixed compression, explicit modes
and timestamps, and a list paragraph split across formatted text runs.

## Verification

```text
DOCX template tests: 7 passed
Grounded generation tests: 29 passed
Full Jobs library tests: 265 passed
Rust strict Clippy: passed
Rust formatting: passed
git diff --check: passed
```

## Honest Boundary

The patcher preserves the original OOXML package and paragraph structure, but
changed wording can still reflow when Word renders the document. Broad visual
certification across representative customer resumes remains part of the final
document-quality gate.

No production feature flag changed in this round. Managed model generation,
local Browser distribution, cloud Browser distribution, and mailbox sync
remain disabled until their respective launch gates pass.
