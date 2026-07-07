# Bluey 0.1.92

## Summary

- Stops Listen immediately when Bluey signs out, loses account verification, or the balance/account watcher marks the desktop as signed out.
- Cancels any racing audio startup so a delayed start cannot flip the overlay back to Listening after auth is gone.
- Routes manual logout, live-audio auth failure, balance refresh sign-out, and balance-watch sign-out through the same fail-closed path.

## Verification

- `cargo test --manifest-path crates/cue-daemon/Cargo.toml signed_out_state_stops_active_audio_capture --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml listen_auth_gate --quiet`
- `cargo check -p cue-daemon -p cue-cli`
- `git diff --check`
