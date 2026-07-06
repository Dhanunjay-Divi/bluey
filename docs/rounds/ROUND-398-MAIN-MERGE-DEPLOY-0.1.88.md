# ROUND-398-MAIN-MERGE-DEPLOY-0.1.88

Date: 2026-07-05
Branch: `main`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Merge the parallel Bluey web UI work and the latest backend/overlay fixes into `main`, cut a new signed release, and deploy it to `bluey.sh` for latest testing.

## Scope

- Merge `codex/bluey-fast-answer-latency-20260705` into `main`.
- Confirm `codex/bluey-web-ui-parallel-20260704` is already included in `main`.
- Bump the desktop release version to `0.1.88` so update/download flows can distinguish this build from `0.1.87`.
- Publish the static web UI and signed release manifest/artifact.
- Fix the production preflight helper so diagnostic log retention checks run instead of failing on a missing `is_uint` helper.

## Included Fixes

- Web UI account/dashboard improvements from the parallel branch.
- Fast-answer routing diagnostics for tracing slow answers.
- Overlay history session-id search.
- Desktop device revocation checks for local logout after web-side device/account removal.
- 10-second live balance refresh across overlay, native dashboard, and web account page.

## Verification Plan

- Rust targeted tests for daemon/dashboard/cloud-client.
- Rust check for daemon and dashboard crates.
- Dashboard web UI build.
- Public web JS syntax check.
- `git diff --check`.
- Release packaging with updater public key.
- Signed live manifest verification after deploy.
- Installer and public URL smoke checks after deploy.
- Production preflight should pass all code checks; any remaining config gap must be recorded explicitly.

## Notes

Recent local release artifacts are macOS arm64. Windows packaging remains a separate Windows build-host step unless a fresh Windows artifact is provided.

## Results

- Confirmed `main` already contained `codex/bluey-web-ui-parallel-20260704`.
- Merged `codex/bluey-fast-answer-latency-20260705` into `main`.
- Pushed `main` to origin.
- Built and published desktop release `0.1.88`.
- Published static web UI updates to `https://bluey.sh`.
- Published signed `latest.json` for `0.1.88`.
- Published macOS arm64 artifact:
  - `releases/v0.1.88/bluey-0.1.88-darwin-arm64.tar.gz`
  - SHA-256: `8c6b0a677d32d20135dc839e9b6386cc2bd9e181aabcd29ef384be6a4150cbd0`
- Installed the backend on the droplet with a native Linux x86_64 build from commit `7bd722c72f63`.
- Production health after backend deploy:
  - `ACTIVE=active`
  - `/health` returned `{"status":"ok","version":"0.1.5","platform":"linux-x86_64"}`

## Deploy Incident

The first backend swap accidentally used a macOS arm64 `server/target/release/bluey-server` binary on the Linux droplet. Systemd rejected it with `Exec format error`.

Immediate recovery:
- Rolled back to `/var/backups/bluey-api/bin/bluey-server.previous`.
- Confirmed `bluey-api.service` returned to `active`.
- Confirmed `/health` returned `ok`.

Final fix:
- Uploaded a clean git archive of `main`.
- Built `bluey-server` on the droplet with Rust `1.96.1` so the artifact is native `linux-x86_64`.
- Swapped the native binary into `/usr/local/bin/bluey-server`.
- Restarted and verified the service.

## Production Preflight

The production preflight helper now runs the retention checks correctly after adding the missing `is_uint` helper.

Remaining configuration gap:
- Turnstile keys are still missing for production signup abuse protection.

Warnings:
- Square checkout branding check was not available from the remote temp preflight context.
- `BLUEY_LOG_STORAGE` is not currently `r2`/`s3`, so server-side diagnostic chunks may not have durable R2 log storage enabled yet.

The beta deploy proceeded at owner request, but Turnstile and durable diagnostic log storage remain production-hardening follow-ups before broad public signup.

## Live Verification

- `scripts/bluey-release-live-verify.sh 0.1.88` passed.
- `latest.json` signature verified.
- Live version is `0.1.88`.
- `install.sh` content type is `application/x-shellscript`.
- `install.ps1` content type is `application/x-powershell`.
- macOS artifact SHA verified.
- Unpacked `bluey` and `bluey-daemon` binaries report `0.1.88`.
- Temporary-home installer smoke passed:
  - `bluey 0.1.88`
  - `bluey-daemon 0.1.88`

## Backup

Before backend deploy, production DB backup completed:

- `/var/backups/bluey-api/hourly/bluey-postgres-20260706T013126Z.pgdump`
- Size: `13,535,399` bytes
- Backend: `postgres`
