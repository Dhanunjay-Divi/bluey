# FIX-656: Jobs Rust 1.97 CI gates

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The Phase 605 pull request passed locally on Rust 1.95 but failed the hosted
Jobs and observability workflows after GitHub advanced its rolling stable
toolchain to Rust 1.97.

## Root Cause

Hosted Rust 1.97 surfaced Clippy findings for one optional filename parser and
for portable Unix code whose `libc` aliases differ across Apple and Linux. The
hosted Linux runner sees `nlink_t` as `u64` and `mode_t` as `u32`, while Apple
uses narrower aliases, so the conversions are required on one supported target
and identity conversions on another. The Linux ACL implementation also keeps a
fallible signature solely to match the audited Darwin implementation.

## Fix Summary

Use `?` for the optional cover-letter filename prefix and document narrowly
scoped Clippy allowances for the cross-target Unix conversions and shared ACL
contract. Runtime behavior and fail-closed storage checks are unchanged.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/execution_leases.rs` | Simplify the optional cover-letter prefix branch for Rust 1.97. |
| `jobs/runner/native-storage/src/unix.rs` | Document the target-dependent aliases and shared fallible ACL contract. |
| `jobs/runner/native-storage/tests/native_storage.rs` | Document the same Apple/Linux file-type alias variance in the special-file test. |

## Edge Cases Handled

- Apple still widens its narrower `libc` aliases without truncation.
- Linux keeps the same validated no-extended-ACL policy boundary.
- Unknown final-submit filenames still return `None` without widening accepted
  document names.

## How to Test

```bash
cargo +1.97.0 fmt --manifest-path jobs/runner/native-storage/Cargo.toml -- --check
cargo +1.97.0 clippy --manifest-path jobs/runner/native-storage/Cargo.toml \
  --all-targets --locked -- -D warnings
cargo +1.97.0 clippy --manifest-path jobs/runner/native-storage/Cargo.toml \
  --target x86_64-unknown-linux-gnu --all-targets --locked -- -D warnings
cargo +1.97.0 test --manifest-path jobs/runner/native-storage/Cargo.toml \
  --all-targets --locked
cargo +1.97.0 build --manifest-path jobs/runner/native-storage/Cargo.toml \
  --release --locked
(cd server && cargo +1.97.0 fmt --all --check)
(cd server && cargo +1.97.0 clippy --all-targets -- -D warnings)
(cd server && cargo +1.97.0 test --lib final_submit)
```

## Known Limitations

- Cross-target Clippy proves the Linux type/lint surface locally; the hosted
  Ubuntu workflow remains authoritative for native execution and managed-image
  smoke coverage.
