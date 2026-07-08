# ROUND-434 Session Audit R2 Retention

Date: 2026-07-08
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Make Bluey preserve enough per-session evidence for later quality review, support lookup, routing review, STT review, and training-set curation without filling the user's laptop or the production droplet.

## Changes

- Added an authenticated server upload path for per-session audit bundles:
  - `POST /sync/session-audit/:session_id/:bundle_id`
  - validates the session UUID and path-safe bundle ID
  - stores the bundle in configured log storage, falling back to object storage
  - indexes only metadata in `diagnostic_log_chunks`
- Added desktop session audit bundle generation under the Bluey data directory:

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
  audio/audio.jsonl
  bundle.json
```

- Added idempotent upload markers under `session-audit-uploaded/<session_id>.json`.
- Uploaded bundle directories are removed locally after successful upload.
- Failed upload directories remain locally only within bounded retention:
  - default `BLUEY_AUDIT_LOCAL_RETENTION_DAYS=7`
  - default `BLUEY_AUDIT_LOCAL_MAX_BYTES=536870912`
- The bundle records include stable session/account/device/event IDs where available, source, schema version, sequence, timestamps, question/response text, transcript text, context metadata, screen context, artifact bodies, and usage/cost metadata.
- Public Terms/Privacy copy no longer uses launch-stage wording. It describes synced-session/audio use directly.

## R2 Shape

The server writes bundles using account-scoped keys:

```text
<prefix>/accounts/<account_id>/date-YYYY-MM-DD/sessions/<session_id>/audit/<bundle_id>.json
```

This keeps lookup partitioned by account, date, and session. The DB stores object key, byte count, hash, retention expiry, session ID, and session code, not the full bundle body.

## App Logs And Disk Guard

Bluey already has a separate API log archive/disk guard path documented in `ROUND-373-R2-LOG-DISK-GUARD.md` and `DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md`. This round does not replace that archive. It adds the missing session-level quality/audit bundle and stores it through the same `log_storage`/R2 configuration model so support can find app logs and session audit data from the account/session indexes without keeping large bodies in Postgres.

Local desktop context remains local when it is needed for the active app/RAG workflow. The generated audit-copy directory is removed after successful upload. Failed upload copies are pruned by age and size so the laptop does not fill up.

## Current Audio Boundary

The audit folder now reserves `audio/audio.jsonl` and records the session-level audio manifest. Raw mic/system audio chunks still need the live STT capture path to write/upload chunk objects with `audio_id`, source, duration, timestamps, language, and STT correlation metadata. That should be the next audio-specific implementation, because the current cloud sync path can only package durable local records it can see.

## Verification

- Added unit coverage for server audit bundle ID/key safety.
- Added unit coverage for desktop audit bundle folder shape and upload marker idempotency.
- Rustfmt/checks were run in this round; any unrelated pre-existing failures should be handled in their owning round.
