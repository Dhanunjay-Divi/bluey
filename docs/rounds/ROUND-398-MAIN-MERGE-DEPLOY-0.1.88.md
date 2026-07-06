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

## Notes

Recent local release artifacts are macOS arm64. Windows packaging remains a separate Windows build-host step unless a fresh Windows artifact is provided.
