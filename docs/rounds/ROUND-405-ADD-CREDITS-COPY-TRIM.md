# Round 405 - Add Credits Copy Trim

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Remove the over-explained amount sentence from the Add Credits modal so the checkout flow feels cleaner and more premium.

## Changes

- Removed the modal-only line that said `$15 adds $15 credits after Square confirms payment.`
- Removed the JavaScript updates that recreated that line when the reload amount changed.
- Kept the shorter trust note in the modal footer: Square handles checkout and Bluey credits after confirmed payment.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Static web deploy to `bluey.sh` with cache key `2026070707`.
- Live `https://bluey.sh/` references `bluey-site.css?v=2026070707` and `bluey-site.js?v=2026070707`.
- Live HTML no longer contains the removed Square-confirmation amount sentence.
- Live JavaScript for `v=2026070707` passes `node --check`.
