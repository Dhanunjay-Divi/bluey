# Bluey v0.1.94

Date: 2026-07-08
Branch: codex/bluey-web-ui-parallel-20260704

## Summary

This release merges the web UI, backend, native overlay, STT, session sync, provider routing, billing/account safety, and audit/logging fixes accumulated across the Bluey release branch and the parallel web/backend branch.

## User-Facing Changes

- Web account and download pages now have clearer balance/reload, connect-code, shortcuts, and session-history behavior.
- Overlay sign-in state is fail-closed: signed-out users cannot continue listen/answer work, and stale sign-in cards clear when account state refreshes.
- Session history is uploaded from persisted desktop state and hides empty shells until real content exists.
- Code and system-design answer flows preserve canvas/artifact context more consistently and keep follow-ups attached to the current work.
- Live captions use lower-latency Deepgram streaming defaults with Indian English as the primary English hint while still staying configurable.

## Backend and Operations

- Provider routing has structured answer ops events, short internal provider-capacity retry sweeps, cooldown logging, and better support refs.
- Billing/account APIs guard negative reload inputs, admin/test checkout paths, account revocation, and live balance refresh behavior.
- Session audit and R2 retention docs define append-only session events, transcript/context/artifact/cost records, and R2-backed diagnostic bundles.
- Windows installer/release docs and download-page command/shortcut copy are included from the parallel branch.

## Verification

- `cargo check -p cue-cli -p cue-cloud-client -p cue-core -p cue-daemon --quiet`
- `cargo check --manifest-path server/Cargo.toml --quiet`
- `cargo test -p cue-daemon session_audit_bundle --quiet`
- `cargo test -p cue-daemon conversation_sync --quiet`
- `cargo test -p cue-daemon deepgram_ --quiet`
- `cargo test -p cue-daemon live_stt_finalize_wait_defaults_and_clamps --quiet`
- `cargo test -p cue-daemon macos_overlay_capture_visible_requires_dev_and_local_gates --quiet`
- `cargo test -p cue-cli bluey_on_boot_title_reflects_signed_out_state --quiet`
- `cargo test --manifest-path server/Cargo.toml internal_capacity_retry_delay --quiet`
- `cargo test --manifest-path server/Cargo.toml short_capacity_wait_default_and_override --quiet`
- `cargo test --manifest-path server/Cargo.toml deepgram_url --quiet`
- `cargo test --manifest-path server/Cargo.toml list_sessions_hides_empty_shells_until_content_arrives --quiet`
- `cargo test --manifest-path server/Cargo.toml tombstoned_session_does_not_resurrect_on_later_sync --quiet`
- `node --check web/assets/bluey-site.js`
- `native/macos/cue-overlay/build.sh`
- `scripts/release-hygiene-scan.sh`
- `git diff --check`
- `cargo fmt --all --check`
