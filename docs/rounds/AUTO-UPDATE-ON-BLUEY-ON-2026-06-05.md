# Auto-update on `bluey on` — 2026-06-05

## User flow

Installed customers run:

```bash
bluey on
```

Before starting the daemon/overlay, the CLI now checks:

```text
https://bluey.sh/latest.json
```

If the hosted version is newer than the installed CLI version, Bluey prints a
short update prompt:

```text
Bluey X.Y.Z is available (current A.B.C, 6.1 MB).
Press Esc within 5 seconds to skip; otherwise Bluey updates before starting.
```

If the user does nothing, Bluey downloads `https://bluey.sh/install.sh`, runs
the installer with the exact artifact URL/checksum from `latest.json`, stops any
running daemon best-effort, installs the new binaries, and relaunches
`bluey on`. If the user presses Esc, the current version starts normally.

## Dev safety

Local checkout binaries under `target/` skip update checks by default so
`./target/debug/bluey on` cannot silently replace a developer build.

Useful env flags:

| Env var | Meaning |
|---|---|
| `BLUEY_SKIP_UPDATE=1` | Never check for updates. |
| `BLUEY_UPDATE_FORCE=1` | Allow checks from `target/debug` or `target/release`. |
| `BLUEY_UPDATE_ASSUME_YES=1` | Skip the 5-second Esc countdown. Useful for smoke tests. |
| `BLUEY_UPDATE_VERBOSE=1` | Print non-fatal update-check errors. |
| `BLUEY_UPDATE_STRICT=1` | Treat update check/install failures as command failures. |
| `BLUEY_UPDATE_MANIFEST_URL=...` | Override `https://bluey.sh/latest.json`. |
| `BLUEY_UPDATE_INSTALL_URL=...` | Override `https://bluey.sh/install.sh`. |

## Manual test commands

```bash
# Dev build should skip by default.
./target/debug/bluey update --check-only

# Force the check against production manifest.
BLUEY_UPDATE_FORCE=1 ./target/debug/bluey update --check-only

# Force a non-interactive install from a custom manifest.
BLUEY_UPDATE_FORCE=1 BLUEY_UPDATE_ASSUME_YES=1 \
  BLUEY_UPDATE_MANIFEST_URL=http://127.0.0.1:9999/latest.json \
  BLUEY_UPDATE_INSTALL_URL=http://127.0.0.1:9999/install.sh \
  ./target/debug/bluey update --yes --force
```

## Hosted release files

`bluey.sh` must serve real static files, not the website fallback, at:

```text
/install.sh
/latest.json
/releases/vX.Y.Z/bluey-X.Y.Z-darwin-arm64.tar.gz
/releases/vX.Y.Z/SHA256SUMS.txt
```

`scripts/publish-bluey-release.sh` stages those files locally and can publish
them when SSH is configured:

```bash
scripts/publish-bluey-release.sh

PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 \
  scripts/publish-bluey-release.sh
```

The script reads artifacts from `dist/`, generates `latest.json`, copies
`ops/install/install.sh`, and writes a release-level `SHA256SUMS.txt`.

## Current security posture

This round keeps the existing HTTPS + SHA256 checksum model and does not add
signed release manifests. That is acceptable for internal alpha but still leaves
the release host as a high-value trust point.

Next hardening step:

- ed25519-sign `latest.json`;
- embed the Bluey release public key in both `install.sh` and the CLI updater;
- fail closed if the manifest signature is missing or invalid.

That should land before broad public distribution.
