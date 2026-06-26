# Round 178 - Backend Frontend UI Robustness Audit

## Trigger

Owner asked to check the backend again and also frontend/UI end to end, making sure Bluey is robust and not missing anything obvious.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-25 15:51 EDT

## Fix

- Ran a broad backend, frontend, native UI, release, and smoke-test sweep.
- Fixed one stale active-code naming issue in the macOS overlay:
  - `pinkyTrustedRemoteInputEventSourceUserData` -> `blueyTrustedRemoteInputEventSourceUserData`
  - `trustedPinkyEvent` -> `trustedRemoteBridgeEvent`
  - `"pinky-trusted-event"` -> `"bluey-trusted-event"`
- Kept the underlying numeric remote-input marker unchanged because it is an interoperability marker for trusted remote input.

## Backend Verification

Passed:

- `cargo fmt --check --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --all-targets`
- `cd server && cargo fmt --check`
- `cd server && cargo clippy --all-targets -- -D warnings`
- `cd server && cargo test --all-targets`
- `scripts/check-server-sqlite-boundary.sh`

Coverage confirmed by tests:

- daemon, cloud client, managed streaming parser, router policy, RAG, STT, overlay IPC/security, and local smoke paths
- server auth, billing, Square reloads, Auto Reload, router complete, streaming SSE, truncated-stream non-billing, idempotency replay, STT fallback, sync/RAG, rate limiting, and pricing

## Frontend And UI Verification

Passed:

- `node --check web/assets/bluey-site.js`
- `cd crates/cue-dashboard/ui && npm test -- --run`
- `cd crates/cue-dashboard/ui && npm run build`
- local static server checks for:
  - landing page credit/reload copy
  - `bluey-site.js`
  - `bluey-site.css`
- `native/macos/cue-overlay/build.sh`
- `native/macos/cue-audio/build.sh`
- `native/macos/cue-picker/build.sh`
- `native/macos/cue-whisper/build.sh`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `scripts/smoke-test.sh`

Smoke coverage confirmed:

- daemon startup/shutdown
- overlay wiring
- transcript capture path
- instructions
- file context
- ask/action-items/recap
- memory search
- real-audio gating
- AI routing scaffold
- cloud scaffold
- archive/end flow

## Release And Environment Gates

Passed:

- shell script syntax checks for install, preflight, scalable-readiness, release hygiene, SQLite boundary, Square branding, and macOS native build scripts
- `scripts/release-hygiene-scan.sh`
  - passed with expected dev-only warnings in smoke scripts and docs
- `scripts/bluey-scalable-readiness.sh`
  - result: alpha-ready with warnings

Environment-gated:

- `scripts/bluey-cloud-preflight.sh` failed in this local shell because production settings and secrets are not loaded:
  - missing `BLUEY_PUBLIC_URL`
  - missing `BLUEY_JWT_SECRET`
  - missing billing provider
  - missing provider API key pools
  - missing `OFFSITE_DESTINATION`
  - SMTP, Redis, object storage, and Turnstile warnings

Not run:

- `scripts/macos-overlay-visual-smoke.sh` because it stops the current Bluey instance and launches a capture-visible dev overlay. That is useful before release, but intrusive during an active user session.
- Windows PowerShell parse/build checks because neither `pwsh` nor `powershell` is installed in this local environment.

## Risk Scan

- `TODO`/`FIXME`/`HACK` scan over active backend/frontend/native UI code found no actionable markers.
- Active-code stale Pinky wording scan found and fixed the macOS overlay remote-input naming issue.
- Remaining `localhost`/`127.0.0.1` matches were in tests or local fallback defaults.
- Round doc numbering remains consistent:
  - `177` numbered docs before this round
  - no missing numbers between `ROUND-001` and `ROUND-177`
  - no title/filename mismatches

## Files Touched

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-178-BACKEND-FRONTEND-UI-ROBUSTNESS-AUDIT.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Mac Windows Parity

- This round changed a macOS overlay internal variable/reason name only.
- `native/windows/cue-overlay` and active backend/frontend code were scanned for the same stale Pinky remote-input wording and no Windows equivalent was found.
- Windows PowerShell parsing could not be run locally because PowerShell is not installed.

## Current State

- No backend, server, static web, dashboard UI, macOS native build, release hygiene, scalable readiness, or local smoke-test code failures remain from this audit.
- Production cloud preflight still requires real deployment environment values before it can pass.
- Next canonical Bluey round doc should start at `ROUND-179-...`.
