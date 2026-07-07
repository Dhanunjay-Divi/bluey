# Round 401 - Connect Code Helper Line

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Trigger

The dashboard `Connect Bluey desktop` card put the helper sentence beside the code field, which made the compact card feel uneven.

## Changes

- Moved the helper sentence below the connect-code input and button row.
- Added a dedicated `.device-code-helper` class so helper copy and validation status can be styled separately.
- Tightened the connect card grid while preserving a clean single-column layout on mobile.
- Bumped static web assets to `2026070703`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Live static deploy checks:
  - `https://bluey.sh/` references `bluey-site.css?v=2026070703`
  - `https://bluey.sh/` references `bluey-site.js?v=2026070703`
  - live JS contains `.device-code-helper` and the Bluey host overlay helper sentence
  - live CSS contains the tighter two-column connect grid and helper-line placement

## Current State

The connect-code input is now the visual anchor, with the explanatory sentence tucked underneath it like helper text.

## Remaining QA/Gates

- Visual smoke on a signed-in browser session to confirm the live dashboard spacing matches the intended compact layout.
