# IMPL: Jobs Round 576 - Discovery Freshness And Durable Workers

## Scope

**Does:**

- package direct and global discovery into one immutable worker release;
- supervise both workers independently from Jobs API deployments;
- detect unavailable workers and configured sources older than 12 hours;
- make the Matches source-health UI reflect actual sync age;
- preserve original-source revalidation and review-first safety.

**Does NOT:**

- enable Jobs model generation;
- distribute the local Bluey Browser;
- enable cloud Browser execution;
- expand ATS submission authority;
- change the native Bluey overlay, audio, STT, or meeting runtime.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/portal/src/views/MatchesView.tsx` | Modified | Render current discovery health |
| `jobs/portal/src/views/MatchesView.test.tsx` | Modified | Verify freshness semantics |
| `ops/bluey-jobs-discovery.service.example` | Modified | Independent direct worker lifecycle |
| `ops/bluey-jobs-global-discovery.service.example` | Modified | Independent global worker lifecycle |
| `ops/build-bluey-jobs-workers.sh` | Created | Build exact shared runtime |
| `ops/install-bluey-jobs-workers.sh` | Created | Verify, activate, retain, and roll back |
| `ops/check-bluey-jobs-discovery.sh` | Created | Worker and source freshness check |
| `ops/bluey-jobs-discovery-health.service.example` | Created | Hardened oneshot check |
| `ops/bluey-jobs-discovery-health.timer.example` | Created | Persistent 15-minute schedule |
| `ops/tests/test-install-bluey-jobs-workers.sh` | Created | Installer and rollback tests |
| `ops/tests/test-check-bluey-jobs-discovery.sh` | Created | Freshness failure tests |
| `ops/tests/test-bluey-jobs-discovery-units.sh` | Created | Service policy tests |
| `jobs/OPERATIONS.md` | Modified | Production procedure and rollback |
| `CHANGELOG.md` | Modified | User-visible fix record |

## Build & Test

Verified on the exact staged tree:

- Jobs strict TypeScript checks passed.
- All 466 Jobs tests passed.
- The production Jobs portal build passed.
- All three discovery service/health/installer shell suites passed.
- Privacy, schema parity, provenance/license, CI guard, client-boundary, and
  server SQLite-boundary gates passed.
- `cargo fmt --all -- --check` passed.
- Workspace library tests passed: 943 passed, 0 failed, 5 ignored.
- Strict all-target/all-feature Clippy passed.
- `git diff --check` passed.
- Stale discovery was visually verified in light and dark themes at desktop
  and 390-by-844 mobile viewports.

Production artifact hashes, source catch-up, and live edge evidence are recorded
in Round 576 only after deployment.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| None | The fix stays within Jobs discovery, portal truth, and operations |

## Known Follow-ups

- Connect `bluey-jobs-discovery-health.service` failures to the production
  alert destination.
- Continue provider-by-provider source expansion only after source rights,
  row-count, availability, and revalidation gates pass.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs
