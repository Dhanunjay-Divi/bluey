# IMPL: BLUEY-OPS-PREFLIGHT — Durable Codex operating memory

> **Codex preflight:** Load `$bluey-ops` before implementation and verify its
> memory against the current repository state.

## Scope

**Does:** Require `$bluey-ops` at Bluey agent entry points, work templates, and
runbooks, and enforce that coverage in CI.

**Does NOT:** Change product code, runtime behavior, release artifacts, or
production services.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `AGENTS.md` | Created | Make the skill preflight and development rules available in normal clones. |
| Agent entry docs and active runbooks | Modified | Put the preflight at each operational entry point. |
| Work templates | Modified | Carry the preflight into future implementation and review records. |
| `scripts/check-bluey-ops-docs.sh` | Created | Detect omitted preflights in current and future runbooks. |
| `.github/workflows/ci.yml` | Modified | Run the coverage check on Linux CI. |
| `CHANGELOG.md` | Modified | Record the repository workflow change. |

## Build & Test

```bash
bash -n scripts/check-bluey-ops-docs.sh
bash scripts/check-bluey-ops-docs.sh
git diff --check
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| None | — |

## Known Follow-ups

- None.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Documentation check covers all current active Bluey runbooks
- [x] No product or production behavior changed
