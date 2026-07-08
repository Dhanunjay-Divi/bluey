# ROUND-424 Sign-In Stale Card Clear

Date: 2026-07-08
Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Issue

The browser dashboard could say `Desktop Bluey is connected` and the overlay header could show the real balance, but the overlay body still showed an older `Sign in reopened` card with a connect code.

That made a successful sign-in look broken.

## Root Cause

The sign-in prompt was stored as a normal feed card. Signed-in state updated the overlay header and controls, but the old login card stayed in the feed.

The daemon also kept the completed device-login task in memory until a later login request cleaned it up, which made stale-code behavior easier to hit during repeated sign-in tests.

## Changes

- Added macOS feed cleanup for login cards when signed-in chrome is applied.
- Kept normal conversation/history cards intact; only system cards with login URLs are removed.
- Cleared the matching background login task immediately after device login completes.
- No deploy or release was performed. Per owner instruction, release promotion waits for an explicit signed deploy.

## Verification

- `native/macos/cue-overlay/build.sh` passed.
- `cargo test -p cue-daemon --lib clear_background_cloud_login_if_current --quiet` compiled and passed; the targeted filter matched no existing tests.
- `git diff --check` passed.
- No deploy or release was performed.
