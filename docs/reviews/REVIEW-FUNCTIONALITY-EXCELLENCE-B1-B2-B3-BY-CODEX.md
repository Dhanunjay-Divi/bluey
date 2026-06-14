# REVIEW — Functionality Excellence B1/B2/B3 + Prior Streaming/Caption Fixes

Reviewer: Codex  
Date: 2026-06-13  
Scope: committed diffs only, with the current working tree left untouched.

Reviewed commits:

- `b7b448b` — `feat(server): bound RAG retrieval so it cannot delay first token (B1)`
- `5aafcab` — `feat(server): first-token deadline fallback for managed streaming (B2)`
- `63b339c` — `feat(cloud-client): bounded retry-with-backoff on idempotent GETs (B3 reliability)`
- Prior related commits:
  - `11cad73` — managed live-caption relay through Bluey server
  - `633c3a2` — live-caption responsiveness/listening state UI
  - `143838a` — managed stream finality + dashboard API URL + smoke cleanup

Verdict: 🟢 ACCEPT WITH NON-BLOCKING NITS

## Findings

No blocking issues found in B1, B2, B3, or the prior streaming/caption closure commits.

P2 — Runtime env knobs are intentionally useful, but currently unbounded.

`BLUEY_RAG_RETRIEVAL_BUDGET_MS`, `BLUEY_STREAM_FIRST_TOKEN_TIMEOUT_MS`, `BLUEY_CLOUD_GET_RETRIES`, and `BLUEY_CLOUD_RETRY_BASE_MS` all accept arbitrary positive values. A bad deploy value like a 60s RAG budget or huge GET retry count can silently defeat the latency guarantee. This is not a correctness blocker, but before wider alpha I would clamp these to sane ranges and log when clamped.

P2 — Several unit tests mutate process-global env vars.

The helper tests for RAG budget, first-token deadline, and cloud GET retry config use `std::env::set_var/remove_var`. If the Rust test harness runs nearby env-sensitive tests in parallel, this can flake. It is acceptable for now, but I would move these behind a serial env lock or inject the config into helpers.

P2 — B3 label does not match the original plan item exactly.

The plan’s B3 says “warm the per-request setup.” Commit `63b339c` instead implements bounded retry/backoff for idempotent cloud GETs. I agree with the code and it improves reliability, but it does not close the original warm-cache idea. Keep “warm per-request setup / collapse duplicate classify” as a separate measured latency follow-up if it still matters after live eval.

## B1 — RAG Retrieval Cannot Delay First Token

Decision: agree with Kiro.

What I checked:

- `server/src/api/router.rs` adds `DEFAULT_RAG_RETRIEVAL_BUDGET_MS = 300`.
- `completion_rag_matches_budgeted` moves the blocking SQLite RAG query into `tokio::task::spawn_blocking`.
- The request waits on that task through `tokio::time::timeout`.
- Timeout, join failure, or short query all return an empty match list and continue answering.
- Both streaming and non-streaming managed completion paths now call the budgeted wrapper.

Why I accept it:

This matches the product rule: RAG improves context, but must never block the first answer token indefinitely. The answer now proceeds without retrieved context after the budget, which is the right reliability tradeoff for a live overlay.

Remaining risk:

`spawn_blocking` work is not actually cancelled after timeout; it continues and its result is dropped. That is fine for normal SQLite queries, but under pathological slow storage or high concurrency it can consume blocking-pool capacity. If this shows up in telemetry, add a RAG semaphore and a slow-query metric.

## B2 — First-Token Deadline → Fallback

Decision: agree with Kiro’s narrowed scope.

What I checked:

- `DEFAULT_FIRST_TOKEN_TIMEOUT_MS = 6000`.
- The server opens the upstream managed stream, then waits for the first event under `tokio::time::timeout(first_token_deadline(), stream_events.next())`.
- If no event arrives before the deadline, the route is treated as stalled and the router moves to the next provider route.
- If the first event is a delta, final event, error frame, or even an empty stream, the selected route is committed and normal consume/finality logic handles it.

Why I accept it:

The exact high-value failure case is a provider that accepts the request and then stalls before producing anything. This commit fixes that without changing existing semantics for explicit provider errors. That is a conservative and correct change.

What I would not call closed:

This is not a full “fallback on any bad first event” implementation. If the first event is an in-band provider error, it is still surfaced through the selected stream path rather than trying the next route. I agree with Kiro that this should stay separate to avoid regressions, but product-wise we may later decide some pre-delta error classes should be route-retryable.

Operational note:

Timing out and dropping an upstream stream can still consume provider-side work if the provider continues processing after disconnect. That is acceptable for UX, but we should add counters for `first_token_timeout` and `fallback_route_selected` so we can see whether this becomes expensive.

