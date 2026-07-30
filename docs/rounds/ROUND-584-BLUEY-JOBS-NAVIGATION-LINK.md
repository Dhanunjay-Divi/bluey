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

Deploy only `web/index.html` and `web/assets/bluey-site.css`. Preserve the
existing Jobs bundle, signed releases, installers, APIs, and native artifacts.

Rollback by restoring the pre-deploy copies of those two static files.
