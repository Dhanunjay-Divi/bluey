# Round 414 - Desktop Connect Cue

## Scope

- Continued Bluey web UI work on `codex/bluey-web-ui-parallel-20260704`.
- Focused on making the pending desktop connect/move action clearer.
- Stayed in web/static/docs files only.

## Changes

- Added a subtle action-needed state for the dashboard desktop connect card.
- Added a small "Next step" cue beside the pending Move desktop action.
- Highlighted Connect / Move desktop buttons with a restrained Bluey pulse ring and sweep.
- Added reduced-motion handling and light theme styling for the new cue.
- Bumped static asset cache keys to `2026070716`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Live page cache key check after deploy.
