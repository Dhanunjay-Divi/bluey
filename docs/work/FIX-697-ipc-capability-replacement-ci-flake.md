# FIX-697: Bound the IPC Capability Replacement CI Test

> **Codex preflight:** Loaded `$bluey-ops` and verified its memory against the Phase 611
> worktree, current round contract, and exact CI failure. No production, deployment, release flag,
> credential, or hosted runtime state was changed.

## Issue

The Ubuntu workspace job on the Phase 611 documentation-only head intermittently failed
`capability_replacement_never_exposes_a_partial_record` while the test published 100 alternating
capability records against one reader. The opener correctly rejected a zero-link inode, exhausted
its bounded retries during the artificial continuous rewrite, and made the test fail.

## Root Cause

`publish_ipc_capability` performs one atomic rename when a daemon boot publishes its capability.
The stress test instead modeled an unbounded sequence of boot replacements. A reader can safely
reopen after one rename, but no finite retry policy can guarantee success while another thread
continuously replaces the path. Treating that artificial load as a required successful read made
the test timing-dependent and contradicted the deliberately bounded, fail-closed production
contract.

## Fix Summary

- Model the real boot-scoped transition with one atomic replacement while concurrent readers prove
  that every successful record is complete and the final record is the second boot's record.
- Keep the deterministic zero-link reopen test and hard-link rejection test unchanged.
- Add a deterministic hook test that replaces the capability after all four allowed opens and
  proves the reader stops with link-count validation failure.
- Keep the production retry constant, opener, and validation logic byte-for-byte unchanged. A
  zero-link inode is never accepted, retries remain bounded at three, and hard links remain denied.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-core/src/ipc_auth.rs` | Correct the replacement model and add bounded-failure coverage |
| `docs/work/FIX-697-ipc-capability-replacement-ci-flake.md` | Record cause, correction, and scope |
| `CHANGELOG.md` | Record the CI-test correction under Unreleased |

## Edge Cases Handled

- The first open racing one rename reopens the stable replacement.
- Four consecutive zero-link opens fail after exactly the configured bounded attempts.
- Owner-only mode, owner identity, regular-file type, symlink, and hard-link checks remain intact.

## How to Test

```bash
CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test -p cue-core ipc_auth::tests::capability_reader -- --test-threads=1
CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test -p cue-core \
  ipc_auth::tests::one_boot_scoped_capability_replacement_never_exposes_a_partial_record \
  -- --test-threads=1
cargo fmt --all -- --check
git -P diff --check
```

## Known Limitations

- The focused Unix tests exercise the inode-link race on Unix. Windows retains its separate
  owner-only handle and replacement implementation and is unchanged by this fix.
