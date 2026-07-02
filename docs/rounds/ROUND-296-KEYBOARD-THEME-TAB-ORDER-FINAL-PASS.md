# Round 296 - Keyboard Theme Tab Order Final Pass

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Release: `0.1.50`

## Trigger

The user shared `IMG_3704.MOV` showing keyboard navigation reaching the Theme button before the Keyboard Shortcuts button, even though the keyboard icon is visually placed before Theme. The user also asked for an end-to-end final pass so nearby UI and release issues are not missed.

## Root Cause

The macOS focus traversal still depended on a manually curated header control list. That list can drift from the rendered header order as buttons appear/disappear, layout changes, or prior focus state moves through the row. Windows also had Theme visually and logically before Keyboard Shortcuts, so parity was not aligned with the current Mac expectation.

## Fix

- macOS header keyboard traversal now sorts header controls by their actual rendered position:
  - top row first
  - left to right within a row
- This makes Keyboard Shortcuts focus before Theme because it is rendered to the left of Theme.
- Windows header layout now places Keyboard Shortcuts before Theme.
- Windows keyboard focus order and hit order now match that visual order.
- Bumped the desktop release to `0.1.50`.

## Verification

- Inspected `IMG_3704.MOV` by extracting frames and confirming the focus-ring sequence.
- `swift build -c debug --package-path native/macos/cue-overlay`
- `cargo check -p cue-core --quiet`
- `cargo check -p cue-daemon --quiet`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `cargo test -p cue-core overlay --lib`
- `cargo test -p cue-daemon deepgram --lib`
- `git diff --check`

## Release Verification

- Built macOS release package with the production update public key.
- Deployed `0.1.50` to `https://bluey.sh`.
- `https://bluey.sh/latest.json` reports `version: 0.1.50`.
- Live artifact:
  `https://bluey.sh/releases/v0.1.50/bluey-0.1.50-darwin-arm64.tar.gz`
- Live artifact SHA256:
  `58d949c727ac73381a84a363549c5d1562ce658519bcee32ff961ed2b6ef447e`
- Live `latest.json.sig` verified successfully against the release Ed25519 public key.
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.
- Unpacked live release reports:
  - `bluey 0.1.50`
  - `bluey-daemon 0.1.50`
- Release artifact scan passed with no configured secrets/dev capture flags present.

## Remaining QA

- Live Mac smoke after update:
  - `bluey off && bluey on`
  - make sure click-through is off
  - press `Tab` from the main overlay
  - confirm the header-right sequence reaches Keyboard Shortcuts before Theme
  - confirm `Enter` opens the selected control
- Windows visual/focus order should be smoke-tested on the next Windows build host because this round only syntax-checked the Windows overlay source locally.
