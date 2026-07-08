# ROUND-430 Session Audit Bundle Review Handoff

Date: 2026-07-08
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Give the runtime/backend agent a clear review target for the next step after `ROUND-429-SESSION-UPLOAD-END-TO-END`: preserve every meaningful local Bluey session event in a reviewable per-session audit bundle, then sync the allowed records to Session History.

This round does not change runtime code. It documents the required end-to-end shape so another agent can review and implement without guessing from UI symptoms.

## Current State To Review

`ROUND-429` appears to fix the immediate web symptom:

- empty local meeting shells should not upload as visible Session History rows
- overlay answers should persist as stable `turn-<id>` local `cue_responses`
- response rows should sync idempotently
- server Session History should hide empty cloud shells
- context artifacts and transcript paths should stay real records instead of UI-only placeholders

That is the right layer for web Session History. The missing product layer is a complete local audit bundle that makes later quality review, STT comparison, routing review, and training-set curation easy.

## Required Audit Bundle

Create one local folder per Bluey session under the configured Bluey data directory:

```text
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
  audio/
```

Each JSONL record should include:

- `schema_version`
- `event_id`
- `session_id`
- `account_id`
- `device_id`
- `sequence`
- `kind`
- `created_at_ms`
- `source`

The writer should be append-only and idempotent. Recovered or replayed events must update or skip by stable IDs instead of duplicating rows.

## Capture Requirements

Capture these local events before cloud sync is attempted:

- user questions/prompts
- Bluey responses, including streamed completions once finalized
- provider/model/routing metadata
- input/output token counts where available
- cost estimate or charged amount where available
- transcript segments and speaker/timing metadata
- attached files, extracted text, MIME/type hints, and local object references
- screen/context snapshots or extracted screen summaries
- canvas/artifact output, including artifact type/body/confidence
- cloud sync state and upload errors

## Audio/STT Quality Scope

For STT quality review, the audit bundle must preserve audio evidence, not only the final transcript.

Implementation target for this round:

- persist raw or chunked mic/system audio locally under `session-audit/<session_id>/audio/`
- give every chunk a stable `audio_id`, codec/container, channel/source label, duration, and start/end timestamp
- add `source_audio_id`, `start_ms`, `end_ms`, STT provider/model, language, and confidence/error fields to transcript records when available
- upload internal alpha audio chunks through the same training/QA retention path used for other session data
- keep upload state visible in the audit metadata so reviewers know whether an audio chunk stayed local, uploaded, failed, or was skipped

Terms and Privacy must clearly state that internal alpha session data can include voice/audio recordings or chunks used for transcription quality review, product improvement, training/tuning, routing, safeguards, and debugging, with the same 90-day retention window as synced session content.

## Sync Requirements

The cloud sync path should read from durable local records, not transient overlay UI state.

Syncable records:

- transcript text/timing
- questions and answers
- artifacts/canvas bodies
- context artifact metadata and extracted text
- file attachment metadata and extracted text
- provider/model/cost metadata

Do not rely on the browser Session History page to infer or fabricate missing content. If the local audit bundle has no question, response, transcript, or context, the web UI should continue to show no saved chat.

## Product Copy Invariants

Session History should say what exists:

- If content exists: show the uploaded chat/session.
- If only a shell exists: hide it from normal users.
- If upload is still pending: show a waiting state tied to a real sync status.
- If no chat was saved: say no saved chat yet, not that the web UI failed.

Avoid internal implementation words in the UI such as backend, webhook, provider confirmation, or cloud shell.

## Tests The Runtime Agent Should Add

- empty local meetings never produce visible Session History rows
- response-only sessions sync and render
- streaming answer recovery writes exactly one final response
- duplicate sync does not duplicate responses, transcript, or artifacts
- transcript segments persist before and after STT finalization
- context/screen artifacts preserve extracted text and object references
- attached-file metadata survives sync
- artifact/canvas body survives sync
- account switching scopes audit bundles and cloud uploads correctly
- device removal stops future sync from that device
- local export can reconstruct one full session from audit files

## Review Questions

- Does `ROUND-429` persist every overlay-visible answer path, including retries, cancellations, and recovered streams?
- Are transcript/audio events persisted before they can be lost on app exit?
- Are screen and file context records saved with enough metadata to audit answer quality later?
- Is any UI still showing empty or fabricated session content?
- Are sync IDs stable enough to make repeated uploads harmless?

## Status

Ready for another agent to review against the runtime implementation. No deploy or runtime edits were made in this round. Companion Terms/Privacy wording is tracked in `ROUND-432-INTERNAL-ALPHA-AUDIO-TERMS.md`.
