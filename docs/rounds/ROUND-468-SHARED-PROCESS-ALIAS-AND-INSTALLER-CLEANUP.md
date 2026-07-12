# ROUND-468 Shared Process Alias And Installer Cleanup

Date: 2026-07-09
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Run a second local-only Bluey cleanup pass after the process identity work.
Remove safe duplication, give developers one Rust source of truth for helper
names, and keep macOS/Windows compatibility behavior unchanged.

## Findings

- Daemon, overlay, and audio aliases were repeated independently in CLI,
  daemon, and system-audio modules.
- Audio helper discovery had two implementations with slightly different
  environment variables and search paths. Permission/status checks could
  therefore find a different helper than live system capture.
- The macOS one-line installer duplicated the entire public-helper linking loop
  in sudo and non-sudo branches.
- The release-tarball installer repeated the same helper list in chmod and PATH
  linking blocks.
- Clippy found one behavior-preserving readability cleanup in the STT tail
  finalization path.

## Changes

- Added `crates/cue-core/src/process_aliases.rs` as the shared Rust contract for:
  - `termb` daemon identity and legacy daemon fallbacks;
  - `hostovb` overlay identity and legacy overlay fallbacks;
  - `adriverb` audio identity and legacy audio fallbacks;
  - macOS overlay app-bundle names;
  - normalized, case-insensitive executable-name matching.
- Updated CLI daemon resolution and macOS audio permission discovery to consume
  the shared contract.
- Updated daemon overlay discovery to consume the shared contract.
- Moved native audio helper discovery into the system-audio module and made
  status/permission checks and live capture use the same resolver.
- Unified support for `BLUEY_SYSTEM_AUDIO_BINARY`, `BLUEY_AUDIO_HELPER_BIN`, and
  the legacy `CUE_AUDIO_HELPER_BIN` override.
- Deduplicated executable directories and candidate paths before probing.
- Replaced duplicated sudo/non-sudo installer branches with one command runner
  and one public-helper list.
- Replaced repeated release-installer chmod/link lists with named arrays.
- Simplified the STT finalizing-session early return with Rust's `?` operator.
- Updated `docs/dev/PROCESS-ALIASES.md` with source-of-truth ownership.

## Preserved Behavior

- No executable was renamed in this round.
- `termb`, `hostovb`, and `adriverb` remain preferred.
- `Terminal`, `host-overlay`, `audio-driver`, and old Bluey/Cue names remain
  compatibility fallbacks.
- Daemon identity aliases remain private to Bluey's install root.
- macOS and Windows candidate order remains preferred alias first.
- No web, billing, auth, answer-routing, or overlay UI behavior was changed.

## Verification

- `cargo fmt -p cue-core -p cue-cli -p cue-daemon`
- `cargo test -p cue-core --lib --quiet`: 93 passed.
- `cargo test -p cue-cli --lib --quiet`: 65 passed.
- `cargo test -p cue-daemon audio::system_capture --lib --quiet`: 5 passed.
- `cargo test -p cue-daemon app::tests --lib --quiet`: 144 passed.
- `cargo clippy -p cue-core -p cue-cli -p cue-daemon --lib --bins --quiet`:
  passed with no warnings after cleanup.
- `bash -n ops/install/install.sh`: passed.
- `bash -n scripts/install.sh`: passed.
- Scoped `git diff --check`: passed.

The full `cargo test -p cue-daemon --lib --quiet` run reached 329 passing and 5
ignored tests. Twelve Deepgram/OpenAI WebSocket tests could not bind localhost
inside the Codex sandbox and failed with OS `Operation not permitted`; focused
non-network daemon tests passed.

## Deployment

- No deployment.
- No release build.
- No GitHub Actions.
