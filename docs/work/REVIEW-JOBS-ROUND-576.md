# REVIEW: Jobs Round 576 - Discovery Freshness And Durable Workers

**Commit range:** feature branch working tree before final commit
**Reviewer:** Codex self-review
**Date:** 2026-07-29

## Per-Task Review

### Durable discovery runtime

| Field | Value |
|-------|-------|
| Files | `ops/build-bluey-jobs-workers.sh`, `ops/install-bluey-jobs-workers.sh` |
| Verdict | 🟢 Accept |

**Findings:**

- Exact archive checksums, safe paths, idempotent reinstall, activation
  rollback, and current-release retention are covered by shell tests.
- The scripts support both GNU `sha256sum` and macOS `shasum` and remain
  compatible with the macOS system Bash.

### Independent supervision and health

| Field | Value |
|-------|-------|
| Files | discovery service, timer, checker, and shell tests |
| Verdict | 🟢 Accept |

**Findings:**

- The units have no `Requires` or `PartOf` dependency on the Jobs API.
- `Restart=always`, independent enablement, and bounded start limits are tested.
- Inactive services, mismatched immutable releases, and overdue active sources
  fail the health check.

### Portal freshness truth

| Field | Value |
|-------|-------|
| Files | `MatchesView.tsx`, `MatchesView.test.tsx` |
| Verdict | 🟢 Accept |

**Findings:**

- A stored healthy source becomes degraded after 12 hours.
- Fresh, waiting, paused, degraded, and overdue states are covered by tests.
- Light/dark desktop and 390-by-844 mobile layouts remain compact and readable.

## Cross-Task Findings

- Worker activation does not change model-generation or Browser-distribution
  flags.
- Global feed rows remain candidate leads, not application truth.
- No native overlay, audio, STT, or meeting-runtime file is in scope.

## Build & Test Verification

- Jobs strict TypeScript: passed.
- Jobs tests: 466 passed.
- Jobs portal production build: passed.
- Discovery shell suites: 3 passed.
- Privacy, schema parity, provenance/license, CI guard, client boundary, and
  SQLite boundary: passed.
- Rust workspace library tests: 943 passed, 0 failed, 5 ignored.
- Rust format and strict all-target/all-feature Clippy: passed.
- `git diff --check`: passed.

## Overall Verdict

🟢 Accept for merge and exact-artifact production deployment.

Production acceptance remains separate: worker activation, source catch-up,
fresh portal state, edge checks, disabled execution flags, and rollback
evidence must pass before the round is marked deployed.

## Follow-ups for Next Batch

- Provider coverage and account ranking can expand independently after the
  source freshness foundation remains healthy in production.
