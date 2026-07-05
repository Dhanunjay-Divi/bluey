# Round 370 - Landing Account Nav Cleanup

## Trigger

The landing page showed signed-in users a plain `Sign out` link in the top-right nav, while the dashboard used a compact human account icon. The landing page also still showed bottom `Start free`, `Add credits`, and `Teams` cards, which mixed billing/team choices into the main landing experience.

## Root Cause/Fix

- Replaced signed-in `Sign out` links on landing, download, and policy pages with the same compact account icon/menu pattern used elsewhere.
- Added a signed-in landing header connect-code form with `Connect` copy, matching the current Bluey code-linking language.
- Kept the full Change Password/Delete Account actions on the dashboard, where the modals live and render correctly.
- Hid `Try Us` for signed-in users and swapped that hero slot to `Dashboard`.
- Removed the landing footer pricing/action strip so the page stays focused on download, trial, and desktop code connection.
- Shared the account-menu JS across multiple page menus instead of depending on one hardcoded account-profile ID.
- Added a global account-menu initializer so non-dashboard account icons open correctly.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.css web/assets/bluey-site.js`
- Local browser smoke at `http://127.0.0.1:4177/` confirmed the guest landing no longer renders pricing cards and keeps `Get Started` / `Try Us`.
- Static deploy preserved installer/release files with `rsync` excludes.
- Live smoke confirmed:
  - `https://bluey.sh/` references `bluey-site.css?v=2026070427` and `bluey-site.js?v=2026070427`
  - no visible landing `class="plan"` cards remain
  - no old landing/download/policy `nav-signout` signed-in buttons remain
  - account profile/menu and `Connect` code markup are present
  - live JS passes `node --check`
  - `https://bluey.sh/health` returns `status=ok`
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json.sig` still return `200`

## Current State

The landing, download, and policy pages now keep the Pinky-like compact account icon on top-right for signed-in users, keep Bluey blue styling, and no longer show the billing/team action cards at the bottom of the landing page.

## Remaining QA/Gates

- Have the owner refresh `https://bluey.sh/` while signed in and point out any remaining sizing/alignment nits.
