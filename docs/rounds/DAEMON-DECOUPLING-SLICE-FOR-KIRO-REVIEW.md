# Daemon Decoupling Slice For Kiro Review

Date: 2026-06-18
Branch: `codex/bluey-ai-site`

## Goal

Bluey is moderately decoupled overall, but the daemon hot path still has too much inline ownership of overlay UI state, local RAG indexing, sessions, audio, cloud, and answer generation. This round intentionally takes a small production-safe slice instead of attempting a large rewrite near alpha.

## What Changed

- Added `crates/cue-daemon/src/overlay_state.rs`.
  - Owns `SharedOverlayUiState`.
  - Owns the reset-on-drop UI state scope used by attach/style/submit flows.
  - Carries the unit tests for enter/reset behavior.
- Added `crates/cue-daemon/src/rag_indexer.rs`.
  - Owns RAG initialization from local OpenAI/dev BYOK config.
  - Owns the per-session async lock.
  - Owns transcript indexing, context artifact indexing, session rebuild, session delete, and query passthrough.
  - Preserves the deleted-session checks that prevent stale queued indexing from resurrecting vectors after delete.
- Replaced raw daemon fields:
  - `rag: Option<Arc<RagPipeline>>`
  - `rag_index_lock: Arc<Mutex<()>>`
  with one `rag_indexer: RagIndexCoordinator`.
- Kept public daemon behavior unchanged. `app.rs` still calls small local wrappers for transcript/context/reindex operations, but those wrappers now delegate to the coordinator.

## Why This Shape

The highest-risk daemon couplings are around money/audio/session behavior. This round avoids touching those state machines while still carving out two reusable boundaries:

- Overlay state can now evolve into a versioned overlay state machine without more `app.rs` sprawl.
- RAG indexing can now absorb tombstones, per-session queues, cloud-backed vector stores, or cancellation without session/audio code reaching for raw vector pipeline locks.

## Verification

- `cargo fmt --all --check`
- `cargo test -p cue-daemon overlay_ui_state --lib`
- `cargo test -p cue-daemon rag --lib`
- `cargo test -p cue-daemon --lib`
- `cd server && cargo test`
- `cargo test --all-targets`
- `cargo clippy --all-targets -- -D warnings`
- `python3 scripts/analyze-tracing-calls.py --check-only`
- `bash scripts/observability-acceptance-smoke.sh`

The server integration tests had three stale route-table expectations from the current model/capacity config and were updated to the live routing contract:

- Anthropic streaming mock expects `claude-sonnet-4-6-20260115`.
- OpenAI fallback mock expects `gpt-5.5`.
- Capacity-skip integration expects both OpenAI candidates to be attempted after Anthropic is capacity-blocked.

## Areas Most Likely Wrong

- The RAG coordinator uses the same global async lock as before. This is intentionally conservative, but a future round should replace it with per-session serialization so unrelated sessions can index concurrently.
- `app.rs` still owns wrapper functions named `index_transcript_for_rag`, `index_context_artifacts_for_rag`, and `reindex_meeting_for_rag`. Those wrappers are now thin, but the next cleanup should move call sites directly to a session/RAG service boundary.
- RAG initialization still uses local OpenAI/BYOK config. That matches current behavior, but managed/cloud RAG should eventually be a server-side service rather than a desktop embedder dependency.

## Recommended Next Split

1. Extract daemon audio runtime orchestration into `audio_runtime` or `audio_controller`.
2. Extract session lifecycle operations into `session_controller`.
3. Move answer generation and context assembly behind an `answer_engine` boundary that owns memory lookup.
4. Turn overlay IPC events into a versioned contract test suite.
