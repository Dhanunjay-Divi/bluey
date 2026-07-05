# Round 389 - Dashboard Refresh Tab Stickiness

## Goal

Keep the selected dashboard tab stable across refreshes, even after opening a session detail URL.

## Changes

- Stopped `?session=...` from forcing Session History when the URL hash points to another dashboard tab.
- Kept direct session URLs working when there is no tab hash or the hash is `#history`.
- Removed stale `session` query parameters when switching away from Session History.
- Removed the old popup fallback click handler now that session rows use normal new-tab links.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/assets/bluey-site.js`
