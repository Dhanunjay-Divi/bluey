# FIX-779: Linux descriptor attestation did not dereference `/dev/fd`

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against the Phase 621/622
> worktree. No production host, schedule, storage object, deployment, or feature flag was changed.

## Issue

The first exact-tip Ubuntu CI run for PR #35 failed the storage-guard suite at the stable
installer-lock positive case even though the same suite passed on macOS. Follow-up review showed
that the failure exposed a production Linux descriptor-attestation defect as well as a test-shim
portability defect.

## Root Cause

`ops/tests/test-bluey-storage-guards.sh` generated a test-only `stat` shim that always invoked BSD
mode/inode forms. GNU `stat` does not interpret those arguments as BSD queries. More importantly,
the production backup, archive, disk-guard, and installer scripts used non-dereferencing `stat`
calls for already-open `/dev/fd/N` descriptors. Linux exposes those descriptor paths through procfs
symbolic links, so a stable regular environment file or installer lock could compare its real path
inode with the procfs-link inode and fail closed. macOS did not expose the defect because its
descriptor path resolved as the regular opened file.

The trusted path must remain non-dereferenced to detect path substitution. Only the already-open
descriptor must be dereferenced to attest the file object that the process actually holds.

## Fix Summary

Each root storage script now uses a separate follow-only identity helper for its already-open
environment descriptor while retaining non-dereferencing identity checks for the trusted path.
The installer likewise dereferences only descriptor 8 for its owner, mode, device, and inode
attestation; its lock path remains non-dereferenced.

The test shim now uses native GNU/BSD forms, requires descriptor calls to request dereference, and
preserves the real mode/inode used to distinguish the stable case from leaf, mode, and ancestor
substitutions. Every negative case also requires its intended rejection message, and a positive
fixture proves that all four root scripts accept one stable descriptor-attested environment file.

## Files Modified

| File | Change |
|------|--------|
| `ops/backup-bluey-db.sh` | Dereference only the already-open environment descriptor for identity comparison |
| `ops/archive-bluey-logs.sh` | Dereference only the already-open environment descriptor for identity comparison |
| `ops/bluey-disk-guard.sh` | Dereference only the already-open environment descriptor for identity comparison |
| `ops/install-bluey-log-guards.sh` | Dereference opened environment/lock descriptors while retaining non-following path checks |
| `ops/tests/test-bluey-storage-guards.sh` | Prove GNU/BSD descriptor semantics, stable positives, and exact substitution rejection reasons |
| `docs/work/FIX-779-storage-lock-test-stat-portability.md` | Record the CI defect, root cause, correction, and evidence |
| `CHANGELOG.md` | Record the Linux descriptor-attestation correction under Unreleased |

## Edge Cases Handled

- A stable regular lock retains its real mode and inode on Linux and macOS.
- A stable trusted environment file is accepted by backup, archive, guard, and installer checks.
- Leaf replacement, writable-mode replacement, and ancestor replacement still fail closed.
- Every negative case proves its expected rejection reason instead of accepting any nonzero exit.
- `/dev/fd` device differences do not hide real inode replacement.
- Trusted paths remain non-dereferenced, so swapping a checked path to a symlink cannot make path
  and opened-descriptor identity equivalent.
- Production lock ownership, mode, type, identity, and process-fence requirements are unchanged.

## How to Test

```bash
bash -n ops/backup-bluey-db.sh ops/archive-bluey-logs.sh \
  ops/bluey-disk-guard.sh ops/install-bluey-log-guards.sh \
  ops/tests/test-bluey-storage-guards.sh
bash ops/tests/test-bluey-storage-guards.sh
git diff --check
```

The local macOS rerun passed the full storage-guard suite. Exact GNU/Linux proof remains the
corrected PR #35 Ubuntu CI gate.

## Known Limitations

- This source correction does not activate production storage schedules or replace the remaining
  hosted backup, restore, alert-delivery, and rollback canaries.
