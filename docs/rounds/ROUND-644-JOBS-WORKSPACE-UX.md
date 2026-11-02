# Round644 — Jobs workspace visual and navigation foundation

Baseba24cc3d2108d5e8c7d3ffdb061f007e92b065bb; branch
`feat/phase-644-jobs-workspace-ux`. Owner requested a full Jobs website revamp.
This is its first bounded implementation slice, not a complete product claim.

## Authority and direction

Current Jobs code/local handoff, AGENTS and `bluey-ops` are authoritative.
Use the existing shared Codex project toolkit: native bounded workers, separate
review, project-scoped Ruflo decisions and the existing Asana project. No new
installer/runtime, hidden hooks, provider-account change or duplicate board.
UI/UX Pro Max guides design and accessibility; preserve the established React19,
Vite, Lucide and CSS-token stack. No framework migration or external font CDN.

The Jobs-specific experience includes public entry/onboarding, search/matches,
resume review, applications/tracking, automation/inbox and settings. Implement
these in reviewed feature batches. Do not change shared Bluey root marketing,
desktop/overlay/Pinky or production deployment as part of this request.

## This batch

1. Make a visibly improved professional workspace shell: persistent labelled
   desktop sidebar, calmer compact utility header and clear active navigation.
   Keep every desktop route reachable; use the existing five mobile destinations
   with Settings/account reachable through the account menu. Preserve all route,
   deep-link, query/preview, sign-out, refresh, balance and intervention behavior.
2. Give Overview one clear data-derived next action and a useful hierarchy of
   pending work, setup, matches and outcomes. Keep every existing authority/
   source-coverage caveat and action; no dashboard button starts execution.
3. Improve semantic light/dark tokens, readable typography, shrinkable content,
   focus/touch/keyboard affordances and responsive layout without affecting
   resume facts, server authority, API contracts, pricing or execution gates.
4. Maintain real loading/empty/error/unavailable states. Never replace unavailable
   integrations or runner capability with optimistic decorative status.

## Ownership

Existing Terra builder owns AppShell.tsx, styles.css, CareerCommandCenterView.tsx,
its CSS, App.test.ts and CareerCommandCenterView.test.tsx. Parent owns this plan,
design-system notes, CHANGELOG/IMPL/REVIEW, test controllers, generated assets,
browser QA and checkpoint/CI. Separate review checks source and observed evidence.
No concurrent edits to owned files; preserve all other agents' work.

## Acceptance

Focused and full relevant portal regressions, typecheck/production build,
deterministic generated assets/privacy and independent review. Actual browser
checks at375/768/1024/1440 CSS widths, keyboard skip/menu/Escape/focus, both
themes and reduced motion; no horizontal overflow or inaccessible destinations.
Check all six existing routes, representative modals and truthful status states.
Measure contrast for actual composed foreground/background pairs; do not infer
accessibility from a palette name or static source-string tests.

No deployment, publication, merge, real application/email, credential/OAuth scope,
paid infrastructure or production flag change. Cloud auto-apply is still required
for v1 but is not qualified by a visual revamp. Asana1220128420788075 tracks the
whole website effort and remains incomplete beyond this bounded slice.
