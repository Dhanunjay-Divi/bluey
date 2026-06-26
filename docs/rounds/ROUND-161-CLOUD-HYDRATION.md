# Round 161 - Cloud Hydration Round

## Why

Bluey already pushed local session records to the managed sync API, but a fresh device did not rebuild local history from cloud data. That meant cloud session history could exist while the desktop still had no local meeting JSON or local RAG index.

## What Changed

- Added cloud session hydration in the daemon sync path.
- Restores missing cloud sessions into the local archived meeting store.
- Recreates restored document/screen context as local markdown preview files under `cloud-restored-context`.
- Preserves answer attachment ids when syncing conversation turns.
- Rebuilds local RAG after cloud sessions are restored.
- Added best-effort auto sync on daemon startup, account status/link refresh, and meeting end when cloud sync is enabled.

## Current Boundary

This restores answer-ready context: transcripts, answers, document previews, screen notes, and searchable RAG text. It does not yet restore original binary files or full-resolution screenshots on another device unless object byte sync is wired separately.

## Verified

- `cargo test -p cue-daemon cloud::sync::tests`
- `cargo check -p cue-daemon`
