# Round 158 - Auto-send stop fix

## What changed

- Auto-send mode selection now persists in `UserDefaults` under `bluey.overlay.autoSendStopMode`.
- The pull-down menu items now call the mode setter directly, so selecting Mic, System, or Mic + System is not lost to the compact title row.
- Stopping Listen waits briefly for final captions and retries for a short window before deciding there is nothing to send.
- Stopping from the compact pill now schedules the same auto-send flow as stopping from the expanded window.

## Expected behavior

- Default mode is System auto-send for fresh installs.
- `Don't auto-send` stops listening only.
- `Mic` sends only when mic captions are available after Stop.
- `System` sends only when system audio captions are available after Stop.
- `Mic + System` sends when either source has captions after Stop.

## Verification

- `./native/macos/cue-overlay/build.sh`
- Local visible install refreshed in `~/.bluey/bin`.
- Visible Bluey relaunched with `./scripts/bluey-visible-local.sh`.
