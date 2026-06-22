# Overlay start surface simplification

## Why

The empty conversation surface showed a large `READY / Ready` block plus onboarding copy. It made the first screen feel busier than necessary when the main starting actions already live in the composer controls.

## Change

- Remove the visible empty-state text from the main feed.
- Remove the empty-state document drop panel.
- Keep the attach flow available from the `+` button and drag/drop handling.
- Leave the first-run surface quiet so the user can start with Listen, Screen, attach, or Ask anything without extra visual noise.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
