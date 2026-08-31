# REVIEW: PHASE-625 — Bluey Isolated Test Disk Authority

> **Codex preflight:** Loaded `$bluey-ops` and `$codex-storage-archive`, reconciled the current
> worktree against the Phase 625 round, FIX-785, and the reported 7–20 GiB/hour test growth, and
> did not inspect or use the SSD archive.

**Commit range:** `abdc40e9..Phase 625 HEAD`

**Reviewer:** fresh independent line-by-line source-delta review of the corrected frozen bytes

**Date:** 2026-08-31

**Review status:** 🟢 SOURCE-DELTA GREEN — no P0–P3 findings; exact-tip hosted CI and
integrated-successor evidence remain required

**Reviewed 12-path canonical content manifest:**
`37a00810346ebe0093c23267da0312c3f2fecac08fb350c17a6c1b378aee7dca`

## Per-Task Review

### Disposable test-state ownership and cleanup

| Field | Value |
|-------|-------|
| Files | `scripts/run-bluey-tests.sh`, CI guard/workflow, operations docs, FIX and round records |
| Verdict | 🟢 accepted; no P0–P3 source-delta findings |

**Verified:**

- Cargo, primary SQLite, temporary, data, configuration, runtime, and log paths are confined to
  one generated private run root; PostgreSQL URLs are removed and SQLite is forced.
- Both canonical and direct modes change to the physical repository root and preserve command,
  cleanup-failure, and ordinary-signal status.
- Ordinary descendants are contained by a distinct process group and stopped before cleanup.
- HUP, INT, and TERM use the portable external `/bin/kill` negative-PGID form after `setsid`
  resets inherited dispositions; the self-test proves the child receives each signal rather than
  inferring delivery from launcher status.
- The wrapper resets signals and calls `setsid` before publishing a private ready handshake. The
  launcher defers signals through handshake, exact PID/PGID verification, and running-marker
  setup, then dispatches before opening the user-command gate. Bounded group/PID polling and KILL
  escalation prevent pre-gate hangs and allow terminated wrappers to be reaped.
- Signals are deferred across the `mkdir` ownership transition. Early owned/unmarked roots clean
  safely, while an absent or malformed marker after marker establishment fails closed and retains
  evidence.
- Marker-established current cleanup binds the established root device/inode, marker
  device/inode/mtime, and all nine canonical fields; marker/root substitution is retained.
- The post-`mkdir` root identity is assigned exactly once. Marker transitions bind the temp marker
  identity before atomic rename and reject byte-identical post-rename replacement before updating
  cleanup authority.
- Both stale and current cleanup atomically rename the exact root into a private same-parent
  quarantine, prove the original path absent, validate moved root/marker authority twice, and
  delete through a verified same-filesystem current-directory boundary. Any uncertainty retains a
  quarantine that blocks the next command.
- Exact and ancestral Git metadata plus registered-worktree overlap are rejected before parent
  mutation. Enumeration preserves producer status, is time-bounded, and is capped at 1 MiB before
  shell buffering.

### Crash recovery and CI regression authority

| Field | Value |
|-------|-------|
| Files | launcher self-test, `jobs/scripts/ci-guards-self-test.mjs`, Jobs CI workflow |
| Verdict | 🟢 accepted; no P0–P3 source-delta findings |

**Verified:**

- Later-launch stale recovery requires the exact owned marker schema, a 60-second grace, inactive
  launcher and process group, and a complete second validation immediately before deletion.
- Bounded canonical decimals prevent overflow/leading-zero arithmetic; the exact run name binds
  marker epoch/PID, and starting/running state binds zero/nonzero command-group identity.
- Root and marker filesystem age plus exact field/device/inode/mtime identity are captured and
  compared across both reads; current time, age, owner, and group are refreshed before removal.
- Failed or ambiguous process inspection is treated as active and retains the candidate.
- A deterministic zombie-only process group is treated as inactive because it can neither execute
  nor retain open files, preventing terminated descendants from intermittently blocking cleanup.
- Negative fixtures cover poisoned environment values, caller CWD, descendant cleanup, active and
  stale groups, symlink candidates, marker loss/corruption, deletion failure, unsafe parents, and
  the post-`mkdir` signal window.
