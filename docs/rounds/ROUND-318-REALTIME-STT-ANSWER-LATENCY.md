# Round 318 - Realtime STT Answer Latency

## Trigger

The owner reported that live transcript text still did not feel realtime, answer streaming took too long to begin, and Bluey answers should feel much faster during live testing.

## Root Cause

- Live managed STT relay waited for an audible audio chunk before creating the cloud STT session and opening the provider WebSocket. That protected silent Listen toggles, but it made the first spoken words pay session-create and socket-open latency.
- Chunked STT fallback still defaulted to 1000 ms chunks, so any non-relay path had a built-in one-second capture delay before transcription could even start.
- Server relay settlement treated any forwarded audio bytes as billable, even if the bytes were silence. That made it hard to safely warm the relay earlier.
- AnswerPlan's optional AI classifier was default-on and could add up to about 900 ms before the real answer dispatch.
- Cloud RAG lookup had a 300 ms default budget and ran before the final plan, so context lookup could delay first-token preparation when it was not clearly needed.

## Fix

- Added a 250 ms live STT startup warm-up window. The daemon now opens the managed live STT relay after audible audio or after the short warm-up, whichever comes first.
- Kept the no-audio protection: if Listen stops before any helper bytes arrive, no cloud STT session is created.
- Changed server STT settlement to track audible chunks separately from raw forwarded bytes. Silent relay sessions now settle as `no_audible_audio` with zero billable elapsed.
- Lowered managed Deepgram realtime endpointing default from 300 ms to 200 ms while keeping `utterance_end_ms=1000`.
- Lowered chunked fallback default from 1000 ms to 500 ms when the default audio config is used.
- Made `BLUEY_ANSWER_PLAN_AI_FALLBACK` opt-in instead of default-on. Normal production routing is local-rule-first with no classifier call.
- Reduced default cloud RAG retrieval budget from 300 ms to 100 ms.
- Added a preliminary local plan gate so direct code, quick, screen, research, and missing-context requests skip memory lookup before dispatch.

## Verification

- `cargo fmt --all`
- `cargo test -p cue-daemon stt -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml answer_plan_ -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml deepgram -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml memory_lookup_is_explicit_or_followup_only -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml rag_retrieval_budget_default_and_override -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml pcm16_i16le_stats_are_privacy_safe_levels -- --nocapture`
- `cargo check -p cue-daemon`
- `cargo build --manifest-path server/Cargo.toml`
- `git diff --check`

## Current State

The local code is tested. Production binaries/server still need the normal release/deploy path before owner live testing picks up this round.

## Remaining QA/Gates

- Run a live signed-in Listen test and verify the overlay shows partial captions quickly.
- Confirm repeated silent Listen on/off toggles do not charge customer balance.
- Compare first-token timing before and after deployment using `managed chat route selected` logs.
