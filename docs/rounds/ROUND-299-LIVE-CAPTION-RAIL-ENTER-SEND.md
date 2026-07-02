# Round 299 - Live Caption Rail Enter Send

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The user wanted the live captions preview to stay simple: keep it as an active
horizontal strip while speaking, and make Enter send the current visible
transcript/question without over-classifying whether it is "meaningful enough."

## Root Cause

- The macOS caption strip already used a horizontal `NSScrollView`, but the tail
  scroll happened before some layout passes completed. That could make the rail
  look static instead of following the newest words.
- The Enter path still depended on the stricter transcript-question heuristics.
  If Bluey had real caption text but the text did not pass those heuristics, the
  overlay could block instead of sending.
- Windows had the same strict "meaningful transcript" check in the empty-input
  transcript send path.

## Work Done

- macOS:
  - added a simpler transcript-send candidate path
  - real non-placeholder caption text is now enough for Enter to send
  - short captions show directly in the visible Question card
  - long captions stay compact by sending the existing live-caption intent while
    the daemon supplies transcript context to the model
  - the caption rail now forces layout before tail scrolling
  - the caption rail also repeats the tail-follow pass shortly after layout, so
    the strip stays at the newest text as partial captions update
- Windows:
  - added the same simpler "usable transcript" gate
  - Enter can use real non-placeholder caption text even when it is not a
    perfect question
- Bumped the desktop workspace version to `0.1.52`.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `cargo check -p cue-core --quiet`
- `cargo check -p cue-daemon --quiet`
- `cargo test -p cue-core overlay --lib`
- `git diff --check`
- `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/deploy-bluey-sh-manual.sh`

## Deploy

- Live version: `0.1.52`
- Release time: `2026-07-02T07:37:53.819831Z`
- macOS artifact:
  `https://bluey.sh/releases/v0.1.52/bluey-0.1.52-darwin-arm64.tar.gz`
- macOS artifact SHA256:
  `cc6f5d6588b06867153e98757f1200f5640c9300af93b6f41b155c0fa4a55d0f`
- Live verifier confirmed:
  - `latest.json` signature verified
  - `install.sh` content type is `application/x-shellscript`
  - `install.ps1` content type is `application/x-powershell`
  - macOS artifact SHA matches
  - unpacked `bluey` and `bluey-daemon` binaries report `0.1.52`

## Current State

The live caption strip should keep following the latest speech horizontally.
When the composer is empty, pressing Enter now sends the current usable caption
text if Bluey has one. Short spoken asks remain visible as the Question card;
long rambling transcript stays compact.

## Remaining QA

- Live macOS smoke:
  - start Listen and speak continuously
  - confirm the bottom caption rail follows the newest tail
  - press Enter while the composer is empty
  - confirm Bluey sends the caption-backed answer immediately
  - confirm the Question card is short for short spoken asks
  - confirm long transcript does not flood the chat as a giant card
- Windows smoke on the next Windows build host:
  - confirm Enter sends real caption text from an empty composer
