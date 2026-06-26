# Round 063 - Paid Alpha Closure Pass — 2026-06-19

## Why This Round Exists

The codebase already has billing, account, routing, signed-update, managed-AI,
and release hardening in place. The remaining risk was operational: tomorrow's
live test still needed crisp gates for money-path proof, fraud/chargeback
handling, and release hygiene.

## What Changed

- Added `docs/deploy/PAID-ALPHA-SMOKE.md`:
  - clean install,
  - account/reload,
  - live captions,
  - answers/canvas,
  - screen/docs,
  - sessions/memory,
  - support bundle pass/fail criteria.
- Added `docs/deploy/ABUSE-FRAUD-CHARGEBACK-PLAYBOOK.md`:
  - dispute response flow,
  - evidence to preserve,
  - paid-alpha guardrails,
  - operational checklist,
  - explicit "do not" list.
- Added `scripts/release-hygiene-scan.sh`:
  - scans release-facing files for real-looking provider/Square/Resend/JWT secrets,
  - fails on production-unsafe debug flag assignments in release-facing surfaces,
  - excludes local DBs/build trees and warns for allowed documentation mentions.
- Updated `docs/deploy/SQUARE-BILLING.md` with webhook reliability, idempotency,
  paid-alpha proof, and dispute/chargeback links.
- Updated `docs/PRELAUNCH-CHECKLIST.md` and `docs/SECURITY-HARDENING.md` so the
  master launch gate points at the new smoke/playbook/hygiene checks.

## What This Closes

- The operator now has a direct checklist for proving live paid credits, webhook
  delivery, real captions, answer cost movement, docs, sessions, and support logs.
- Abuse and chargeback handling is no longer just "we should think about it";
  there is a concrete operational response path.
- Release hygiene is repeatable before site/release deployment.

## What This Does Not Magically Close

- Live Square reload proof still requires a sandbox/live transaction and Square
  dashboard verification.
- Real provider proof still requires funded OpenAI/Anthropic/Deepgram accounts.
- Local binaries still cannot be made unreverse-engineerable. Bluey's security
  boundary remains the server, signed updates, and managed provider access.

## Verification

```bash
bash -n scripts/release-hygiene-scan.sh
bash scripts/release-hygiene-scan.sh
git diff --check
```

## Most Likely Follow-Up

Run `docs/deploy/PAID-ALPHA-SMOKE.md` end-to-end with a real account and credits,
then attach the screenshots/log paths to the next round doc.