- Missing, linked, malformed, mismatched, or future-dated owned marker authority now retains the
  exact root and blocks before child start. Failed deletion of an eligible stale root does the
  same, with bounded exact-root operator guidance.
- A young root carrying an old marker and a byte-equivalent same-field marker whose inode is
  replaced between checks both retain the root and block before child start.
- CI runs the self-test once and the structural guard rejects removal of critical authority,
  including each stale/current nine-field binding, root/marker identity checks, exact-parent and
  producer status/time/byte bounds, marker/quarantine transition ordering, both deletion call
  sites, anchored deletion, final-delete fixtures, exact-parent/worktree failures, and
  handshake/delivery/stop ordering.

### Daemon microphone-setting test containment

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/Cargo.toml`, `Cargo.lock`, `crates/cue-daemon/tests/mic_device_selection.rs` |
| Verdict | 🟢 accepted; no P0–P3 source-delta findings |

The test now uses a collision-safe `tempfile::TempDir` beneath `std::env::temp_dir()`, which the
launcher binds to its owned temporary directory. RAII removes the directory after normal test
completion; launcher cleanup remains the process-loss boundary. The former shared
`/tmp/cue_test_mic_device_<pid>.db` path and best-effort single-file cleanup are gone.

## Cross-Task Findings

- The prior green verdict was superseded by four audit rounds: signal/stale-residue/test-database;
  marker arithmetic/cross-binding/pre-gate; unbound current cleanup/exact-parent Git/discarded
  producer status; and finally mutable root authority/pathname-recursive deletion/full-buffered
  worktree output/incomplete guard mutations. The corrected 12-path manifest received a fresh
  independent line-by-line review with no P0–P3 findings.
- The launcher prevents new test residue; it intentionally does not delete historical targets,
  databases, caches, user data, production artifacts, or SSD contents.
- SIGKILL and host loss can only be addressed on a later launch; commands that deliberately escape
  the supervised process group remain outside this bounded contract.

## Build & Test Verification

```text
PASS  shell syntax
PASS  launcher self-test, including running-child and deterministic pre-gate HUP/INT/TERM,
      stale deletion failure, canonical number/name/state binding, filesystem age,
      stale/current same-field inode replacement, same-inode field rewrite, root substitution,
      atomic marker/root transition, pre-quarantine and current+stale final-delete substitution,
      retained-quarantine admission, exact Git parents, bounded worktree enumeration failure,
      negative cleanup, and residue scenarios
PASS  Jobs CI guard self-tests, including independent negative mutation of every new identity,
      field argument, producer bound/status, ordering fence, and stale/current quarantine call
PASS  focused microphone-setting integration test: 1 passed, 0 failed, 4 filtered out
      (earlier correction evidence; affected Rust files unchanged by this launcher-only follow-up)
PASS  Bluey operations-doc guard
PASS  diff whitespace check
PASS  pre/post free-space observation; no run-root residue
PASS  independent frozen-byte manifest recomputation:
      37a00810346ebe0093c23267da0312c3f2fecac08fb350c17a6c1b378aee7dca
PASS  native macOS Bash 3.2 launcher self-test in 26.1 seconds
PASS  Node syntax and complete Jobs CI guard in 15.4 seconds
PASS  no canonical/self-test residue and no surviving Cargo/Rust/launcher process
SKIP  actionlint was not installed; no download was attempted
SKIP  full Rust and Docker gates; exact-tip hosted CI remains required
```

## Overall Verdict

🟢 **SOURCE-DELTA GREEN** — The independent review found no P0–P3 issues in the exact frozen
12-path correction manifest. The review covered source and documentation line by line, including
macOS Bash 3.2 and Linux portability, TOCTOU and deletion authority, process safety, failure status,
residue, and structural-guard truth. This verdict is source-delta authority only.

🟡 **HOSTED/INTEGRATED EVIDENCE PENDING** — Do not treat Phase 625 as aggregate release evidence
until an exact reviewed tip passes the Jobs workflow self-test and the integrated successor is
green.

## Follow-ups for Release

- Push the exact reviewed commit and capture the hosted Jobs workflow URL and result.
- Use this launcher for future local Rust checks; keep release/package artifact builds on their
  separately reviewed promotion paths.
- Keep historical generated-output cleanup as a distinct proof-led operation rather than widening
  this launcher.
