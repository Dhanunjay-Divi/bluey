# ROUND-435 Session Flight Recorder Audit

Date: 2026-07-08
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Make Bluey keep an append-only record of what the user actually saw during a session, including visible question cards, status text, streamed answer deltas, final answer cards, and user-facing errors. This is needed so a short session ID or request ref can later explain why an answer was slow, partial, not human enough, routed oddly, or failed during streaming.

## Implemented

- Added a durable local event log under `session-audit-events/<session_id>/events.jsonl`.
- Added `append_session_audit_event(...)` for best-effort, append-only desktop UI events.
- Included raw desktop UI events in generated session audit bundles before upload.
- Made audit bundle IDs include the raw event log fingerprint, so new UI-only glitches produce a new upload instead of being hidden by an old marker.
- Removed uploaded raw event logs only after the audit bundle upload succeeds.
- Extended audit pruning to cover both generated bundles and raw event logs.
- Hooked the answer UI path to record:
  - `ui_question_card`
  - `ui_answer_started`
  - `ui_answer_status`
  - `ui_answer_delta`
  - `ui_answer_replay_text`
  - `ui_answer_update_done`
  - `ui_answer_finished`
  - `ui_answer_error`
- Added a regression test proving append-only UI events land in the reviewable audit bundle.

## Current Storage Shape

Local before upload:

```text
session-audit-events/<session_id>/events.jsonl
session-audit/<session_id>/
  manifest.json
  events.jsonl
  questions.jsonl
  responses.jsonl
  transcript.jsonl
  context.jsonl
  screen.jsonl
  artifacts.jsonl
  costs.jsonl
  attachments.jsonl
  audio/audio.jsonl
  bundle.json
```

Remote upload:

```text
prod/logs/accounts/<account_id>/date-YYYY-MM-DD/sessions/<session_id>/audit/<bundle_id>.json
```

The server records the uploaded object in `diagnostic_log_chunks` as `session_audit_bundle`.

## Important Notes

- The recorder is best-effort and must not block answering.
- The local raw event log is append-only until a successful upload.
- Text fields are capped so a broken stream cannot fill disk indefinitely.
- This captures UI-visible answer events and bundle metadata. Raw mic/system audio chunk upload is still handled separately by the STT/diagnostic path and needs continued verification for full audio QA replay.
- No deploy was performed in this round.

## Verification

- `cargo check -p cue-daemon --quiet`
- `cargo test -p cue-daemon session_audit_bundle_includes_append_only_ui_events --quiet`
- `cargo test -p cue-daemon session_audit_bundle_writes_reviewable_shape_and_marker --quiet`
