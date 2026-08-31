# FIX-780: Cloud preflight did not dereference its opened environment descriptor

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this correction against the exact
> Phase 622 branch and PR #35 Linux failure. No host, deployment, schedule, provider, database,
> production flag, or cohort state was changed.

## Issue

After FIX-779 passed the Ubuntu storage-guard step, PR #35 failed the next release-preflight
regression while loading a stable trusted environment file. The preflight reported the generic
closed error that the file or its parent chain was not trusted.

## Root Cause

`scripts/bluey-cloud-preflight.sh` correctly used non-dereferencing metadata for the trusted path,
opened that file on descriptor 9, and then reused the non-dereferencing inode helper for
`/dev/fd/9`. Linux represents `/dev/fd/9` as a procfs symbolic link, so GNU `stat` returned the
link inode instead of the opened file inode. The stable path and descriptor therefore differed.

FIX-779 corrected the same boundary in the four root storage scripts but did not include this
separate release-preflight loader.

## Fix Summary

- Keep every trusted path and parent-chain check non-following.
- Add a dedicated follow-only inode helper for the already-open descriptor.
- Make the portable preflight fixture reject descriptor identity calls that omit `stat -L`, then
  delegate all GNU/BSD forms to the platform's native `stat`.

This preserves substitution resistance: a symlink or changed path is still rejected, while the
descriptor comparison identifies the regular file object the process already holds.

## Files Modified

| File | Change |
|------|--------|
| `scripts/bluey-cloud-preflight.sh` | Dereference only descriptor 9 for opened-file identity |
| `ops/tests/test-bluey-cloud-preflight-disk-guard.sh` | Require explicit descriptor dereferencing on GNU and BSD hosts |
| `CHANGELOG.md` | Record the completed preflight portability correction |

## Verification

```bash
bash -n scripts/bluey-cloud-preflight.sh \
  ops/tests/test-bluey-cloud-preflight-disk-guard.sh
bash ops/tests/test-bluey-cloud-preflight-disk-guard.sh
bash ops/tests/test-bluey-storage-guards.sh
git diff --check
```

The exact corrected head must also pass the Ubuntu CI storage and release-preflight steps. This
fix does not implement the external restore dead-man provider and does not make production release
green by itself.
