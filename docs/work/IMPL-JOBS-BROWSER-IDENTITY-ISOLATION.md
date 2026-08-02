# IMPL: JOBS-BROWSER-IDENTITY-ISOLATION - Application Identity Browser Profiles

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state before implementation.

## Scope

**Does:**

- Derives one opaque Browser profile ID from the Bluey account and application identity.
- Gives each application identity its own persistent Chromium profile directory.
- Preserves the existing profile path and server-owned profile-ID format across upgrades.
- Prevents a context, profile ID, or stored session from being rebound across identities.
- Rejects unsafe roots and identity values without echoing private values in errors.

**Does NOT:**

- Distribute or enable the local Bluey Browser.
- Enable cloud Browser execution.
- Change browser cookies, credentials, or existing on-disk profile locations.
- Certify an ATS adapter or employer submission.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/browser/src/browser-profile-policy.ts` | Created | Pure identity, path, and registry authority |
| `jobs/browser/src/browser-context-registry.ts` | Modified | Chromium lifecycle uses the identity-scoped authority |
| `jobs/browser/tests/browser-context-registry.test.ts` | Created | Isolation, compatibility, traversal, collision, and privacy tests |
| `CHANGELOG.md` | Modified | Unreleased product record |

## Build & Test

```bash
(cd jobs/browser && npm test -- --run tests/browser-context-registry.test.ts)
# 12 passed

(cd jobs/browser && npm run typecheck)
# passed

(cd jobs/browser && npm test)
# 27 files, 112 tests passed

(cd jobs/browser && npm run build)
# passed
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Browser distribution remains disabled | Identity isolation is necessary but not sufficient for signed distribution, updater, recovery, and physical-platform certification. |

## Known Follow-ups

- Durable cloud-runner leases and restart recovery.
- Signed Browser installers and exact-artifact platform certification.
- Provider-specific ATS submission certification.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs
