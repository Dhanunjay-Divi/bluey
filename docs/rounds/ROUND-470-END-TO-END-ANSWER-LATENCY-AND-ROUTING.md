# Round 470 - End-to-End Answer Latency and Routing

Date: 2026-07-10

## Goal

Make Bluey feel immediate for quick questions while preserving complete,
production-quality answers for coding, system design, screen analysis,
interviews, and research. The target is fast first useful text, reliable
fallback, relevant context, and enough diagnostics to explain every slow turn.

This round was implemented and tested locally. It was not deployed, released,
or sent through GitHub Actions.

## Findings

The slow behavior was not one model problem:

1. The desktop sent a large general/coding/design/interview prompt contract to
   the managed server. The server then appended another AnswerPlan contract.
   That duplicated instructions and inflated every managed request.
2. The provider dispatcher created a fresh `reqwest::Client` for each upstream
   call. That discarded reusable TCP/TLS connections from the first-token path.
3. The default balanced provider mix rotated normal questions across both fast
   models and slower full/pro models. The same question could therefore start
   quickly on one request and slowly on another.
4. The desktop explicitly sent `reasoning_effort=high` for every deep request,
   overriding the server's intended medium default. Ordinary coding and system
   design turns could enter the most expensive reasoning mode unnecessarily.
5. The server measured provider first event but did not separate memory,
   AnswerPlan, web search, prompt composition, and total request-to-first-event
   time. A slow support reference could not identify the phase responsible.
6. The first-event and connect budgets were one-size-fits-most: 6/12 seconds for
   normal routes and 30/45 seconds for deep routes. A stalled provider could
   hold a user too long before fallback.

## Changes

### Compact managed prompt

`call_bluey_managed_provider` now sends a small stable base contract plus any
explicit per-session answer rules. The managed server remains responsible for
intent-specific coding, design, interview, screen, research, and artifact
instructions through AnswerPlan.

Direct/BYOK providers still receive the complete standalone daemon contract.
The managed prompt test proves it preserves question, context, explicit answer
rules, image payloads, grounding, and private-instruction protection while
remaining less than one-third the size of the previous full contract.

### Connection reuse

All server-managed OpenAI-compatible, Anthropic, Gemini, embedding, Deepgram,
and transcription requests now share one pooled HTTP client. The pool keeps
idle connections for 90 seconds and enables TCP keepalive, avoiding a fresh
connection/TLS setup for every answer.

### Latency-aware provider tiers

Default `provider_mix` still spreads load across OpenAI, Anthropic, Gemini,
DeepSeek, and Z.AI, but instant and balanced first attempts now use only the
fast-capable tier. Full/pro/flagship routes remain fallbacks or deep-lane
candidates.

- Gemini Flash moved from the preview route to stable `gemini-3.5-flash`.
- Z.AI `glm-4.7-flashx` is the fast instant/balanced route.
- Z.AI `glm-5.2` remains available for deep work and balanced fallback.
- DeepSeek V4 Flash remains fast-tier; V4 Pro remains deep-tier.
- Claude Sonnet, Haiku, Opus and OpenAI mini/accurate routes remain in their
  appropriate quality tiers.

Pricing entries were added for Gemini 3.5 Flash and GLM-4.7-FlashX. Historical
Gemini preview pricing remains in code so old usage rows can still reconcile.

Official route references reviewed:

- https://ai.google.dev/gemini-api/docs/models/gemini-3.5-flash
- https://ai.google.dev/gemini-api/docs/pricing
- https://api-docs.deepseek.com/quick_start/pricing
- https://docs.z.ai/guides/overview/overview
- https://docs.z.ai/guides/overview/pricing
- https://docs.z.ai/guides/capabilities/thinking-mode

Every route still requires a configured server-side key and a successful live
smoke before release. Provider keys never enter desktop binaries.

### Reasoning ownership

The desktop no longer forces high reasoning for the deep lane. The server owns
the lane default and can tune it centrally. Deep currently defaults to medium;
instant, balanced, and vision default to no extended thinking unless explicitly
requested by server policy.

### Lane-specific fallback budgets

Default first-event budgets:

| Lane | First provider event | Route connect |
| --- | ---: | ---: |
| instant | 2 s | 4 s |
| balanced | 4 s | 7 s |
| vision | 6 s | 10 s |
| deep/thinking | 15 s | 25 s |

Legacy environment overrides remain supported. New lane-specific overrides are
available for controlled production tuning. A provider that connects but emits
no event within the lane budget falls through to the next healthy candidate.

### User-visible responsiveness

The desktop emits an immediate honest status before opening the managed stream:
answering, preparing the answer, working through the complete answer, or
reading visual context. This is not presented as generated answer text and is
replaced by real streamed text as soon as it arrives.

### Phase diagnostics

Managed streaming logs now include the same request/session references plus:

