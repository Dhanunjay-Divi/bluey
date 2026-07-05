# Round 394 - Install Command Copy Reveal

Date: 2026-07-05
Thread backup id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Make Bluey's download install commands behave more like Pinky's command rows: quiet by default, highlighted when selected, and clear when copied.

## Changes

- Added focusability to the install and start command blocks.
- Added group roles and clear aria labels so screen readers announce the command groups properly.
- Added selected-row styling for hover, click, keyboard focus, and copied states.
- Revealed the Copy pill only when the desktop install command row is selected, hovered, focused, or copied.
- Kept Copy visible on narrow mobile layouts to avoid an awkward blank stacked row.
- Added a little more vertical spacing before the terminal setup panel.
- Added explicit footer spacing before the email link so `Reach us at hello@bluey.sh` does not visually run together.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.css web/assets/bluey-site.js docs/rounds/ROUND-394-INSTALL-COMMAND-ACCESSIBILITY.md`
- `awk '/[ \t]$/{print FILENAME ":" FNR ": trailing whitespace"; bad=1} END{exit bad}' docs/rounds/ROUND-394-INSTALL-COMMAND-ACCESSIBILITY.md`
