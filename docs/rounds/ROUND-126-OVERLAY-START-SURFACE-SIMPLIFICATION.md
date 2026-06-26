# Round 126 - Overlay start surface context copy

## Why

The empty conversation surface should help a new user understand what to do without feeling like a tutorial or a generic status screen. The copy needs to point toward the real workflow: give Bluey enough context for the current conversation, then ask.

## Change

- Keep `Ready when you are` as the empty-state headline.
- Tell users they can ask, attach what they have, or capture the screen.
- Make the drop target conversation-specific: `Drop files for this conversation`.
- Keep the supported context hint short: docs, images, code, notes.

## Verification

- `swiftc -typecheck native/macos/cue-overlay/Sources/cue-overlay/main.swift`
