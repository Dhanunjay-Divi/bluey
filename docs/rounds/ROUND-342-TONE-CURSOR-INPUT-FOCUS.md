# Round 342: Tone Cursor Input Focus

Date: 2026-07-04

Branch: `codex/bluey-overlay-spacing-20260626`

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner reported two macOS overlay polish issues:

- In the Tone editor, hovering over the text field still showed the text I-beam cursor instead of the normal mouse pointer.
- When the main Ask input was selected, the selected state was not visually clear enough; it needed a stronger Bluey-blue border highlight.

## Diagnosis

The Tone field itself already used `ArrowCursorTextField`, but AppKit swaps in the shared `NSTextView` field editor while the field is active. That shared editor can register its own text cursor rects, so the owner could still see an I-beam even though the visible field subclass preferred an arrow cursor.

The main Ask composer already had a focus callback and focus chrome, but the focused border was only a subtle 1 px/low-alpha change in several paths. In practice it did not read as a clear selected input state.

## Changes

- The macOS Tone panel now forces the active field editor to use the arrow cursor too.
- The overlay cursor policy treats the Tone panel as an arrow-cursor area while it is open.
- The Tone editor still keeps the Bluey-blue insertion indicator and focused border.
- The main Ask input surface now switches to:
  - 2 px focused border
  - stronger Bluey-blue focused border
  - subtle blue focused fill
  - stronger focused shadow
- Initial composer setup and theme refresh paths both pass the stronger focus colors, so dark and light modes stay consistent.
- Desktop workspace version bumped to `0.1.81`.

## Windows Parity Note

The current Windows C overlay does not have the same Tone editor UI surface as the macOS AppKit overlay, so there was no parallel Tone cursor path to patch in `native/windows/cue-overlay/main.c` for this round.

## Verification

```bash
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.81
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

## Deployment

- Desktop release `0.1.81` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.81/bluey-0.1.81-darwin-arm64.tar.gz`
- Artifact SHA256:
  `37040ce65cfe236bcb5b5d3f0c8571151f1e800edc3deb92a223d761e3e65414`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.81`
- Public installer smoke installed `0.1.81` locally and both installed binaries report `0.1.81`.
- The shell used for smoke testing had no interactive `/dev/tty` for sudo, so installer fallback used `/Users/uno/.local/bin/bluey`; that fallback is expected in non-interactive Codex shells.
- `bluey on` started a fresh `0.1.81` daemon pid `22487`.

