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

## Deployment

- Desktop release `0.1.57` published to `bluey.sh`.
- Live release signature verified.
- Live installer MIME checks passed.
- Unpacked macOS release reports `bluey 0.1.57` and `bluey-daemon 0.1.57`.
- Production API deployed from commit `5d2f1f379a64059d7d3ec68abaeaf69f80ae0e26`.
- Production API health returned OK for commit `5d2f1f379a64059d7d3ec68abaeaf69f80ae0e26`.
- Production API binary SHA:
  `d3568f41e947c49993cd1f50bb205c12908b88c543e5bd81e726ba7875dede2e`
- Previous production API binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260703T091652Z`
