# ROUND-457-MANUAL-DEPLOY-PROCESS-IDENTITY-WEB-BACKEND

Date: 2026-07-09
Branch: `codex/bluey-web-ui-parallel-20260704`
Release version: `0.1.96`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Deploy the current Bluey web, backend, desktop packaging, and installer changes without using GitHub Actions.

This round includes:

- process identity/helper naming parity with Pinky-style installs
- release version bump from `0.1.95` to `0.1.96` so auto-update can install the new artifact
- download page command/support-surface polish
- verification email polish
- Try Us monthly device trial window changes
- account/card setup retry fixes already present in the web branch
- Deepgram/STT formatting cleanup currently staged in the backend
- signed macOS release artifact and production API rebuild

## Important Product Invariants

- Manual deploy path only; do not use GitHub Actions for this round.
- Keep a stable traceable commit before publishing.
- Keep old Bluey binary/helper names supported during transition.
- Add generic process names only inside the Bluey install root, with path-filtered cleanup.
- Do not store provider secrets or keys in docs.

## Local Verification

Planned before commit/deploy:

```bash
bash -n ops/install/install.sh scripts/install.sh scripts/build-macos.sh scripts/build-macos-universal.sh scripts/bluey-release-live-verify.sh scripts/bluey-visible-local.sh scripts/macos-overlay-visual-smoke.sh native/macos/cue-overlay/build.sh native/macos/cue-audio/build.sh scripts/deploy-bluey-sh-manual.sh scripts/publish-bluey-release.sh
node --check web/assets/bluey-site.js
git diff --check
cargo check -p cue-cli -p cue-daemon --quiet
cargo check --manifest-path server/Cargo.toml --bin bluey-server --quiet
```

Initial result:

- shell syntax checks passed
- web JavaScript syntax check passed
- `git diff --check` passed
- CLI/daemon cargo check passed
- server cargo check passed
- after the `0.1.96` version bump, `git diff --check`, CLI/daemon cargo check, and server cargo check passed again

## Deployment Plan

1. Commit the full local batch.
2. Fast-forward/merge to `main` if the branch is clean.
3. Package macOS arm64 with the release public key embedded.
4. Publish web/static files, installers, release manifest, signature, and macOS artifact to the droplet with `scripts/deploy-bluey-sh-manual.sh`.
5. Rebuild `bluey-server` on the droplet from the same commit and restart `bluey-api.service`.
6. Run live checks:
   - `https://bluey.sh/health`
   - `https://bluey.sh/latest.json`
   - installer MIME checks
   - `scripts/bluey-release-live-verify.sh`
   - `bluey-api.service` status/log warnings

## Pending Live Results

Completed.

## Source Commit

```text
738b237d1654a6e979c2064d2aa0b9c8910bfcfa
```

Pushed to:

```text
codex/bluey-web-ui-parallel-20260704
```

`main` was not pushed in this round to avoid triggering GitHub Actions. The production deploy used the exact commit above through the manual droplet path.

## Desktop/Web Deploy

Command:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64

BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem \
PUBLISH_DO=1 \
PUBLISH_HOST=root@165.227.77.152 \
PUBLISH_PATH=/var/www/bluey \
scripts/deploy-bluey-sh-manual.sh
```

Live release:

- Version: `0.1.96`
- Artifact: `https://bluey.sh/releases/v0.1.96/bluey-0.1.96-darwin-arm64.tar.gz`
- SHA256: `b0e0c64bfb7dc416d2957a7e3ad25b36c56a069dac37850400746480b1f75f0f`
- Live `latest.json` signature verified.
- `install.sh` MIME: `application/x-shellscript`
- `install.ps1` MIME: `application/x-powershell`
- Live artifact includes:
  - `bin/Terminal`
  - `bin/host-overlay`
  - `bin/audio-driver`
  - legacy `bluey-*` helper names for transition compatibility
- Unpacked macOS arm64 binaries report `0.1.96`.

Windows note:

- The Windows installer script is live.
- A Windows binary artifact was not built in this manual macOS/droplet deploy path; the Windows package still requires the Windows/MSVC builder.

## API Deploy

Pre-deploy guards:

- `bluey-api.service` was active before deploy.
- Disk guard passed; `/` was 31% used before deploy.
- Fresh Postgres backup:

```text
/var/backups/bluey-api/hourly/bluey-postgres-20260709T104252Z.pgdump
```

Build source:

```text
/opt/bluey-build-codex-round457-0.1.96
```

Runtime binary:

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

Post-deploy verification:

- `bluey-api.service` is `active`.
- `NRestarts=0`.
- Local droplet `/health` returned commit `738b237d1654a6e979c2064d2aa0b9c8910bfcfa`.
- Public `https://bluey.sh/health` returned commit `738b237d1654a6e979c2064d2aa0b9c8910bfcfa`.
- Recent `journalctl -u bluey-api.service --since "5 min ago" -p warning..alert` returned no entries.
- Disk after deploy: `/` at 34% used.

## Final Notes

- No GitHub Actions were used.
- No provider secrets were copied into docs.
- The ignored `server/Cargo.lock` is not part of the committed release source; the server build generated its lock in the isolated droplet build tree.
