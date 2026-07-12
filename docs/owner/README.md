# Bluey Owner Docs

Updated: 2026-07-12

This folder is the owner-facing map for Bluey's current state. It exists because
the repo has many useful historical docs, but several older docs now conflict
with the current Rust server, Square billing, deployed `bluey.sh`, and managed
provider path.

## Read First

1. `docs/reviews/CODEX-BLUEY-SENIOR-REVIEW-20260626.md`
2. `docs/owner/BLUEY-POSITIONING-AND-MARKETING-PLAN.md`
3. `docs/owner/BLUEY-LAUNCH-GATES-AND-RISK-REGISTER.md`
4. `docs/deploy/FIRST-100-PAID-USERS.md`
5. `docs/deploy/PAID-ALPHA-SMOKE.md`
6. `docs/deploy/ABUSE-FRAUD-CHARGEBACK-PLAYBOOK.md`

## Current Working Position

Bluey should be positioned as:

> The live context bridge for engineering meetings.

Bluey should help users answer from the live conversation, current screen/page,
attached docs, repo/project context, prior decisions, and managed model routing.

Do not lead with stealth. Best-effort screen-share capture exclusion is an
explicit privacy control, not the main product identity or an invisibility
guarantee.

## Current Launch Posture

Current public release record:

- The signed manifest reports `0.1.99` for macOS Apple silicon and Windows
  x86-64. Its release record install-smokes Windows 11; it does not claim
  Windows 10, macOS Intel/universal, or Linux support.
- Signed manifest and checksum verification are distinct from Apple Developer
  ID/notarization or Windows Authenticode signing.
- Cloud sync and Auto Reload are separate opt-in choices. Sign-in alone must not
  enable either one.
- Raw audio is processed transiently; there is no retained raw-audio library or
  customer-content training option in the current product.

Do not expand trial or broad marketing until AI cost reservation is closed for
LLM, embeddings, and chunked transcription.

## Stale Doc Warning

Treat these docs as historical unless updated:

- `ARCHITECTURE.md`
- `SERVER-REFERENCE.md`
- Older sections of `AGENT-HANDOFF.md`
- Older sections of `docs/OPERATIONS-RUNBOOK.md`
- Older sections of `docs/PRODUCTION-READINESS.md`

Known drift:

- Some old docs say product server/distribution are not provisioned.
- Some old docs reference Stripe as primary billing.
- Some old docs reference a Go server or separate server repo.
- Some old docs conflict on `$15` versus `$30` reload language.

## Owner Decisions Needed

- Confirm first ICP: engineering meetings is the recommended wedge.
- Decide whether macOS Intel/universal or Linux should enter the release matrix.
- Keep reload amount and payment-provider copy sourced from the live product
  configuration rather than owner-doc defaults.
- Confirm legal-hold policy for billing/usage evidence after deletion.
- Confirm provider daily spend budget for trial and paid alpha.
- Confirm when public Product Hunt/social/community launch should happen.

## Keep Updated

For every meaningful product, billing, reliability, launch, or ops change:

- Add a round doc under `docs/rounds/`.
- Add a review or gate report under `docs/reviews/` when there is a formal
  review, deploy gate, or production-risk decision.
- Keep this folder as the short owner map, not a replacement for detailed docs.
