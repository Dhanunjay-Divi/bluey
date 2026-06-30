# Round 261 - Strict Click-Through and Attachment Strip

## Trigger

The owner reported that click-through still was not reliable enough for a large-scale overlay app:

- Blank Bluey space should click the app behind it when click-through is enabled.
- Buttons and real Bluey controls should still be clickable.
- The overlay still needs an intentional way to move/resize.
- Uploaded files should prepare context immediately, but the bottom file strip should not stay visible after attach/send; users should click `Show files` when they want to inspect attachments.

## Root Cause

Round 246 made blank expanded-panel space draggable to make moving Bluey easier, but that contradicted strict click-through semantics. In macOS pass-through mode, the expanded panel still returned itself for blank hit-test areas and `mouseDown` started a panel drag. The window-level hit-test helper also returned interactive for blank space.

The Windows overlay had the same contract problem: expanded blank space returned `HTCAPTION`, so it captured clicks and moved the overlay instead of passing clicks through.

For attachments, the macOS overlay auto-opened the visible file chip strip when new context arrived and reopened it after sends. Windows always reserved/drew its context chip band when context existed.

## Fix

- macOS click-through mode now only receives mouse events for:
  - explicit controls
  - manual overlay controls
  - open history drawer
  - open canvas pane
  - resize edges
  - the Bluey logo/name drag handle
- macOS blank expanded-panel space now returns `nil` from hit testing in click-through mode, so clicks pass to the app behind Bluey.
- macOS no longer starts a blank-space drag from pass-through mode.
- macOS window-level interactivity now returns `false` for blank click-through areas.
- Windows expanded blank space now returns `HTTRANSPARENT`, while controls, resize edges, and the brand drag handle remain interactive.
- Updated tooltips/toasts so the product copy says `Click-through on` instead of the older `Move-anywhere on`.
- New file attachments still prepare context immediately and remain eligible for the next answer.
- macOS attachment chips are hidden by default. The header badge shows `Show N files`; clicking it reveals the strip.
- After sending an answer, macOS collapses the attachment strip back to `Show N files`.
- Windows context chips are hidden by default behind an explicit show state and collapse again after sends/new attach flows.

## Verification

Local checks passed:

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
cargo check -p cue-cli -p cue-daemon --quiet
git diff --check
```

Local smoke:

```text
Installed patched macOS overlay into ~/.bluey/bin for immediate testing.
Installed live 0.1.21 package from https://bluey.sh/install.sh.
Restarted local Bluey with BLUEY_SKIP_UPDATE=1.
bluey --version returned bluey 0.1.21.
bluey status returned daemon pid 26781 and overlay_visible=true.
```

Release/deploy checks:

```text
Release artifact dev-flag/secret scan passed.
https://bluey.sh/latest.json version: 0.1.21
latest.json.sig size: 88 bytes
OpenSSL: Signature Verified Successfully
darwin-arm64 sha256: 6c1ff0a731c63e10ca66d7d06aa0b936c7da6044960c2d13ca4a4f9cc20db0f4
temp-home installer smoke: bluey 0.1.21
local install smoke: bluey 0.1.21 / daemon pid 26781
```

## Current State

The local machine is running the patched overlay for immediate testing.

Fresh installs and signed auto-update metadata are live on `bluey.sh` as version `0.1.21`.

## Remaining QA/Gates

- Human QA should verify click-through in the real overlay:
  - blank center panel clicks the app behind Bluey
  - blank header/composer chrome clicks through
  - buttons remain clickable
  - logo/name drag handle moves Bluey
  - resize edges still resize
  - drawer/canvas remain scrollable/clickable when open
  - `Show N files` reveals uploaded context and sending collapses the strip again
- Windows source parity is implemented and syntax-checked; Windows downloadable artifact still requires a Windows build host/package.
