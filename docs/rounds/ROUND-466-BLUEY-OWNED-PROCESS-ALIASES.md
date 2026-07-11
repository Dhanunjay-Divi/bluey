# ROUND-466 Bluey-Owned Process Aliases

Date: 2026-07-09
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Avoid collisions with Pinky or real OS tools while still keeping Bluey process names stable enough for install, update, doctor, support, and uninstall.

The user asked whether Bluey could use names like `adriverb`, `hostovb`, and `termb` instead of reusing generic names such as `Terminal`, and confirmed this should work on both macOS and Windows.

## Decision

Use stable Bluey-owned aliases, not random per-run process names:

- `termb` / `termb.exe`: daemon companion identity.
- `hostovb` / `hostovb.exe`: native overlay helper identity.
- `adriverb` / `adriverb.exe`: audio helper identity.

Keep legacy aliases as fallbacks only:

- `Terminal` / `Terminal.exe`
- `host-overlay` / `host-overlay.exe`
- `audio-driver` / `audio-driver.exe`
- old `bluey-*` and `cue-*` helper names

Random or 6-character hashes should stay in sockets/logs/support refs if needed, not in process executable names. Stable names keep cleanup and support reliable.

## Changes

- CLI daemon discovery now prefers `termb` before `Terminal`, `bluey-daemon`, and `cue-daemon`.
- CLI uninstall cleanup recognizes the new macOS and Windows aliases.
- CLI macOS permission prompt helper discovery now prefers `adriverb`.
- Daemon audio helper discovery now prefers `adriverb` on macOS and `adriverb.exe` on Windows.
- System audio supervisor now prefers `adriverb` / `adriverb.exe`.
- Native overlay discovery now prefers `hostovb` / `hostovb.exe`.
- macOS socket overlay detection accepts `hostovb`.
- macOS overlay app bundle lookup accepts `hostovb.app`.
- macOS installers create `termb`, `hostovb`, and `adriverb` aliases inside Bluey's install root.
- macOS installers do not publish `termb` or `Terminal` into shared PATH helper links.
- Windows installer creates `termb.exe`, `hostovb.exe`, and `adriverb.exe` aliases.
- Windows path-filtered cleanup includes the new aliases, so cleanup only stops helpers inside the Bluey install root.
- macOS and Windows build scripts now emit the new aliases alongside legacy names.
- Release verification now checks the new aliases so downloadable bundles cannot drift back to only `Terminal`, `host-overlay`, and `audio-driver`.

## Image And Icon Note

Bluey should copy Pinky's process-identity structure, not Pinky's visual assets.
Pinky does not have separate per-helper icon images that need to be copied here.
Bluey keeps its own logo and helper bundle metadata while using stable helper
process names.

## Deployment

Not deployed in this round. The user said not to deploy until explicitly requested.

## Verification

- `cargo fmt -p cue-cli -p cue-daemon`
- `cargo test -p cue-cli resolve_daemon_bin --quiet`
- `cargo check -p cue-daemon --quiet`
- `bash -n ops/install/install.sh`
- `bash -n scripts/install.sh`
- `bash -n native/macos/cue-overlay/build.sh`
- `bash -n native/macos/cue-audio/build.sh`
- `bash -n scripts/build-macos.sh`
- `bash -n scripts/build-macos-universal.sh`
- `bash -n scripts/bluey-release-live-verify.sh`
- `bash -n scripts/bluey-visible-local.sh`
- `bash -n scripts/macos-overlay-visual-smoke.sh`
- `git diff --check -- crates/cue-cli/src/app.rs crates/cue-daemon/src/app.rs crates/cue-daemon/src/audio/system_capture.rs ops/install/install.sh scripts/install.sh ops/install/install.ps1 native/macos/cue-overlay/build.sh native/macos/cue-audio/build.sh native/windows/cue-overlay/build.ps1 native/windows/cue-audio/build.ps1 scripts/build-macos.sh scripts/build-macos-universal.sh scripts/build-windows.ps1 scripts/bluey-release-live-verify.sh scripts/bluey-visible-local.sh scripts/macos-overlay-visual-smoke.sh docs/dev/PROCESS-ALIASES.md docs/rounds/ROUND-466-BLUEY-OWNED-PROCESS-ALIASES.md`

PowerShell syntax parsing was not run because `pwsh` is not installed on this Mac.
