# Windows Installer + Auto-Update Wiring

Date: 2026-06-19
Branch: `codex/bluey-ai-site`

## Why

Windows should follow the same user-facing shape as macOS:

```powershell
irm https://bluey.sh/install.ps1 | iex
bluey on
```

`bluey on` should also use the same signed update manifest before starting.
Before this round, the manifest could list a Windows zip, but the root
installer was only `install.sh` and the CLI updater always executed `bash`.

## What changed

- Added `ops/install/install.ps1`.
  - Installs per-user to `%LOCALAPPDATA%\Bluey\bin`.
  - Resolves `latest.json` and the `windows-x86_64` artifact.
  - Verifies artifact SHA256 from the signed manifest path or
    `SHA256SUMS.txt`.
  - Replaces the local `bin` directory and adds it to the user PATH.
- Updated `scripts/publish-bluey-release.sh`.
  - Publishes `install.ps1`.
  - Adds `windows_install` with SHA256 and size to `latest.json`.
- Updated `crates/cue-cli/src/update.rs`.
  - macOS/Linux continue to use `install.sh` with `bash`.
  - Windows uses `install.ps1` with `powershell.exe -NoProfile
    -ExecutionPolicy Bypass -File`.
  - The installer hash still comes from the signed manifest before anything is
    executed.
- Updated `web/index.html`.
  - `/download` now shows the Windows PowerShell path instead of "coming soon".
- Updated deploy/docs so manual web deploys preserve `/install.ps1` and the
  release runbooks mention platform-specific installer hashes.

## Still required before paid Windows users

- Produce and publish a real `bluey-<version>-windows-x86_64.zip` from a
  Windows release build.
- Clean Windows 10/11 smoke from the signed-in desktop session, not SSH session
  0:
  - `irm https://bluey.sh/install.ps1 | iex`
  - `bluey on`
  - sign in
  - Listen, Answer, Screen, Docs, sessions, balance, update check
- Verify capture exclusion and clickability in the native Windows overlay.

## Verification

Planned command gate for this round:

```bash
cargo fmt --all --check
cargo test -p cue-cli update --lib
bash -n scripts/publish-bluey-release.sh
pwsh -NoProfile -Command '$null = [scriptblock]::Create((Get-Content ops/install/install.ps1 -Raw)); "ok"'
git diff --check
```
