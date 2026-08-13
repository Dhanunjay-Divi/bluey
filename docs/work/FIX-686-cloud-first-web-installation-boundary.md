# FIX-686: Cloud-First Web Installation Boundary

> **Codex preflight:** Loaded `$bluey-ops` before diagnosis and reconciled its Jobs launch
> invariants against the Phase 606 successor worktree.

## Issue

The Jobs portal presented an installable local Bluey Browser as a peer launch path even though the
launch product is the browser-delivered portal backed by a managed cloud runner.

## Root Cause

The original Browser view intentionally supported both local and cloud runner experiments. Later
server work preserved signed local release and recovery authority, but the portal product boundary
was never narrowed after the cloud-first launch decision. As a result, customer UI still contained
local setup, native download, processor selection, protocol-open, local queue, and related plan
copy. That presentation created installation friction and implied a disabled, uncertified local
distribution path was required to use Bluey Jobs.

## Fix Summary

- Make `/jobs/automation` the canonical customer execution route.
- Redirect legacy `/jobs/browser` links while preserving only the bounded preview mode.
- Present only the managed cloud Background runner as a customer queue choice.
- Prevent retained local-only ATS authority from making Auto-submit or runner availability appear
  enabled in Matches, Applications, or certification summaries.
- Remove local installation, download, architecture, protocol, setup, queue, and pricing prompts.
- Preserve active managed browser-session status, intervention, takeover, email-code approval, and
  final-review controls.
- Remove the cosmetic pause/resume mutation until a durable command reaches the live workflow.
- Keep server-side local release/recovery authority and the disabled local-distribution flag intact
  so unresolved historical state is never stranded.
- Park Phase 607 local updater work for possible P2 demand rather than merging it into launch.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/App.tsx`, `main.tsx`, `components/AppShell.tsx`, `components/AuthGate.tsx` | Canonical pre-auth Automation route, bounded Browser redirect, navigation, and launch copy |
| `jobs/portal/src/views/AutomationView.tsx` and test | Add the cloud-only surface and active-run regression coverage |
| `jobs/portal/src/views/BrowserView.tsx`, test, and `lib/browser-release.ts` | Remove the install-first/local-run surface, release presentation, and obsolete tests |
| `jobs/portal/src/lib/application-flow.ts` and test | Recheck exact cloud eligibility, exclude active runs, and bind final approval to takeover authority |
| `jobs/portal/src/lib/ats-certification.ts` and test, `components/AtsCertificationSummary.tsx` | Project local authority false and distinguish reviewed beta from active certification |
| `jobs/portal/src/lib/portal-navigation.ts` and test | Allowlist preview scenarios and strip all other legacy query state |
| `jobs/portal/src/lib/runner-access.ts` and test | Provide cloud-only availability and launch copy |
| `jobs/portal/src/views/ApplicationsView.tsx` and test | Use cloud-only authority and preview-safe Automation/handoff actions |
| `jobs/portal/src/views/MatchesView.tsx` and test | Prevent local-only authority from presenting Auto-submit |
| `jobs/portal/src/api.ts`, `SettingsView.tsx`, `data/preview.ts` | Remove cosmetic session mutation and remaining local-install plan/preview copy |
| `jobs/portal/src/styles.css` | Remove selectors used only by deleted installation UI |
| `jobs/OPERATIONS.md` | Make local packaging/device gates P2-only rather than cloud-launch blockers |
| `web/jobs/index.html`, `web/jobs/assets/*` | Regenerate the exact production bundle without Browser/install UI |
| Round 608, IMPL, REVIEW, changelog | Record product decision, evidence, and limitations |

## Edge Cases Handled

- A bookmarked Browser preview reaches Automation with the bounded preview/scenario state intact.
- Unknown legacy query parameters do not propagate into the canonical route.
- Cloud disabled or unavailable does not reveal a local installer fallback.
- Active managed runs retain takeover and intervention controls.
- A possible employer-facing side effect retains its server recovery and receipt path.
- Mobile and narrow layouts retain the active browser preview and cloud queue.

## How to Test

```bash
npm run test --workspace @bluey/jobs-portal
npm run typecheck --workspace @bluey/jobs-portal
npm run build --workspace @bluey/jobs-portal
git diff --check
```

Also search the production portal source and generated bundle for installation, native-download,
architecture-choice, protocol-open, and local-queue controls, then visually inspect active and empty
Automation states at desktop/mobile light and dark viewports.

## Known Limitations

- This fix does not enable the managed cloud runner or change a production flag.
- It does not delete retained local server authority or Phase 607 evidence.
- Temporal, browser-pool, ATS, provider, tenant, credential, and canary gates remain external.
