# Round 374 - Download Nav Footer Parity

Date: 2026-07-05
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Scope

- Keep Bluey visually closer to Pinky while preserving Bluey colors.
- Keep landing top-right simple: theme, Download, Login, profile when signed in.
- Keep connect-code affordance on non-landing app pages.
- Make the download flow show Mac, Windows, and Linux clearly, with Windows/Linux marked coming soon.
- Push footer meta to the page edge and add Bluey Instagram.

## Changes

- Removed the dashboard self-link from the account header.
- Removed the landing header connect form while keeping the hero connect form.
- Added the compact connect form to download and policy/app headers for signed-in users.
- Added `@bluey.sh` Instagram footer links across landing, download, account, and policy routes.
- Reworked the download platform selector into three compact cards:
  - macOS ready
  - Windows coming soon
  - Linux coming soon
- Improved light-theme contrast for download platform cards, terminal setup, and command rows.
- Bumped the web asset cache key to `2026070503`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.css web/assets/bluey-site.js`
- Local SPA fallback screenshots:
  - `/tmp/bluey-round374-landing-dark.png`
  - `/tmp/bluey-round374-download-dark.png`
  - `/tmp/bluey-round374-download-final.png`
  - `/tmp/bluey-round374-account-dark-viewport.png`

## Notes

- No native overlay, audio, backend runtime, ops, or installer files were edited.
- Existing unrelated ops/log-guard worktree changes were left untouched.
