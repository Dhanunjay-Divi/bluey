# ROUND-442 Signed Release Merge

Date: 2026-07-08
Branch: codex/bluey-web-ui-parallel-20260704
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Merge and release the accumulated Bluey web UI, backend, native overlay, STT, session sync, audit, and billing/account fixes as a signed production build without leaving local work dirty.

## Included Work

- Web UI branch polish for landing/account/download surfaces, billing card layout, shortcuts copy, connect-code flow, session history, and balance rendering.
- Backend account/session fixes for uploaded sessions, account-scoped session ownership, deletion/tombstone behavior, session detail rendering, and empty-shell filtering.
- Native overlay/runtime fixes for sign-in state clearing, account switch safety, click-through/controls behavior, answer typography, code canvas persistence, STT lifecycle, VAD/autosend finalization, and lower-latency captions.
- Provider/routing fixes for short capacity waits, 429/cooldown behavior, request refs, and answer-plan quality regressions.
- Session audit/logging work so UI-visible questions, answers, streaming states, errors, transcript/context/artifact records, and costs can be reviewed later through stable session IDs and R2-backed audit bundles.
- Deepgram streaming defaults updated toward live-caption behavior with Indian English as the primary English accent and configurable readability fallbacks.

## Release Notes

- Previous live release observed before this round: 0.1.93.
- Workspace release version bumped to 0.1.94 so the signed manifest can advance the public installer/update channel.
- Signed deploy requested explicitly by owner in this round. GitHub Actions should not be used unless a future owner request asks for the GitHub release workflow.

## Verification Checklist

- Cargo/native/server/web checks are run before merge.
- Round docs are included with unique round numbers.
- Release notes are added for `v0.1.94`.
- Main is updated from this branch after checks pass.
- Signed macOS release artifact and web assets are published to bluey.sh.
- Backend server is rebuilt from the merged commit and restarted.
- Live manifest, installer MIME, and API health are verified after deployment.

## Completed Local Verification

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

## Operational Notes

- Do not treat R2 as the source of truth for accounts, balances, or live session metadata. Postgres remains the production source of truth.
- R2 stores release artifacts, backups, original uploaded objects when object storage is enabled, and diagnostic/session audit bundles.
- Redis/Valkey is a shared runtime coordination layer for rate limits/capacity/cooldowns when configured; it is not durable product state.
- Local RAG and local cached sessions stay account-scoped. Moving local sessions to a new account must remain explicit and consent-based.
