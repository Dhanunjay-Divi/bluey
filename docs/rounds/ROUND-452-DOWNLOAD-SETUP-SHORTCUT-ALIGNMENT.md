# Round 452 - Download Setup And Shortcut Alignment

## Goal

Keep the download page easier to scan by showing the selected platform setup first and the overlay shortcut reference below it.

## Changes

- Moved Mac and Windows setup instructions above the overlay shortcuts panel.
- Aligned the visible borders for setup, shortcuts, notes, and command sections.
- Removed the extra inner horizontal padding that made the shortcuts panel appear narrower than setup.
- Kept platform-specific shortcut labels working for Mac and Windows.
- Bumped the Bluey site CSS cache key.

## Verification

- Confirmed `PowerShell setup` appears before `Overlay shortcuts` in `web/index.html`.
- Ran `node --check web/assets/bluey-site.js`.
- Ran `git diff --check` on the touched web files and this round doc.
