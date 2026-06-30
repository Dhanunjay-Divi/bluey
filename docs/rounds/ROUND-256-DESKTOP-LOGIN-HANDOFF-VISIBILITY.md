# Round 256 - Desktop Login Handoff Visibility

## Trigger

The owner ran `bluey login`, the browser opened, login succeeded, and the page switched to the dashboard, but the terminal still appeared to be waiting.

## Root Cause

`bluey login` uses a secure device-code flow:

1. CLI starts a device code.
2. CLI opens `/login?user_code=...`.
3. Browser account page must approve that user code.
4. CLI polls until approval, then stores desktop tokens.

The web UI already required an explicit confirmation before linking the desktop account, which is the right security posture. The problem was visibility: the pending device-link hint lived inside `#accountAuthCard`, and once the user signed in, the page hid that card and showed the dashboard. The terminal kept polling, but the dashboard no longer showed the obvious `Connect desktop` action.

## Fix

- Added a dashboard-level pending device-link banner:
  - `#dashboardDeviceLinkHint`
  - visible after login when a `user_code` is present
  - clear copy: `Finish connecting desktop Bluey`
  - clear action: `Connect desktop`
- Kept the explicit confirmation requirement so a random pasted code cannot silently attach someone else's desktop to a signed-in account.
- Updated the browser copy to say the terminal is waiting on that code.
- Updated CLI `bluey login` source to print:
  - `Waiting for browser approval...`
  - `After signing in, click Connect desktop on the Bluey page.`
  - periodic waiting hints every 20 seconds
  - clearer timeout text

## Deployment

Static web assets were deployed to `https://bluey.sh` using release-safe rsync excludes so installer and release files were preserved:

```text
/install.sh
/install.ps1
/latest.json
/latest.json.sig
/releases/**
```

The web-side fix is live now. The CLI-side terminal wording is in source and will appear for installed users after the next packaged binary release.

## Verification

Passed locally:

```bash
node --check web/assets/bluey-site.js
cargo check -p cue-cli
git diff --check
```

Passed live checks:

```bash
curl -fsSL 'https://bluey.sh/login?user_code=TEST-CODE'
curl -fsSL https://bluey.sh/assets/bluey-site.js
curl -fsSL https://bluey.sh/assets/bluey-site.css
curl -fsSL https://bluey.sh/install.sh | sed -n '1p'
curl -fsSL https://bluey.sh/latest.json
```

Observed:

```text
dashboardDeviceLinkHint present
Finish connecting desktop Bluey present
Click Connect desktop to finish bluey login present
install_head=#!/usr/bin/env bash
latest=0.1.17
```

## Current State

If a user signs in from a `bluey login` browser page, the dashboard now keeps the desktop connection action visible. The terminal should finish as soon as the user clicks `Connect desktop`.

## Remaining QA/Gates

- Package and publish the next CLI binary so installed users also see the improved terminal waiting hints.
- Full browser automation against a real account would be useful for the exact sign-in -> dashboard -> approve -> CLI exits path.
