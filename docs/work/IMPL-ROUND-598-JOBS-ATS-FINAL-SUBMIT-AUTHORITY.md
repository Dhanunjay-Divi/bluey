# IMPL: Round 598 - Jobs ATS Final-Submit Authority

> **Codex preflight:** Loaded `$bluey-ops`, confirmed the feature branch and
> retained the disabled production model, Browser and mailbox flags.

## Scope

**Does:** centralize ATS capability metadata, require exact provider hosts,
remove final-submit behavior from generic adapters, and prevent adapter output
from forging a successful employer submission.

**Does NOT:** enable public runners, certify employer tenants, bypass protected
portals, or change production services and flags.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/automation/src/adapter-capabilities.ts` | Created | Shared typed capability and version registry |
| `jobs/automation/src/policy.ts` | Modified | Fail-closed URL and capability policy |
| `jobs/automation/src/execute.ts` | Modified | Execution-layer final-submit guard |
| `jobs/automation/src/standard-adapters.ts` | Modified | Review/takeover generic adapter behavior |
| `jobs/automation/src/source-catalog.ts` | Modified | Truthful ATS source capability labels |
| `server/src/db/jobs/eligibility.rs` | Modified | Exact server-side provider-host classification |
| Automation and server test files | Modified | Capability, spoofing, review and authority coverage |
| `CHANGELOG.md` | Modified | Records the final-submit authority fix |
| Round 598 docs | Created | Implementation, bug and review evidence |

## Build & Test

```text
Jobs automation tests                       227 passed
Jobs automation strict TypeScript            passed
Jobs automation production build             passed
Server unit tests                            829 passed
Server HTTP integration tests                 81 passed
Server runner-plan/schema tests                2 passed
Server strict Clippy                          passed
Rust formatting                              passed
git diff --check                             passed
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| No live ATS submission | Requires an owner-authorized employer tenant and remains outside this source-safety gate |
| No production deployment | Distribution flags remain disabled until provider certification and fault tests pass |

## Known Follow-ups

- Certify exact Greenhouse and Lever adapter versions against authorized test
  tenants, including crash-after-submit reconciliation.
- Implement provider-specific Workday, Ashby and SmartRecruiters state machines
  before granting final-submit authority.
- Keep unknown and protected portals in review/takeover mode.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated Browser release scripts included
- [x] Tests cover spoofed hosts and unauthorized submitted outcomes
- [x] Code style matches repository rules
- [x] Production feature flags remain disabled
