# Round 625 — Bluey Isolated Test Disk Authority

> **Codex preflight:** Load `$bluey-ops` and `$codex-storage-archive` before changing this
> boundary. The current checkout is authoritative. The SSD is not a test-output destination.

**Date:** 2026-08-31

**Branch:** `feat/phase-625-bluey-test-isolation`

**Base:** `feat/phase-622-jobs-opt-in-autonomy-v1@abdc40e9`

**Status:** INDEPENDENT SOURCE-DELTA GREEN — no P0–P3 findings; exact-tip hosted CI and
integrated-successor evidence remain required

## Incident

Repeated Bluey local Rust verification was reported to recreate approximately 7–20 GiB of build
and temporary data per hour. The prior commands wrote to persistent checkout-relative Cargo target
directories and relied on individual tests or the operating system to retire temporary SQLite
state. Failed, interrupted, and repeated worktree runs therefore had no single lifecycle owner.
Low-debug environment flags reduced growth but did not create an enforceable cleanup boundary.

## Decision

Bluey local test/check commands that may compile Rust use
`scripts/run-bluey-tests.sh all` or its direct-command form. One launcher invocation owns one
unique, private run root below `/tmp/bluey-test-runs-<uid>` and exports:

- `CARGO_TARGET_DIR` to the run-local Cargo target;
- `TMPDIR`, `TMP`, `TEMP`, and `SQLITE_TMPDIR` to the same run-local temporary directory;
- `BLUEY_DB_PATH` to a run-local primary database, forces `BLUEY_SERVER_DB_BACKEND=sqlite`, and
  removes inherited `BLUEY_DATABASE_URL` and `BLUEY_TEST_POSTGRES_URL`;
- Bluey/Cue data, configuration, runtime, and log directories to run-local paths; and
- Cargo incremental and dev/test debug settings that prevent the known high-growth artifact shape.

Every `all` and direct command changes to the physical repository root, receives its arguments
without string re-parsing, and returns its exact nonzero or signal status. A successful command
becomes cleanup status `74` when verified deletion fails. Signals are deferred while `mkdir`
transitions the generated root to owned state, so an early unmarked owned root can be safely
cleaned. Once marker establishment begins, however, an absent, linked, malformed, or mismatched
marker retains the root and makes cleanup fail explicitly for audit. A Perl `setsid` boundary gives
the command and ordinary descendants a process group distinct from the launcher; cleanup signals
that group with the portable external `/bin/kill` interface, waits for bounded graceful exit, and
escalates to KILL before touching storage. Before waiting on the command gate, the wrapper resets
inherited HUP, INT, and TERM signal dispositions, enters its private `setsid` group, and publishes
a private ready handshake. The launcher defers those signals across spawn, handshake, exact
PID/PGID verification, and running-marker setup; it delivers any deferred signal before opening
the command gate. Bounded group/PID polling and KILL escalation precede `wait`, and a terminated
zombie wrapper is reaped instead of being mistaken for executable work.

The run-parent base must already exist. The launcher rejects dot segments, broad temporary roots,
`/Volumes`, exact or ancestral Git metadata (including links), registered worktrees, repository
ancestors, and Downloads siblings before mutation. Worktree discovery must succeed and remain at or
below 1 MiB before shell buffering; it also preserves the producer status and enforces a ten-second
producer timeout. It creates only the private leaf when absent and never chmods an existing parent.

Normal current-run cleanup binds the root device/inode once immediately after `mkdir` and, after
marker establishment, the marker device/inode/mtime plus all nine canonical fields recorded by the
latest verified marker write. Each marker transition pre-binds its temp identity, atomically
renames it, and proves the immutable root and renamed marker before committing the snapshot. It
re-reads those identities immediately before removal. Any current root substitution,
byte-equivalent marker replacement, or same-inode field rewrite retains the root with explicit
status 74 rather than widening deletion authority.

An eligible current or stale root is never recursively removed at its validated source pathname.
The launcher first creates and binds a unique 0700 same-parent quarantine, atomically renames the
exact root into it, requires the original pathname to be absent, and validates the moved root and
marker before and after the final-delete substitution boundary. It then traverses only from the
verified quarantined directory with a same-filesystem fence, removes the empty payload and
quarantine nonrecursively, and verifies absence. Ambiguous or failed work retains the quarantine;
because its `run.*` name has no ordinary marker, the next invocation blocks before starting a
large command rather than retrying or accumulating another run.

## Crash Recovery Boundary

SIGKILL and host power loss cannot run a shell trap. The launcher therefore scans only its private
run parent at the beginning of a later invocation. A candidate is eligible for removal only when:

1. it is an immediate, current-user-owned, non-symlink directory with the exact generated name;
2. its marker is a current-user-owned regular file whose numeric fields are bounded canonical
   decimals, whose exact name repeats its creation epoch and owner PID, and whose starting/running
   state requires process-group ID zero/greater-than-one respectively;
3. both the claimed creation epoch and the root/marker filesystem modification times are at least
   60 seconds old, so a newly copied old marker cannot authorize a young root;
4. neither the recorded launcher nor command process group is active, with any process-inspection
   error treated as active and a zombie-only group treated as inactive because it cannot execute
   or retain open files; and
5. a second read has byte-equivalent canonical fields, the same root and marker device/inode/mtime,
   and freshly rechecks current time, age, owner PID, and process group immediately before removal.

