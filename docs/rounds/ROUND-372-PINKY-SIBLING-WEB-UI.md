# Round 372 - Pinky Sibling Web UI

## Trigger

The owner felt Bluey's web UI still did not match Pinky's style or vibe closely enough. The requested direction was to make Bluey feel like the same product family while keeping Bluey's colors and product wording.

## Root Cause/Fix

- Bluey's landing page had Pinky-inspired pieces, but the proportions had drifted: smaller hero type, different spacing rhythm, a missing pricing strip, and heavier account/dashboard surfaces.
- Re-aligned the landing layout to Pinky's desktop rhythm: 1040px hero/terminal content, 500px terminal preview, 46px CTAs, compact connect field, and the three-card bottom strip.
- Restored the compact landing footer cards for Start free, Add credits, and Teams with Bluey copy.
- Tightened shared nav controls, account icon sizing, connect-code inputs, account auth card spacing, and dashboard overview/tabs to feel more like Pinky.
- Clarified temporary-account balance copy by showing "Trial time" only for temporary trials and keeping normal accounts as "Balance".
- Saturated the Bluey wordmark gradient so the logo stays crisp in light and dark themes without adding blur/shadow.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.css web/assets/bluey-site.js web/assets/bluey-wordmark.svg`
- In-app browser visual checks:
  - desktop landing in dark theme
  - desktop landing in light theme
  - mobile landing
  - mobile account/login with pending desktop-code panel

## Current State

- The landing page now matches Pinky's overall silhouette more closely: compact top nav, centered hero/terminal pair, visible bottom offer strip, and small footer meta.
- Account login remains Bluey-branded but uses Pinky-like card density.
- Dashboard styling is quieter and closer to Pinky's simple tab/card rhythm.
- Trial protection remains admin-only under the Admin tab.

## Remaining QA/Gates

- Verify the signed-in dashboard against real account data after live deploy, especially the Balance/Auto Reload row and saved-session tab.
- Continue avoiding native overlay, audio, and backend runtime changes for this web-only UI branch.
