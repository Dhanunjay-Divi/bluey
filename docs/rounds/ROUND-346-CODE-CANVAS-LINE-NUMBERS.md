# Round 346 - Code Canvas Line Numbers

Date: 2026-07-04
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner noted that Bluey can say things like "line 10" or emit `Line notes`, but the code canvas did not visually show line numbers. The owner also clarified that even when line numbers are shown, copied code must not include those numbers.

## Product Rule

Line numbers are a visual canvas aid only.

- Code canvas should show line numbers in the `CODE` section so `Line notes` can reference visible lines.
- `LINE NOTES`, `COMPLEXITY`, and other explanation sections should not be numbered.
- The canvas copy button must copy runnable code only.
- Keyboard copy from selected canvas text should strip display gutters before writing to the pasteboard.

## Fix

- Added a macOS display-only code gutter in `CanvasPaneView`.
- The gutter is rendered only for `CanvasKind.code` and only within the `CODE`, `PATCH`, `DIFF`, or changed-block code section.
- Artificial blank separators before `LINE NOTES` / `COMPLEXITY` are left unnumbered so note references map to real code lines.
- Existing code comment tinting now understands the visual gutter and still dims comments in the code area.
- `ArrowCursorTextView.copy(_:)` now strips display gutters from selected text before copying.
- The existing canvas copy button remains clean because it copies the raw extracted code section, not the rendered display text.
- Desktop workspace version bumped to `0.1.85`.

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
cargo test -p cue-daemon code_artifact_preview_keeps_line_notes_out_of_code_fence --lib
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.85
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

Result:

- Swift parse passed.
- Native macOS release build passed.
- Rust format/check passed.
- Code artifact regression test passed.
- Release artifact dev-flag/secret scan passed.
- `latest.json` signature verified.
- Installer MIME checks passed.
- Darwin arm64 artifact SHA verified.
- Unpacked binaries report `0.1.85`.
- Public installer smoke installed `0.1.85`.
- Local daemon started successfully with pid `12534`.

## Deployment

Desktop release:

```text
0.1.85
```

Live artifact:

```text
https://bluey.sh/releases/v0.1.85/bluey-0.1.85-darwin-arm64.tar.gz
```

Artifact SHA256:

```text
7f6c6ebd4740f0a20ab1019849c119e20f0b14fc64b5e7b795a87e90d6a9c15e
```

## Windows Parity

The current Windows C overlay does not yet render the same right-side code canvas artifact pane that the macOS Swift overlay renders. No Windows source change was applicable in this round. The parity rule to preserve for Windows canvas work is: visible line-number gutters must be display-only, and any copy path must strip them before writing to the clipboard.
