# Round 144 - Default Size, Auto-send Menu, And Theme Toggle

Date: 2026-06-23

## Changed

- Increased the default expanded Bluey overlay size so the first open state has more room for chat, captions, attachments, and canvas.
- Changed the Auto-send control into a compact pull-down. Closed state now shows `Auto-send` or `✓ Auto-send`; the opened menu shows full choices for off, mic, system, or mic/system stop behavior.
- Added a header theme toggle next to the balance label. The setting persists locally and switches the main readable surfaces between dark and light mode.
- Re-rendered existing feed cards when theme changes so old questions and answers remain readable after toggling.

## Verified

- Ran `./native/macos/cue-overlay/build.sh`.
