# Round 348 - Streaming Attachment Events

Date: 2026-07-04
Branch: `codex/bluey-stream-attachments-20260704`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner reported that while Bluey is streaming an answer, attaching screen context or adding documents does not work. The expected behavior is that the current answer can keep streaming while the user prepares the next question with a fresh screenshot or document.

## Root Cause

The daemon's overlay event loop handled events serially. `AskRequested` awaited the full streamed answer before returning to the loop, so later overlay events such as `AttachRequested`, `AttachFilesRequested`, and `AnalyzeScreenRequested` sat behind the active answer.

That made the UI feel like attachments were broken during streaming even when the buttons were visible.

There was also a staging weakness in the macOS overlay: newly arrived context was marked as pending only during a short explicit mutation window. If a file picker or screen capture completed while an answer was streaming, the new context could appear saved but not be staged for the next answer.

## Fix

- Overlay answer requests now run in a background task.
- The daemon overlay event loop stays responsive while an answer streams.
- A daemon-side `overlay_answer_active` guard prevents accidental second answers during an active stream.
- If the user tries to start another answer while one is running, Bluey shows a warning and tells them that files/screens can still be prepared for the next answer.
- macOS context updates now stage newly added items for the next answer when:
  - an explicit attach/screen mutation is expected
  - the file picker was open
  - screen context was armed
  - an answer is currently streaming
- Desktop workspace version bumped to `0.1.87`.

## Intended Behavior

While an answer is streaming:

- User can click `Screen`; capture should proceed and become pending for the next answer.
- User can click `+` and attach documents; selected files should index and become pending for the next answer.
- User cannot start a second simultaneous answer from the overlay.
- After the current stream finishes, pressing `Answer` uses the newly staged screen/files.

## Verification

```bash
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.87
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

Result:

- Rust format/check passed.
- Swift parse and release build passed.
- Release artifact dev-flag/secret scan passed.
- `latest.json` signature verified.
- Installer MIME checks passed.
- Darwin arm64 artifact SHA verified.
- Unpacked binaries report `0.1.87`.
- Public installer smoke installed `0.1.87`.
- Local daemon started successfully with pid `4255`.

## Deployment

Desktop release:

```text
0.1.87
```

Live artifact:

```text
https://bluey.sh/releases/v0.1.87/bluey-0.1.87-darwin-arm64.tar.gz
```

Artifact SHA256:

```text
ecd0c9eb00c3f40259d9a9ab9f351f4ae7b88e579b21dcd733656c8da0bcbb1e
```

## Windows Parity

The daemon event-loop fix applies to both macOS and Windows because both native overlays talk to the same daemon overlay event handler.

The macOS staging change is in the Swift overlay. Windows has a separate native overlay implementation; the parity rule is that documents or screenshots added while an answer is streaming must become pending for the next answer instead of being hidden as saved-only context.
