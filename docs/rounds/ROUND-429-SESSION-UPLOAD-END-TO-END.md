# ROUND-429 Session Upload End To End

Date: 2026-07-08
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Fix Bluey session history upload at the desktop persistence/cloud-sync layer, not by faking content in the web UI. A saved local overlay session should later appear in the signed-in web account with real conversation/context records instead of an empty session shell.

## Findings

- Web Session History only renders what desktop sync uploads.
- Desktop cloud sync could build a batch containing only `cloud_sessions` for a local `MeetingRecord` with no transcript, conversation, context, or local response rows.
- Normal overlay answers were persisted to the local meeting JSON conversation list, but not to `sessions.db` `cue_responses`, which made local debugging look empty and weakened the durable response trail.
- The server session list showed any non-deleted `cloud_sessions` row, including old empty shells that had no transcript, answer, or context records.

## Changes

- Desktop sync now skips truly empty local meeting shells before uploading.
- Desktop sync still uploads sessions that have real syncable records:
  - transcript segments
  - context artifacts/files/screen context
  - conversation turns
  - local cue response rows
  - non-empty summary chunks
- Completed overlay answers now write an idempotent local `cue_responses` row with stable ID `turn-<conversation_turn_id>`.
- The local response DB path now ensures a matching local `sessions` row exists before inserting the response, so foreign-key enforcement does not silently prevent durable response persistence.
- Local `cue_responses` inserts are now upserts by stable response ID, so retries/sync refreshes update the same row instead of duplicating turns.
- Server-side session listing now hides old empty cloud shells unless the session has at least one transcript segment, cue response, or context artifact.

## Acceptance Coverage

- Questions/prompts and Bluey responses: persisted in meeting JSON and mirrored to stable local `cue_responses` rows.
- Canvas/artifact output: preserved on conversation turns and local cue response artifact fields.
- Attached files/screen context: context artifacts remain part of the sync batch and are not collapsed into UI-only state.
- Transcript/voice segments: transcript sync path remains unchanged and is part of the syncable-content gate.
- Idempotency: stable `turn-...` response IDs and DB upserts prevent duplicate answer rows on repeated sync.
- Empty shells: skipped by desktop sync and filtered from server list.

## Tests

Passed:

- `cargo test --package cue-daemon empty_meeting_shell_does_not_sync --quiet`
- `cargo test --package cue-daemon cue_response_only_meeting_still_syncs_session --quiet`
- `cargo test --package cue-daemon cue_response_insert_is_idempotent_for_stable_turn_id --quiet`
- `cargo test --package cue-daemon conversation_sync --quiet`
- `cargo test --package cue-daemon conversation_turn_falls_back_to_cue_response --quiet`
- `cargo test --manifest-path server/Cargo.toml list_sessions_hides_empty_shells_until_content_arrives --quiet`
- `git diff --check -- crates/cue-daemon/src/cloud/sync.rs crates/cue-daemon/src/db/mod.rs crates/cue-daemon/src/app.rs server/src/db/sync.rs`

## Deploy Status

Not deployed. Per owner instruction, this round did not use GitHub Actions and did not publish a release. It is ready for local testing or a later signed deploy when requested.

