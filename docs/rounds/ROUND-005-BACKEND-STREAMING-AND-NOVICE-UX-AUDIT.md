# Round 005 - Backend Streaming And Novice UX Audit

## Trigger

Owner asked to check the backend clearly, make sure everything streams properly without issues, and think from the perspective of an inexperienced user: what can be clearer, easier, and more reliable?

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-25 15:19 EDT

## Backend Streaming Audit

Current streaming path:

- Server endpoint: `server/src/api/router.rs` `/router/complete/stream`
- Managed client: `crates/cue-cloud-client/src/client.rs` `auth_post_stream`
- Managed LLM adapter: `crates/cue-llm/src/bluey_managed.rs`
- Daemon overlay bridge: `crates/cue-daemon/src/app.rs` `OverlayAnswerStream`

What is already strong:

- Server streams provider deltas before billing metadata.
- Server only bills after the upstream stream reaches terminal usage metadata.
- Truncated streams after deltas emit an error event and are not billed.
- Completed idempotency responses are replayed as SSE chunks.
- Managed client parses both SSE and NDJSON, buffers split UTF-8 correctly, and requires final billing metadata.
- Daemon uses answer-generation ids so old answer updates do not overwrite newer asks.

## Fix

- Added server-side SSE keep-alives for both live and cached streaming responses.
  - Keep-alive interval: 15 seconds.
  - This helps long or slow answers survive proxies and keeps clients from treating a quiet stream as dead.
- Fixed daemon user-facing error classification for incomplete managed streams.
  - Previously, `stream ended before final billing metadata` could match the generic `billing` branch and show a misleading billing/quota message.
  - It now shows a plain retry message: the connection dropped before the answer finished, the answer was not saved as completed, and the user should retry.
- Added daemon regression tests for:
  - incomplete stream errors taking priority over billing/quota keyword matching
  - capacity-busy retry hints staying intact

## Verification

Passed:

- `cargo fmt --check -p cue-daemon -p cue-llm`
- `cargo test -p cue-daemon user_facing_answer_error --lib`
- `cargo test -p cue-llm complete_stream_errors_when_managed_stream_ends_without_billing_final --lib`
- `cargo test -p cue-llm complete_stream_errors_when_done_arrives_before_billing_final --lib`
- `cargo test -p cue-llm parses_managed_sse_deltas_and_billing_metadata --lib`
- `cd server && cargo fmt --check`
- `cd server && cargo check --all-targets`
- `cd server && cargo test --all-targets router_complete_stream -- --nocapture`
- `cargo clippy -p cue-daemon -p cue-llm --all-targets -- -D warnings`
- `cd server && cargo clippy --all-targets -- -D warnings`

Streaming integration coverage confirmed:

- OpenAI deltas arrive before billing.
- Anthropic deltas arrive before billing.
- Cached completed responses replay through the stream endpoint.
- OpenAI/Anthropic truncated streams after a delta emit stream errors and are not billed.

## Current App State

- Bluey daemon is running in local visible test mode.
- pid `93283`
- active meeting id `2ffa3c6c-df9e-4d12-a8c3-9990ab946c88`
- overlay visible `true`
- screen capture active `false`
- transcript segments `0`
- context items `0`
- managed AI status: healthy, streaming answers enabled, vision and STT available
- cloud status: token configured for `https://bluey.sh`, sync ready

## Novice User Improvements To Prioritize Next

1. Add visible stream status events.
   - Server can emit non-content `status` SSE events for `routing`, `fallback`, `retrying provider`, and `billing complete`.
   - Overlay can show this as small calm text like `Finding the best lane...` or `Switching provider...`.

2. Add provider-level cancellation for superseded answers.
   - Daemon already prevents stale cards from overwriting newer cards.
   - Next step is aborting the upstream request when a newer ask supersedes it, reducing spend and background work.

3. Add a first-time-user health summary.
   - One backend-driven status should answer: signed in, balance/trial, AI ready, STT ready, mic/system permissions, screen capture ready, cloud sync ready.
   - User-facing label should be simple: `Ready`, `Needs sign-in`, `Needs mic permission`, `Add credits`, `Provider busy`.

4. Make error copy action-oriented.
   - Every backend error should map to one concrete next action.
   - Examples: sign in, add credits, wait 17 seconds, reconnect internet, enable mic permission, attach a clearer screenshot.

5. Add a long-session streaming smoke.
   - Simulate slow deltas, idle keep-alives, provider stall before first token, provider cut after partial text, and retry/idempotency behavior.
   - This should be a named command before release.

6. Attach sources and missing-source notices to final metadata.
   - The final billing/metadata event can carry which docs/screens/RAG chunks were used and what was missing.
   - This helps new users understand why an answer was confident or why Bluey asked for more context.

7. Keep Mac and Windows parity explicit.
   - This round touched backend/server/daemon logic only, not native Mac or Windows UI code.
   - No Windows native parity change was required.

## Files Touched

- `server/src/api/router.rs`
- `crates/cue-daemon/src/app.rs`
- `docs/rounds/ROUND-005-BACKEND-STREAMING-AND-NOVICE-UX-AUDIT.md`
