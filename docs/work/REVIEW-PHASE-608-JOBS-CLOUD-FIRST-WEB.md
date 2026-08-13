# REVIEW: PHASE-608 - Jobs Cloud-First Web Automation

> **Codex preflight:** Loaded `$bluey-ops` and reviewed the Phase 608 successor worktree directly.
> No SSD/archive, production service, provider credential, live tenant, deployment, or external
> write was used.

**Reviewed snapshot:** `84db81dd5e4f383b625eecc8dd0686e5d37ab222` on
`feat/phase-608-jobs-cloud-first-web` (`b1ed19024b2a24488771e2cef6328bb655274fbb..84db81dd5e4f383b625eecc8dd0686e5d37ab222`)

**Reviewers:** three independent agents covering source authority, docs/tests/generated output,
and route/UX/accessibility behavior; final evidence assembled by the root agent

**Date:** 2026-08-13

## Per-Task Review

### Canonical Automation route and bounded redirect

| Field | Value |
|-------|-------|
| Files | `App.tsx`, `main.tsx`, `AppShell.tsx`, `AuthGate.tsx`, `portal-navigation.ts` and test |
| Verdict | 🟢 accept |

**Findings:**

- `/jobs/automation` is canonical. `/jobs/browser` is normalized before AuthGate renders, retains
  only `preview=1` plus one allowlisted scenario, and drops unknown/private parameters.
- Navigation, Applications fallbacks, and Automation fallbacks preserve the bounded preview state.
- Browser QA verified both an allowlisted `runner-beta` redirect and an unknown-scenario redirect.

### Cloud-only admission and approval authority

| Field | Value |
|-------|-------|
| Files | `AutomationView.tsx` and test, `application-flow.ts` and test, `App.tsx`, `api.ts` |
| Verdict | 🟢 accept |

**Findings:**

- Queue presentation and the App callback independently require a queued application, server
  cloud availability, cloud eligibility, and no nonterminal run for the same application.
- Final approval requires the exact current intervention ID and active takeover URL. Replacement
  of valid review target A by valid target B invalidates A's checkbox confirmation.
- Email-code approval, scoped takeover, empty queue, queue error, and final confirmation are
  covered. The prior pause/resume control was removed because it mutated presentation state without
  signaling the live workflow.
- Historical local sessions remain visible only as retained device-session recovery; new execution
  is never presented as local.

### Web launch truth, installation removal, and responsive behavior

| Field | Value |
|-------|-------|
| Files | ATS/runner helpers and tests, Applications/Matches/Settings/preview files, styles, generated portal |
| Verdict | 🟢 accept |

**Findings:**

- Every portal projection forces retained local queue authority false. Reviewed Greenhouse/Lever
  beta is labeled as cloud-capable with mandatory final review, while exact active certification
  remains distinct and local-only/stale authority stays Review-only.
- The install-first Browser view, installer release helper, local queue callback, protocol launch,
  architecture picker, setup prompts, and local-plan copy are removed rather than hidden.
- The decorative browser preview is `aria-hidden`, introduces no nested landmark, and the active
  card exposes one accessible company/status surface.
- At 320px and 330px, document width equals viewport width and the active card, preview, and control
  have equal client/scroll widths. Desktop, light/dark, queue dialog, final dialog, and an empty
  console were also verified.

## Cross-Task Findings

- Independent review initially found and drove closure of stale queue projection, fake
  pause/resume, final-approval lineage, provider-copy, nested-landmark, preview-query, local-session
  labeling, reviewed-beta ATS truth, mobile overflow, and stale generated-output defects.
- All three final independent verdicts are green with no blocker or minor finding.
- Retained server/local authority is recovery compatibility, not a customer launch path.
- Source completion does not certify infrastructure, an ATS tenant, a production flag, or a live
  rollout.

## Build & Test Verification

```text
Jobs workspace                         1,478 tests / 128 files passed
Jobs typecheck/build                       5/5 + 5/5 passed
Portal                                  270 tests / 20 files passed
Portal bundle                          2,292 modules / 26 files / no maps
Portal aggregate SHA-256               20e70aaf4db85a10838ba1a9131963aec5ca79556eedf16d7c787d2afba2f326
Second sequential portal build        byte-identical
Server fmt/strict Clippy               passed; all targets; warnings denied
Server Rust all targets               1,351 tests passed
Server Jobs DB focused run              631 tests passed
Native runner storage                    14 tests + release build passed
Schema parity                             71 tables / 66 indexes passed
Dependency/source provenance           663 entries / 14 repositories passed
Browser release/account delete          10/10 + 3/3 passed
CI/privacy guards                      passed; 2,516 paths / 2,243 text files
Responsive/browser QA                  passed; 320/330px, desktop, light/dark
Generated asset graph                  complete; no maps or missing references
Diff hygiene                           passed
Independent final review               3/3 green; no blocker or minor
```

The complete unchanged server all-target suite passed 1,351 tests as a cross-repository regression
gate. No live PostgreSQL URL, hosted runner, credential, provider, tenant, or production service
was used, so those remain external evidence rather than local claims.

## Overall Verdict

🟢 **ACCEPT - SOURCE COMPLETE AND LOCALLY VERIFIED.** Phase 608 meets its source acceptance
criteria in the frozen worktree. Merge still requires fresh hosted exact-SHA checks. No cloud
distribution enablement, ATS certification, credential, provider write, tenant mutation,
deployment, canary, or production flag authority is claimed.

## Follow-ups for Next Batch

- Transactional workflow command outbox and ambiguity-safe start/resume delivery.
- Durable original-source employer/risk authority for cloud queue admission.
- Resume truth/provenance and source-layout fidelity.
- Reviewed mailbox/calendar proposal intelligence and production deployment authority.
