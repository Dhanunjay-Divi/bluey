# Round 344: Tone Underscore Caret

Date: 2026-07-04

Branch: `codex/bluey-overlay-spacing-20260626`

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed the Tone editor popup and asked for the text input to behave like a normal editing field without showing the I-beam mouse cursor. The requested behavior was:

- Keep the normal mouse pointer over the Tone input and modal.
- When the Tone input is focused, show a Bluey-blue blinking insertion indicator.
- Make that indicator look like an underscore/block instead of the default vertical text caret.

## Diagnosis

The previous Tone cursor fix forced the AppKit field editor to register arrow cursor rects, but the field editor still drew the native vertical insertion caret inside the input. AppKit also owns most key and selection handling while an `NSTextField` is being edited, so the visual caret needs to be controlled separately from the visible field shell.

## Changes

- Added a custom `answerStyleCaretIndicator` view inside the Tone panel.
- The active Tone field editor now uses a transparent native insertion point while the custom Bluey-blue underscore caret is shown instead.
- The custom caret follows the field editor's insertion rect when AppKit exposes one.
- Added a safe fallback position based on the typed prefix width for empty/short text cases.
- Added a blink timer that starts when Tone editing begins and stops when editing ends.
- Re-applies the caret and arrow cursor after key events so arrow/delete/selection movement stays visually stable.
- Keeps the normal arrow mouse cursor over the Tone modal and active field editor.
- Desktop workspace version bumped to `0.1.83`.

## Product Rule

The Tone editor should feel editable without looking like a conventional text-document surface:

1. Mouse pointer stays as an arrow.
2. Focused input shows Bluey-blue editing state.
3. The text insertion indicator is a small blinking underscore/block.
4. Selection hides the custom caret until the insertion point is collapsed again.

## Verification

Passed locally:

```bash
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
cd native/macos/cue-overlay && swift build -c release
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.83
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

## Deployment

- Desktop release `0.1.83` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.83/bluey-0.1.83-darwin-arm64.tar.gz`
- Artifact SHA256:
  `1753c3dc95416107889ff0ce0d553541406c17b2255d82cc005f16bbc92b8fe1`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.83`
- Public installer smoke installed `0.1.83` locally and both installed binaries report `0.1.83`.
- The non-interactive Codex shell had no `/dev/tty` for sudo, so installer fallback used `/Users/uno/.local/bin/bluey`; that fallback is expected in this shell.
- `bluey on` started a fresh `0.1.83` daemon pid `68363`.

## Windows Parity

This round changes the macOS AppKit Tone editor only. The current Windows overlay does not have this same AppKit field-editor path. If Windows later adds the same Tone modal, it should follow the same product rule: arrow cursor over the modal plus a Bluey-blue custom caret instead of an I-beam pointer.
