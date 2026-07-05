# Round 357 - Compact Landing Hero

## Trigger

The owner compared Bluey and Pinky landing screenshots and said Bluey felt too large. They asked to make the landing feel nicer and more compact instead of making every element oversized.

## Fix

- Reduced the Bluey landing hero scale:
  - smaller headline clamp,
  - tighter headline margin and line height,
  - smaller lede/proof copy,
  - shorter primary/secondary buttons,
  - narrower action row,
  - shorter connect-code input and Connect button,
  - smaller helper/status/note text.
- Tightened the landing layout max width and column gap.
- Shortened proof copy while keeping the `Install, run, ask.` message.
- Replaced `saved sessions only by choice` with clearer privacy copy:
  - `session history saved only when you choose`.
- Bumped static web cache keys to `2026070414`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Local route-aware preview server on `127.0.0.1:4178`.
- In-app browser smoke:
  - desktop landing serves cache key `2026070414`,
  - desktop h1 is about `43px`, action buttons are `46px` tall, and the connect bar is `36px` tall,
  - mobile landing h1 is about `35px`, controls remain readable, pricing cards are visible below the hero, and there is no horizontal overflow.
- Static live deploy:
  - `rsync -av --delete ... web/ root@165.227.77.152:/var/www/bluey/`
  - excluded `/install.sh`, `/install.ps1`, `/latest.json`, `/latest.json.sig`, `/releases/***`, and `/backups/***`.
- Live checks:
  - `https://bluey.sh/` serves `bluey-site.css?v=2026070414`, `bluey-site.js?v=2026070414`, compact hero copy, and the clearer session-history note,
  - live `bluey-site.css?v=2026070414` contains the smaller h1 clamp, `46px` buttons, `344px` action/connect widths, and smaller helper/note text,
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json` still return the protected installer/release responses.

## Current State

- Only static web UI files and this round doc changed.
- Static web is live on `bluey.sh`.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.
- The unrelated untracked review handoff file remains untouched.

## Remaining QA/Gates

- Owner visual QA on the compact live landing page.