- memory lookup duration and match count
- AnswerPlan duration and source
- managed web-search duration
- system/user prompt characters and estimated input tokens
- pre-dispatch duration
- route connect and first-event deadlines
- selected provider/model/fallback index
- provider-dispatch-to-first-event latency
- total request-to-first-event latency
- total completion latency

Slow-first-token audit records now use the total request-to-first-event clock,
not only the provider dispatch clock. The warning threshold is 2.5 seconds for
normal lanes and 8 seconds for deep/thinking lanes, both configurable.

## End-to-End Flow

1. The macOS or Windows desktop authenticates and creates a stable request and
   session reference.
2. The question, transcript, screen context, files, and current artifact are
   persisted locally before or alongside display. Local SQLite/files and local
   RAG support immediate device context and recovery.
3. The daemon selects managed Auto/Instant/Balanced/Deep/Vision intent and sends
   a compact prompt plus relevant context to the server.
4. Server AnswerPlan classifies the turn with deterministic rules first. The
   optional tiny AI classifier remains off unless explicitly enabled and only
   runs for ambiguous low-confidence cases.
5. Memory lookup is selective and capped at 100 ms. Unrelated standalone quick,
   coding, screen, research, and missing-context turns do not wait on cloud
   memory. Managed web search runs only when the plan needs current/public
   evidence and has its own bounded quota, cost, and timeout.
6. The server checks account state, idempotency, balance/trial, shared capacity,
   provider/model/key cooldown, and upstream spend guards before dispatch.
7. `provider_mix` rotates within the lane's approved latency/quality tier.
   Redis/Valkey shares rate limits and provider cooldowns across server
   instances when configured; local guards remain the availability fallback.
8. The first healthy provider stream is normalized to Bluey SSE events. The
   daemon renders status, answer deltas, sources, billing, and artifacts.
9. Complete code/design artifacts and chat answers are persisted with stable
   IDs, then synced idempotently to the account's cloud session.
10. Postgres is the durable account/session/usage source of truth. R2/S3 owns
    large artifact bytes, diagnostic/audit objects, release files, and backups;
    it is not the balance or auth database.

## Honest Production Boundaries

Bluey can target sub-second first useful text for warm instant requests. It
cannot guarantee that a complete long code solution, system design, web search,
or image analysis finishes in one second. Release SLOs should be measured as:

- instant first useful text: p50 <= 1 s, p95 <= 2 s
- balanced first useful text: p50 <= 1.5 s, p95 <= 3 s
- vision first useful text: p50 <= 2.5 s, p95 <= 5 s
- deep first useful text: p50 <= 3 s, p95 <= 10 s
- answer completion: tracked separately by output length and intent

These are rollout targets, not claims that the local code change has already
met them in production. A signed canary and real provider measurements are
required before promotion.

One existing cloud-memory boundary remains: Postgres has a pgvector column and
index, but the current completion lookup uses bounded lexical retrieval because
it does not yet obtain a query embedding inside the 100 ms request budget. Do
not claim runtime pgvector KNN for managed answers until query embeddings and
tenant-filtered KNN are measured and enabled without hurting first token.

## Next Production Improvements

1. Run a signed canary with at least 30 prompts per intent and record p50/p95
   request-to-first-text, completion, error, fallback, token, and cost metrics.
2. Add provider/model EWMA latency and error scoring in Redis so rotation can
   adapt to live health instead of using only static tiers and cooldowns.
3. Pool Redis/Valkey connections; current shared limit and cooldown operations
   open multiplexed connections on demand.
4. Move pre-dispatch work inside an immediately returned SSE task if live
   measurements show status itself is delayed by planning/search.
5. Add a cached query-embedding lane and real tenant-filtered pgvector KNN,
   while retaining lexical fallback and the 100 ms answer-path budget.
6. Promote only if billing reservation/settlement, 429 failover, code artifact,
   context isolation, audit upload, and macOS/Windows smoke all pass together.

## Verification

Passed:

- `cargo test --manifest-path server/Cargo.toml routing::dispatcher::tests --lib --quiet` (31)
- `cargo test --manifest-path server/Cargo.toml api::router::tests --lib --quiet` (90)
- `cargo test --manifest-path server/Cargo.toml pricing::tests --lib --quiet` (14)
- `cargo test -p cue-daemon app::tests --lib --quiet` (147)
- `cargo clippy -p cue-daemon --lib -- -D warnings`
- `cargo clippy --manifest-path server/Cargo.toml --lib -- -D warnings`
- scoped `git diff --check`

The full server library run passed 277 tests. Two unrelated mail transport tests
could not bind a Wiremock OS port in the restricted sandbox and failed with
`PermissionDenied`; routing, pricing, prompt, and AnswerPlan tests passed.

## Files

- `crates/cue-daemon/src/app.rs`
- `server/src/api/router.rs`
- `server/src/routing/dispatcher.rs`
- `server/src/pricing/mod.rs`
- `docs/MODEL-ROUTING.md`
- `docs/PRICING-MODEL.md`
- `docs/rounds/ROUND-470-END-TO-END-ANSWER-LATENCY-AND-ROUTING.md`
