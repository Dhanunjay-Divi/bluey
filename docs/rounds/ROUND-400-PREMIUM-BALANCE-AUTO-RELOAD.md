# Round 400 - Premium Balance Auto Reload

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Trigger

The dashboard balance card felt visually scattered: `$0.00` sat too far from the balance copy, Auto Reload looked off by default, and the zero-balance sentence sounded awkward.

## Changes

- Tightened the balance card layout so the balance amount stays with the balance copy instead of drifting toward the middle of the card.
- Replaced `Add credits when you need paid cloud work.` with clearer wallet copy:
  `Add credits to start. This balance is shared across your Bluey account.`
- Made Auto Reload present as ready/on by default when a card can be saved but is not connected yet.
- Updated dashboard Auto Reload copy to explain that saving a card during Add Credits activates automatic reloads.
- Updated the Add Credits modal to default Auto Reload on and explain the first-payment flow more clearly.
- Bumped static web assets to `2026070702`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Localhost visual mock:
  - zero-balance card showed `$0.00` near the balance copy
  - Auto Reload toggle rendered checked
  - Add Credits modal rendered Auto Reload checked by default

## Current State

The balance/reload entry point now reads more like a premium wallet setup: add credits first, keep Auto Reload ready, and save a card during checkout to activate future automatic reloads.

## Remaining QA/Gates

- Deploy static web assets to `bluey.sh` and smoke `/account` after deploy.
