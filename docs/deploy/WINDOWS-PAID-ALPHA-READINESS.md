# Bluey Windows Paid Alpha Readiness

This is the Windows parallel track for Bluey. It does not change the first
100 paid user architecture: Windows uses the same `bluey.sh` account, billing,
provider routing, release, support, and storage systems as macOS. There is no
separate backend for Windows.

Windows is not a blocker for a macOS-only first paid alpha. If we invite any
paid Windows users, this document becomes a P0 launch gate.

## Product Decision

Use a native Windows overlay, following the same shape as Pinky:

- Win32 HWNDs for native windowing and input.
- Direct2D/DirectWrite for sharp overlay text and rounded geometry.
- `SetWindowDisplayAffinity(..., WDA_EXCLUDEFROMCAPTURE)` for normal screen
  capture exclusion, with the documented Windows limitations.
- WASAPI for microphone and system-loopback audio capture.
- No Electron, WebView, WPF, .NET, Wails, or Fyne for the overlay surface.

Reason: the product is an always-on overlay. Native Win32 keeps startup fast,
keeps capture hiding under our control, avoids a large runtime, and matches the
platform-specific approach already used by the macOS Swift/AppKit overlay.

## Current State

Existing Windows pieces:

- Native overlay helper exists under `native/windows/cue-overlay`.
- Native audio helper exists under `native/windows/cue-audio`.
- Overlay helper uses Win32/Direct2D-style primitives and already has capture
  exclusion hooks.
- Audio helper uses WASAPI loopback/microphone capture and emits 16 kHz mono
  PCM for transcription.
- CLI/daemon have Windows paths for helper discovery, detached daemon launch,
  file picker, screen/page capture, and Credential Manager token storage.
- `scripts/build-windows.ps1` builds Rust plus overlay/audio into a loose
  `dist/bluey-windows-x64` directory.
- `infra/scoop/bluey.json` exists as a placeholder.

Known gaps:

- No production `install.ps1` yet.
- No signed/checksummed hosted Windows artifact yet.
- Makefile Windows packaging currently does not include all native helpers.
- Release matrix is not enabled for Windows release artifacts.
- Web download copy correctly still says Windows is coming soon.
- The Windows whisper helper is a stub; release behavior must either use
  managed STT only or document a no-local-whisper waiver.
- Clean Windows 10/11 paid flow has not passed.

## P0 Gate Before Any Paid Windows User

- Build on a clean Windows 10/11 machine with Visual Studio Build Tools:

```powershell
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --release
powershell -ExecutionPolicy Bypass -File native\windows\cue-overlay\build.ps1
powershell -ExecutionPolicy Bypass -File native\windows\cue-audio\build.ps1
powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
```

- Produce one canonical `bluey-<version>-windows-x86_64.zip` containing:
  `bin/bluey.exe`, `bin/bluey-daemon.exe`, `bin/bluey-overlay.exe`, and
  `bin/bluey-audio.exe`.
- Either ship a real `cue-whisper.exe` or explicitly waive local whisper for
  Windows and route captions through managed Bluey STT only.
- Add `scripts/install.ps1` with:
  per-user install under `%LOCALAPPDATA%\Programs\bluey`, checksum
  verification, PATH/shim setup, reinstall/upgrade behavior, and uninstall
  notes.
- Publish SHA256 and signed update manifest entries for the Windows artifact.
- `bluey on` starts daemon and overlay without dev flags.
- The first visible state is the compact pill, not a large expanded window.
- Clicking the pill expands the overlay and all controls are clickable in
  interactive mode.
- Sign-in/deep-link account linking works against `bluey.sh`.
- Listen captures real microphone and system audio and sends it through managed
  Bluey STT without exposing provider keys.
- Answer streams through managed Bluey routes, shows cost, and updates balance.
- Screen analysis works or is clearly disabled in UI until it is supported.
- Docs attach/indexing works or is clearly disabled in UI until it is supported.
- Session history loads, renames, and deletes with confirmation.
- `bluey off` exits daemon and helper processes.
- Capture exclusion is verified with Snipping Tool plus at least one common
  meeting/recording path such as Teams, Zoom, or Windows screen recording.
- Support/log export redacts tokens, provider keys, device codes, and user
  paths where required.
- Release artifact hygiene scan shows no provider keys, BYOK/customer-key
  release paths, mock transcript mode, or dev capture flags.

## P1 Before Broader Windows Self-Serve

- Code-sign binaries and installer, or keep Windows invite-only with clear
  unsigned-build guidance.
- Decide package format: PowerShell installer first; Scoop/MSI/MSIX later.
- Add Start Menu shortcut and uninstall cleanup.
- Verify multi-monitor behavior.
- Verify DPI scaling at 100%, 125%, and 150%.
- Verify click-through versus interactive mode.
- Verify drag/move/resize behavior.
- Verify sleep/wake/reconnect.
- Verify Defender/SmartScreen behavior.
- Verify corporate-managed Windows restrictions where possible.

## Operator Inputs Needed

- Access to a clean Windows 10 or Windows 11 test machine.
- Visual Studio Build Tools installed on that machine.
- A real Bluey test account with credits.
- Snipping Tool plus one meeting/recording app for capture-exclusion testing.
- Confirmation whether Windows is included in the first paid alpha invite list.

## Launch Rule

If the first paid alpha is macOS-only, this file is a parallel readiness track.
If the first paid alpha includes Windows users, every P0 item above must pass
before those users are invited.
