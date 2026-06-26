# Round 030 - Web Account + Download UX Pass — 2026-06-04

## Scope

Cleaned the public Bluey web surface so it feels closer to the simple Pinky-style flow:

- product page stays focused on `bluey on`
- account page is for sign-in, reload, usage, and synced sessions
- download page is the single place for platform availability
- privacy/terms/controls live in footer-style links instead of competing in top nav

## Changes

- Removed the accidental rounded capsule around the Bluey wordmark by treating the wordmark SVG separately from icon images.
- Tightened the device-code link form so it reads like a compact command input rather than a giant hero control.
- Added `/download` as a routed page with explicit macOS and Windows cards.
- Kept macOS marked as the current alpha path.
- Kept Windows visible as the planned parity path, but not falsely marked as available.
- Simplified the account rail to Account / Credits / Sessions and moved policy links to the lower rail.
- Reworded account copy around what customers actually do: sign in, reload credits, review synced sessions, then run `bluey on`.
- Added copy-to-clipboard handling for install commands.

## Verification

- Parsed `web/index.html` with Python's `HTMLParser`; no tag balance errors.
- Rendered the product page through Quick Look for a visual sanity check.
- Checked `/download` routing through the local static fallback server.
- No provider keys or customer secrets were written to repository files.

## Notes For The Next Agent

- Do not paste or commit live provider keys. Configure them only in the server environment or secret manager.
- `OPENAI_API_KEYS`, `ANTHROPIC_API_KEYS`, and `DEEPGRAM_API_KEYS` are supported by the server.
- Gemini is not wired into the managed server routing config yet; adding it requires a provider adapter and routing policy update.
- The Windows card is intentionally honest until the public Windows installer and clean-machine smoke are ready.
