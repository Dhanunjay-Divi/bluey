# Round 349 - Homepage Pinky Element Parity

## Trigger

The owner compared the live Bluey homepage against Pinky's homepage and asked Bluey to match Pinky's UI elements while keeping Bluey's blue color system.

## Root Cause/Fix

- Added the Pinky-style top-right theme toggle pill to Bluey's homepage nav using Bluey's cyan/blue palette.
- Changed the primary hero action pair to the same element pattern as Pinky: `Get Started` and `Try Us`.
- Changed the join form to Pinky's `session code` + `Join` structure while preserving the existing device-code login route.
- Tightened the compact proof notes under the join row to match Pinky's dense two-line element.
- Expanded the bottom homepage action/pricing strip from two cards to three cards: Start free, Add credits, and Teams.
- Kept Bluey-specific product copy and truthful pricing/credit language.
- Bumped the homepage CSS/JS cache keys to `2026070402`.

## Verification

- `node --check web/assets/bluey-site.js`
- Local static server: `python3 -m http.server 8787 --directory web`
- In-app browser visual smoke at desktop screenshot proportions:
  - confirmed logo, theme toggle, Download/Login nav, paired CTAs, session-code row, terminal preview, and three bottom cards are visible
- In-app browser mobile smoke:
  - confirmed the same elements remain usable at a phone-width viewport

## Current State

- Static changes are local and ready to deploy to `bluey.sh`.
- No native overlay, audio, backend runtime, billing API, dashboard app, or release artifact files were changed.

## Remaining QA/Gates

- Deploy static `web/` assets with release-safe excludes.
- Owner visual QA on the live homepage.
