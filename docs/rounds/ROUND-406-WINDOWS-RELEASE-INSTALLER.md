# ROUND-406 Windows Release Installer

Date: 2026-07-07
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Continue the code-follow-up replacement release and make the Windows binary installer real, not preview-gated.

## Changes

- Enabled `windows-latest` in `.github/workflows/release.yml` with the `x86_64-pc-windows-msvc` target.
- Split Windows Cargo builds onto PowerShell so MSVC uses the Visual Studio linker instead of Git Bash's `link.exe`.
- Updated the macOS dashboard build command for the current Tauri CLI so macOS packaging does not cancel the Windows matrix job.
- Made the Tauri dashboard UI build path-independent in CI after `cargo tauri build` ran `beforeBuildCommand` from a different working directory than expected.
- Disabled matrix fail-fast so a macOS packaging issue does not cancel Windows artifact validation before we can inspect it.
- Fixed the Windows overlay helper link line to include `advapi32.lib`, which is required for the registry placement APIs used by the overlay.
- Removed the `BLUEY_WINDOWS_INSTALL_PREVIEW=1` guard from `ops/install/install.ps1`.
- Prepared desktop release `0.1.90` so macOS and Windows can be published under one signed manifest.

## Expected User Behavior

- macOS users continue installing with `curl -fsSL https://bluey.sh/install.sh | bash`.
- Windows users can install with `irm https://bluey.sh/install.ps1 | iex` once the Windows artifact is present in `latest.json`.
- Both installers verify the downloaded artifact checksum before installing.

## Verification

- `cargo check -p cue-cli -p cue-daemon --quiet`
- Release workflow YAML parse.
- Tauri config JSON parse.
- `git diff --check`
- Pending in this round: build macOS artifact, build Windows artifact through GitHub Actions, publish, and verify live manifest.
