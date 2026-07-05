# Round 371 - Premium UI Compression

## Trigger

The owner asked for a senior-designer pass across Bluey web UI because the landing, download, login, dashboard, reload, and session surfaces still carried too much explanatory weight. The goal was to keep Pinky-like compactness while preserving Bluey colors.

## Root Cause/Fix

- Tightened landing copy to one clear promise, one proof line, and one connect hint.
- Removed redundant top-nav Product text where the Bluey logo already acts as home.
- Kept the signed-in nav centered on the account icon menu and removed the hidden legacy sign-out control.
- Simplified download to one macOS path, a small Windows/Linux note, two terminal steps, and three core commands.
- Compressed dashboard language for balance, computers, sessions, summary, billing, reload, and Auto Reload.
- Made session history read as saved chats that open in a new tab, with less database-style metadata.
- Made connect-code entry compact enough for the login card and dashboard.
- Sharpened Bluey logo/wordmark rendering by removing SVG/CSS glow in dark mode and adding light-mode-only contrast for readability.
- Fixed `#sessions` dashboard hash routing so the Sessions tab opens the session-history panel instead of Computers.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.css web/assets/bluey-site.js web/assets/bluey-logo.svg web/assets/bluey-wordmark.svg`
- Local routed preview at `http://127.0.0.1:4178`:
  - `/` desktop visual pass
  - `/download` desktop visual pass
  - `/account` dark and light theme visual pass
- Static deploy:
  - `rsync -av --delete --exclude 'install.sh' --exclude 'install.ps1' --exclude 'latest.json' --exclude 'latest.json.sig' --exclude 'releases/***' --exclude 'backups/***' web/ root@165.227.77.152:/var/www/bluey/`
- Live smoke:
  - `https://bluey.sh/` serves `bluey-site.css?v=2026070428`, `bluey-site.js?v=2026070428`, and `bluey-logo.svg?v=2026070428`
  - `https://bluey.sh/download` serves the simplified install copy and core commands
  - `https://bluey.sh/health` returns `status=ok`
  - `https://bluey.sh/install.sh` returns `200`
  - `https://bluey.sh/latest.json.sig` returns `200`
  - live `https://bluey.sh/assets/bluey-site.js?v=2026070428` passes `node --check`

## Current State

The live web UI is deployed with cache key `2026070428`. The public pages are quieter, the account/login card is more compact, the download flow is focused on macOS, and session history opens saved sessions in a new tab.

## Remaining QA/Gates

- User should review the live site for final taste calls on logo contrast, copy tone, and dashboard density.
- Native overlay/audio/backend runtime files were intentionally not touched in this round.
