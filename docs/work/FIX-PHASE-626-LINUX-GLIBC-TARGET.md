# FIX-PHASE-626: Match the Linux Linker to the Ubuntu OpenSSL ABI

> **Codex preflight:** `$bluey-ops` was loaded before diagnosis and the current
> repository state was checked against its local build and queue requirements.

## Issue

The first clean, immutable Linux cross-build of `bluey-server` at commit
`6a15d927e772830058a6143d271744f2b1e48ff2` failed during its final link step.
The pinned Ubuntu 24.04 OpenSSL libraries require glibc symbols introduced in
2.33, 2.34, and 2.38, including `__isoc23_strtol`. The base cargo-zigbuild
target selected an older glibc ABI and could not resolve those symbols.

No Linux artifact was retained from the failed build.

## Root Cause

`scripts/build-pinky-integration-linux.sh` passed only
`x86_64-unknown-linux-gnu` to cargo-zigbuild. That is the correct installed Rust
target, but without a glibc suffix cargo-zigbuild chooses an older compatibility
baseline. It did not match the pinned Ubuntu 24.04 OpenSSL archive, whose
SHA-256 remains:

`2d960d2b686783043678c6fa90f3669aa1dbf23c22c673b2e63545f93aa8505d`

## Fix Summary

The script now keeps `x86_64-unknown-linux-gnu` as the installed Rust target and
uses `x86_64-unknown-linux-gnu.2.39` as the cargo-zigbuild linker target. glibc
2.39 is the Ubuntu 24.04 baseline and therefore includes every symbol required
by the pinned OpenSSL libraries.

The preflight also asks Zig to resolve `x86_64-linux-gnu.2.39` and rejects the
build unless Zig reports a glibc 2.39 target. Zig and cargo-zigbuild caches now
live inside the owned temporary build root and are removed on every exit. The
retained manifest records the base Rust target, the versioned linker target,
and the minimum glibc version separately.

No undefined-symbol suppression, library substitution, or remote compilation
was added.

## Files Modified

| File | Change |
|------|--------|
| `scripts/build-pinky-integration-linux.sh` | Select glibc 2.39 explicitly, validate support, isolate Zig caches, and record both targets. |
| `docs/work/FIX-PHASE-626-LINUX-GLIBC-TARGET.md` | Record the failed build, root cause, correction, and remaining verification gate. |

## Edge Cases Handled

- The normal Rust target must still be installed; the version suffix is used
  only by cargo-zigbuild.
- Artifact lookup continues under Cargo's base-target directory.
- A Zig installation that cannot represent glibc 2.39 fails before fetching
  the sysroot or compiling.
- The pinned OpenSSL archive, strict SSH policy, source pin, clean-worktree
  gate, and ELF-format validation are unchanged.

## How to Test

```bash
bash -n scripts/build-pinky-integration-linux.sh
scripts/build-pinky-integration-linux.sh --self-test
scripts/build-pinky-integration-linux.sh --dry-run \
  --source-sha "$(git rev-parse HEAD)"

# After committing this correction and returning to a clean exact source pin:
scripts/build-pinky-integration-linux.sh \
  --source-sha "$(git rev-parse HEAD)"
```

The retained build is accepted only when the output is an x86-64 ELF binary,
its checksum matches `bluey-server.sha256`, and `manifest.json` records
`x86_64-unknown-linux-gnu.2.39` with minimum glibc `2.39`.

## Known Limitations

- The corrected retained build has not run yet. It must run from the clean
  commit containing this fix under the shared `mac-heavy` lock.
- The resulting binary intentionally targets Ubuntu 24.04/glibc 2.39 and is
  not claimed to run on older glibc distributions.
