# Round 415 - Desktop Connect Instant List

## Scope

- Continued Bluey web UI work on `codex/bluey-web-ui-parallel-20260704`.
- Focused on the dashboard desktop handoff after clicking Connect desktop.
- Stayed in web/static/docs files only.

## Changes

- Renamed the pending handoff action from "Move desktop" to "Connect desktop".
- Changed the handoff copy to say the desktop connects to this account's shared balance.
- After approval, the dashboard now shows a connecting state and polls the linked-desktop list briefly instead of leaving the old empty list visible.
- Added a subtle loading treatment for the temporary waiting row in light and dark themes.
- Bumped static asset cache keys to `2026070717`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Live page cache key and live JS syntax checks after deploy.
