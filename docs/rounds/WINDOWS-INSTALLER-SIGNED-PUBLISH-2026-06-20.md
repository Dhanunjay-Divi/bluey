# Windows Installer Signed Publish - 2026-06-20

## Scope

Publish the Windows install/update path for Bluey without using GitHub Actions.

This round closed the live gap where `https://bluey.sh/install.ps1` returned the
website HTML and `latest.json` only advertised `darwin-arm64`.

## Release Inputs

- Branch: `codex/bluey-ai-site`
- Release version: `0.1.10`
- Release signing key file: local operator key at `/Users/uno/.bluey/release/bluey-release-ed25519.pem`
- Embedded update public key source: `/Users/uno/.bluey/release/bluey-release-ed25519.pub.b64`
- Publish target: `root@165.227.77.152:/var/www/bluey`

Private signing material was not pasted into chat and was not committed.

## What Changed

- Rebuilt the macOS artifact with `BLUEY_UPDATE_PUBKEY` embedded.
- Built the Windows artifact on the Windows machine from a clean source archive,
  also with `BLUEY_UPDATE_PUBKEY` embedded.
- Packaged Windows as `dist/bluey-0.1.10-windows-x86_64.zip` with the installer
  expected `bin\...` layout.
- Published `install.ps1`, `install.sh`, `latest.json`, `latest.json.sig`, and
  both platform artifacts manually from local/cloud machines.
- Hardened `scripts/publish-bluey-release.sh` so staged and remote release files
  are web-readable. This prevents Caddy from falling through to the SPA HTML
  when a Windows zip copied back from Windows has restrictive permissions.

## Verification

- `make package-darwin-arm64` passed with the update public key embedded.
- Windows `scripts\build-windows.ps1` passed from a clean release build folder.
- `strings` confirmed the embedded public key in:
  - macOS `bin/bluey`
  - Windows `bin/bluey.exe`
- Local signed manifest dry run produced:
  - `latest.json`
  - `latest.json.sig`
  - `windows_install`
  - `platforms.darwin-arm64`
  - `platforms.windows-x86_64`
- Live checks confirmed:
  - `https://bluey.sh/install.ps1` returns PowerShell.
  - `https://bluey.sh/latest.json` lists both Mac and Windows.
  - `https://bluey.sh/latest.json.sig` is present.
  - `https://bluey.sh/releases/v0.1.10/bluey-0.1.10-windows-x86_64.zip`
    serves as `application/zip`, not HTML.
  - Live Windows artifact SHA256 matches the manifest.
- Windows SSH installer smoke:
  - `irm https://bluey.sh/install.ps1 | iex` downloaded the 16 MB zip.
  - SHA256 verification passed.
  - Bluey installed to `%LOCALAPPDATA%\Bluey\bin`.
  - `bluey --version` returned `bluey 0.1.10`.
  - `bluey update --check-only` returned `Bluey is up to date (0.1.10).`

## Not Verified Here

Windows graphical overlay smoke was not run from SSH. Windows SSH runs in
session 0, so real overlay, capture hiding, input, and tray behavior must be
tested from the signed-in Windows desktop session.

## Notes For Kiro

- No GitHub Actions were used for this preprod/manual publish.
- The release manifest is signed and the platform CLIs embed the update public
  key.
- The publish-permission hardening landed in:
  `fix(release): publish artifacts with web-readable permissions`.
- The remaining Windows gate is a signed-in desktop smoke, not an installer or
  update-manifest blocker.
