# Windows SSH Build Smoke - 2026-06-19

## Scope

This round verifies the current Bluey branch on the Windows/Dell machine without touching Pinky.

Windows desktop capture/control smoke must not be launched over SSH. SSH runs outside the signed-in interactive desktop session, so it is valid for build/test/log work only.

## Machine

- Host: `100.118.147.23`
- SSH user: `dhanu`
- Windows identity observed: `dell\dhanu`
- Working copy: `C:\Users\Dhanu\bluey-codex-ai-site`
- Source branch: `codex/bluey-ai-site`
- Source commit before this round's fix: `e90f15c`

## What Changed

Windows `cargo clippy --all-targets -- -D warnings` exposed platform-specific warnings that macOS did not:

- macOS-only overlay socket code imported `ErrorKind` at module scope.
- Unix-only stale daemon cleanup helpers compiled unused stubs on Windows.
- Unix-only update countdown imports compiled on Windows.
- macOS meeting bundle constants compiled unused on Windows.
- A Unix-only test import in `cue-core` compiled unused on Windows.

Fixes are narrow `cfg` / direct-path cleanups only. No product behavior changed.

## Windows Verification

All commands below were run through SSH from the Windows clone:

```powershell
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --release
powershell -ExecutionPolicy Bypass -File native\windows\cue-overlay\build.ps1
powershell -ExecutionPolicy Bypass -File native\windows\cue-audio\build.ps1
powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
.\dist\bluey-windows-x64\bluey.exe --version
.\dist\bluey-windows-x64\bluey.exe --help
.\dist\bluey-windows-x64\bluey-daemon.exe --help
```

Results:

- `cargo fmt --all --check`: pass
- `cargo clippy --all-targets -- -D warnings`: pass
- `cargo build --release`: pass
- `native\windows\cue-overlay\build.ps1`: pass, produced `build\bluey-overlay.exe`
- `native\windows\cue-audio\build.ps1`: pass, produced `build\bluey-audio.exe`
- `scripts\build-windows.ps1`: pass
- Packaged folder: `dist\bluey-windows-x64`
- Packaged binaries:
  - `bluey.exe`
  - `bluey-daemon.exe`
  - `bluey-overlay.exe`
  - `bluey-audio.exe`
  - legacy aliases: `cue.exe`, `cue-daemon.exe`, `cue-overlay.exe`, `cue-audio.exe`
- CLI smoke:
  - `bluey.exe --version` prints `bluey 0.1.10`
  - `bluey.exe --help` prints the Bluey command surface
  - `bluey-daemon.exe --help` prints daemon options

## Local Mac Verification

Run from `/Users/uno/Downloads/cue`:

```bash
cargo fmt --all --check
cargo test -p cue-core -p cue-cli -p cue-daemon --lib
cargo clippy --all-targets -- -D warnings
```

Results:

- `cargo fmt --all --check`: pass
- `cargo test -p cue-core -p cue-cli -p cue-daemon --lib`: pass
  - `cue-cli`: 51 passed
  - `cue-core`: 78 passed
  - `cue-daemon`: 209 passed, 2 ignored
- `cargo clippy --all-targets -- -D warnings`: pass

## Not Verified Over SSH

The following must be run from the signed-in Windows desktop session, not SSH:

- `bluey on`
- overlay clickability
- capture exclusion via `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`
- mic/system audio capture
- real desktop sign-in/linking
- paid answer flow
- update/install from the packaged artifact

## Remaining Windows Gates

Before Windows alpha:

1. Add or finalize a Windows installer/update path. The signed updater is currently Unix installer shaped.
2. Run signed-in desktop smoke on the Dell:
   - launch from PowerShell or Windows Terminal in the user session
   - verify pill, expanded overlay, clickability, pass-through, fullscreen
   - verify capture exclusion with a normal screen recorder
   - verify mic and system audio captions
   - verify sign-in, answer, docs, screen analysis, balance
3. Decide whether the public web copy should keep Windows as "coming soon" until that desktop smoke is green.

