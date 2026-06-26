# Round 168 - Screen Context Dedup

## Changed

- Repeated Screen captures now replace the previous pending screen capture for the next Answer.
- Normal attached images and documents can still accumulate normally.
- The Screen button now behaves as "use the latest screen" instead of stacking duplicate `Screen context` chips.

## Why

When the user captured the screen twice and pressed Answer without typing a question, Bluey sent two screenshot attachments. The model correctly answered the visible SQL problem, but the duplicate chips made the experience feel confusing and stale.

## Verified

- Built the macOS overlay with `native/macos/cue-overlay/build.sh`.
