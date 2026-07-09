# ROUND-455 Download Command Support Surface

Date: 2026-07-09
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Make the Bluey download page simpler for new users by copying Pinky's command-page structure without copying Pinky's exact command list.

## Changes

- Reworked the download-page command section into:
  - install instructions
  - `bluey on` start command
  - support note immediately after setup
  - visible everyday commands
  - collapsed advanced commands under `More commands`
- Kept the public command grid focused on:
  - `bluey on`
  - `bluey off`
  - `bluey update`
  - `bluey status`
  - `bluey usage`
  - `bluey portal`
  - `bluey support`
  - `bluey help`
- Moved technical commands into the collapsed advanced section:
  - `bluey login`
  - `bluey logout`
  - `bluey sessions`
  - `bluey doctor`
  - `bluey doctor --zip`
  - `bluey logs export`
  - `bluey export`
  - `bluey uninstall`
- Added `bluey doctor --zip` as an alias for the redacted `bluey support` bundle, so support can ask for either command.
- Made the CLI help surface cleaner by exposing the everyday commands and keeping advanced account/data commands hidden from the main help surface.

## Product Notes

- `bluey support` remains the main support command.
- `bluey doctor` remains the permission/setup diagnostic command.
- `bluey doctor --zip` is intentionally an alias so users coming from Pinky-style support instructions have a familiar path.
- No deploy was performed in this round.

## Verification

- Passed `node --check web/assets/bluey-site.js`.
- Passed `cargo check -p cue-cli`.
- Passed `cargo run -p cue-cli --bin bluey --quiet -- help`.
- Passed `cargo run -p cue-cli --bin bluey --quiet -- doctor --help`.
- Passed `git diff --check -- crates/cue-cli/src/app.rs web/index.html web/assets/bluey-site.css docs/rounds/ROUND-455-DOWNLOAD-COMMAND-SUPPORT-SURFACE.md`.
