# Round 277 - Shortcuts Focus Drag Polish

## Trigger

The owner asked to make the shortcut help clearer and better looking:

- say global shortcuts when click-through is on
- say inside-Bluey shortcuts work when click-through is off and Ask is not focused
- center the Done button
- remove any implication that `Ctrl+Option+H` is a global History shortcut
- clarify `Enter` vs `Ctrl+Option+Enter`
- blur Ask focus when clicking outside the input
- make blank overlay space easy to drag when click-through is off

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Fix

- Reworked macOS shortcut help into attributed text:
  - shortcut keys render in accent color
  - actions render in primary text color
  - mode notes render muted
  - click-through on shows only global shortcuts
  - click-through off shows inside-Bluey shortcuts plus global shortcuts
- Centered the shortcut sheet Done button and widened the shortcut sheet panel.
- Kept `H` only as an inside-Bluey History shortcut when Ask is not focused and click-through is off.
- Clarified:
  - `Ctrl+Option+Enter` answers globally
  - `Enter` answers inside Bluey
  - `Shift+Enter` creates a new line in Ask
- Blurs the Ask composer when clicking outside Ask.
- Makes answer text non-selectable in the feed so blank/feed text behaves like draggable overlay space instead of trapping the drag.
- Kept copy buttons as the intended answer-copy path.
- Updated Windows shortcut help copy to match the Mac mode split.
- Gated Windows single-letter inside-Bluey shortcuts behind interactive mode so click-through mode does not claim inside shortcuts.
- Added Windows blur-on-click-outside-Ask parity.
- Bumped desktop workspace version to `0.1.31`.

## Screen-Share / Click-Through Semantics

- Click-through on: blank Bluey space clicks the app behind it. Use global shortcuts.
- Click-through off: blank Bluey space drags Bluey. Inside-Bluey letter shortcuts work when Ask is not focused.
- Real Bluey controls remain clickable.
- Normal production capture exclusion is unchanged.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `git diff --check`
- `cargo check -p cue-daemon --offline`
- `cargo check -p cue-dashboard --locked`
- `cargo test -p cue-daemon --lib --locked`
  - `281 passed; 0 failed; 2 ignored`
- `cargo test -p cue-core sign_in_event_serializes --locked`
- Release artifact dev-flag/secret scan passed.
- Published `v0.1.31` to `https://bluey.sh/latest.json`.
- `latest.json.sig` verified successfully.
- `https://bluey.sh/install.sh` serves `application/x-shellscript`.
- Local update installed `bluey 0.1.31`.
- Local status after restart showed `"overlay_capture_excluded": true`.
- Local macOS shortcut smoke:
  - `Ctrl+Option+B` restored from collapsed state.
  - `Ctrl+Option+H` did not change Bluey overlay state.
- Windows packaging attempt on this Mac did not produce a ZIP because the Makefile target expects `x86_64-pc-windows-msvc`, while this host only has `x86_64-pc-windows-gnu` installed. Windows source parity and syntax check are complete; a fresh Windows artifact needs the Windows/MSVC release runner.

## Current State

The macOS shortcut sheet now presents the right model:

- click-through on means global shortcuts
- click-through off means inside Bluey shortcuts, only when Ask is not focused
- `H` is not global
- Done is centered

Blank answer/feed text no longer blocks dragging in interactive mode, and clicking outside Ask clears Ask focus.

## Live Artifact

```text
https://bluey.sh/releases/v0.1.31/bluey-0.1.31-darwin-arm64.tar.gz
SHA256: 698e885a66fc60e35c84f85053200cb45aa837af2f44b5d5962ca04ade133e8a
```

## Remaining QA / Gates

- Manual visual QA should confirm the shortcut sheet copy and centered Done button inside the live overlay.
- Manual pointer QA should confirm:
  - click outside Ask clears focus
  - blank/feed space drags when click-through is off
  - blank space passes through when click-through is on
  - `Ctrl+Option+Enter` answers globally
  - `Enter` answers inside Bluey
- Build and publish a fresh Windows ZIP from the Windows/MSVC release runner.
