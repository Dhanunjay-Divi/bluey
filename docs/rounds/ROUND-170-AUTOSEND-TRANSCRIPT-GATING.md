# Round 170 - Auto-send Transcript Gating

Date: 2026-06-25

## Goal

Stop Bluey from sending paid answers when Listen is stopped but no new Mic or System transcript was captured.

## Changes

- Auto-send now keeps a separate per-listen-run transcript buffer.
- The buffer starts only when Listen starts from the expanded window or compact pill.
- Auto-send sends only text captured from the selected source mode: Mic, System, or Mic + System.
- If the selected source captured no text, Bluey does not send a question.
- The auto-send buffer clears after send, explicit clear, or repeated empty stop checks.
- Final transcript duplicates are collapsed even when the same final text arrives through more than one overlay path.
- Blank manual Answer no longer uses old saved conversation files by itself. It can still use a fresh screen capture or freshly attached pending files.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/macos/cue-overlay/build.sh`
- Installed the rebuilt overlay into `~/.bluey/bin`.
- Restarted Bluey with `./scripts/bluey-visible-local.sh`.
- Confirmed the daemon is running in local visible overlay mode with zero transcript segments in the fresh session.
