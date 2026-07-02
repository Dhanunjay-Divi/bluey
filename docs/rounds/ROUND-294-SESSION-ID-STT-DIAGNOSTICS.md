# ROUND-294 Session ID + STT Diagnostics

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User Report

The user still was not getting live transcript for each session and could not point to a failing run because there was no visible session id in the overlay or web dashboard.

The attached recording showed:

- Listen active with no user-visible session id.
- Live captions not appearing.
- A visible warning: `Deepgram relay frame parse failed: protocol violation: invalid Deepgram JSON: invalid type: integer 0...`

## Root Cause

- Bluey created a `MeetingRecord` lazily after the first final transcript segment. If STT failed before the first transcript, there was no stable session id for the user to report.
- The Deepgram relay parser tried to deserialize every relay frame as a transcript `Results` shape. Deepgram control frames like `SpeechStarted` and `UtteranceEnd` may send `channel: 0` or `channel: [0,1]`, which should be ignored rather than treated as fatal transcript JSON.
- The web dashboard listed saved sessions but did not expose a simple support id in the main session list.

## Changes

- Bumped desktop workspace version to `0.1.48`.
- Added `MeetingDiagnostics` to `MeetingRecord` with sanitized counters and last-error metadata:
  - listen runs
  - STT parse errors
  - STT provider errors
  - audio start/source errors
  - last audio session id
  - last STT provider
  - last sanitized error kind/message/time
- Added `short_session_code` / `MeetingRecord::session_code()` for stable 8-character support codes.
- Ensured an active meeting/session is created and saved as soon as Listen starts preparing, before any transcript arrives.
- Recorded listen-start diagnostics once audio capture is actually active.
- Recorded sanitized session diagnostics for audio runtime, relay, and source failures.
- Added `set_active_session` overlay command.
- Mac overlay:
  - Shows `ID XXXXXXXX` in the header when a session is active.
  - Shows the same session id in History rows.
  - Restores the session id after temporary "Answer streaming" status finishes.
- Windows overlay:
  - Accepts `set_active_session`.
  - Shows `ID XXXXXXXX` in the header.
- Web dashboard:
  - Shows `ID XXXXXXXX` in Saved Sessions rows.
  - Adds `Copy ID` / `Copy full session ID`.
  - Shows synced diagnostics in session details when available.
- Cloud sync:
  - Includes `session_code` and sanitized diagnostics in session metadata.
- Deepgram parser:
  - Ignores non-transcript control frames like `SpeechStarted` and `UtteranceEnd`.
  - Ignores non-object `channel` values on non-transcript frames.
  - Still surfaces real Deepgram `Error` frames as provider errors.

## Verification

- `cargo fmt --all`
- `cargo test -p cue-core overlay --lib`
- `cargo test -p cue-daemon deepgram --lib`
- `cargo check -p cue-core --quiet`
- `cargo check -p cue-daemon --quiet`
- `cargo check --manifest-path server/Cargo.toml --quiet`
- `swift build -c debug --package-path native/macos/cue-overlay`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c ...`
- `node --check web/assets/bluey-site.js`
- `git diff --check`
- `BLUEY_UPDATE_PUBKEY=... make package-darwin-arm64`
- `BLUEY_RELEASE_SIGNING_KEY_FILE=... PUBLISH_DO=1 ... scripts/deploy-bluey-sh-manual.sh`
- Live checks:
  - `https://bluey.sh/latest.json` reports `0.1.48`
  - `latest.json.sig` verifies successfully with the release Ed25519 public key
  - live artifact SHA256:
    `68eb1993e58d0485b9fa07c0c8d43c3842967a92870b73d75dc43ed86a3b4403`
  - `/install.sh` returns `application/x-shellscript`
  - `/install.ps1` returns `application/x-powershell`
  - unpacked release reports `bluey 0.1.48` and `bluey-daemon 0.1.48`
  - release artifact scan passed with no configured secrets/dev flags present

## Live Artifact

`https://bluey.sh/releases/v0.1.48/bluey-0.1.48-darwin-arm64.tar.gz`

## Follow-Up

- Run a live Listen smoke test after deployment and confirm:
  - Header shows `ID XXXXXXXX`.
  - History shows the active session even if no transcript arrives.
  - Web dashboard shows the same id after sync.
  - Deepgram control frames no longer break live captions.
