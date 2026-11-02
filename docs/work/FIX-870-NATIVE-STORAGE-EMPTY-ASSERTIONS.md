# FIX870 — Preserve native inventory emptiness assertions under strict Clippy

## Issue / root cause

PR39 exact-head Jobs CI41367721820 Darwin job125031348022 compiled the native
storage crate, then rejected two test assertions under `clippy::assert_is_empty`.
The toolchain now recommends direct collection equality rather than a Boolean
`is_empty` assertion. This is not a native storage or cleanup failure.

## Fix / files

Only `jobs/runner/native-storage/tests/native_storage.rs` changes: compare the
returned entries directly with the empty collection. Both assertions still
require zero entries and now provide the collection in failure diagnostics.
Native code, permissions, retention/deletion fences and lint policy are unchanged.

## Verification / limitations

Native-only10970 terminated0 after launcher self-test, formatting, strict
offline/locked all-target Clippy and14 tests/zero failed or ignored. Source,
status, module hash and empty index unchanged; fresh Cargo/root/parent/process
cleanup verified. Journal SHA256
`97e218d87c3731b8b3adfe9c9224e69f2a3813c556c1da6cd75c88c6d6e40ea6`.
Existing failed CI stays failed; no retry, addon qualification or publication
is inferred from the local native-library pass.
