# IMPL: Phase 625 recovery checkpoint

> Codex preflight: read `$bluey-ops` and the exact recovery handoff. This is a
> preservation checkpoint, not a completed implementation or release candidate.

## Scope

Recovered 79 chronological literal patches and separately verified source
snapshots into a managed checkout. Preserved the missing-checkout journal data
privately, with input hashes, actual output-UUID timestamps, and before/after
file hashes. Did not execute historical JavaScript/shell, edit Bluey Jobs,
touch the dirty canonical checkout, merge a PR, or deploy.

## Files and ownership

The checkpoint includes shared HTTP-client changes, core IPC/observability
boundaries, partial daemon/dashboard changes, partial native sign-in/styles,
server output shaping/latency/auth changes, and browser sign-in presentation.
The Git diff is the exact inventory. Root was the only product-source writer;
supporting agents inspected evidence and validated captured diffs.

## Verification

- Rustfmt 1.98: both root and server workspaces formatted and checked.
- `git diff --check`: passed.
- The release artifact scanner self-test passed (2 clean + 8 rejection cases).
- Broad source hygiene scanning also found existing test/development strings
  in unchanged baseline lines; this is not a clean broad-scan result.
- No reconstructed-tree full build, Clippy, test, UI smoke, or native runtime
  certification has passed. Do not reuse historical test counts as current.

## Deviations and follow-ups

The temporary checkout was missing and original mutation records do not cover
all of its dirty source. Recovery stops at a missing Windows account-state
predecessor. The handoff documents exact next source evidence, unresolved
files, replay/formatting order, six parent P1 findings, current CI failures,
and separate deployment gates. Resolve those before final implementation review.