## B3 — Idempotent GET Retry/Backoff

Decision: agree with implementation, disagree with calling it the original B3 closure.

What I checked:

- Only `auth_get`/`public_get` paths are retried.
- Retryable cases are network timeout/connect and 502/503/504/408.
- It explicitly does not retry 401, 402, 429, trial-ended, 4xx, 500, or 501.
- Defaults are bounded and modest: two retries, 120ms base with jitter and exponential backoff.
- POST/mutation paths are not retried, which avoids duplicate billing or account side effects.

Why I accept it:

This is a safe reliability improvement for account/dashboard/status calls. It avoids retrying business-state errors and rate-limit responses, which is important for billing clarity.

Remaining gap:

The original B3 from the plan was about warming per-request setup and collapsing duplicate classification. This retry work is good, but it is a different item. Keep the warm path as a future latency task only if measurements show local setup/classification contributes meaningful time.

## Prior Fix — Managed STT Relay (`11cad73`)

Decision: agree.

What I checked:

- Real audio switches to `real_audio_relay_loop` when `RealSttTransport::BlueyManagedRelay` is selected.
- Each enabled source opens a Bluey STT session and websocket, then streams native helper PCM chunks through the relay.
- Deepgram relay payloads are parsed with the existing Deepgram frame parser.
- Mic/system source labels are preserved through transcript segments.
- Stop/session mismatch propagates through `watch` shutdown and helper process cleanup.

Why I accept it:

This is the right architecture for low-latency live captions: desktop → Bluey server websocket → Deepgram websocket → overlay. It also keeps provider keys server-side.

Non-blocking UX/cost notes:

- Dual mic+system opens separate source streams, which is expected but doubles live STT usage.
- Partial transcript hypotheses can still be noisy until real Deepgram behavior is observed. The existing final-partial dedup helps, but live QA should watch for repeated interim text.

## Prior Fix — Live Caption Responsiveness (`633c3a2`)

Decision: agree.

What I checked:

- The daemon now emits overlay listening states: connecting, listening, paused, failed.
- Chunked audio capture handles mic/system jobs concurrently with `join_all`, reducing serial latency.
- The Swift overlay has explicit run-state handling and visible listening/paused/failed transitions.
- Composer Enter/Shift+Enter behavior and Answer action are wired in the overlay code.

Why I accept it:

This addresses the “I clicked Listen but cannot tell if it is alive” class of failure. It is not a full visual QA stamp, but the code paths are coherent.

Non-blocking UX note:

The state changes to “Listening” after capture starts, not after first transcript arrives. That is okay, but the best user signal is source-level activity: mic/system glyphs should animate only when frames or transcripts are actually flowing.

## Prior Fix — Stream Finality + Dashboard API URL (`143838a`)

Decision: agree.

What I checked:

- `BlueyManagedProvider::complete_stream` tracks `seen_billing_final`.
- The managed stream now errors if EOF happens before a final billing/cost event.
- This prevents a partial managed response from being treated as a successful complete answer.
- Dashboard cloud client construction now reads saved `account.api_url` and uses it as `base_url`, so staging/local accounts do not silently call production.
- The smoke-test audio assertion no longer only passes on an unauthenticated setup-error phrase.

Why I accept it:

The managed stream finality guard is the right trust boundary: final billing metadata is what proves the server completed accounting. Requiring it avoids showing truncated, unbilled answers as if they were complete.

Potential future edge:

If the server ever emits a valid final event without cost metadata, the client will reject it. For Bluey managed traffic that is correct; every successful managed answer should carry billing/cost metadata.

## Overall Agreement / Disagreement

I agree with Kiro on:

- B1 being the correct first-token protection for RAG.
- B2 being intentionally stall-only fallback, not a broad first-error fallback.
- GET-only retry/backoff being safe and useful.
- The STT relay architecture as the correct path for live caption latency.
- Requiring managed stream final billing metadata before treating the answer as successful.

I disagree only on naming/scope:

- `63b339c` should be called “B3 reliability” or “cloud GET retry hardening,” not closure of the plan’s original “warm per-request setup.” The warm setup item can remain open until latency measurements justify it.

## Verification

I reviewed the committed diffs with `git show` and line-level reads. I did not rerun the full pipeline because the current working tree contains unrelated dirty server files and the local `bluey-dev.db`; I left them untouched.

Recommended follow-up tests before wider alpha:

- Live provider latency eval after funded OpenAI/Anthropic accounts.
- Real Deepgram dual-source caption smoke: mic + system, start/stop, idle stop, reconnect failure.
- Streaming disconnect billing tests at server and managed-client layers.
- Metrics for RAG timeout, first-token timeout, fallback route selected, and STT relay source failures.
