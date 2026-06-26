# Round 163 - Files Badge Click Fix - 2026-06-24

## Issue

The header "Show files" badge could appear but fail to open the conversation file strip. It was rendered as a passive text label with a gesture recognizer, while the overlay's header drag and click-through hit testing only preserved real controls or selectable/editable text fields.

## Change

- Replaced the passive badge hit behavior with a `ClickableHeaderBadge` that accepts first mouse, returns itself from hit testing, shows a pointing-hand cursor, and calls the file toggle directly.
- Added the clickable badge to the overlay's explicit interactive hit-test whitelist.
- Changed the copy from `Show files · 2` to `Show 2 files`, and `Hide files · 2` to `Hide 2 files`.
- Added an overlay-level fallback hit area for the badge so click-through/header drag mode still toggles files.
- Forced the manually framed attachment strip to relayout immediately when files are shown or hidden.

## Expected Behavior

Clicking `Show 2 files` opens the bottom horizontal file strip with every file attached to the conversation. Clicking `Hide 2 files` hides the strip again.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `git diff --check -- native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `./native/macos/cue-overlay/build.sh`
- Installed the rebuilt overlay into `~/.bluey/bin` and restarted visible local Bluey.
