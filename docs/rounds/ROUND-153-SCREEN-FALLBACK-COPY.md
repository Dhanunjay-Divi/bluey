# Round 153 - Screen Fallback Copy

## Why

The Screen flow showed a technical warning when browser page text was unavailable. That fallback is normal: Bluey uses page text when it can, and captures a screenshot when it cannot.

## Changed

- Reworded the fallback card to `Screen captured`.
- Removed the technical `Page text was unavailable` language from the user-facing card.
- Changed the fallback card from warning to context so it feels ready, not broken.
- Kept the behavior the same: pressing Answer still uses the captured screen with the question, captions, and files.

## Verified

- Ran `cargo check -p cue-daemon`.
- Built release `bluey` and `bluey-daemon`.
- Refreshed the local installed binaries and restarted visible Bluey.
