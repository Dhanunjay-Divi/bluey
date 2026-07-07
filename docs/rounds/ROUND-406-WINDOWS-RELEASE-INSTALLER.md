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
- Made the Tauri CLI install idempotent with `cargo install --force` so cached GitHub runners do not fail on an existing `cargo-tauri` binary.
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
- GitHub Actions run `28848569586` produced:
  - `bluey-0.1.90-darwin-arm64.tar.gz`
  - `bluey-0.1.90-windows-x86_64.zip`
- Windows runner passed Rust release build, native helper build, ZIP packaging, checksum, and artifact upload.
- macOS arm64 runner passed daemon/CLI build, dashboard app build, native helper build, tarball packaging, checksum, and artifact upload.
- Release hygiene scan passed for both published artifacts.
- Published `0.1.90` to `https://bluey.sh`.
- Live verification passed:
  - `latest.json` signature verified.
  - `install.sh` served as `application/x-shellscript`.
  - `install.ps1` served as `application/x-powershell`.
  - `darwin-arm64` artifact SHA verified and unpacked binaries reported `0.1.90`.
  - `windows-x86_64` artifact SHA verified.

## Published Artifact Hashes

- `darwin-arm64`: `79943c1ffbcee3680ba420316f2cf969eaf6af49d69cfbdde20e844493b0ca99`
- `windows-x86_64`: `48d12fc79cd079b7c82367c584f2d81c10277d7061900be79bb5a2cc95b9aa80`

## Notes

- The public `0.1.90` manifest includes Apple Silicon macOS and Windows x86_64. Intel macOS was not published in this round because the GitHub-hosted Intel macOS runner stayed queued after the validated platform artifacts were ready. The previous live manifest also did not include an Intel artifact, so this release does not remove an existing public platform.
