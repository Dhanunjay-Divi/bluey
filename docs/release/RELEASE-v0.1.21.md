# Bluey 0.1.21

## Overlay Click-Through

- Click-through mode now treats blank Bluey space as truly pass-through.
- Actual controls still receive clicks.
- The Bluey logo/name remains the intentional drag handle while click-through is enabled.
- Resize edges remain available.
- Open history drawers and canvas panes remain interactive so users can scroll and copy inside them.
- Windows overlay source now follows the same blank-space contract by returning transparent hit tests for expanded blank areas.

## Attachments

- Uploaded/dropped files still prepare context immediately for the next answer.
- The attachment strip is no longer auto-opened by default.
- Users can reveal files with the `Show N files` badge.
- After sending an answer, the visible file strip collapses again.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
- `cargo check -p cue-cli -p cue-daemon --quiet`
- `git diff --check`
- Release artifact scanned clean for configured secrets and visible-overlay dev flags.
- `latest.json` is signed with the Bluey Ed25519 release key.
- Live installer smoke from `https://bluey.sh/install.sh` installed `bluey 0.1.21`.
