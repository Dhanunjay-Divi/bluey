# Round 215 - Automatic Saved Session Sync

Date: 2026-06-27 01:29 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner pointed at the web account empty state saying:

`No saved sessions yet. Run bluey cloud sync after a local session, then refresh here.`

That is the wrong product expectation. Users should not need to discover or run a background sync command for saved sessions. Bluey should upload synced session snapshots automatically when the desktop app is signed in and running.

## Root Cause / Fix

- The daemon already had a real cloud sync implementation and a best-effort auto-sync path.
- The default setting was still `cloud_sync_enabled: false`, so normal linked accounts could silently avoid background sync unless a hidden/CLI preference was turned on.
- The web and CLI empty states still taught manual `bluey cloud sync` as the happy path.
- Auto-sync mostly ran on startup/status/end-session, but not consistently after live session mutations such as answers, attachments, transcript finals, instruction edits, and session switches.

Implemented:

- Default `CueSettings.cloud_sync_enabled` is now `true`.
- `bluey login` turns saved-session background sync on for linked accounts, so older settings files created under the old default do not block new linked accounts.
- Added a debounced daemon auto-sync scheduler:
  - debounce window: `20s`
  - aborts the previous pending debounce when a newer session change arrives
  - does not block local saves or answer flow
  - aborts cleanly on daemon shutdown
- Scheduled background sync after durable local session changes:
  - meeting start
  - final transcript add
  - final audio transcript add
  - answer saved
  - context attach
  - context remove
  - transcript clear
  - answer instructions save
  - session continue/open/rename/new
- Kept manual `bluey cloud sync` available as a support/debug command, but removed it from normal user-facing empty-state and download copy.
- Added explicit env opt-out support:
  - `BLUEY_AUTO_CLOUD_SYNC=0`
  - `CUE_AUTO_CLOUD_SYNC=0`

## Product Copy

- Web saved-session empty state now says:
  - `No saved sessions yet. Keep Bluey on while signed in; sessions sync automatically, then refresh here.`
- Download command grid now shows:
  - `bluey account` - `Check account and sync status`
- CLI cloud sessions empty state now says saved sessions sync automatically in the background.
- CLI settings now prints `Cloud sync: automatic` when the preference is enabled.
- Privacy/terms copy now describes signed-in saved-session sync and the ability to turn cloud sync off.

## Mac / Windows Parity

- This is shared daemon/core/CLI/web behavior, so Mac and Windows desktop builds use the same background sync defaults and scheduler.
- No native macOS or Windows overlay UI files required product-specific changes in this round.
- Windows overlay syntax was still checked for parity hygiene.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo fmt --manifest-path crates/cue-cli/Cargo.toml`
- `cargo fmt --manifest-path crates/cue-core/Cargo.toml`
- `git diff --check`
- `cargo test -p cue-core default_settings_enable_cloud_sync_after_sign_in --lib`
- `cargo test -p cue-daemon cloud --lib`
- `cargo build -p cue-cli`
- `cargo build -p cue-daemon --bin bluey-daemon`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
- `cargo test -p cue-daemon --lib` (`268 passed; 2 ignored`)
- `cargo test -p cue-core --lib` (`83 passed`)
- `cargo test -p cue-cli --lib` (`53 passed`)

Local visible run refreshed:

- `/Users/uno/Downloads/cue/target/debug/bluey settings --cloud-sync true`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`
- `/Users/uno/Downloads/cue/target/debug/bluey cloud status`

Latest local status after relaunch:

- daemon pid `30166`
- overlay visible `true`
- overlay capture excluded `false`
- overlay opacity `0.92`
- cloud auth `TokenConfigured`
- cloud sync `Ready`
- cloud sync preference `automatic`

## Current State

- The local debug overlay is running in visible QA mode from `/Users/uno/Downloads/cue/target/debug/bluey`.
- This machine has `Cloud sync: automatic`.
- A signed-in Bluey desktop session should now sync saved-session snapshots automatically without requiring the user to run `bluey cloud sync`.

## Remaining QA / Gates

- Manually create an answer or attach a context item while signed in, wait at least 20 seconds, then refresh the web saved sessions list.
- Before release/deploy, restart normal capture-excluded mode and run the visible-flag release hygiene scan.
- Cloud deletion remains separate: this round does not invent a cloud delete protocol because the existing sync endpoint is an upsert batch.
