# REVIEW: PHASE-625 — Bluey Isolated Test Disk Authority

> **Codex preflight:** Loaded `$bluey-ops` and `$codex-storage-archive`, reconciled the current
> worktree against the Phase 625 round, FIX-785, and the reported 7–20 GiB/hour test growth, and
> did not inspect or use the SSD archive.

**Commit range:** `abdc40e9..Phase 625 HEAD`

**Reviewer:** Independent final source reviewer; evidence consolidated by the primary agent

**Date:** 2026-08-31

**Review status:** 🟢 ACCEPT for the bounded local source; exact-tip hosted CI remains required

## Per-Task Review

### Disposable test-state ownership and cleanup

| Field | Value |
|-------|-------|
| Files | `scripts/run-bluey-tests.sh`, CI guard/workflow, operations docs, FIX and round records |
| Verdict | 🟢 accept |

**Verified:**

- Cargo, primary SQLite, temporary, data, configuration, runtime, and log paths are confined to
  one generated private run root; PostgreSQL URLs are removed and SQLite is forced.
- Both canonical and direct modes change to the physical repository root and preserve command,
  cleanup-failure, and ordinary-signal status.
- Ordinary descendants are contained by a distinct process group and stopped before cleanup.
- Signals are deferred across the `mkdir` ownership transition. Early owned/unmarked roots clean
  safely, while an absent or malformed marker after marker establishment fails closed and retains
  evidence.
- Parent/root/marker validation, symlink rejection, exact-root removal checks, and conservative
  process inspection prevent cleanup from widening beyond generated test state.

### Crash recovery and CI regression authority

| Field | Value |
|-------|-------|
| Files | launcher self-test, `jobs/scripts/ci-guards-self-test.mjs`, Jobs CI workflow |
| Verdict | 🟢 accept |

**Verified:**

- Later-launch stale recovery requires the exact owned marker schema, a 60-second grace, inactive
  launcher and process group, and a complete second validation immediately before deletion.
- Failed or ambiguous process inspection is treated as active and retains the candidate.
- A deterministic zombie-only process group is treated as inactive because it can neither execute
  nor retain open files, preventing terminated descendants from intermittently blocking cleanup.
- Negative fixtures cover poisoned environment values, caller CWD, descendant cleanup, active and
  stale groups, symlink candidates, marker loss/corruption, deletion failure, unsafe parents, and
  the post-`mkdir` signal window.
- CI runs the self-test once and the structural guard rejects removal of critical authority.

## Cross-Task Findings

- No P0-P3 correctness, safety, privacy, lifecycle, documentation, or scope findings remain.
- The launcher prevents new test residue; it intentionally does not delete historical targets,
  databases, caches, user data, production artifacts, or SSD contents.
- SIGKILL and host loss can only be addressed on a later launch; commands that deliberately escape
  the supervised process group remain outside this bounded contract.

## Build & Test Verification

```text
PASS  shell syntax
PASS  launcher self-test, including negative cleanup and residue scenarios
PASS  Jobs CI guard self-tests
PASS  Bluey operations-doc guard
PASS  diff whitespace check
PASS  pre/post free-space observation; no run-root residue
SKIP  actionlint was not installed; no download was attempted
SKIP  heavy Rust and Docker gates by Phase 625 scope; exact-tip hosted CI remains required
```

## Overall Verdict

🟢 **LOCAL SOURCE ACCEPT** — Committed on the bounded feature branch and ready for exact-tip CI.

🟡 **HOSTED EVIDENCE PENDING** — Do not treat Phase 625 as integrated release evidence until the
exact pushed tip passes the Jobs workflow self-test and the aggregate release candidate is green.

## Follow-ups for Release

- Push the exact reviewed commit and capture the hosted Jobs workflow URL and result.
- Use this launcher for future local Rust checks; keep release/package artifact builds on their
  separately reviewed promotion paths.
- Keep historical generated-output cleanup as a distinct proof-led operation rather than widening
  this launcher.