Foreign-owned and linked candidates remain untouched. An owned run with missing, linked,
malformed, mismatched, or future-dated marker metadata is preserved but blocks the next command
with bounded exact-root operator guidance. Young or active valid runs remain untouched. If an
eligible stale root cannot be deleted exactly, recovery also blocks before a new command starts.
Nested links inside a valid stale root are unlinked as entries by anchored cleanup and are never
traversed. Nested mount points fail the same-filesystem deletion and retain evidence. The run parent
itself is never recursively removed.

This is next-launch recovery, not a claim that cleanup survives SIGKILL directly. A command that
deliberately escapes its process group remains outside the launcher contract.

## CI Authority

The Jobs CI lane runs the fast fake-command self-test before installing Jobs dependencies or
compiling Rust. Its existing structural guard requires exactly one self-test invocation and the
critical parent, database, log, low-growth, signal, process-group, marker, deletion-verification,
repository-CWD, and status-preservation clauses. Negative guard fixtures prove that removing the
CI invocation, restoring a repository Cargo target, retaining inherited PostgreSQL authority, or
opening the `mkdir` ownership window fails closed. They also reject Bash-builtin negative-PGID
signaling, incomplete HUP/INT/TERM receipt coverage, continuation after stale deletion failure,
continuation past unknown owned metadata, non-canonical arithmetic, run-name or state drift,
young copied roots, loss of final eligibility revalidation, each of the nine exact marker-field
comparisons, every current marker snapshot/revalidation, root/marker identity loss, exact-parent
Git drift, worktree producer failure/timeout/oversize/process-substitution, immutable-root and
pre-bound marker transition loss, either stale/current quarantine call, source-absence/moved-root/
marker authority drift, premature or unanchored deletion, private-handshake ordering, deferred
delivery ordering, bounded PID fallback, and byte-equivalent marker inode swaps.

## Acceptance Evidence

- Direct-command success/failure from `/tmp`, canonical `all` mapping from `/tmp`, and
  `--self-test` are covered.
- Running-command HUP, INT, and TERM produce statuses 129, 130, and 143 only after the child
  records receipt, terminates, and the launcher removes its exact root. Deterministic pre-gate
  probes produce those same exact statuses while proving the user command never starts, the
  wrapper exits, and the exact root is removed.
- Poisoned primary DB, PostgreSQL URL, backend, and log values are overridden by run-local values.
- Ordinary descendants are stopped; stale roots bound to an active group or ambiguous `ps` result
  survive the reaper, while a deterministic zombie-only group does not block safe cleanup.
- Repository, Downloads-sibling, `/Volumes`, dot-segment, and public existing parents are rejected
  before mutation; exact Git directory/file/broken-symlink parents and worktree enumeration failure
  are also rejected before command/root creation, and existing modes remain unchanged.
- A deterministic post-`mkdir` TERM cleans an unmarked just-created root. Removed and corrupted
  current markers retain their roots and return status 74. The next invocation refuses status 75
  before child start for missing/malformed retained metadata or an eligible stale deletion failure.
  Forced current-root deletion failure produces status 74 for a successful child and preserves an
  existing status 39 with an explicit cleanup error.
- Future, overflow, leading-zero, run-name epoch/PID drift, state/process-group mismatch, young-root
  stale-marker, and byte-equivalent same-field marker-inode replacement fixtures all retain the
  exact root and block before child start.
- Current byte-equivalent marker replacement, same-inode state/group rewrite with restored mtime,
  and whole-root substitution all retain both known and substituted evidence and return status 74.
- Deterministic transition fixtures substitute the root before the running marker, replace the
  byte-identical marker after its atomic rename, replace the source immediately before quarantine
  rename, and replace the quarantined payload immediately before final deletion. Current and stale
  paths retain both original and substituted sentinels; a retained quarantine blocks the next
  command before child start.
- The microphone-setting integration test uses a collision-safe RAII temporary directory rooted in
  the launcher-controlled `TMPDIR`; it no longer creates a process-ID-only database in shared
  `/tmp`.
- No test run, database, log, or Cargo sentinel remains in the repository or test root.
- The Jobs CI guard, Bluey operations-doc check, workflow lint when already installed, shell syntax,
  and diff whitespace checks are required before handoff.

## Independent Source-Delta Review

The fresh independent review covered the committed Jobs CI wiring and every line in the frozen
12-path correction delta. The canonical sorted per-file content manifest was:

```text
37a00810346ebe0093c23267da0312c3f2fecac08fb350c17a6c1b378aee7dca
```

The reviewer found no P0–P3 issues. On native macOS Bash 3.2, shell syntax and the launcher
self-test passed; Node syntax and the complete Jobs CI guard passed; the Bluey operations-doc guard
and diff whitespace check passed. The canonical run parent was empty afterward, no launcher
self-test root remained, and no Cargo, Rust compiler, or launcher process survived. `actionlint` was
unavailable and was not installed. No Rust, Cargo, Docker, network, commit, push, or production
action was performed by this review.

This is source-delta authority only. Exact-tip hosted CI and verification on the integrated
successor remain pending and are not implied by the GREEN verdict.

## Explicit Non-Effects

- No existing Cargo target, database, build cache, rollback artifact, source file outside this
  phase, production host, provider, deployment, feature flag, or customer state is deleted or
  changed.
- No output is copied or moved to the SSD.
- This phase does not claim a fresh full Rust or Docker verification; those heavy gates remain
  exact-tip hosted/release evidence where required.
