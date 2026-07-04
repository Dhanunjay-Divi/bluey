# Round 337 - Overlay Content Scrolling

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Trigger

The owner asked how users can scroll answers/questions when click-through is off. The specific concern was that if users cannot scroll the visible conversation, history, or canvas reliably, the overlay feels broken. The owner suggested an `Option` plus up/down arrow fallback.

## Root Cause

The macOS overlay already forwards mouse-wheel events to the answer feed, history drawer, transcript, composer, or canvas under the pointer, but the behavior was not discoverable and there was no keyboard fallback for content scrolling. When click-through is off, blank overlay space is also used for dragging the window, so users need a clear and predictable scroll path.

## Fix

- Added a shared `scrollClipView` helper that clamps scroll offsets safely.
- Added keyboard scroll helpers for the main answer feed and right-side canvas.
- Added macOS local keyboard scrolling:
  - `Option+Down` / `Option+PageDown` scrolls the visible content down.
  - `Option+Up` / `Option+PageUp` scrolls the visible content up.
  - The target is chosen by current mouse position: history drawer, canvas, or answer feed.
- Kept text editing safe: if the Ask box or another text editor is actively editing, `Option+Arrow` remains available to text editing instead of hijacking the input.
- Updated the macOS shortcuts overlay to mention:
  - `Opt+Down` / `Opt+Up`
  - mouse wheel scrolls the answer, canvas, or history under the pointer.
- Bumped the desktop workspace version to `0.1.76`.

## Current UX

- Click-through off:
  - mouse wheel scrolls the answer feed/canvas/history under the pointer
  - blank Bluey space still drags the overlay window
  - `Option+Up/Down` scrolls content under the pointer when Ask is not actively editing
- Click-through on:
  - blank space clicks through to the app behind Bluey
  - actual Bluey controls remain interactive
  - mouse wheel and the same keyboard scroll fallback are available while Bluey has focus

## Verification

```bash
cd native/macos/cue-overlay && swift build -c release
cargo check -p cue-daemon -p cue-cli
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.76
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
```

## Release

Desktop release `0.1.76` is live on `bluey.sh`.

Darwin arm64 artifact:

`https://bluey.sh/releases/v0.1.76/bluey-0.1.76-darwin-arm64.tar.gz`

Artifact SHA256:

`f95d32d983e41a4a3817e63217ae263d2a42d3c384b29d6db613b3cbe79dbf66`

Public installer smoke installed `0.1.76` locally. In this non-interactive shell, sudo prompting was unavailable, so install correctly fell back to the user-local symlink path.

## Windows Parity

The concrete scroll issue and implementation path were in the macOS Swift overlay. The Windows C overlay does not currently expose the same scrollable feed/canvas/history implementation, so this round did not make a Windows code change. A separate Windows parity pass should add equivalent content scrolling once the Windows overlay has matching scrollable content panes.
