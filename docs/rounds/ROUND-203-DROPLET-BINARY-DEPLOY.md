# Round 203 - Droplet Binary Deploy

## Trigger

Owner asked to deploy the current Bluey work to the droplet so the latest binaries can be downloaded from `bluey.sh`.

## Scope

- Published the current macOS arm64 desktop bundle as `v0.1.15`.
- Published signed release metadata to `https://bluey.sh/latest.json` and `https://bluey.sh/latest.json.sig`.
- Deployed the current API server build to the DigitalOcean droplet.
- Kept production overlay capture-hidden. Visible/capture debug flags remained local QA-only and were not enabled in shipped binaries.

## Release Artifact

- Bumped workspace desktop artifact version from `0.1.14` to `0.1.15`.
- Added `docs/release/RELEASE-v0.1.15.md`.
- Built:
  - `dist/bluey-0.1.15-darwin-arm64.tar.gz`
- Published:
  - `https://bluey.sh/releases/v0.1.15/bluey-0.1.15-darwin-arm64.tar.gz`
  - `https://bluey.sh/releases/v0.1.15/SHA256SUMS.txt`
  - `https://bluey.sh/releases/v0.1.15/RELEASE.md`

Live artifact SHA256:

```text
71ab873543a173c95020ff21f1fd9a9b10a0c8b8b9c6efbca45f79d8522573c1
```

## Server Deploy

- Synced the current source snapshot to `/opt/bluey-build-codex-0.1.15` on the droplet.
- Built `bluey-server` on the droplet with the current Rust toolchain.
- Replaced `/usr/local/bin/bluey-server` with a rollback backup:

```text
/usr/local/bin/bluey-server.bak-20260626T205440Z
```

- Restarted `bluey-api.service`.
- Live health now reports:

```json
{"status":"ok","version":"0.1.5","commit":"3d6bc5f-v0.1.15","platform":"linux-x86_64"}
```

## Verification

Passed:

- `BLUEY_UPDATE_PUBKEY=... make package-darwin-arm64`
- release archive scan:
  - no `._*` AppleDouble files
  - no capture-visible/dev overlay flag strings in shipped binaries
- `BLUEY_RELEASE_SIGNING_KEY_FILE=... PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 scripts/deploy-bluey-sh-manual.sh`
- live `latest.json` reports `0.1.15`
- live `latest.json.sig` verifies against the release public key
- live artifact SHA256 matches `latest.json`
- temp-home live installer smoke:
  - `curl -fsSL https://bluey.sh/install.sh | HOME=<tmp> BLUEY_INSTALL_NO_SUDO=1 BLUEY_SKIP_LOCAL_TOOLS=1 bash`
  - installed CLI reported `bluey 0.1.15`
  - user-local `bluey` and `bluey-daemon` symlinks were created
- `curl -fsS https://bluey.sh/health`
- `curl -fsS https://bluey.sh/pricing/tiers`
- `curl -fsSI https://bluey.sh/releases/v0.1.15/bluey-0.1.15-darwin-arm64.tar.gz`
- `curl -fsSL https://bluey.sh/install.sh | bash -n`
- `curl -fsSL https://bluey.sh/install.ps1 | wc -c`
- `scripts/release-hygiene-scan.sh`
- Droplet Postgres shape:
  - `balance_ledger_entries` exists
  - `accounts.billing_restricted`, `accounts.billing_restriction_reason`, and `accounts.billing_restricted_at` exist
- `systemctl is-active bluey-api.service`

## Current State

- Public macOS arm64 install path is live at `v0.1.15`.
- Live API server is running the current branch snapshot with commit label `3d6bc5f-v0.1.15`.
- Windows installer remains preview-gated in `ops/install/install.ps1`, and `latest.json` does not advertise a public Windows artifact. This avoids presenting an unsupported Windows install path as production-ready.

## Remaining QA/Gates

- Run an owner-device paid alpha smoke with a real account, credits, STT, answer, screen, document, and support bundle path.
- Produce and validate a current signed Windows artifact from the MSVC release path before advertising Windows publicly.
- If desired, cut a GitHub release/tag for `v0.1.15`; this round deployed directly to the droplet/static release host.
