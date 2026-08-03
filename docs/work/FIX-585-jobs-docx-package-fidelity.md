# FIX-585: Jobs DOCX Package Fidelity Verification

> **Codex preflight:** Loaded `$bluey-ops` and verified its application-kit,
> evidence, spending, feature-flag, and deployment boundaries against the
> current repository before implementation.

## Issue

Bluey patched `word/document.xml`, but did not prove after generation that the
rest of the source DOCX package and its ZIP metadata remained intact.

## Root Cause

`server/src/jobs_resume_template.rs` copied source entries into a new archive
without retaining their timestamps and performed no independent post-write
package comparison. It also accepted duplicate entry names and normalized
no-op rewrites.

## Fix Summary

- Read and validate the complete source DOCX package before patching.
- Preserve entry order, compression, timestamps, Unix mode and directory state.
- Reject duplicate names, incomplete packages and normalized no-op rewrites.
- Reopen every generated DOCX and compare all non-document parts byte for byte.
- Verify paragraph count, exact one-for-one rewrites and unchanged unrelated
  paragraphs.
- Exercise realistic styles, numbering, relationships, headers, footers, media,
  formatting runs, XML escaping, mixed compression and package metadata.

## Files Modified

| File | Change |
|------|--------|
| `server/src/jobs_resume_template.rs` | Package validation, metadata preservation, output verification and tests |

## Edge Cases Handled

- Duplicate ZIP names cannot shadow a package part.
- Missing content-types or document parts fail closed.
- Ambiguous, duplicate, absent, oversized and no-op rewrites fail closed.
- XML-sensitive replacement text remains ATS-readable after escaping.
- List/style/run structure survives a split-run bullet replacement.
- Non-document package parts remain byte-identical.

## How to Test

```bash
cd server
cargo test jobs_resume_template -- --nocapture
cargo test jobs_resume_generation -- --nocapture
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

## Known Limitations

- Changed text can reflow during Word rendering; representative visual
  certification remains required before calling the output pixel-identical.
