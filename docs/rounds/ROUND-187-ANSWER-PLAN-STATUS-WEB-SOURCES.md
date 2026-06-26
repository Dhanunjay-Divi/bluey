# Round 187 - Answer Plan Status Web Sources

## Trigger

Owner asked to implement the Round 185 path: add answer planning before streaming, decide evidence needs, add managed server-side web search with guardrails, stream visible status, and keep answers more organized like ChatGPT or Claude when current context is insufficient.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-26 07:03 EDT

## Implemented

- Added shared answer stream metadata in `cue-core`:
  - `AnswerRetrievalStatus`
  - `AnswerSourceMetadata`
  - `AnswerStreamEvent::RetrievalStatus`
  - `AnswerStreamEvent::Sources`
  - `AnswerResponseMetadata.sources`
- Added shared LLM metadata in `cue-llm`:
  - `LlmStatusMetadata`
  - `LlmSourceMetadata`
  - `LlmChunk.status`
  - `LlmChunk.sources`
  - `LlmResponse.sources`
- Extended managed Bluey stream parsing so server SSE can emit:
  - `event: status`
  - `event: sources`
  - final billing responses with `sources`
- Updated the daemon overlay answer stream:
  - shows status text before first answer token
  - clears status as soon as real answer text starts
  - shows local preflight status such as `Reading screen context` or `Checking saved Bluey memory`
  - shows managed status messages such as `Searching web`
  - shows `Found N sources` when source metadata arrives
- Added server-side `AnswerPlan` in the managed router:
  - detects quick, coding, screen, research, follow-up, missing-context, writing, and general intents
  - decides whether screen, docs, saved memory, or managed web search is needed
  - injects compact answer-shape instructions without exposing the plan to the user
- Added a managed web-search retrieval lane in `server/src/api/router.rs`:
  - disabled unless server env is configured
  - supports first-pass `tavily`, `brave`, and generic POST providers
  - sanitizes search queries from the public-facing question
  - avoids sending session context, attached docs, emails, URLs, code fences, obvious secrets, long token-like strings, or private prompt material as search terms
  - caps result count and request time
  - filters unsafe local/private URLs
  - uses search-result snippets as untrusted evidence, not page instructions
  - adds source labels such as `[W1]` for cited/current answers
- Added source replay for idempotency-cached streaming responses.
- Added example server env knobs in `ops/bluey-api.env.example`.

## Files Touched

- `crates/cue-core/src/ai.rs`
- `crates/cue-cloud-client/src/types.rs`
- `crates/cue-llm/src/lib.rs`
- `crates/cue-llm/src/bluey_managed.rs`
- `crates/cue-llm/src/openai.rs`
- `crates/cue-llm/src/anthropic.rs`
- `crates/cue-llm/src/ollama.rs`
- `crates/cue-llm/src/router.rs`
- `crates/cue-router/src/speculative.rs`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/llm/answer.rs`
- `crates/cue-daemon/src/llm/recap.rs`
- `crates/cue-daemon/src/llm/suggest.rs`
- `crates/cue-daemon/tests/auto_recap_integration.rs`
- `crates/cue-daemon/tests/cue_streaming_integration.rs`
- `server/src/api/router.rs`
- `ops/bluey-api.env.example`
- `docs/rounds/ROUND-187-ANSWER-PLAN-STATUS-WEB-SOURCES.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Verification

Passed:

- `cargo fmt`
- `cargo test -p cue-llm bluey_managed -- --nocapture`
- `cargo test -p cue-core request_response_and_stream_events_serialize -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml router::tests -- --nocapture`
- `cargo test -p cue-router speculative -- --nocapture`
- `cargo check -p cue-daemon`

## Current State

- Bluey now has the first product-code version of the answer-planning and retrieval-status foundation.
- Managed web search remains off by default and must be enabled on the server with env configuration.
- The first-pass search lane does not crawl arbitrary pages; it uses bounded search API results/snippets as evidence.
- Overlay status is text-in-card for now. It is visible and safe, but not yet a polished source-chip UI.
- Mac and Windows native overlay schemas were not changed in this round; the status/source data now flows through the Rust managed-answer path. Native source chips remain a parity UI task.

## Remaining QA Gates

- Pick and configure the first production search provider.
- Decide whether search calls consume credits separately or remain bundled into answer cost.
- Add account/day search quotas and/or Redis-backed search rate limits before broad production use.
- Add a small source-chip/source-drawer UI on macOS and Windows.
- Add privacy copy explaining when Bluey searches the web and what leaves the device.
- Run a live managed smoke with web search enabled:
  - public/current question gets web status
  - sources arrive
  - answer cites `[W1]`
  - no private transcript/doc text leaves as query
  - duplicate request id replays source metadata
- Add e2e coverage for no duplicate sends, no empty transcript auto-send, and no private-context leakage into search queries.
