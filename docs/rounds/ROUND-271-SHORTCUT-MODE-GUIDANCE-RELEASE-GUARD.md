# Round 271 - Shortcut Mode Guidance Release Guard

## Trigger

The owner asked whether shortcut guidance should change by click-through state, whether Bluey can avoid shortcut collisions, and whether inside-Bluey shortcuts are actually useful.

## Decision

Global shortcuts are the primary shortcut model. Inside-Bluey single-letter shortcuts are kept as optional power-user shortcuts only when the full overlay is interactive and Ask is not focused. They should not be taught as the main interaction path because typing focus makes them feel inconsistent.

## Fix

- macOS shortcuts panel is now mode-aware:
  - Click-through on: explains that blank Bluey space passes clicks through and tells the user to use global shortcuts.
  - Interactive on: lists global shortcuts first, then labels inside-Bluey shortcuts as optional.
- Windows shortcuts dialog now mirrors the same mode-aware guidance.
- Windows global hotkey registration now checks every `RegisterHotKey` result and emits `global_shortcuts` as:
  - `ready` with registered count when all shortcuts are registered.
  - `partial` with failed key/error details when another app/system shortcut owns one.
- macOS already had Carbon registration failure logging from Round 270.
- Release packaging now fails closed unless `BLUEY_UPDATE_PUBKEY` is set, preventing future desktop artifacts from shipping without the embedded update-verification public key.
- Bumped desktop workspace to `0.1.25`.

## Release Correction

During local update verification, `bluey 0.1.24` refused the signed `0.1.25` update because the `0.1.24` binary had been built without `BLUEY_UPDATE_PUBKEY`. The signed server manifest was valid; the installed client lacked the embedded verifier key.

This round rebuilt and republished `0.1.25` with:

```text
BLUEY_UPDATE_PUBKEY="$(cat ~/.bluey/release/bluey-release-ed25519.pub.b64)"
```

The Makefile guard now prevents repeating that mistake for release package targets.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `git diff --check`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=release bash native/macos/cue-overlay/build.sh`
- Local hot install, ad-hoc sign, and restart.
- `Ctrl+Option+B` smoke:
  - first press: `overlay_visible: false`
  - second press: `overlay_visible: true`
- `env -u BLUEY_UPDATE_PUBKEY make require-update-pubkey` fails closed.
- `BLUEY_UPDATE_PUBKEY=... make package-darwin-arm64`
- `scripts/publish-bluey-release.sh` dev-flag/secret scan passed.
- Live `https://bluey.sh/latest.json` reports `0.1.25`.
- Live `latest.json.sig` verified successfully.
- Live `SHA256SUMS.txt` matches local artifact checksum.
- Local reinstall from `https://bluey.sh/install.sh` installed `bluey 0.1.25`.
- `bluey update` now reports `Bluey is up to date (0.1.25)` without unsigned-update warnings.

## Release

- Published artifact:
  - `dist/bluey-0.1.25-darwin-arm64.tar.gz`
- Artifact SHA256:
  - `cf6ab7072d61a78de166463240060ed6e4792cdd7328f93df5f10e573fd0cfab`
- Published to:
  - `root@165.227.77.152:/var/www/bluey`

## Current State

The local machine is running `bluey 0.1.25`, Bluey is visible, and capture exclusion remains enabled. The live macOS arm64 release is corrected with the embedded updater public key. Windows source parity is implemented and syntax-checked, but the live downloadable manifest still only publishes the macOS arm64 artifact.

