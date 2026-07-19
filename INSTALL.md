# Installing Bluey

This source documentation targets Bluey `0.1.104`. The signed live manifest at
`https://bluey.sh/latest.json` is the authority for the version and artifacts
customers can currently download.

## Supported Release Artifacts

| Platform | Status |
| --- | --- |
| macOS on Apple silicon | Available as `darwin-arm64` |
| macOS on Intel | Available as `darwin-x86_64` |
| macOS universal | Available as `darwin-universal` |
| Windows x86-64 | Available; install-smoked on Windows 11 |
| Linux | Not in the current manifest |

Do not treat source compatibility or an older artifact as a current support
promise. The live manifest is the source of truth for downloadable platforms.

## macOS

Open Terminal and run:

```bash
curl -fsSL https://bluey.sh/install.sh | bash
bluey on
```

The installer selects the signed Apple silicon or Intel archive for the current
machine; a universal archive is also published. Bluey requests operating system
permissions only when a selected feature needs them. Microphone access is
needed for mic transcription; system-audio, screen-recording, automation, or
accessibility access depends on the context features you choose.

## Windows

Open PowerShell and run:

```powershell
irm https://bluey.sh/install.ps1 | iex
bluey on
```

The current public artifact is for Windows x86-64 and has been install-smoked
on Windows 11. Windows 10 is not claimed by the current release record.

## What The Installer Verifies

The hosted installers read the current release manifest, select the matching
platform artifact, and verify its pinned SHA256 before replacing an install.
The Bluey updater also verifies the detached signature over `latest.json`.

These checks protect the release download path. They do not imply macOS
notarization, Windows Authenticode signing, or support for a platform that is
not listed in the manifest.

## First Run

`bluey on` opens the compact Bluey overlay. The overlay is the normal product
surface; no additional terminal commands are required for listening, attaching
context, asking, or changing settings.

Sign-in enables managed answers and account balance. It does not turn cloud
session sync on. Cloud sync is off by default on new installs and can be enabled
separately in desktop Settings.

Listening and screen analysis have visible controls. Supported desktop paths
request capture exclusion for the overlay, but users should test their meeting
app before sharing because capture exclusion is best effort rather than a
security boundary.

## Update

Bluey checks the signed release manifest during normal startup. To install the
latest listed build manually, run:

```bash
bluey update
```

## Support

If installation or first run fails:

```bash
bluey support
```

That command creates a redacted support bundle for `hello@bluey.sh`. For a
local setup report without creating a bundle, `bluey doctor` remains available
as a diagnostic command even though it is not shown in normal customer help.

## Uninstall

Remove Bluey from the current device with:

```bash
bluey uninstall
```

The default uninstall preserves local account tokens, settings, sessions, and
logs. To remove those local data files too, use the explicit destructive option:

```bash
bluey uninstall --purge-data
```

Cloud account deletion is separate and is available from the account surface.
Review unused balance and synced data before deleting an account.
