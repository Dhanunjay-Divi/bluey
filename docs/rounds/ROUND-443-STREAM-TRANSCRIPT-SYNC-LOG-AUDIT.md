# ROUND-443 Stream, Transcript, And Sync Log Audit

Date: 2026-07-08
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The overlay showed an answer start and then stop, and pressing Enter after Listen did not include the spoken transcript in the sent prompt.

## Log Findings

- Local active session: `b641fbab-c59d-4ba6-afa4-63538dbbfb64` (`B641FBAB`).
- The first Listen run produced stored transcript segments.
- The later short Listen/stop/Enter run forwarded audible microphone audio, but Deepgram returned text frames without usable transcript frames before the user sent the answer.
- The audit bundle captured streamed answer deltas for request `72E2BBFD`; Bluey streamed a long answer, then replaced it with `Capacity busy`.
- The server-side reason was not true provider capacity. It was `code_artifact_missing`: the answer plan expected a code artifact, but the provider returned prose only after already streaming useful text.
- Cloud sync also failed with Postgres binding error:
  `cannot convert between the Rust type i64 and the Postgres type int4`
  for `cloud_rag_chunks.chunk_index`.

## Fixes In This Round

- Preserve streamed prose when a code artifact is missing after streaming completes. The server now logs `code_artifact_missing_stream_preserved` instead of replacing the visible answer with an error.
- Add daemon error mapping so future `code_artifact_missing` failures are shown as a code-panel issue, not as provider capacity.
- Convert Postgres `cloud_rag_chunks.chunk_index` and `token_count` to `i32` before binding, matching the current database schema.
- Remember the latest fresh interim live caption for the current audio session and include it as transcript context for live-caption answers when the final STT segment has not arrived yet.

## Still Open

- The Listen/Enter path now has a fresh-interim fallback, but the UI should still show an explicit "Still transcribing" state when audio was forwarded and neither interim nor final transcript is available.
- STT quality still needs the separate interim-results/finalization pass: render interim captions immediately, commit final captions, and only auto-send after the final transcript flush is complete.

## Verification

- `cargo test -p cue-daemon user_facing_answer_error --quiet`
- `cargo test -p cue-daemon live_stt_finalize_wait_defaults_and_clamps --quiet`
- `cargo test -p cue-daemon live_caption_answer_prompt_detection_is_specific --quiet`
- `cargo test --manifest-path server/Cargo.toml sync_batch_round_trips_session_bundle_and_rag --quiet`
- `git diff --check`
