# Round 250 - Droplet Release Deploy 0.1.17

## Trigger

Owner asked to deploy all current Bluey changes so the binaries are downloadable from the droplet curl/install path.

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## What Changed Locally

- Bumped the workspace release version from `0.1.16` to `0.1.17`.
- Added release notes at `docs/release/RELEASE-v0.1.17.md`.
- Built the macOS arm64 package with the embedded Bluey update public key:
  - `dist/bluey-0.1.17-darwin-arm64.tar.gz`
  - SHA256: `8a91bdc1bd34444fb31f76450461e5d2b287405f205a415611c82d069359451d`

## Download Deploy

Published signed release metadata and the macOS arm64 artifact to the droplet.

Live URLs:

```text
https://bluey.sh/latest.json
https://bluey.sh/latest.json.sig
https://bluey.sh/install.sh
https://bluey.sh/install.ps1
https://bluey.sh/releases/v0.1.17/bluey-0.1.17-darwin-arm64.tar.gz
https://bluey.sh/releases/v0.1.17/SHA256SUMS.txt
https://bluey.sh/releases/v0.1.17/RELEASE.md
```

Live `latest.json` now reports:

```text
version: 0.1.17
platforms: darwin-arm64
```

Important caveat: no fresh Windows artifact was built in this round because this Mac host cannot produce the validated Windows zip path. `install.ps1` is still published, but `latest.json` does not advertise a `windows-x86_64` `0.1.17` artifact until the Windows build host supplies one.

## Server Deploy

The public API service is Linux x86_64, so the local macOS server binary was not uploaded. Instead, a lean source snapshot was synced to:

```text
/opt/bluey-build-codex-0.1.17
```

The snapshot includes only:

- root cargo manifests
- `server/`
- `crates/cue-core/`
- `infra/postgres/`

It excludes `target/`, `dist/`, `.git`, and node modules.

Built on the droplet with the rustup toolchain:

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo build --release --manifest-path server/Cargo.toml --bin bluey-server
```

Installed to:

```text
/usr/local/bin/bluey-server
```

Previous binary backup:

```text
/var/backups/bluey-api/bin/bluey-server.previous-20260630T090432Z
```

Restarted service:

```text
bluey-api.service
MainPID: 886371
```

## Verification

Passed:

```bash
BLUEY_UPDATE_PUBKEY=... make package-darwin-arm64
cargo build --release --manifest-path server/Cargo.toml --bin bluey-server
bash scripts/release-hygiene-scan.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey bash scripts/deploy-bluey-sh-manual.sh
```

Live checks passed:

- `https://bluey.sh/health` returned `{"status":"ok","version":"0.1.5","platform":"linux-x86_64",...}`.
- `https://bluey.sh/latest.json` reports `0.1.17`.
- `https://bluey.sh/latest.json.sig` is present and 88 bytes.
- OpenSSL verified `latest.json.sig` against the Bluey release public key.
- Downloaded `bluey-0.1.17-darwin-arm64.tar.gz` from `bluey.sh` and verified it against live `SHA256SUMS.txt`.
- Archive shape is installer-compatible with top-level `bin/`.
- Temp-home install smoke from `https://bluey.sh/install.sh` completed and reported `bluey 0.1.17`.
- `bluey-api.service` is active after restart.
- Post-restart logs show Postgres runtime migration applied and server listening on `0.0.0.0:8080`.

Release hygiene warnings were limited to allowed dev-only flag mentions in local QA scripts/docs; the scan passed.

## Operator Notes

- The temp-home installer smoke printed a `/dev/tty` sudo warning because the installer was piped in a noninteractive shell. It correctly fell back to the user-local command path and installed successfully.
- If a Windows release is needed, run the Windows packaging path on the signed-in Windows build host, place `dist/bluey-0.1.17-windows-x86_64.zip` in this repo, rerun `scripts/publish-bluey-release.sh` with the release signing key, and re-check `latest.json`.
