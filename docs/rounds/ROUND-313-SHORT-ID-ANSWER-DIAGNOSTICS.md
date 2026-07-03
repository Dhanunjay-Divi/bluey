# ROUND-313 Short ID Answer Diagnostics

Date: 2026-07-02

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Goal

Make screenshot/session short ids such as `25594F6D` directly traceable in logs and keep redacted answer failure diagnostics synced instead of losing failures as transient overlay-only cards.

## Diagnosis

- Server logs had full `request_id` and `session_id`, but not a consistent short `request_ref` / `session_ref`.
- A user screenshot with only `25594F6D` required recovering the full session id from local state before production logs could be searched.
- Successful answers were uploaded as synced session responses, but failed answer cards were not persisted as conversation turns.
- Session diagnostics were already synced in `cloud_sessions.metadata_json`, but answer failures were not written into that diagnostics path.
- Raw local logs should not be uploaded by default because they can contain operational detail and nearby private context. The safer production pattern is redacted breadcrumbs: ids, hashes, counts, provider/lane/timing, and sanitized error category.

## Changes

- Added stable short-ref formatting in the server:
  - `25594f6d-4cc7-4315-b99b-017b567851ae` -> `25594F6D`
  - `74c0a385-e56a-4afd-bb90-5abb4941cebb` -> `74C0A385`
- Added `request_ref` and `session_ref` to key managed-answer log lines:
  - request accepted
  - route selected
  - usage event recorded
  - completed and billed
  - upstream route failure
  - stream read failure
  - incomplete stream without billing metadata
- Added redacted `ops_audit_events` records for:
  - `answer_failed`
  - `answer_slow_first_token`
- Added `request_ref` to admin support usage rows and `session_ref` to admin support session rows so a screenshot id can be matched without exposing transcript text.
- Audit metadata intentionally excludes prompt text, transcript text, document text, URLs, screenshots, and raw account email.
- Audit metadata includes request/session refs, full request/session ids, trace id, lane, provider/model, timing, route index, retry-after/capacity status, and sanitized error preview.
- Desktop answer failures now call `record_active_session_diagnostic(..., "answer_error", ...)`, which updates synced session metadata with the failure ref and user-safe error message.

## Verification

```bash
cargo fmt --all
cargo test -p cue-core short_observability_ref -- --nocapture
cargo test --manifest-path server/Cargo.toml short_observability_ref -- --nocapture
cargo test -p cue-daemon answer_error_ref -- --nocapture
cargo build --manifest-path server/Cargo.toml
cargo build -p cue-daemon --bin bluey-daemon
git diff --check
```

## Current State

- Code changes are committed and pushed on `codex/bluey-overlay-spacing-20260626`.
- Production server is deployed from commit `5998379aea686d65db2570bdc2736defc2effd40`.
- Server binary backup:
  - `/var/backups/bluey-api/bin/bluey-server.previous-20260703005840`
- Live server binary SHA256:
  - `06919d0c285f1ee46c69cf0132d5f74684c41193bd0e43227837c09bfee1718c`
- Public health check:
  - `https://bluey.sh/health`
  - reported commit `5998379aea686d65db2570bdc2736defc2effd40`
- Public desktop release:
  - `0.1.54`
  - `https://bluey.sh/latest.json`
- Live macOS artifact SHA256:
  - `bc81e08c501d1ab112dbd2850dcd44755bd292337aab3287aff1d991f56afe24`
- Live verifier passed:
  - signed `latest.json`
  - `install.sh` content type `application/x-shellscript`
  - `install.ps1` content type `application/x-powershell`
  - `darwin-arm64` artifact SHA
  - unpacked binary version `0.1.54`
