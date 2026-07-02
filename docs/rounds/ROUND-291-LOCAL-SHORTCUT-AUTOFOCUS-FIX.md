# Round 291 - Local Shortcut Autofocus Fix

## Trigger

The owner reported that pressing `L` in click-through-off mode focused the Ask text box and typed `l` instead of starting Listen. The owner noted this was not only `L`; any key could auto-focus the text box and type.

## Root Cause

The macOS overlay key router had an old fallback after shortcut handling:

- If the key was printable and Ask was not focused, it called `focusComposerForInput()`.
- Then it inserted the printable character into Ask.

That made ordinary keys behave like text input even when the user had not clicked Ask or pressed the text-input shortcut. It also meant a local shortcut miss could become unexpected typing.

## Fix

- Removed the automatic printable-key-to-Ask fallback.
- Ask now only receives typed characters when it is already the focused responder.
- Users can still focus Ask intentionally by:
  - clicking the Ask box
  - pressing `T` in local shortcut mode
  - pressing `Ctrl+Option+T` globally on macOS
- Local shortcuts such as `L`, `S`, `I`, `H`, `F`, and `Enter` remain available when click-through is off and Ask is not focused.
- Bumped desktop workspace version to `0.1.45`.

## Mac/Windows Parity

This was a macOS-only bug. Windows already routes local shortcuts only when the edit box is not focused and does not auto-focus the edit box for arbitrary printable keys. Windows source was syntax-checked again.

## Verification

Passed locally:

```bash
swift build -c debug --package-path native/macos/cue-overlay
cargo check -p cue-daemon --quiet
x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
```

Release verification:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

Live checks passed:

- `https://bluey.sh/latest.json` reports version `0.1.45`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live `darwin-arm64` artifact:
  `releases/v0.1.45/bluey-0.1.45-darwin-arm64.tar.gz`
- Live/local artifact SHA256:
  `9f3457203262d9aaa03afbb4b86569a6af32a3608424d76337a038d98cc18034`
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.

## Current State

- macOS `0.1.45` is live and downloadable from the droplet.
- Windows source remains syntax-checked.

## Remaining QA

- Owner should run `bluey off && bluey on`, confirm update to `0.1.45`, then test:
  - With click-through off and Ask not focused, press `L`; Listen should start/stop.
  - Press unrelated letters; they should not auto-focus Ask and type.
  - Press `T`; Ask should focus intentionally.
  - Click Ask and type; normal typing should still work.
