# IMPL: Jobs Round 575 - Track Auto-submit Authority And Form Read-back

> Codex preflight: load the `bluey-ops` skill, then verify its memory against
> the current repository state and task-specific docs.

## Scope

**Does:**

- Adds revisioned, Track-scoped Auto-submit authorization.
- Binds authorization to one verified identity, current source resume, Track
  policy, and confirmed candidate facts.
- Invalidates authorization when any bound authority changes.
- Separates Review-first packet approval from Auto-submit Track authority.
- Covers execution admission with the frozen Application Kit checksum.
- Rechecks authorization and candidate truth immediately before Submit.
- Verifies that ATS forms retained every value and uploaded document after
  filling and immediately before Submit.
- Adapts the useful form-registration lesson from `santifer/career-ops` without
  adopting its human-submit-only architecture.

**Does NOT:**

- Enable model generation.
- Distribute the local or cloud Bluey Browser.
- Claim that every ATS is certified for unattended submission.
- Deploy or restart production services.
- Change the native overlay, audio, STT, or meeting runtime.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `infra/sqlite/server-runtime/037_jobs_auto_submit_authorizations.sql` | Added | SQLite authorization revisions and active-row uniqueness |
| `infra/postgres/server-runtime/017_jobs_auto_submit_authorizations.sql` | Added | PostgreSQL authorization revisions and indexes |
| `server/src/db/jobs/auto_submit.rs` | Added | Authorization fingerprint, revision, revoke, and validation logic |
| `server/src/db/jobs.rs` | Modified | Public authorization type and module exports |
| `server/src/db/jobs/execution_authority.rs` | Modified | Pre-submit authorization and packet-admission recheck |
| `server/src/db/jobs/workspace.rs` | Modified | Workspace authorization status |
| `server/src/api/jobs.rs` | Modified | Authorize/revoke endpoints and schema-two execution admission |
| `server/src/db/jobs/tests.rs` | Modified | Authority, staleness, revision, and checksum regressions |
| `jobs/portal/src/App.tsx` | Modified | Authorization actions and stale-state integration |
| `jobs/portal/src/views/SettingsView.tsx` | Modified | Per-Track Auto-submit control |
| `jobs/portal/src/views/MatchesView.tsx` | Modified | Truthful Auto-submit mode availability |
| `jobs/portal/src/lib/application-flow.ts` | Modified | Authorization-aware submission mode |
| `jobs/automation/src/form-readback.ts` | Added | Provider-neutral fill expectations and verification |
| `jobs/automation/src/standard-adapters.ts` | Modified | Shared ATS/fallback read-back guard |
| `jobs/automation/src/providers/greenhouse.ts` | Modified | Greenhouse post-fill and pre-submit read-back |
| `jobs/automation/src/providers/lever.ts` | Modified | Lever post-fill and pre-submit read-back |
| `jobs/automation/tests/*readback*` | Added/Modified | Silent-discard and private-value regressions |
| `jobs/THIRD_PARTY_PROVENANCE.md` | Modified | Current career-ops audit and adaptation boundary |
| `CHANGELOG.md` | Modified | Unreleased product behavior |

## Build & Test

```bash
(cd server && cargo test --lib --quiet)
(cd server && cargo fmt --all -- --check)
(cd server && cargo clippy --all-targets -- -D warnings)

(cd jobs/automation && npm test -- --run)
(cd jobs/automation && npm run typecheck)
(cd jobs/automation && npm run build)

(cd jobs/portal && npm test -- --run)
(cd jobs/portal && npm run typecheck)
(cd jobs/portal && npm run build)

(cd jobs && npm test)
(cd jobs && npm run typecheck)

node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/check-provenance-licenses.mjs
node scripts/check-bluey-jobs-client-boundary.mjs
git diff --check
```

All commands passed.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| No production deployment | Runner distribution remains intentionally disabled until adapter and recovery certification |
| No direct career-ops source copy | Bluey's typed multi-tenant authority and browser contracts required an original implementation |

## Known Follow-ups

- Certify provider-specific browser flows against representative ATS tenants.
- Prove durable local/cloud runner restart recovery and side-effect-unknown
  reconciliation before enabling distribution.
- Run physical Windows and packaged macOS Browser certification before a public
  Browser release.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated native/runtime changes included
- [x] Tests cover authorization and silent form-write rejection
- [x] Candidate values are absent from read-back failures
- [x] Rust and TypeScript strict checks pass
- [x] Production Jobs flags remain disabled
