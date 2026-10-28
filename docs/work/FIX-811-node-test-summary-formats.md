# FIX-811: Final Jobs gate accepted only TAP summary formatting

> **Codex preflight:** `bluey-ops`; current source and raw local output are authoritative.

## Issue

Workspace controller14411 exited1 at `node_test_count_mismatch`. npm test itself
exited0; the controller had not reached typechecks/builds/final browser inventory.

## Root Cause

The recipe expected Node's TAP `# pass N`, but installed Node24.1 emitted its
spec summary `ℹ pass N`. Raw output contains totals/pass8,7,17 and zero skips,
plus five Vitest totals804,219,308,321,387. No product test failure was observed;
the overall qualification remained failed, not silently relabeled green.

## Fix Summary

Recognize only the two anchored Node summary prefixes. Require exact tests and
pass vectors8/7/17 AND exact skip vector0/0/0; absent summaries, changed totals,
skips or unexpected workspace counts still fail. Retain strict five-workspace
Vitest totals and zero-skip validation. Do not force a reporter or change tests.

## Files Modified

| File | Change |
|------|--------|
| `docs/work/COMMAND-PHASE-636-FINAL-JOBS-GATES.md` | Strict TAP/spec summary parser. |

## Edge Cases Handled

- TAP and spec formatting have the same closed count contract.
- Missing summaries and nonzero skips cannot be mistaken for a zero value.
- Correct pass count with extra skipped tests fails both total/skip assertions.

## How to Test

Check both synthetic formats, missing/changed/skipped negatives and the actual
14411 raw output, then rerun the full Jobs recipe on unchanged product source.
Require all remaining typecheck/build/browser/source/cleanup gates, not only tests.

## Known Limitations

14411's760 hashes/manifest, HEAD and scoped status matched after terminal; root
and parent cleanup plus no process executable under the exact owned root were
verified. Its later browser-open-file check did not run, so no such proof is
inferred. Full Rust88910 remains1872/1872 with its own unchanged-source cleanup.
Eight pure parser checks passed: actual14411 output and both synthetic formats,
plus five missing/wrong/skipped negatives. Node syntax passed. Revised full
workspace qualification and final privacy/review remain pending.
