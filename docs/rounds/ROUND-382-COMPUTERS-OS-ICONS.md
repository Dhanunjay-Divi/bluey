# Round 382 - Computers OS Icons

## Trigger

Owner showed the Computers tab listing "Browser session" rows with text badges like `WEB` / `BLUEY` and asked for the section to show computers with system icons like Pinky's Windows and Mac rows.

## Changes

- Filtered browser-style rows out of the Computers renderer so the tab shows linked desktops only.
- Changed the section copy from "Linked desktops and browsers" to "Linked computers running Bluey desktop."
- Changed the count copy from "linked devices" to "computers."
- Replaced text icon badges with inline SVG platform icons for macOS, Windows, Linux, and a generic desktop fallback.
- Kept the compact account-dashboard spacing while making the icons scale down for smaller screens.

## Verification

- `node --check web/assets/bluey-site.js` passed.
- `git diff --check` passed.
- `rg` confirmed the old browser-session wording/text badges are gone from the account web files.
