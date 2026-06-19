# Review: Paid Alpha Closure Pass

Verdict: 🟡 ACCEPT WITH OPERATOR GATE

## Findings

- No code blocker found in this pass because the changes are docs and a hygiene
  script only.
- The launch gate remains yellow until a real paid smoke proves Square crediting,
  live STT, answer billing, docs, sessions, and support logs against the deployed
  server.

## What I Checked

- The smoke checklist covers the actual customer flow from fresh install through
  paid reload, captions, answers, docs, sessions, and support export.
- The chargeback playbook keeps Square-hosted checkout as the card-data boundary
  and tells operators what evidence to preserve without dumping transcript
  contents by default.
- The hygiene scan focuses on real secret-shaped values and production-unsafe
  debug assignments rather than noisy broad words like `key`.
- The security wording stays honest: binaries can be inspected; server-side
  provider access and signed updates are the boundary.

## Residual Risk

- Dispute automation is still manual for alpha. That is acceptable only while
  reload volume is low and an operator reviews Square notices quickly.
- The hygiene scan is a guardrail, not a full SAST/secret-scanning platform.
  Add provider-native secret scanning before public repositories or larger team
  access.

## Required Next Proof

Run `docs/deploy/PAID-ALPHA-SMOKE.md` on the clean Mac and production droplet.
Attach:

- Square event ID,
- account email hash or test account email,
- trace ID,
- support zip path,
- before/after balance screenshots.
