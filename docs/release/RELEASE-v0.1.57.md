# Bluey 0.1.57

## Focus

Realtime STT and answer startup latency.

## Changes

- Live STT relay starts after audible audio or a 250 ms warm-up window.
- Silent relay sessions settle with zero billable elapsed when no audible chunks are detected.
- Deepgram realtime endpointing default is now 200 ms.
- Chunked STT fallback defaults to 500 ms chunks.
- AnswerPlan AI fallback is opt-in with `BLUEY_ANSWER_PLAN_AI_FALLBACK=1`.
- Cloud memory/RAG pre-answer budget defaults to 100 ms and is skipped for direct code, quick, screen, research, and missing-context requests.

## Verification

- `cargo test -p cue-daemon stt -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml answer_plan_ -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml deepgram -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml memory_lookup_is_explicit_or_followup_only -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml rag_retrieval_budget_default_and_override -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml pcm16_i16le_stats_are_privacy_safe_levels -- --nocapture`
- `cargo check -p cue-daemon`
- `cargo build --manifest-path server/Cargo.toml`
