# Round 289 - Tab Order and Composer Tab Fix

## Trigger

The owner reported two follow-up issues after Round 288:

- The keyboard shortcuts icon and theme icon were selected in the reverse order from how they appear visually.
- After selecting the Ask text box, pressing `Tab` inserted tab whitespace or restarted focus from the first header control instead of moving to the next Bluey control.

## Root Cause

- The macOS keyboard focus order had `themeButton` before `shortcutsButton`, while the visible header lays out the keyboard shortcuts icon before the theme icon.
- The Bluey key router handled `Tab`, but the composer `NSTextView` did not have its own fallback interception. If AppKit delivered the event directly to the composer, `NSTextView` inserted a tab character.
- When the composer was focused, Bluey's custom keyboard focus state was intentionally cleared. Pressing `Tab` from that state had no current control anchor, so navigation started from the first control.

## Fix

- Reordered the macOS header Tab order to match the visible header:
  - history
  - new session
  - move handle when visible
  - canvas
  - keyboard shortcuts
  - theme
  - full screen
  - click-through
  - pill
  - close
- Added a composer-level `Tab` fallback so plain `Tab` / `Shift+Tab` navigates Bluey controls instead of inserting whitespace.
- Anchored composer-originated Tab navigation to the Ask input surface, so:
  - `Tab` from Ask moves to the next control after Ask
  - `Shift+Tab` from Ask moves to the previous control before Ask
- Bumped desktop workspace version to `0.1.43`.

## Mac/Windows Parity

This was a macOS-specific follow-up:

- The reverse keyboard/theme issue was in the macOS custom focus order.
- The tab-character issue was caused by macOS `NSTextView` behavior.
- Windows already uses native edit/combo focus from Round 288 and was syntax-checked again in this round.

## Verification

Passed locally:

```bash
swift build -c debug --package-path native/macos/cue-overlay
cargo check -p cue-daemon --quiet
x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
```

Release verification:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

Live checks passed:

- `https://bluey.sh/latest.json` reports version `0.1.43`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live `darwin-arm64` artifact:
  `releases/v0.1.43/bluey-0.1.43-darwin-arm64.tar.gz`
- Live/local artifact SHA256:
  `4a69b176f028c1abbf3b140bcbf4f9b0067cfd277e08aac5d311df8515b81b16`
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.

## Current State

- macOS `0.1.43` is live and downloadable from the droplet.
- Windows source remains syntax-checked.

## Remaining QA

- Owner should run `bluey off && bluey on`, confirm update to `0.1.43`, then test:
  - Header Tab order selects keyboard shortcuts before theme.
  - `Tab` inside Ask does not insert whitespace.
  - `Tab` from Ask moves forward from Ask instead of restarting at History.
  - `Shift+Tab` from Ask moves backward from Ask.
