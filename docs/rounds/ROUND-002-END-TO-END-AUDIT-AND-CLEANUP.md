# Round 002 - End To End Audit And Cleanup

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-25 15:06 EDT

## Goal

Run a broad local health scan of Bluey, fix obvious mechanical failures, and record what is still missing or worth improving.

## Starting State

- The local Bluey daemon was not running at the start of this audit. CLI status calls failed with connection refused.
- Restarted local visible mode with `./scripts/bluey-visible-local.sh`.
- Current daemon after restart:
  - pid `93283`
  - meeting id `2ffa3c6c-df9e-4d12-a8c3-9990ab946c88`
  - overlay visible `true`
  - overlay capture excluded `false`, local visible test mode only
  - screen capture active `false`
  - transcript segments `0`
  - context items `0`
- `bluey audio status`, `bluey cloud status`, and `bluey ai status` respond successfully after restart.

## Cleanups Made

- Fixed root daemon clippy warnings in `crates/cue-daemon/src/app.rs`.
  - Collapsed a redundant notes match guard.
  - Replaced a boolean `map_or` shape with `is_none_or`.
- Fixed server clippy and formatting warnings.
  - Removed needless borrows in router/object-storage paths.
  - Removed useless conversion closures in STT accounting connection handling.
  - Changed a Postgres vector helper to take `&[f32]` instead of `&Vec<f32>`.
  - Used `limit.clamp(1, 250)` in trial-abuse query limiting.
  - Moved a test module to the bottom of `server/src/db/mod.rs`.
- Fixed release hygiene false positive from `scripts/bluey-square-branding.sh`.
  - The script no longer hardcodes an app-id-shaped expected Square application id.
  - It now requires `BLUEY_SQUARE_APPLICATION_ID_EXPECTED` before applying branding.
- Added root local dev database artifacts to `.gitignore`.
  - `bluey-dev.db`
  - `bluey-dev.db-journal`
  - `bluey-dev.db-shm`
  - `bluey-dev.db-wal`

## Verification

Passed:

- `cargo fmt --check --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --all-targets`
- `cargo test -p cue-daemon --lib`
  - `254 passed`
  - `2 ignored`
- `cd server && cargo fmt --check`
- `cd server && cargo clippy --all-targets -- -D warnings`
- `cd server && cargo test --all-targets`
  - server lib: `157 passed`
  - connectinfo: `1 passed`
  - GDPR: `2 passed`
  - integration E2E: `41 passed`
- `native/macos/cue-overlay/build.sh`
- `native/macos/cue-audio/build.sh`
- `native/macos/cue-picker/build.sh`
- `native/macos/cue-whisper/build.sh`
- `cd crates/cue-dashboard/ui && npm test -- --run`
  - `15 passed`
- `cd crates/cue-dashboard/ui && npm run build`
- `bash -n scripts/bluey-square-branding.sh`
- `scripts/release-hygiene-scan.sh`
  - passed with expected local/dev-mode warnings in smoke scripts and docs
- `scripts/bluey-scalable-readiness.sh`
  - reported alpha-ready with environment warnings

Did not pass because local production environment variables are intentionally incomplete:

- `scripts/bluey-cloud-preflight.sh`
  - missing current-shell production settings such as public URL, JWT secret, billing provider, provider keys, and offsite backup destination
  - warnings for Turnstile, Redis strict mode, object storage, and SMTP

## Findings

1. Daemon availability needs tighter supervision evidence.
   - The daemon was down at audit start even though earlier rounds had restarted it.
   - Add a lightweight local watchdog/log summary or launch-supervisor check so the next failure shows why it exited.

2. Default test coverage still skips the riskiest live Listen paths.
   - Ignored tests include STT factory integration, whisper stub E2E, native capture, and keychain-backed secret behavior.
   - Keep them optional for CI if needed, but add a stable live-smoke tier that can be run before release.

3. Production readiness depends on environment configuration, not only code.
   - Scalable readiness is alpha-ready with warnings.
   - Cloud preflight is red without prod env values.
   - This is expected locally, but release should not proceed until a real deployment env passes it.

4. Release hygiene now passes, but its warning list should stay visible.
   - Dev capture-visible flags appear only in local smoke scripts and docs.
   - That is acceptable for local testing, but the warning is a useful release reminder.

5. Manual GUI QA is still needed for the recent user-facing issues.
   - Pill drag should be physically tested.
   - Silent Listen should not send an empty answer.
   - Duplicate transcript sends should stay suppressed.
   - Unrelated questions should close stale coding canvas.
   - Multiple screen attachments should show separately in sent question cards.

6. The worktree is intentionally very dirty.
   - Many modified and untracked files are from previous Bluey rounds.
   - Do not reset or broad-revert this workspace.

## Current App State

- Bluey is running locally in visible test mode.
- Audio is idle and ready, with native macOS helper installed.
- Cloud status is token-configured for `https://bluey.sh`.
- AI status reports managed Bluey healthy, with vision and STT available.
- Active meeting is empty after restart, so any manual QA that depends on images/docs should attach or reopen context first.

## Recommended Next Improvements

1. Add daemon crash/exit diagnostics and a one-command local health summary.
2. Turn the ignored STT/whisper/native-capture coverage into an explicit live-smoke command.
3. Run a real manual overlay QA pass against the user screenshots: pill drag, silent Listen, duplicate sends, stale canvas, and multi-screen attachment cards.
4. Run `scripts/bluey-cloud-preflight.sh` in the actual production env and resolve remaining config gaps.
5. Keep release hygiene in CI now that the Square false positive is removed.

## Files Touched In This Audit

- `.gitignore`
- `crates/cue-daemon/src/app.rs`
- `server/src/api/router.rs`
- `server/src/db/mod.rs`
- `server/src/db/stt_accounting.rs`
- `server/src/db/sync.rs`
- `server/src/db/trial_abuse.rs`
- `server/src/object_storage.rs`
- `scripts/bluey-square-branding.sh`
- `docs/rounds/ROUND-002-END-TO-END-AUDIT-AND-CLEANUP.md`
