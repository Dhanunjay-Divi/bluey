# FIX-785: Bluey test artifact growth

> **Codex preflight:** Loaded `$bluey-ops` and `$codex-storage-archive`, reconciled their guidance
> with the clean Phase 625 worktree, and did not inspect or use the SSD archive.

## Issue

Repeated Bluey local tests were reported to recreate approximately 7–20 GiB per hour. A failed or
interrupted verification could leave enough persistent build and temporary database state to
recreate the same root-disk pressure on the next phase.

## Root Cause

Local Rust commands normally use checkout-relative Cargo target directories. Multiple large
worktrees and repeated dev/test builds therefore retain separate compiled dependency graphs,
incremental state, and debug-heavy artifacts. Tests use operating-system temporary locations for
SQLite and related files, but there was no repository launcher that owned those paths together or
removed them after every command outcome. Shell failure and ordinary signals had no common cleanup
contract; SIGKILL and host crashes had no marker-bound later recovery.

The Jobs hosted lane already disables incremental/debug-heavy Cargo output and reclaims selected
ephemeral state, but those controls did not protect arbitrary local commands.

## Fix Summary

Add canonical `scripts/run-bluey-tests.sh` with `--self-test`, `all`, and direct-command modes. It
confines Cargo, temporary, primary SQLite, data, config, runtime, and log paths to one private
local `/tmp` root; forces SQLite; removes inherited PostgreSQL URLs; disables incremental/dev-test
debug bloat; supervises a distinct command process group; and runs every mode from the physical
repository root.

A 60-second, marker-bound next-launch reaper handles eligible SIGKILL/crash leftovers. It refuses
symlinks, unowned or malformed paths and markers, broad/repository/worktree/Downloads/Volumes
parents, young/future-dated candidates, and any run whose launcher or process group is active. It
revalidates immediately before deletion. Current cleanup checks removal status and exact absence;
signals are deferred through `mkdir` ownership; and marker establishment makes later missing or
corrupt marker state fail closed. Failure turns child success into status 74 without hiding a
nonzero/signal result.

## Files Modified

| File | Change |
|---|---|
| `scripts/run-bluey-tests.sh` | Add the canonical launcher, process-group supervisor, reaper, and embedded self-test. |
| `.github/workflows/jobs-ci.yml` | Run the lightweight launcher self-test before expensive Jobs gates. |
| `jobs/scripts/ci-guards-self-test.mjs` | Require the CI wiring and critical disk/signal/reaper authority. |
| `AGENTS.md`, `README.md` | Make the launcher the documented local test/check path and state the SIGKILL boundary. |
| `CHANGELOG.md`, Round 625, Phase 625 IMPL | Record the incident, correction, evidence, and limitations. |

## Edge Cases Handled

- Success, arbitrary nonzero exit, HUP, INT, and TERM preserve their defined status unless a
  successful child cannot be cleaned, which returns explicit status 74.
- A distinct process group contains ordinary descendants; signal and normal cleanup stop and wait
  for that group, while the stale reaper retains any root bound to a live group.
- Cleanup safely removes an owned early unmarked root, but after marker establishment refuses a
  replaced, linked, unowned, absent, malformed, or marker-mismatched current root and leaves it
  for manual audit rather than risking unrelated data.
- The stale reaper checks both launcher PID and process group twice and skips permission-ambiguous
  live state conservatively, including any `ps` failure. It ignores a group only when every
  matching process is a zombie, which cannot execute or retain an open file; this avoids a
  nondeterministic cleanup failure while a parent is waiting to reap a terminated descendant.
- A startup grace prevents a second launcher from mistaking a concurrent marker update for a
  stale crash.
- Parent validation occurs before mutation, creates only an absent 0700 leaf, never chmods an
  existing parent, and rejects dot segments, non-private existing parents, repositories,
  worktrees, Downloads siblings, broad temp roots, and `/Volumes`.
- Poisoned primary database, backend, PostgreSQL URL, and log values are overridden by safe
  run-local SQLite and log paths.

## How to Test

```bash
bash -n scripts/run-bluey-tests.sh
bash scripts/run-bluey-tests.sh --self-test
node jobs/scripts/ci-guards-self-test.mjs
bash scripts/check-bluey-ops-docs.sh
actionlint .github/workflows/jobs-ci.yml  # only when already installed
git diff --check
```

## Known Limitations

- SIGKILL and host crashes cannot execute cleanup directly. Only a later launcher invocation may
  recover a sufficiently old, marker-valid leftover whose recorded processes are inactive.
- Commands that deliberately escape the supervised process group are out of scope.
- An intentionally corrupted or removed marker fails safe by retaining the directory for manual
  inspection.
- Isolation deliberately trades cross-run compilation reuse for bounded disk use. Related checks
  may be grouped inside one explicitly reviewed shell command when reuse is important.
- The launcher prevents new residue; it does not delete historical repository targets or databases.
