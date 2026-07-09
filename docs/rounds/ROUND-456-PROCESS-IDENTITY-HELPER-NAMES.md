# Round 456 - Process Identity Helper Names

Date: 2026-07-09
Branch: codex/bluey-web-ui-parallel-20260704
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Copy Pinky's process identity pattern so Bluey does not show obvious product process names in Activity Monitor / Task Manager while still keeping CLI, updater, doctor, uninstall, support, and logs safe.

## User-Visible Problem

Activity Monitor still showed:

- `bluey-overlay-macos`
- `bluey-daemon`

The intended pattern is:

- User command stays `bluey`.
- Background daemon identity uses `Terminal` on macOS and `Terminal.exe` on Windows.
- Overlay helper uses `host-overlay`.
- Audio helper uses `audio-driver`.
- Legacy product-named binaries continue to work during update/uninstall/dev.

## Changes Made

- macOS install/update scripts now create aliases in the install `bin` directory:
  - `Terminal` from `bluey-daemon` / `cue-daemon`
  - `host-overlay` from `bluey-overlay-macos` / `cue-overlay-macos`
  - `audio-driver` from `bluey-audio-macos` / `cue-audio-macos`
- Windows installer now creates:
  - `Terminal.exe`
  - `host-overlay.exe`
  - `audio-driver.exe`
- `bluey on` now prefers launching the daemon identity binary when present.
- Daemon PID reuse checks accept `Terminal` only when the expected daemon path is known and matches after canonicalization.
- Windows installer cleanup can stop `Terminal`, `host-overlay`, `audio-driver`, and `screen-driver` only when the executable path is inside the Bluey install root.
- Runtime helper discovery now prefers:
  - `host-overlay` / `host-overlay.exe`
  - `audio-driver` / `audio-driver.exe`
  while keeping old helper names as fallback.
- macOS overlay build now emits `.build/host-overlay` and an app bundle whose executable is `host-overlay`.
- Windows native helper builds now emit `host-overlay.exe` and `audio-driver.exe`.
- Release verification now requires the daemon identity and generic host overlay helper in macOS artifacts.
- Support log prefixing maps `Terminal` and `Terminal.exe` back to daemon logs so support tooling remains honest.
- CLI uninstall removes both new and legacy names.

## Safety Notes

- The real system Terminal must never be killed. The Windows stop path is install-root filtered for generic names.
- On Unix, stale PID command matching rejects generic `Terminal` unless Bluey knows the exact expected binary path.
- Logs still identify components as daemon, overlay, or audio; only the OS-visible executable names change.
- No random visible process names were added. Hashes remain appropriate for sockets/log correlation only.

## Compatibility

Legacy names remain supported:

- `bluey-daemon`
- `cue-daemon`
- `bluey-overlay-macos`
- `cue-overlay-macos`
- `bluey-audio-macos`
- `cue-audio-macos`
- Windows equivalents

This keeps old installs, dev workflows, and uninstall/update paths from breaking during the transition.

## Deployment

No deploy was performed in this round. User asked not to deploy every round and to reserve GitHub Actions for explicit signed deploy requests.

## Verification Planned

- Shell syntax checks for macOS/Linux scripts.
- PowerShell parser checks when `pwsh` is available.
- Targeted CLI tests for daemon identity lookup and stale PID matching.
- Targeted logging test for `Terminal` log prefix.
- `cargo check` for touched Rust crates where feasible.
- `git diff --check`.

