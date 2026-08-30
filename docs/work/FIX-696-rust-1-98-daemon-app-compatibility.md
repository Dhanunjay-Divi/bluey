# FIX-696: Complete Rust 1.98 Daemon Application Compatibility

## Issue

The first corrected exact-head pull-request run at `978ac4fe` reached Rust 1.98 Clippy and failed
the macOS, Ubuntu, Windows, and observability jobs on two remaining constant-width
`chunks_exact(2)` calls in `crates/cue-daemon/src/app.rs`. The first failures are recorded in Actions runs
`33322554768` and `33322554753`.

## Root Cause

FIX-695 faithfully backported the isolated eleven-file patch from `2f3910a1`, but that commit did
not include these two daemon-application sites. They were corrected later at descendant pull-request
head `83f15263`, whose macOS, Ubuntu, Windows, and observability jobs were green. Phase 611 therefore
needed the two additional source transformations rather than another warning allowance.

## Fix

- Decode PCM pairs through `as_chunks::<2>()`, assert the already-even remainder is empty, and
  retain the same little-endian sample calculation.
- Pair the already-even metadata-budget vector through `as_chunks::<2>()`, assert the remainder is
  empty, and retain one budget pair per retained context item.
- Import no other daemon, prompt, overlay, or Phase 623 changes from the descendant branch.

## Evidence and Remaining Gate

The two source hunks match the corresponding transformations at descendant head `83f15263`.
Repository search finds no remaining numeric-width `.chunks_exact(...)` call in Rust source.
`cargo +1.98.0 fmt --all -- --check` and `git diff --check` pass.

Independent review found no P0/P1/P2/P3 issue. The exact Phase 611 combined tip still requires a
fresh resource-capable run. No production flag, runtime authority, provider, deployment, or
customer state changes.
