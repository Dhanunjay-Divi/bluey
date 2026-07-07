# ROUND-406 Windows Release Installer

Date: 2026-07-07
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Continue the code-follow-up replacement release and make the Windows binary installer real, not preview-gated.

## Changes

- Enabled `windows-latest` in `.github/workflows/release.yml` with the `x86_64-pc-windows-msvc` target.
- Removed the `BLUEY_WINDOWS_INSTALL_PREVIEW=1` guard from `ops/install/install.ps1`.
- Prepared desktop release `0.1.90` so macOS and Windows can be published under one signed manifest.

## Expected User Behavior

- macOS users continue installing with `curl -fsSL https://bluey.sh/install.sh | bash`.
- Windows users can install with `irm https://bluey.sh/install.ps1 | iex` once the Windows artifact is present in `latest.json`.
- Both installers verify the downloaded artifact checksum before installing.

## Verification

- Pending in this round: build macOS artifact, build Windows artifact through GitHub Actions, publish, and verify live manifest.
