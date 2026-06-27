# Round 218 - Visible Mode History Boot

Date: 2026-06-27 02:24 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner ran the visible-mode helper, but `bluey status` still showed:

- `overlay_capture_excluded: true`

The owner also reported that history/saved sessions take time as soon as the app opens.

## Root Cause

Visible mode failed because the previous release/security pass built the macOS overlay in `release` mode:

- release macOS overlay binaries compile out the debug visible-capture gate
- the visible helper was starting Bluey successfully, but the native overlay still reported secure capture exclusion
- the helper printed success before checking the final daemon status

Saved sessions felt slow because account dashboard boot awaited saved-session history together with account and usage calls. A slow `/sync/sessions` response could delay the account dashboard's first useful render.

## Fix

Visible local helper:

- `scripts/bluey-visible-local.sh` now prefers the repo debug `target/debug/bluey` before installed `~/.bluey/bin/bluey`
- rebuilds the macOS overlay in debug mode before local visible QA
- pins `BLUEY_DAEMON_BIN` to the matching local daemon when possible
- pins `BLUEY_OVERLAY_BIN` to the local rebuilt macOS overlay
- forces the raw helper path for local QA
- sets both capture-visible request env names for compatibility
- verifies `bluey status` reports `overlay_capture_excluded: false` before printing success
- exits with a clear error and latest status if visible mode does not take effect

Web dashboard:

- saved sessions now show `Loading saved sessions...` immediately
- account and usage load no longer wait for saved-session history
- linked devices and saved sessions continue loading in the background
- saved-session errors are still shown in the saved-session detail area

## Mac / Windows Parity

- The visible-mode fix is macOS-specific because screenshot-visible overlay QA is a macOS overlay capture-exclusion concern.
- The helper still works as a local QA wrapper on other platforms, but the debug overlay rebuild/pin path only runs on Darwin.
- The dashboard saved-session load improvement is shared web UI and benefits all users/platforms.

## Verification

Passed:

- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`
- `bash -n scripts/bluey-visible-local.sh`
- `node --check web/assets/bluey-site.js`
- `cargo test -p cue-daemon macos_overlay_capture_visible_requires_dev_and_local_gates --lib`
- `scripts/release-hygiene-scan.sh`

Latest local visible status after the fixed helper:

- daemon pid `78144`
- overlay visible `true`
- overlay capture excluded `false`
- screen capture active `false`

## Current State

- Local Bluey is currently running in visible QA mode.
- The helper now fails closed if the native overlay does not actually become visible to capture.
- The dashboard can render account/usage first while saved sessions load separately.

## Remaining QA / Gates

- After visible QA, return to normal capture-excluded mode with:
  - `/Users/uno/Downloads/cue/target/debug/bluey off`
  - `/Users/uno/Downloads/cue/target/debug/bluey on`
- Before release/upload, confirm `overlay_capture_excluded: true` for normal runtime and run the release hygiene scan.
