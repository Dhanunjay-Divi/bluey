# Round 416 - Auto Reload Default Warning

## Scope

- Continued Bluey web UI work on `codex/bluey-web-ui-parallel-20260704`.
- Focused on the Add balance / Auto Reload setup flow.
- Stayed in web/static/docs files only.

## Changes

- Kept Auto Reload selected by default for regular accounts that can save a card, unless the user explicitly turns it off.
- Added a Bluey-styled confirmation when a user turns Auto Reload off, warning that balance can run out during calls, interviews, or long conversations.
- Remembered a confirmed no-card Auto Reload opt-out per browser/account so the setup does not flip back on after refresh.
- Changed the Add balance modal Auto Reload label from "Optional" to "Recommended".
- Bumped static asset cache keys to `2026070718`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Live page cache key and live JS syntax checks after deploy.
