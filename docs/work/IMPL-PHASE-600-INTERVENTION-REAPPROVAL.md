# IMPL: PHASE-600 - Intervention Answer Reapproval

> **Codex preflight:** Loaded `$bluey-ops` and checked current repository and
> deployment boundaries before implementation.

## Scope

**Does:**

- Makes an intervention answer a packet revision rather than a runner-resume
  action.
- Invalidates the previous approval checksum and execution authority.
- Requires the candidate to inspect and approve the revised Application Kit.
- Preserves fail-closed reconciliation after a possible Submit click.
- Aligns portal state, labels, and browser-session status with server truth.

**Does NOT:**

- Enable model generation, local Browser distribution, cloud Browser
  distribution, mailbox synchronization, or employer-facing production flags.
- Change OTP approval or explicit provider final-review behavior.
- Deploy any runtime.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `server/src/db/jobs/customer_data.rs` | Modified | Transactional packet revision and runner fencing. |
| `server/src/db/jobs/applications.rs` | Modified | Guarded lifecycle transition. |
| `server/src/api/jobs.rs` | Modified | Answer-specific API path and resume-action allowlist. |
| `server/src/db/jobs/tests.rs` | Modified | Database workflow regressions. |
| `server/tests/integration_e2e.rs` | Modified | HTTP-level workflow regression. |
| `jobs/portal/src/App.tsx` | Modified | Portal reconciliation and toast behavior. |
| `jobs/portal/src/lib/application-flow.ts` | Modified | Shared client transition helpers. |
| `jobs/portal/src/views/ApplicationsView.tsx` | Modified | Review-first intervention UX. |
| `jobs/portal/src/**/*.test.ts*` | Modified | Client acceptance coverage. |
| `web/jobs/` | Rebuilt | Checked-in production portal bundle. |

## Build & Test

```bash
cd server
cargo fmt --all                         # success
cargo check                             # success
cargo clippy --all-targets -- -D warnings  # success
cargo test                              # 832 unit + 82 HTTP integration tests passed

cd ../jobs/portal
npm test -- --run                       # 93 tests passed
npm run typecheck                       # success
npm run build                           # success

git diff --check                        # success
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Answer Memory persistence is best effort after the packet transaction. | A reusable-memory outage must not roll back or retry an otherwise safe packet revision. |

## Known Follow-ups

- Continue the production-readiness sequence with the next unblocked Jobs
  launch gate; keep all distribution and employer-facing feature flags disabled
  until their independent acceptance matrices pass.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Rust and strict TypeScript checks pass
- [x] No new untracked source, secret, or runtime flag is included
