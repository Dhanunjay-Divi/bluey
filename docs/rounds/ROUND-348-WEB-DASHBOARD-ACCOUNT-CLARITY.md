# Round 348 - Web Dashboard Account Clarity

## Trigger

The owner asked for a focused Bluey UI/web pass on the beginner dashboard, reload and credits copy, balance clarity, saved sessions, account and login states, and the landing/download flow. The work needed to stay on the web/UI surface and avoid native overlay, audio, backend, or runtime changes.

## Root Cause/Fix

- Replaced the desktop dashboard home placeholder with a beginner-oriented Home page that surfaces the first session action, recent local sessions, account state, credits balance, sign-in state, and direct paths to saved sessions, search, live transcript, settings, and the web account page.
- Simplified the dashboard sidebar to the primary beginner destinations: Home, Sessions, Live, Answers, Search, and Settings.
- Clarified desktop dashboard credits copy so the balance pill, settings account card, trial-time state, Auto Reload state, and signed-out state all use consistent “credits” language.
- Tightened the static account page copy around sign-in, desktop link codes, credits balance, manual reload, Auto Reload, linked desktop/browser sessions, and cloud-synced saved sessions.
- Made the download route show the macOS install flow immediately, added a three-step install strip, and clarified the landing CTAs as Download Bluey and Sign in.
- Added a light-theme override layer for the Tailwind dashboard while keeping the existing dark dashboard palette as the default.

## Verification

- `npm --prefix crates/cue-dashboard/ui run build`
- `node --check web/assets/bluey-site.js`

## Current State

- Dashboard home is no longer an empty placeholder.
- Reload errors now render in the signed-in account message area instead of the hidden auth message area.
- The download page defaults to the macOS setup panel so new users see the install command without an extra click.
- No native overlay, audio, backend runtime, billing API, or cloud sync API files were changed.

## Remaining QA/Gates

- Optional visual QA in a real browser against `web/index.html` routes and the packaged Tauri dashboard.
- Deploy static web assets only after the owner is ready to publish the updated account and landing copy.
