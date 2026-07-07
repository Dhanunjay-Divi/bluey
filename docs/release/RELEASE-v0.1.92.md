# Bluey 0.1.92

## Summary

- Stops Listen immediately when Bluey signs out, loses account verification, or the balance/account watcher marks the desktop as signed out.
- Cancels any racing audio startup so a delayed start cannot flip the overlay back to Listening after auth is gone.
- Routes manual logout, live-audio auth failure, balance refresh sign-out, and balance-watch sign-out through the same fail-closed path.

## Verification

- `cargo test --manifest-path crates/cue-daemon/Cargo.toml signed_out_state_stops_active_audio_capture --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml listen_auth_gate --quiet`
- `cargo check -p cue-daemon -p cue-cli`
- `cargo check --manifest-path server/Cargo.toml --bin bluey-server`
- `git diff --check`

## Publish

- Published signed release files to `root@165.227.77.152:/var/www/bluey`.
- Live manifest: `https://bluey.sh/latest.json`
- macOS arm64: `bluey-0.1.92-darwin-arm64.tar.gz`
  - SHA256 `657932d5191b038f5a7f95b47023e025bb548c791049321bbaae7424c51e7682`
- Windows x86_64: `bluey-0.1.92-windows-x86_64.zip`
  - SHA256 `98561dec5b6eb1dc757a75876e346ac8230be2d3e48c4b3adf76df3d988c1185`

## Live Verification

- `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.92`
- `BLUEY_RELEASE_VERIFY_PLATFORM=windows-x86_64 BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.92`
