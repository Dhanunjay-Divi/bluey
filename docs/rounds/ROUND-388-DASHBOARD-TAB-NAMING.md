# Round 388 - Dashboard Tab Naming

## Goal

Make Bluey dashboard tab labels clearer and closer to the Pinky dashboard mental model.

## Changes

- Renamed `Computers` to `My Computers`.
- Renamed `Sessions` to `Session History`.
- Renamed `Summary` to `Usage Summary`.
- Renamed the admin-only `Admin` tab to `Trial Ops`.
- Clarified that `Trial Ops` is an internal admin-only trial protection view.
- Added dashboard hash aliases for `#my-computers`, `#usage-summary`, and `#trial-ops`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.js`
