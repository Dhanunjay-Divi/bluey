# Round 186 - Web Dashboard Reload Simplification

## Trigger

Owner said the web dashboard felt bad and asked to rethink it with beginner eyes, especially the reload flow. A first-time user should understand what the balance is, what `$30` gives, how checkout works, how to refresh after payment, and that Auto Reload is optional.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-26 06:29 EDT

## UX Problem

- The dashboard showed the reload area before the current balance, which made the page feel like billing configuration instead of account status.
- `$30` needed to be stated as stored account credit: `$30` reload becomes `$30` Bluey credits.
- The checkout return path was too vague. A beginner needs a visible `Refresh balance` action after payment.
- Auto Reload looked too much like a required billing setup. It should read as an optional backup.
- Landing-page pricing needed the same clear `$30 becomes $30 credits` wording.

## Fix

- Moved account KPIs above the credits section so current balance is the first dashboard task after the header.
- Added a `Refresh balance` button in the dashboard header.
- Changed the billing badge from `Secure checkout` to `No subscription`.
- Reframed the credits section as `Add credits` with a simpler explanation:
  - credits are stored balance
  - trial minutes are used first when available
  - `$30 reload = $30 Bluey credits`
  - each paid answer shows cost and remaining balance
  - checkout opens in a new tab
  - return and press `Refresh balance` if checkout is still processing
  - paid cloud work pauses at `$0` unless Auto Reload is enabled
- Reframed Auto Reload as `Auto Reload (optional)` with copy that says to keep it off if the user wants manual control.
- Updated dynamic JavaScript copy for manual reloads, Auto Reload rules, balance hints, checkout success, and the authenticated account rail.
- Updated landing-page pricing copy so the first public pricing card says `$30 becomes $30 Bluey credits`, `$15 minimum`, and `no subscription`.
- Added responsive dashboard styles for the refresh action, highlighted balance KPI, and clear credit-equation row.

## Files Touched

- `web/index.html`
- `web/assets/bluey-site.js`
- `web/assets/bluey-site.css`
- `docs/rounds/ROUND-186-WEB-DASHBOARD-RELOAD-SIMPLIFICATION.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Verification

Passed:

- `node --check web/assets/bluey-site.js`
- Local static smoke server with mocked authenticated account APIs at `http://127.0.0.1:4179/account`
- Desktop in-app browser smoke:
  - dashboard rendered
  - current balance appeared before reload
  - `Refresh balance` visible
  - `$30 reload = $30 Bluey credits` visible
  - no horizontal overflow
- Mobile in-app browser smoke at `390x844`:
  - no horizontal overflow
  - balance surfaced before reload
  - refresh action remained visible

## Current State

- Dashboard now reads as: account status, balance, usage, add credits, optional Auto Reload, then devices/sessions.
- Reload instructions are beginner-friendly and consistent between landing page, dashboard HTML, and dynamic JS copy.
- No server, checkout, auth, billing, or account API behavior changed.

## Remaining QA Gates

- Live checkout QA should confirm Square still opens in a new tab and returns to `/reload?reload=success`.
- Production account QA should confirm `Refresh balance` reloads the updated balance after a real checkout succeeds.
- Visual QA on the deployed site should confirm the public landing cards and authenticated dashboard match the local smoke.
