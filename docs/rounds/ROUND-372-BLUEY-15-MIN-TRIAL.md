# Round 372 - Bluey 15-Minute Trial Consistency

## Goal

Make Bluey's free trial consistently grant 15 minutes across the public Try Us path, normal account signup, fresh database schemas, and trial-aware billing responses.

## Changes

- Added a shared `DEFAULT_TRIAL_SECONDS = 15 * 60` server constant.
- Updated normal account creation to explicitly insert the 15-minute trial budget instead of relying on database defaults.
- Updated the temporary Try Us account path to use the same shared default.
- Updated fresh SQLite/Postgres schema defaults and Postgres runtime default-alter statements to 900 seconds.
- Kept embedding/RAG trial metering active and added `trial_seconds_remaining` to embed responses so clients can show accurate remaining trial time.
- Updated operational docs/checklists from 10 minutes / 600 seconds to 15 minutes / 900 seconds.

## Deployment

No preprod or production deploy was performed in this round. Changes are staged in the working tree on branch `codex/bluey-15min-trial-20260705` for review/testing first.

## Verification

- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml --lib --quiet` passed: 243 tests.
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e trial_start_creates_temporary_account_with_fifteen_minutes --quiet` passed.
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e router_embed_consumes_trial_seconds_and_records_bluey_cost --quiet` passed.
- `cargo test -p cue-cloud-client --lib --quiet` passed: 24 tests.
- Active-file stale trial scan for old 600-second / 10-minute wording passed with no matches.

Full integration note:

- `cargo test --manifest-path server/Cargo.toml --test integration_e2e --quiet` was not fully green: 43 passed, 5 failed.
- Remaining failures were router completion/streaming behavior tests, not the trial signup, Try Us, or embed metering paths:
  - `router_complete_reports_upstream_error_after_capacity_skip`
  - `router_complete_stream_openai_error_frame_is_retryable`
  - `router_complete_stream_openai_truncated_after_delta_is_not_billed_or_released`
  - `router_complete_stream_anthropic_truncated_after_delta_is_not_billed_or_released`
  - `router_complete_stream_proxies_anthropic_messages_sse`

## Notes

Historical round/review docs that describe old 600-second behavior were intentionally left as history.
