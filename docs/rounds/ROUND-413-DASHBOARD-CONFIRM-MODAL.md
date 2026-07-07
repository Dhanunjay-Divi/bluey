# Round 413 - Dashboard Confirm Modal

## Scope

- Continued Bluey web UI work on `codex/bluey-web-ui-parallel-20260704`.
- Focused on the account dashboard device-removal confirmation flow.
- Stayed in web/static/docs files only.

## Changes

- Replaced browser-native `window.confirm` prompts for removing one computer or all computers with a compact Bluey-styled modal.
- Added clear destructive-action copy:
  - removing a computer signs Bluey out on that desktop;
  - saved chats stay in Session History;
  - the desktop can be connected again from the host overlay.
- Added light/dark theme styles for the new confirmation modal.
- Bumped static asset cache keys to `2026070715`.

## Verification

- `node --check web/assets/bluey-site.js`
- `rg -n "window\\.confirm" web/assets/bluey-site.js web/index.html`
- `git diff --check`
