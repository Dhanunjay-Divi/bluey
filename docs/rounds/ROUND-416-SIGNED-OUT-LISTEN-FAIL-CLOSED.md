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
- Cleaned CI/release Linux dependency setup so Ubuntu jobs install one appindicator family and observability jobs install the Tauri/WebKit dependencies they need before clippy/test.

## Verification

- `cargo test --manifest-path crates/cue-daemon/Cargo.toml signed_out_state_stops_active_audio_capture --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml listen_auth_gate --quiet`
- `cargo check -p cue-daemon -p cue-cli`
- `cargo check --manifest-path server/Cargo.toml --bin bluey-server`
- `git diff --check`

## Deploy

- Prepared desktop release `0.1.92`.
- GitHub Actions release artifacts completed for macOS arm64 and Windows x86_64.
- Published signed release files to `root@165.227.77.152:/var/www/bluey`.
- Live `latest.json` now reports version `0.1.92`.
- Verified live `latest.json` signature, installer MIME types, macOS artifact SHA/version, and Windows artifact SHA.
- macOS arm64 SHA256: `657932d5191b038f5a7f95b47023e025bb548c791049321bbaae7424c51e7682`
- Windows x86_64 SHA256: `98561dec5b6eb1dc757a75876e346ac8230be2d3e48c4b3adf76df3d988c1185`
