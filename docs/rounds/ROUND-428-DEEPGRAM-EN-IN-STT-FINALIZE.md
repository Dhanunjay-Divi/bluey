# ROUND-428-DEEPGRAM-EN-IN-STT-FINALIZE

Date: 2026-07-08
Repo: /Users/uno/Downloads/cue
Branch: codex/bluey-web-ui-parallel-20260704
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Make Bluey's Deepgram transcription default better for Indian English while still supporting other English accents, and make live-caption answers wait for the last received voice to become transcript context before sending.

## Changes

- Set Deepgram's default language hint to `en-IN` for the server live STT relay.
- Set Deepgram's default language hint to `en-IN` for managed chunked transcription calls.
- Set the daemon direct Deepgram provider factory to default to `en-IN`.
- Kept `BLUEY_DEEPGRAM_LANGUAGE` as the override for other English accents such as `en-US`, `en-GB`, `en-AU`, or generic `en`.
- Allowed `BLUEY_DEEPGRAM_LANGUAGE=auto`, `detect`, `none`, or `off` to omit the language hint if we want Deepgram's generic/default detection path.
- Added URL-safe escaping for chunked Deepgram model/language query params.
- For live-caption answers, Bluey now waits briefly for final transcript segments, then rehydrates answer context from the latest session transcript before routing the answer.

## Why

The live caption bar can show a recent partial while the final transcript segment is still arriving. If the user presses Answer at that exact moment, the previous path could answer from a stale transcript snapshot. The new path uses the existing finalize wait window, then rebuilds context so the most recent received speech has a chance to land before the answer request is sent.

Deepgram only accepts one language hint per request, not an ordered accent fallback list. Product default is now Indian English because that is the primary accent we are optimizing for. Other English accents remain a server/daemon env override instead of a code change.

## Verification

- `cargo test --manifest-path server/Cargo.toml deepgram_url --quiet`
- `cargo test --manifest-path server/Cargo.toml deepgram_language_defaults_to_indian_english_but_can_be_overridden --quiet`
- `cargo test --manifest-path server/Cargo.toml deepgram_query_escape_keeps_language_and_model_url_safe --quiet`
- `cargo test --package cue-daemon deepgram_language_defaults_to_indian_english_but_can_be_overridden --quiet`
- `cargo test --package cue-daemon live_caption_answer_prompt_detection_is_specific --quiet`
- `git diff --check -- server/src/api/stt.rs server/src/routing/dispatcher.rs crates/cue-daemon/src/stt/factory.rs crates/cue-daemon/src/app.rs`

All checks above passed.

## Deploy

No deploy was performed in this round. Per current Bluey workflow, signed release/deploy should only happen when explicitly requested.
