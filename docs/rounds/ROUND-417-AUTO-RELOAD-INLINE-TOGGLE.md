# Round 417 - Auto Reload Inline Toggle

## Scope

- Continued Bluey web UI work on `codex/bluey-web-ui-parallel-20260704`.
- Focused on dashboard Auto Reload toggle behavior.
- Stayed in web/static/docs files only.

## Changes

- Stopped the dashboard Auto Reload switch from opening the Add balance popup.
- Kept Add balance as the only entry point for the Add balance / card setup modal.
- When Auto Reload cannot be enabled inline, the dashboard now restores the switch and shows an inline account message instead.
- Used the account-specific unavailable reason when present, so internal/test accounts do not get misleading Add balance copy.
- Bumped static asset cache keys to `2026070719`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Live page cache key and live JS syntax checks after deploy.
