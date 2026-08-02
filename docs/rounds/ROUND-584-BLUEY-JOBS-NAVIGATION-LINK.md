# Round 584 - Bluey Jobs Navigation Link

Date: 2026-07-30

## Goal

Give visitors a clear path from the main Bluey website to Bluey Jobs without
replacing or interrupting the current page.

## Implementation

- Added a compact `Apply for Jobs` header action to the landing, account,
  download, and policy shells in `web/index.html`.
- The action opens `/jobs/` in a new tab with `noopener noreferrer`.
- Added an explicit accessible label announcing the new-tab behavior.
- Kept the full label on larger screens and the compact `Jobs` label below
  600 px.
- Tightened the landing header below 520 px so the theme, Jobs, Download, and
  Login actions stay aligned.
- At 360 px and below, the duplicate header Download link is hidden while the
  primary Download call to action remains available in the page.
- Added light-theme colors and retained the established Bluey accent treatment.
- Bumped the public CSS cache key so browsers receive the new navigation styles.

## Scope

Changed:

- `web/index.html`
- `web/assets/bluey-site.css`
- `CHANGELOG.md`

Not changed:

- Bluey Jobs application code or assets
- APIs, billing, authentication, native clients, overlay, audio, or runtime
- signed release metadata or installers

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- HTML parser check for exactly four Jobs links
- Verified `/jobs/`, `_blank`, `noopener noreferrer`, and accessible labels
- Verified unique HTML IDs
- Verified CSS brace and quote balance
- Browser QA at 390 x 844 in dark and light themes
- Verified no horizontal overflow and a single-line Login action
- Verified the action opens the Bluey Jobs route in a separate tab

## Deployment

Deployed only `web/index.html` and `web/assets/bluey-site.css` to
`/var/www/bluey` on 2026-07-30. The existing Jobs bundle, signed releases,
installers, APIs, and native artifacts were preserved.

Pre-deploy backup:

- `/var/backups/bluey-web/round584-before-20260730T210453Z`

Pre-deploy SHA-256:

- `index.html`:
  `80c2d1136960bfd341806a1179a25bd325da6a15d63295a391faeaa60134bf70`
- `assets/bluey-site.css`:
  `317b00712e4ac625c08f0fda1856e7dd24ccf0ece11c20013b1620606397d6ab`

Deployed SHA-256:

- `index.html`:
  `b55f33a7d0ce16162453887ab262d8c464507b7879ee777c78b712d26a037dbe`
- `assets/bluey-site.css`:
  `5d97974cebbfc303589d82c19234303642dbdbf4f871e82d6ece9bb387aaaf3c`

Live verification:

- `https://bluey.sh/` returns `200` HTML.
- `https://bluey.sh/jobs/` returns `200` HTML.
- The versioned stylesheet returns `200` CSS.
- The live page contains all four Jobs links with the expected new-tab
  security attributes.
- A browser click from the landing header opened
  `https://bluey.sh/jobs/` in a separate tab.
- The 390 x 844 live landing header has zero horizontal overflow.

Rollback by restoring the two files from the timestamped backup.
