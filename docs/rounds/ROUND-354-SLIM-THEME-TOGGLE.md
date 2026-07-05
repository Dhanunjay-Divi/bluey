# Round 354 - Slim Theme Toggle

## Trigger

The owner pointed at the Bluey sun/moon theme toggle and asked to decrease its height so it looks cleaner.

## Fix

- Reduced the default theme toggle from `54x28` to `50x24`.
- Reduced the default knob from `22px` to `20px` and tightened icon/shadow scale.
- Reduced the small-screen product/account header toggle from `48x26` to `44x22`.
- Reduced the small-screen knob from `20px` to `18px`.
- Updated the light-theme knob travel distance so the smaller toggle still lands cleanly.
- Bumped static web cache keys to `2026070411`.

## Verification

- `git diff --check`
- Local route-aware preview server on `127.0.0.1:4178`.
- In-app browser smoke:
  - desktop landing toggle renders at `50x24` with a `20px` knob,
  - mobile `/login` account header toggle renders at `44x22` with an `18px` knob,
  - mobile `/login` has no horizontal overflow after the change.
- Static live deploy:
  - `rsync -av --delete ... web/ root@165.227.77.152:/var/www/bluey/`
  - excluded `/install.sh`, `/install.ps1`, `/latest.json`, `/latest.json.sig`, `/releases/***`, and `/backups/***`.
- Live checks:
  - `https://bluey.sh/` serves `bluey-site.css?v=2026070411` and `bluey-site.js?v=2026070411`,
  - live `bluey-site.css?v=2026070411` contains the `50x24` default toggle, `44x22` small-screen toggle, and updated knob travel distances,
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json` still return the protected installer/release responses.

## Current State

- Only static web UI files and this round doc changed.
- Static web is live on `bluey.sh`.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.
- The unrelated untracked review handoff file remains untouched.

## Remaining QA/Gates

- Owner visual QA on the live header toggle.
