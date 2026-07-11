# Round 491 - Jobs Two Portal Application Flow Deep Dive

Date: 2026-07-11
Branch: `codex/bluey-jobs-20260710`
Scope: compare Bluey Jobs screens against inspected competitor onboarding/application/payment surfaces and capture the next UX requirements.

## Evidence captured

Assets live in `ROUND-491-JOBS-TWO-PORTAL-APPLICATION-FLOW-DEEP-DIVE.assets/`.

Captured Bluey surfaces:

- `bluey-matches-desktop.png`
- `bluey-matches-mobile.png`
- `bluey-applications-desktop.png`
- `bluey-resume-desktop.png`
- `bluey-browser-desktop.png`
- `bluey-settings-desktop.png`

Captured competitor surfaces:

- `aiapply-after-social-proof-continue.png`
- `aiapply-direct-step-22.png`
- `applyblast-after-get-my-plan.png`
- `applyblast-checkout-public.png`
- `applyblast-direct-available-start.png`
- `applyblast-direct-email.png`
- `applyblast-direct-stay-ahead.png`
- `applyblast-stripe-or-pay-route.png`

## Main finding

Bluey already has the correct high-level areas: Matches, Applications, Resume, Browser, and Settings. The next gap is not another tab. The gap is first-run confidence: a new user needs to understand what Bluey is doing, which identity/resume/track will be used, how many fresh jobs are eligible, and what will happen next.

## UX changes needed

1. `Matches` should open with a short job-readiness strip: active Career Track, location policy, application identity, daily limit, and freshness window.
2. `Applications` should lead with receipts and state, not raw automation internals.
3. `Resume` should show the current baseline resume, job-specific variants, and unconfirmed suggestions separately.
4. `Browser` should show the local/cloud run state and preserved interventions, not low-level browser implementation.
5. `Settings` should group Answer Memory, identities, exclusions, integrations, and billing controls by user task.
6. Mobile should keep only the most important next action visible; detailed history belongs behind row expansion or a details page.

## Competitive lessons

ApplyBlast and similar flows are aggressive about asking for a plan/payment path early. Bluey should stay calmer, but the payment and plan path still needs to be easy to understand:

- Free should mean profile, tracker, and a small number of reviewed packets.
- Paid plans should explain number of agents, application packets, local browser, and cloud browser.
- Overage should be tied to one completed packet, not each retry.
- Users should know when Bluey is preparing, waiting, or actively applying.

## Required product guardrails

- Every application row must include the exact resume version and identity used.
- Multiple emails require separate browser profiles.
- Same company across SDE/Data Engineering tracks needs a collision warning and user choice.
- Old jobs should be skipped unless the user changes the freshness rule.
- Unknown required questions should become Answer Memory candidates after the user answers them.

## Implementation handoff

This round feeds into Round 490's implementation order. The next UI pass should not add more visual density; it should make the existing five views more decisive:

- One obvious next action.
- One active Career Track summary.
- One application identity context.
- One receipt trail per job.
- One place to answer and remember recurring questions.
