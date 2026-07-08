# ROUND-417-CI-CLIPPY-CLEANUP

Date: 2026-07-07
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Keep the Bluey 0.1.92 sign-out/listen fail-closed release clean in CI after GitHub's strict clippy pass rejected style-only lints.

## What Changed

- Cleaned the daemon answer parsing helpers to satisfy newer clippy suggestions without changing behavior.
- Cleaned STT relay logging counters to use the current integer multiple helper.
- Added an explicit `too_many_arguments` allowance to the managed stream recovery helper because it intentionally carries provider, stream, request, timing, and failure context for recovery logging.
- Cleaned duplicate internal-disclosure guard trimming logic in the LLM answer helper.
- Added the missing Ubuntu ALSA development package to CI, observability, and release Linux dependency setup so audio crates can build on hosted runners.
- Moved the Bluey CLI billing tests below production items to satisfy the Rust 1.96 `items_after_test_module` lint in the full workspace clippy pass.
- Tightened platform-specific daemon `cfg` boundaries so Linux/Windows/macOS clippy do not see impossible native-helper, overlay-socket, or paste-helper branches as unused or unreachable.
- Marked the ffmpeg runtime path as intentionally unused on Linux because chunked desktop audio capture only reads it on macOS and Windows.

## Verification

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml signed_out_state_stops_active_audio_capture --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml listen_auth_gate --quiet`
- `cargo check -p cue-daemon -p cue-cli`
- `cargo check --manifest-path server/Cargo.toml --bin bluey-server`
- `cargo clippy -p cue-daemon --all-targets -- -D warnings`
- `cargo clippy --all-targets -- -D warnings`

## Notes

This round does not require a new desktop release by itself. The behavior-bearing signed-out listening fix is already published as 0.1.92 for macOS arm64 and Windows x86_64.
