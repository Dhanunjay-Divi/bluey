# Round 385 - Host Overlay Computers Focus

## Goal

Keep the Computers tab focused on Bluey host overlay desktop logins, matching the Pinky-style "My Computers" mental model.

## Changes

- Hid browser-login/session rows from the Computers list for now.
- Tightened browser-row detection for `web` / `browser` platform and kind values.
- Changed the desktop code card to say "Connect Bluey desktop".
- Updated code-entry copy to reference the Bluey host overlay.
- Changed count and empty states to say "Bluey desktop" instead of generic devices/computers.
- Updated the Computers section copy to "Host overlay desktops connected to this account."

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/assets/bluey-site.js web/index.html`
- `awk '/[ \t]$/{print FILENAME ":" FNR ": trailing whitespace"; bad=1} END{exit bad}' docs/rounds/ROUND-385-HOST-OVERLAY-COMPUTERS-FOCUS.md`
