# Bluey Release v0.1.96

Date: 2026-07-09
Source branch: `codex/bluey-web-ui-parallel-20260704`
Source commit: `738b237d1654a6e979c2064d2aa0b9c8910bfcfa`

## Summary

`0.1.96` ships the manual deploy batch for process identity/helper names, download/support page polish, verification email polish, Try Us monthly device windows, web billing/card setup fixes, and the current backend STT formatting cleanup.

## Desktop Release

Published:

```text
https://bluey.sh/releases/v0.1.96/bluey-0.1.96-darwin-arm64.tar.gz
```

SHA256:

```text
b0e0c64bfb7dc416d2957a7e3ad25b36c56a069dac37850400746480b1f75f0f
```

The macOS artifact includes both compatibility and generic helper names:

- `bluey`
- `bluey-daemon`
- `Terminal`
- `bluey-overlay-macos`
- `host-overlay`
- `bluey-audio-macos`
- `audio-driver`

## Web And Installers

- Static site synced to `https://bluey.sh`.
- `latest.json` signed and verified.
- `install.sh` served as `application/x-shellscript`.
- `install.ps1` served as `application/x-powershell`.

## API Deploy

Production API was rebuilt on the droplet from:

```text
/opt/bluey-build-codex-round457-0.1.96
```

Installed binary:

```text
/usr/local/bin/bluey-server
```

Binary SHA256:

```text
a34d73e8a9fcf05515a543609116d949025b75c88b28727a5bb4e476ddde180a
```

Previous binary backup:

```text
/var/backups/bluey-api/bin/bluey-server.previous-20260709T105852Z
```

Pre-deploy Postgres backup:

```text
/var/backups/bluey-api/hourly/bluey-postgres-20260709T104252Z.pgdump
```

## Verification

- `scripts/bluey-release-live-verify.sh 0.1.96` passed.
- `https://bluey.sh/health` returned commit `738b237d1654a6e979c2064d2aa0b9c8910bfcfa`.
- `bluey-api.service` is active with `NRestarts=0`.
- Recent warning/error journal scan returned no entries.

## Notes

- No GitHub Actions were used.
- `main` was not pushed in this round to avoid triggering GitHub Actions; production was deployed from the exact commit above through the manual droplet path.
- The Windows installer script is published, but this manual macOS/droplet release path did not build a Windows binary artifact.
