# ROUND-396 Overlay History Session ID Search

Date: 2026-07-05
Thread backup ID: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-fast-answer-latency-20260705

## User Issue

Users need to search overlay History by the short session ID shown in Bluey, so a support/debug reference can be found without scrolling through local recordings.

## Changes

- Added a search field to the macOS overlay History drawer.
- Search matches:
  - recording title
  - subtitle/date/count text
  - full session UUID
  - short Bluey session ID such as `AD0A09E8`
  - `ID AD0A09E8` with or without spaces/hyphens/case differences
- Pressing Enter in the search field opens the first visible match.
- Pressing Esc clears the search query, or closes History when the query is empty.
- Kept row action tags mapped to the full unfiltered session list so continue, rename, and delete still target the correct recording after filtering.
- Added a no-match state: `No sessions match ... Try the session ID, title, or date.`
- Avoided per-keystroke lifecycle logging so search does not spam diagnostics.

## Files Changed

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`

## Verification

- `swift build --package-path native/macos/cue-overlay`
- `git diff --check`

## Notes

- The daemon already includes the short ID in each session subtitle using the same `MeetingRecord::session_code()` logic displayed in the header, so this round keeps the overlay command contract unchanged.
- This is a macOS overlay UI fix. Windows history parity should be handled if/when the native Windows overlay exposes the same session drawer surface.
