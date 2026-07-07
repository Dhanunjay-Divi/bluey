# ROUND-416-SIGNED-OUT-LISTEN-FAIL-CLOSED

Date: 2026-07-07
Repo: /Users/uno/Downloads/cue-answerplan-fix
Branch: main
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Problem

The overlay could show `Sign in` while an already-running Listen session still displayed `Listening` and kept live captions active. The start path already blocked new Listen attempts when signed out, but auth loss after Listen had started only updated account/balance UI.

## Root Cause

Bluey had several signed-out paths:

- manual `bluey logout`
- live-audio account verification failure
- balance refresh returning signed out
- balance watcher clearing its account snapshot

Those paths cleared account/balance state, but they did not all stop the daemon-owned audio runtime. So the UI could become signed out while the existing audio session kept running until a separate stop condition happened.

## Changes

- Added one shared signed-out helper for daemon account loss.
- The helper clears Listen verification, stops balance polling, stops active audio capture, cancels racing audio startup generations, sets overlay Listen state to paused, and renders `Sign in`.
- Routed `CloudLogout`, `mark_cloud_account_signed_out`, and balance-watch account clearing through that helper.
- Avoided a balance-watch self-notify loop by letting the watcher apply signed-out state without clearing the watch channel again.
- Added a focused daemon test proving an active audio session is stopped when signed-out state is applied.

## Verification

- `cargo test --manifest-path crates/cue-daemon/Cargo.toml signed_out_state_stops_active_audio_capture --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml listen_auth_gate --quiet`

## Deploy Plan

- Prepare desktop release `0.1.92`.
- Publish macOS and Windows artifacts to `https://bluey.sh`.
- Verify live `latest.json`, signatures, installer MIME types, and artifact SHA/version.
