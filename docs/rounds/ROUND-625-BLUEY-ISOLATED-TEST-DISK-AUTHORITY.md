# Round 625 — Bluey Isolated Test Disk Authority

> **Codex preflight:** Load `$bluey-ops` and `$codex-storage-archive` before changing this
> boundary. The current checkout is authoritative. The SSD is not a test-output destination.

**Date:** 2026-08-31

**Branch:** `feat/phase-625-bluey-test-isolation`

**Base:** `feat/phase-622-jobs-opt-in-autonomy-v1@abdc40e9`

**Status:** LOCAL SOURCE ACCEPTED — exact-tip hosted CI required after push

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
and waits for that group before touching storage.

The run-parent base must already exist. The launcher rejects dot segments, broad temporary roots,
`/Volumes`, repositories, registered worktrees, repository ancestors, and Downloads siblings
before mutation. It creates only the private leaf when absent and never chmods an existing parent.

## Crash Recovery Boundary

SIGKILL and host power loss cannot run a shell trap. The launcher therefore scans only its private
run parent at the beginning of a later invocation. A candidate is eligible for removal only when:

1. it is an immediate, current-user-owned, non-symlink directory with the exact generated name;
2. its marker is a current-user-owned regular file with the exact schema, root, name, UID, launcher
   PID, distinct command process-group ID, launcher group ID, state, and creation epoch;
3. it is at least 60 seconds old, preventing a concurrent launch/marker-update race;
4. neither the recorded launcher nor command process group is active, with any process-inspection
   error treated as active and a zombie-only group treated as inactive because it cannot execute
   or retain open files; and
5. the marker and process checks still pass immediately before removal.

Unmarked, malformed, foreign-owned, linked, future-dated, young, or active candidates remain
untouched. Nested links are unlinked as entries by recursive cleanup and are never traversed. The
run parent itself is never recursively removed.

This is next-launch recovery, not a claim that cleanup survives SIGKILL directly. A command that
deliberately escapes its process group remains outside the launcher contract.

## CI Authority

The Jobs CI lane runs the fast fake-command self-test before installing Jobs dependencies or
compiling Rust. Its existing structural guard requires exactly one self-test invocation and the
critical parent, database, log, low-growth, signal, process-group, marker, deletion-verification,
repository-CWD, and status-preservation clauses. Negative guard fixtures prove that removing the
CI invocation, restoring a repository Cargo target, retaining inherited PostgreSQL authority, or
opening the `mkdir` ownership window fails closed.

## Acceptance Evidence

- Direct-command success/failure from `/tmp`, canonical `all` mapping from `/tmp`, and
  `--self-test` are covered.
- HUP, INT, and TERM produce statuses 129, 130, and 143 only after child termination and root
  cleanup.
- Poisoned primary DB, PostgreSQL URL, backend, and log values are overridden by run-local values.
- Ordinary descendants are stopped; stale roots bound to an active group or ambiguous `ps` result
  survive the reaper, while a deterministic zombie-only group does not block safe cleanup.
- Repository, Downloads-sibling, `/Volumes`, dot-segment, and public existing parents are rejected
  before mutation; existing modes remain unchanged.
- A deterministic post-`mkdir` TERM cleans an unmarked just-created root. Removed and corrupted
  current markers retain their roots and return status 74. Forced deletion failure produces status
  74 for a successful child and preserves an existing status 39 with an explicit cleanup error.
- No test run, database, log, or Cargo sentinel remains in the repository or test root.
- The Jobs CI guard, Bluey operations-doc check, workflow lint when already installed, shell syntax,
  and diff whitespace checks are required before handoff.

## Explicit Non-Effects

- No existing Cargo target, database, build cache, rollback artifact, source file outside this
  phase, production host, provider, deployment, feature flag, or customer state is deleted or
  changed.
- No output is copied or moved to the SSD.
- This phase does not claim a fresh full Rust or Docker verification; those heavy gates remain
  exact-tip hosted/release evidence where required.
