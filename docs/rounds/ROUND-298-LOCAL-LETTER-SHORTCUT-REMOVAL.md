# Round 298 - Local Letter Shortcut Removal

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The user reported that individual letter shortcuts were still interfering with
normal typing. For example, pressing `t` while composing a question could trigger
Bluey behavior instead of simply typing the word.

## Root Cause

The overlay still had a local, unmodified shortcut layer for letters such as
`T`, `L`, `S`, `I`, `H`, and `F` when click-through was off. Even with focused
composer fixes from earlier rounds, that design remained too fragile because a
typing-focused overlay should not treat plain alphabet keys as commands.

## Work Done

- Removed macOS local single-letter shortcuts from the expanded overlay.
- Kept macOS local `Enter` for Answer and `Esc` for closing panels/canvas.
- Kept macOS `Tab` and `Shift+Tab` keyboard navigation through normal focus
  traversal.
- Removed Windows local single-letter shortcut handling in the expanded overlay.
- Removed the Windows local `Ctrl`-only shortcut branch so inside-Bluey shortcuts
  do not conflict with normal text editing expectations.
- Kept Windows local `Enter`, `Esc`, `Tab`, and `Shift+Tab`.
- Kept global modified shortcuts:
  - macOS: `Ctrl+Option+...`
  - Windows: `Ctrl+Alt+...`
- Updated the Mac and Windows keyboard help copy to remove the individual letter
  shortcuts and state that letters type normally when Ask is focused.
- Bumped the desktop workspace version to `0.1.51`.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `cargo check -p cue-core --quiet`
- `cargo check -p cue-daemon --quiet`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `cargo test -p cue-core overlay --lib`
- `git diff --check`
- `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.51`

## Deploy

- Live version: `0.1.51`
- Release time: `2026-07-02T07:17:25.820156Z`
- macOS artifact:
  `https://bluey.sh/releases/v0.1.51/bluey-0.1.51-darwin-arm64.tar.gz`
- macOS artifact SHA256:
  `f54d50e3939ab58dacdc4e36da60bb65846ed06558aea8042d44f2038ff788c2`
- Live verifier confirmed:
  - `latest.json` signature verified
  - `install.sh` content type is `application/x-shellscript`
  - `install.ps1` content type is `application/x-powershell`
  - macOS artifact SHA matches
  - unpacked `bluey` and `bluey-daemon` binaries report `0.1.51`

## Current State

Plain alphabet keys no longer act as Bluey commands inside the overlay. Users
can type normally in Ask without a letter such as `t`, `l`, or `s` toggling an
overlay action.

## Remaining QA

- Live macOS smoke:
  - click Ask and type words containing `t`, `l`, `s`, `i`, `h`, and `f`
  - confirm no Bluey command fires while typing
  - confirm `Enter` sends the answer
  - confirm `Esc` closes open panels
  - confirm `Tab` and `Shift+Tab` traverse controls in visual order
  - confirm global `Ctrl+Option+...` shortcuts still work
- Windows smoke on the next Windows build host:
  - same local keyboard checks
  - confirm global `Ctrl+Alt+...` shortcuts still work
