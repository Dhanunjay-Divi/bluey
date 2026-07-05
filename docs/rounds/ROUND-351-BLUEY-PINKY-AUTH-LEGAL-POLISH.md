# Round 351 - Bluey Pinky Auth and Legal Polish

## Trigger

The owner asked Bluey to keep its blue color system while matching Pinky's web element patterns more closely: the small sun/moon theme switch, Pinky-like footer, clearer Try Us fallback, usable login/signup/verify cards, and nice `/terms` and `/privacy` pages.

## Root Cause/Fix

- Reworked the guest account card into the Pinky-style structure with Bluey branding:
  - centered logo and wordmark,
  - `Welcome back`, `Create account`, and `Check your email` states,
  - password visibility buttons,
  - confirm-password field for signup,
  - Terms/Privacy acceptance row,
  - Home/Download/Pricing links,
  - clearer red/green auth message styling.
- Added a guest-account sun/moon theme switch and tightened the shared theme pill so dark/light modes stay polished.
- Improved light-theme account contrast for inputs, links, cards, footer, and legal pages.
- Changed Try Us failure handling so unavailable API states do not display fake username/password fields.
  - Current fallback now explains that the web UI is ready but the temp-trial API endpoint is not live yet.
  - Modal actions reduce to `Download Bluey` and `Create account`.
- Added `/terms` and `/privacy` route aliases while preserving `/docs/terms` and `/docs/privacy`.
- Simplified Bluey's footer copy to the Pinky-like pattern:
  - `Reach us at hello@bluey.sh`
  - `Built with open source technology`
  - `Terms`
  - `Privacy`
- Retouched legal pages into a narrower, simpler document layout.
- Fixed mobile landing nav spacing so the theme switch does not overlap the Bluey wordmark.
- Bumped static web cache keys through `2026070408`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Local fallback preview server on `127.0.0.1:4175` for app-route smoke.
- Local mock-only signup preview server on `127.0.0.1:4176` for the signup verification UI state.
- In-app browser desktop smoke:
  - landing footer/cards/theme toggle render with Bluey colors,
  - Try Us fallback hides credential fields and copy button when the API is unavailable,
  - login opens as `Welcome back`,
  - signup shows confirm password and normal-sized Terms checkbox,
  - failed login renders a red message panel,
  - mocked signup start opens `Check your email` with code input, Verify, Resend, and alternate email actions,
  - light theme keeps the toggle knob inside the pill and preserves readable auth links,
  - `/terms` shows `Terms of Use`,
  - `/privacy` shows `Privacy Policy`.
- In-app browser mobile smoke:
  - login card fits at `390x844` without horizontal overflow,
  - landing nav no longer overlaps at `390x844`,
  - landing cards stack into one column.

## Current State

- This round only changes static web UI files and this round doc.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.
- The Try Us temporary-account backend endpoint is still not live in production from Round 350's API rollout blocker, so the web fallback remains intentionally truthful.
- The unrelated untracked review handoff file remains untouched.

## Remaining QA/Gates

- Deploy the static web bundle to `bluey.sh` and do a live visual smoke.
- After the production API toolchain/build path is fixed, smoke a real 24-hour Try Us account and account-conversion flow on live.
