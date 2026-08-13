# IMPL: PHASE-608 - Jobs Cloud-First Web Automation

> **Codex preflight:** Loaded `$bluey-ops` and reconciled it against the Phase 606 successor
> worktree before implementation. No archive, production service, provider credential, live tenant,
> deployment, or external write was used.

## Scope

**Does:**

- Makes `/jobs/automation` the canonical customer route for managed Jobs execution.
- Preserves a bounded `/jobs/browser` compatibility redirect.
- Presents the cloud Background runner as the sole customer queue choice.
- Projects Auto-submit and ATS certification through current cloud authority only; retained local
  server authority never becomes web launch availability.
- Removes customer installation, native download, processor selection, protocol-open, local queue,
  setup, and local-plan presentation from the portal.
- Preserves active managed browser sessions, status/progress, interventions, takeover, email-code
  approval, final form review, and exact submit approval.
- Removes the presentation-only pause/resume mutation because it did not command the running
  workflow; durable workflow-command authority remains a separate P0 batch.
- Keeps cloud access truthful when plan, distribution, workflow, or eligibility authority is absent.
- Records the local Browser product as parked P2 work rather than a launch dependency.

**Does NOT:**

- Delete server-side local-run, release, recovery, evidence, or reconciliation authority.
- Merge or publish Phase 607 local Browser updater work.
- Enable cloud or local Browser distribution, model generation, mailbox sync, communication, ATS,
  provider, tenant, or production flags.
- Deploy a service, use a credential, mutate a live tenant, or claim a cloud/ATS canary.
- Complete original-source risk authority, workflow outbox reliability, or resume provenance.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/portal/src/App.tsx` | Modified | Canonical Automation route, cloud-only queue callback, and legacy redirect |
| `jobs/portal/src/api.ts` | Modified | Remove the unused presentation-only browser-session mutation client |
| `jobs/portal/src/components/AppShell.tsx` | Modified | Automation navigation label and route |
| `jobs/portal/src/components/AuthGate.tsx` | Modified | Present the web/cloud-first launch flow before authentication |
| `jobs/portal/src/components/AtsCertificationSummary.tsx` | Modified | Distinguish reviewed-beta cloud automation from exact active certification |
| `jobs/portal/src/views/AutomationView.tsx` and test | Created | Cloud-only customer execution surface with active-run safety preserved |
| `jobs/portal/src/views/BrowserView.tsx` and test | Removed | Delete the install-first/local-run portal surface and its obsolete release tests |
| `jobs/portal/src/lib/browser-release.ts` | Removed | Delete portal-only installer selection and download presentation code |
| `jobs/portal/src/lib/runner-access.ts` and test | Modified | Truthful cloud availability, launch copy, and redirect target |
| `jobs/portal/src/lib/portal-navigation.ts` and test | Created | Normalize bounded preview state and the pre-auth legacy redirect |
| `jobs/portal/src/lib/application-flow.ts` and test | Modified | Require cloud authority, exclude active runs, and bind final review to the exact takeover URL |
| `jobs/portal/src/lib/ats-certification.ts` and test | Modified | Suppress retained local authority and project bounded reviewed-beta truth |
| `jobs/portal/src/views/ApplicationsView.tsx` and test | Modified | Cloud-only availability, preview-safe Automation links, and takeover copy |
| `jobs/portal/src/views/MatchesView.tsx` and test | Modified | Require current cloud runner authority for Auto-submit presentation |
| `jobs/portal/src/views/SettingsView.tsx` | Modified | Replace installed-runner plan language with web/cloud-first truth |
| `jobs/portal/src/data/preview.ts` | Modified | Keep preview state aligned with the cloud-only launch boundary |
| `jobs/portal/src/main.tsx` | Modified | Canonicalize the legacy route before AuthGate renders |
| `jobs/portal/src/styles.css` | Modified | Remove dead installation-only selectors; retain active browser/cloud styles |
| `jobs/OPERATIONS.md` | Modified | Separate cloud-first launch gates from parked P2 local-device gates |
| `CHANGELOG.md` | Modified | Record the cloud-first, no-install product boundary |
| `docs/rounds/ROUND-608-JOBS-CLOUD-FIRST-WEB-AUTOMATION.md` | Created | Objective, contracts, acceptance, and external boundary |
| `docs/work/FIX-686-cloud-first-web-installation-boundary.md` | Created | Root cause and closure of the conflicting install surface |
| `docs/work/IMPL-PHASE-608-JOBS-CLOUD-FIRST-WEB.md` | Created | Implementation handoff |
| `docs/work/REVIEW-PHASE-608-JOBS-CLOUD-FIRST-WEB.md` | Created | Independent review findings, gates, and verdict |
| `web/jobs/index.html`, `web/jobs/assets/*` | Regenerated | Fresh production portal bundle with 26 files and no source maps |

## Build & Test

The frozen local matrix is green:

```text
Jobs workspace                         1,478 tests / 128 files passed
  automation                             644 tests / 35 files
  browser                                219 tests / 34 files
  runner                                 269 tests / 32 files
  workflows                               76 tests /  7 files
  portal                                 270 tests / 20 files
Jobs typecheck/build                       5/5 + 5/5 passed
Portal production bundle               2,292 modules / 26 files / no maps
Portal aggregate SHA-256                20e70aaf4db85a10838ba1a9131963aec5ca79556eedf16d7c787d2afba2f326
Second sequential portal build         byte-identical
Server fmt/strict Clippy                passed; all targets; warnings denied
Server Rust all targets                  1,351 tests passed
Native runner storage                       14 tests + release build passed
Schema parity                               71 tables / 66 indexes passed
Dependency/source provenance             663 entries / 14 repositories passed
Browser release/account-delete guards      10/10 + 3/3 passed
CI/privacy guards                       passed; 2,516 paths / 2,243 text files
Responsive browser QA                   320/330px, desktop, light/dark passed
Independent reviews                     3/3 green; no blocker or minor
Diff/generated/source-map hygiene       passed
```

The unchanged server all-target cross-repository regression gate passed 1,351 tests with zero
failures; the focused 631-test Jobs database module also passed before the final portal-only
freeze. No live PostgreSQL URL or production service was used.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Phase 607 local updater work is parked | The launch product is browser-delivered and cloud-first; customer installation is not justified for the launch path. |
| Server local authority remains | Portal removal must not strand unresolved recovery/evidence and does not authorize a destructive server migration. |

## Known Follow-ups

- Original-source employer identity and job-risk verification authority.
- Transactional workflow command outbox and ambiguity-safe delivery.
- Resume extraction, correction, layout fidelity, and exact provenance closure.
- Mailbox-to-reviewed-reply/calendar proposal intelligence.
- Unified immutable Jobs production verification, canary, promotion, and rollback evidence.
- External cloud infrastructure, ATS/provider certification, and live production authorization.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from Round 608
- [x] Code style matches `AGENTS.md`
- [x] No TODOs without linked task IDs
- [x] Local installation UI is unreachable and removed, not merely visually hidden
- [x] Active browser-run and cloud queue visuals remain intact
- [x] Every external/production gate remains explicit
