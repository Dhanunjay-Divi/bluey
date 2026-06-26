# Round 204 - Overlay Clickability Restore

## Trigger

After the `v0.1.15` deploy, the owner reported the overlay felt dead: buttons were not clickable and blank space could not be held and dragged to move Bluey.

## Root Cause

Round 194 made expanded click-through mode too aggressive:

- macOS expanded overlay defaulted to `passThroughMode = true`.
- `updateExpandedMousePolicy()` toggled `expandedWindow.ignoresMouseEvents` based on a point detector.
- If the detector missed the active control or the mouse was over blank chrome, AppKit never delivered the click to the overlay.
- Windows returned `HTTRANSPARENT` for blank expanded surface, so only controls/logo behaved as Bluey-owned surface.

That matched strict click-through semantics but broke the owner's preferred overlay behavior: buttons should always click, and blank panel space should be draggable.

## Fix

- macOS expanded overlay now defaults to interactive mode.
- macOS normal expanded-window policy keeps `ignoresMouseEvents = false` except for existing special passthrough paths such as remote-input passthrough and external file-drag capture.
- macOS blank expanded panel surface now returns `self` for hit-testing and starts a drag when it is not over an explicit control.
- macOS interaction-mode copy now says `Move-anywhere` instead of promising blank-space click-through.
- Windows expanded `WM_NCHITTEST` now returns:
  - `HTCLIENT` for real overlay controls
  - `HTCAPTION` for blank expanded surface

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `native/macos/cue-overlay/build.sh`
- Installed refreshed macOS overlay binaries and `BlueyOverlay.app` into `~/.bluey/bin`
- Killed only the old overlay child process and relaunched via `bluey overlay show`
- Process check confirmed new overlay child PID launched from the refreshed app bundle

## Current State

- Local running overlay is refreshed and should be clickable/draggable again.
- Public release is bumped to `v0.1.16`, so new installs no longer receive the broken `v0.1.15` overlay.
- `https://bluey.sh/latest.json` now reports `0.1.16`.
- Live macOS arm64 artifact:

```text
https://bluey.sh/releases/v0.1.16/bluey-0.1.16-darwin-arm64.tar.gz
```

Live artifact SHA256:

```text
c88e6d7faa32c0242f249076d4bf39c43ba61a557460368702d9a0ef62f3a5b1
```

Additional release verification passed:

- `BLUEY_UPDATE_PUBKEY=... make package-darwin-arm64`
- release archive scan found no AppleDouble `._*` files and no capture-visible/dev overlay flag strings in shipped binaries
- `scripts/release-hygiene-scan.sh`
- `BLUEY_RELEASE_SIGNING_KEY_FILE=... PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 scripts/deploy-bluey-sh-manual.sh`
- live `latest.json.sig` verified
- live artifact SHA256 matched `latest.json`
- fresh temp-home install from `https://bluey.sh/install.sh` reported `bluey 0.1.16`
- live artifact `HEAD` returned HTTP 200
- live `/health` remained OK; no server redeploy was needed because this round changed only desktop/native overlay behavior

## Remaining QA/Gates

- Owner should manually try:
  - click toolbar/composer buttons
  - hold blank panel space and drag
  - click inside composer and type
  - attach button and screen/context controls
- If strict blank-space click-through is still desired later, add it as a separate explicit mode with a visible move handle and stronger automated hit-test coverage.
