# PR Comment Safety Fixes - 2026-06-20

## Scope

This round handled the non-dependent review comments that could be safely fixed in one pass without touching `bluey-dev.db` or using GitHub Actions:

- Browser account/session safety on the Bluey web UI.
- Reload CTA behavior for signed-in users.
- Square webhook/idempotency correctness for live billing and auto reload.
- Gemini streaming completion safety before billing/caching.

## Changes

### Web account session safety

- Added `/auth/logout` and wired browser sign-out to revoke the current browser refresh token before clearing local storage.
- Serialized browser refresh-token refreshes so parallel account dashboard requests do not race a single-use refresh token.
- Stopped clearing browser tokens for generic account-load failures; real auth expiry is handled inside `apiJson`.
- Device-code links now require explicit signed-in user confirmation before `/auth/device/approve`.
- The landing Add Credits CTA now opens `/reload?checkout=1`, and `/reload` starts checkout only for that explicit CTA path.
- Recomputed the `web/assets/bluey-site.js` SHA-384 SRI in `web/index.html`.

### Square billing safety

- Replaced long Square idempotency keys for customer, card, reload checkout, and Square auto reload payment calls with short stable keys under Square's 45-character cap.
- Made unrelated completed `payment.updated` events a no-op instead of a 500. This should prevent Square from retrying non-Bluey payment events against the Bluey webhook.
- Added tests for the Square idempotency-key length guard and non-Bluey payment no-op behavior.

### Gemini streaming safety

- Gemini streaming now requires a terminal marker before yielding `Done`.
- The terminal signal can be `[DONE]` or a Gemini candidate `finishReason`.
- A partial stream with usage metadata but no terminal signal now errors before billing/caching.

## Confirmed Already Covered In Current Tree

These pasted review comments were checked and already had code-level protection before this round:

- OpenAI stream completion requires `[DONE]` and final usage before billing.
- Anthropic stream completion requires `message_stop` and final usage before billing.
- Square sandbox webhooks are rejected when the active billing environment is production.
- Environment-specific Square webhook keys are preferred before the generic fallback.
- Verified updates do not inherit `BLUEY_SKIP_CHECKSUM`.
- CLI logout notifies the running daemon through `CloudLogout`.
- Balance polling starts/restarts after account linking.

## Still Separate Rounds

The pasted comment batch includes larger architecture items that should not be mixed into this web/billing safety pass:

- Stream disconnect accounting/reservation reconciliation.
- Expiring/refunding STT reservations that never reach relay settlement.
- RAG tombstones and stale indexing cancellation.
- Document conversion offloading and Markdown cleanup on attachment/session removal.
- Windows artifact publication gating and installer process handling.
- Full overlay clickability and live-caption visual smoke on Mac.

Those need focused implementation plus their own round docs and review docs.

## Verification

Completed verification for this round:

- `cargo fmt --all --check`
- `git diff --check`
- `node --check web/assets/bluey-site.js`
- `cargo test --manifest-path server/Cargo.toml --lib` - 145 passed
- `bash scripts/observability-acceptance-smoke.sh` - PASS
