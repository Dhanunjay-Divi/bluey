# FIX-695: Backport the Proven Rust 1.98 Compatibility Repair

## Issue

Pull request #32 runs `33313140450` and `33313140451` failed on macOS, Ubuntu, Windows, and the
observability job after the CI stable toolchain advanced to Rust 1.98. The first shared failure was
Clippy's `chunks_exact_to_as_chunks` diagnostic in `crates/cue-core/src/ipc_auth.rs`, promoted to an
error by `-D warnings`.

## Root Cause

The Phase 611 branch did not change the failing desktop/server files. The rolling stable toolchain
introduced compatibility diagnostics across eleven existing files, so fixing only the first line
would expose later failures. Current `main` retained the same patterns. The independent Phase 623
branch already carried a complete isolated repair in commit
`2f3910a1a71ac920d52c26247fd460cff24ce295`. The unchanged patch was subsequently green on all
four affected CI jobs at descendant pull-request head `83f15263`.

## Fix

- Backport exactly the complete eleven-file compatibility patch without importing unrelated
  Phase 623 UI work.
- Replace fixed-width byte chunk loops with explicit indexed or `as_chunks` handling.
- Preserve framer capacity after padded flush and add its regression test.
- Let system-audio integration polls honor their declared overall deadline under parallel load.
- Apply narrowly scoped `clippy::result_large_err` allowances where Axum handlers intentionally
  return the complete bounded error envelope.

## Evidence and Remaining Gate

The backported diff is exactly 53 insertions and 12 deletions across the same eleven files as
`2f3910a1`; `cargo fmt --manifest-path server/Cargo.toml -- --check` and `git diff --check` pass.
The unchanged patch's macOS, Ubuntu, Windows, and observability jobs are green at descendant
pull-request head `83f15263`, but Phase 611 still requires its own corrected exact-head CI run. No
production flag or runtime authority changes.
