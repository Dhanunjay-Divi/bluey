# ROUND-467 Process Alias Developer Cleanup

Date: 2026-07-09
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Clean up obvious duplication and developer-onboarding friction after the Bluey
process alias work, without touching unrelated web/backend changes and without
doing a broad risky split of very large files.

## Findings

The largest files still need future architectural cleanup:

- `crates/cue-daemon/src/app.rs`: about 20k lines.
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`: about 15k lines.
- `server/src/api/router.rs`: about 10k lines.
- `web/assets/bluey-site.css`: about 8.5k lines.

Those should be split intentionally by subsystem in future rounds. This round
kept scope to process alias duplication because it is low-risk and directly
related to the current branch work.

## Changes

- Centralized CLI daemon/helper/uninstall alias lists in
  `crates/cue-cli/src/app.rs`.
- Centralized daemon audio helper candidate lists in
  `crates/cue-daemon/src/app.rs`.
- Centralized native overlay helper candidate lists in
  `crates/cue-daemon/src/app.rs`.
- Centralized system audio helper candidate lists in
  `crates/cue-daemon/src/audio/system_capture.rs`.
- Aligned macOS and Windows build outputs so release artifacts include the
  current aliases and the legacy compatibility aliases.
- Updated live release verification and local visual-smoke helpers to recognize
  `termb`, `hostovb`, and `adriverb`.
- Added `docs/dev/PROCESS-ALIASES.md` so a new developer can see the current
  process identity rules without reading the install scripts first.

## Preserved Behavior

- `termb` / `termb.exe` remains the preferred daemon identity.
- `hostovb` / `hostovb.exe` remains the preferred native overlay identity.
- `adriverb` / `adriverb.exe` remains the preferred audio helper identity.
- Legacy helper names remain available as fallbacks.
- The installer scripts remain explicit rather than over-abstracted so support
  can audit them quickly during install failures.
- Bluey replicates Pinky's process-identity mechanism, not Pinky's branding or
  image assets.

## Not Done

- No deployment.
- No GitHub Actions.
- No large-file module split.
- No unrelated web, billing, auth, or overlay UI cleanup.

## Verification

Run after code changes:

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
- `git diff --check -- crates/cue-cli/src/app.rs crates/cue-daemon/src/app.rs crates/cue-daemon/src/audio/system_capture.rs ops/install/install.sh scripts/install.sh ops/install/install.ps1 native/macos/cue-overlay/build.sh native/macos/cue-audio/build.sh native/windows/cue-overlay/build.ps1 native/windows/cue-audio/build.ps1 scripts/build-macos.sh scripts/build-macos-universal.sh scripts/build-windows.ps1 scripts/bluey-release-live-verify.sh scripts/bluey-visible-local.sh scripts/macos-overlay-visual-smoke.sh docs/dev/PROCESS-ALIASES.md docs/rounds/ROUND-465-COMPANION-IDENTITY-COLLISION-GUARD.md docs/rounds/ROUND-466-BLUEY-OWNED-PROCESS-ALIASES.md docs/rounds/ROUND-467-PROCESS-ALIAS-DEVELOPER-CLEANUP.md`

PowerShell parser validation may need to be run on a machine with `pwsh`
installed.
