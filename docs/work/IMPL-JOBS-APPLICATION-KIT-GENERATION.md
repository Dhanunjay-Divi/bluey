# IMPL: JOBS-APPLICATION-KIT-GENERATION — Grounded Application Kits

> **Codex preflight:** Loaded `$bluey-ops` and verified its production and
> branch authority against the current repository before implementation.

## Scope

**Does:**

- Extends managed resume generation with an optional job-specific cover letter.
- Requires each model-authored paragraph to cite existing Career Profile evidence.
- Rejects unsupported metrics, employers, roles, locations, and strengthened claims.
- Persists the exact cover letter in the same transaction as the tailored resume.
- Shows the exact letter, or an explicit not-included state, in packet review.

**Does NOT:**

- Enable managed model generation in production.
- Enable local or cloud Browser distribution.
- Invent candidate facts, legal answers, authorization, salary, or availability.
- Change employer-facing submission authority.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `server/src/api/jobs_resume_generation.rs` | Modified | Generate, validate, and materialize evidence-grounded cover letters. |
| `server/src/api/jobs_resume_generation/tests.rs` | Modified | Cover valid and unsupported model output. |
| `server/src/api/jobs.rs` | Modified | Pass the complete generated kit into the transaction. |
| `server/src/db/jobs/resume_truth.rs` | Modified | Share evidence grounding checks across resume and application text. |
| `server/src/db/jobs/applications.rs` | Modified | Persist cover letter and receipt status atomically. |
| `server/src/db/jobs/tests.rs` | Modified | Verify transactional kit finalization. |
| `jobs/portal/src/views/ApplicationsView.tsx` | Modified | Render exact cover-letter content in packet review. |
| `jobs/portal/src/styles.css` | Modified | Keep long letter content compact and readable. |
| `web/jobs/*` | Generated | Refresh the deployable portal bundle. |

## Build & Test

```bash
cargo fmt --all -- --check
# success

cargo clippy -p bluey-server --all-targets -- -D warnings
# success

cargo test -p bluey-server jobs -- --nocapture
# 247 Jobs-focused server unit tests and 16 Jobs integration tests passed

npm --prefix jobs/portal test
# 12 files, 87 tests passed

npm --prefix jobs/portal run typecheck
# success

npm --prefix jobs/portal run build
# success; deployable assets refreshed

npm --prefix jobs/portal audit --omit=dev
# two high React Router advisories remain. They concern RSC/server-action modes;
# this portal is a client-only Vite SPA and does not use either affected surface.
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Managed generation remains disabled in production. | Provider acceptance, spend, and production quality gates must pass before rollout. |

## Known Follow-ups

- Run a controlled provider acceptance corpus over representative resumes and JDs.
- Add application-answer model generation only after it receives the same evidence binding.
- Preserve exact source DOCX layout in the separate OOXML fidelity slice.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs
