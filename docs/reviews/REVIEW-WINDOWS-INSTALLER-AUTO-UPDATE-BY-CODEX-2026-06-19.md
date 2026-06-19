# Review: Windows Installer + Auto-Update Wiring

Verdict: Yellow until a clean Windows desktop smoke publishes and installs a
real `windows-x86_64` artifact.

## Findings

- P0 external gate: Windows install/update plumbing is now present, but it is
  not enough to invite paid Windows users. The remaining blocker is a clean
  Windows 10/11 desktop smoke from an interactive session with the release zip
  hosted at `bluey.sh`.
- P1 operational note: `/install.ps1` must be treated like `/install.sh`,
  `/latest.json`, and `/latest.json.sig` during static deploys. The manual
  deploy script now preserves and checks it.

## What passed review

- The CLI still requires a verified signed manifest before install unless the
  explicit dev escape `BLUEY_UPDATE_ALLOW_UNSIGNED=1` is set.
- Windows chooses the `windows_install` manifest entry and runs PowerShell;
  macOS/Linux continue to use `install.sh`.
- The PowerShell installer verifies the Windows artifact SHA256 before replacing
  `%LOCALAPPDATA%\Bluey\bin`.
- The public download page now gives the expected command:

```powershell
irm https://bluey.sh/install.ps1 | iex
```

## Areas most likely wrong

- We have not yet run the new installer against a freshly published Windows
  release zip from a real signed-in desktop session.
- Windows Smart App Control / Defender behavior may still need user-facing
  guidance once the first unsigned alpha zip is smoke-tested.
