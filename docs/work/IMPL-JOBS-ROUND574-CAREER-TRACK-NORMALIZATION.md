# IMPL: Jobs Round 574 - Career Track Authority And Normalization

## Scope

**Does:**

- Binds every Career Track to a verified application identity and the current
  source resume asset.
- Normalizes role families, locations, remote policy, employment types,
  contract engagement, work authorization, experience, and seniority.
- Counts relevant experience from non-overlapping role-family employment.
- Keeps required and preferred experience separate and applies the documented
  minus-one/plus-two-year search window.
- Freezes exact candidate evidence, claims, resume revision, identity, and
  receipt authority before employer-facing execution.
- Improves resume import parsing, location suggestions, role selection,
  skills, certifications, and server-owned policy copy.
- Normalizes PDF and DOCX extraction artifacts and canonicalizes server-side
  role and skill aliases with token-boundary matching.

**Does NOT:**

- Enable managed Jobs generation.
- Enable local or cloud browser distribution.
- Deploy or restart production services.
- Change the native overlay, audio, STT, meeting runtime, or signed release.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `server/src/db/jobs/candidate_policy.rs` | Modified | Canonical role and experience policy |
| `server/src/db/jobs/eligibility.rs` | Modified | One typed server-authoritative eligibility context |
| `server/src/db/jobs/execution_authority.rs` | Modified | Exact pre-submit authority recheck |
| `server/src/db/jobs/evidence.rs` | Modified | Evidence and claim revision authority |
| `server/src/db/jobs/profile_postings.rs` | Modified | Profile and posting normalization |
| `server/src/db/jobs/resume_assets.rs` | Modified | Current source-resume authority |
| `server/src/db/jobs/applications.rs` | Modified | Packet finalization recheck |
| `server/src/db/jobs/workspace.rs` | Modified | Workspace authority hydration |
| `server/src/db/jobs/tests.rs` | Modified | Career Track, eligibility, evidence, and receipt regressions |
| `server/src/api/jobs.rs` | Modified | Verified Career Track creation and test fixtures |
| `infra/postgres/server-runtime/016_jobs_evidence_immutability.sql` | Added | PostgreSQL evidence immutability |
| `jobs/portal/src/lib/career-track.ts` | Added | Portal track draft and save authority |
| `jobs/portal/src/lib/documents/parser.ts` | Modified | Stronger PDF and DOCX section parsing |
| `jobs/portal/src/data/use-location-suggestions.ts` | Modified | Complete bounded location suggestions |
| `jobs/portal/src/components/Onboarding.tsx` | Modified | Canonical Career Track setup |
| `jobs/portal/src/views/SettingsView.tsx` | Modified | Compact policy and authority controls |
| `jobs/portal/vite.config.ts` | Modified | Runtime/vendor split and lazy-parser budget |
| `web/jobs/` | Rebuilt | Production portal assets from this source state |
| Bluey agent/runbook templates | Modified | Require the validated `bluey-ops` preflight |

## Build & Test

```bash
(cd server && cargo test --quiet)
# success: 799 unit tests, 76 integration tests, and all focused binaries

(cd server && cargo clippy --all-targets -- -D warnings)
# success: no warnings

(cd server && cargo fmt --all -- --check)
# success

(cd jobs/portal && npm test)
# success: 13 files, 87 tests

(cd jobs/portal && npm run typecheck)
# success

(cd jobs/portal && npm run build)
# success: no chunk-size warning; source maps disabled

npm test
# success: 476 Jobs tests across automation, Browser, runner, workflows, and portal

npm run typecheck
# success: all Jobs workspaces

node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/check-provenance-licenses.mjs
node scripts/check-bluey-jobs-client-boundary.mjs
# success

bash scripts/check-server-sqlite-boundary.sh
# completed with the existing 43-line migration warning; this batch adds no
# SQLite access outside server/src/db
```

The external `bluey-ops` Codex skill was updated for this authority model and
validated with the skill validator. Repository code and current runbooks remain
the source of truth.

Responsive browser QA covered:

- Settings at 1440 by 1000 in dark and light themes.
- Settings at 390 by 844, including Search Rules and policy controls.
- Onboarding at 1440 by 1000.
- Onboarding at 390 by 844, including contact fields and the complete
  role/location/employment/engagement policy step.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| No production deployment | Repository rules require review through a feature branch, and employer-facing Jobs flags remain intentionally off |
| No Browser runtime change | This slice establishes candidate and Career Track authority before Browser distribution |

## Known Follow-ups

- Certify employer-facing adapters and runner recovery independently before
  enabling any Browser distribution flag.
- Keep managed generation disabled until its exact evidence and spend gates are
  separately reviewed.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No native overlay, audio, or meeting runtime files changed
- [x] Tests cover the acceptance criteria
- [x] Rust and TypeScript strict checks pass
- [x] No untracked secrets, source maps, or runtime databases are included
