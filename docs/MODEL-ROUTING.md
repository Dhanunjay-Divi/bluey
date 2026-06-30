# Bluey Model Routing

Last updated: 2026-06-29

This document is the source-of-truth snapshot for the model/provider routing
currently implemented in the Bluey codebase. It is intentionally operational:
it says what runs today, which keys are required, and which provider ideas are
not wired yet.

## Model Freshness Release Gate

Provider model catalogs and prices change frequently. Before every deploy that
can reach paying users, treat model freshness as a required release gate:

1. Check the official provider docs/dashboards for OpenAI, Anthropic, Gemini,
   Z.AI, DeepSeek, Deepgram, and the embedding provider in use.
2. Confirm each Bluey lane still points at an available, non-deprecated model:
   `instant`, `balanced`, `deep`, `vision`, `embed`, and STT.
3. Re-check pricing for every routed model and update
   `server/src/pricing/mod.rs` if any upstream price changed.
4. Re-check provider rate-limit and context-window notes. If a new model is
   better but has tighter limits, either keep the old model or update capacity
   env vars and fallback order in the same release.
5. Run the model-routing tests and at least one live smoke per provider with
   funded keys before promoting the deploy:

   ```bash
   cargo test --manifest-path server/Cargo.toml routing::dispatcher -- --nocapture
   cargo test --manifest-path server/Cargo.toml pricing -- --nocapture
   # Operator smoke: instant, balanced, deep, vision, embed, STT against deployed env.
   ```

6. Record the check in the release notes or round doc:

   ```text
   Model freshness checked: YYYY-MM-DD
   OpenAI: <model ids> / pricing checked
   Anthropic: <model ids> / pricing checked
   Gemini: <model ids> / pricing checked
   Z.AI: <model ids> / pricing checked
   DeepSeek: <model ids> / pricing checked
   Deepgram/STT: <model ids> / pricing checked
   Embeddings: <model ids> / pricing checked
   Live smoke: pass/fail + trace ids
   ```

Do not silently switch a production route to a new model just because it exists.
Every model change must include pricing, fallback, capacity, and smoke evidence.

## Managed LLM Routing

When a user is logged in, the desktop talks to `bluey-server` through
`BlueyManagedProvider`. The server owns provider API keys and maps lanes to
actual upstream models.

Default routing is `provider_mix`: Bluey keeps a lane-appropriate top tier, then
rotates the first attempt by request id so one burst does not hammer only Claude,
OpenAI, Gemini, GLM, or DeepSeek. Missing keys, provider capacity denials, and
HTTP 429/529 cooldowns still fall through to the next approved route.

| Bluey lane | Default top tier | Primary use |
| --- | --- | --- |
| `instant` | OpenAI `gpt-5.4-mini`, DeepSeek `deepseek-v4-flash`, Gemini `gemini-3.1-flash-lite`, Anthropic `claude-haiku-4-5-20251001`, Z.AI `glm-5.2` | Easy questions, quick answers, optional cheap draft |
| `balanced` | Anthropic `claude-sonnet-4-6`, DeepSeek `deepseek-v4-flash`, Z.AI `glm-5.2`, Gemini `gemini-3.1-pro-preview`, OpenAI `gpt-5.5` | Default technical/general answer |
| `deep` | Anthropic `claude-opus-4-8`, Z.AI `glm-5.2`, DeepSeek `deepseek-v4-pro`, Gemini `gemini-3.1-pro-preview`, OpenAI `gpt-5.5` | Hard coding, system design, long reasoning with a larger thinking/output budget |
| `vision` | OpenAI `gpt-5.5`, Gemini `gemini-3.1-pro-preview`, Gemini `gemini-3-flash-preview` | Analyse Screen, screenshots, image context |

Optional server-managed text candidates are also wired when their key pools are
configured:

| Provider | Model | Lanes | Notes |
| --- | --- | --- | --- |
| Z.AI | `glm-5.2` | `instant`, `balanced`, `deep` | OpenAI-compatible endpoint; deep lane sends thinking enabled |
| DeepSeek | `deepseek-v4-pro` | `deep` | OpenAI-compatible endpoint; deep lane sends thinking enabled |
| DeepSeek | `deepseek-v4-flash` | `instant`, `balanced`, `deep` fallback | OpenAI-compatible endpoint; instant/balanced send thinking disabled |

## AnswerPlan Pre-Routing

AnswerPlan is default-on for managed server routing. Set
`BLUEY_ANSWER_PLAN_ROUTING=0` only as a temporary rollback. The step runs before
provider route selection and is local rules first, not an extra AI classifier
call, so it does not add latency or cost.

The planner classifies the request into intents such as `quick`, `coding`,
`coding_followup`, `behavioral`, `system_design`, `screen`, `research`,
`missing_context`, `writing`, `meeting`, and `general`. It also decides the
evidence needs and preferred output shape:

| Intent | Preferred lane | Output shape | Guardrail |
| --- | --- | --- | --- |
| `quick` | `instant` | compact | Short answers stay cheap and fast |
| `coding` / `coding_followup` | `deep` | code artifact | Code requests should include real code, not vague summaries |
| `behavioral` | `balanced` | compact | Self-intro/resume answers must not become system-design answers |
| `system_design` | `deep` | canvas detail | Architecture prompts get larger reasoning/output budget |
| `screen` | `vision` | canvas detail | Image/screen requests stay on image-capable routes |
| `research` | `balanced` | source answer | Public/current unknowns can enter managed web search |
| `missing_context` | `balanced` | compact | Missing docs/screen are named once with a concrete next step |

When AnswerPlan is enabled, it may promote `balanced` Auto traffic to `instant`,
`deep`, or `vision` before the dispatcher applies `BLUEY_ROUTE_POLICY`. If it is
rolled back with `BLUEY_ANSWER_PLAN_ROUTING=0`, managed requests keep the lane
selected by the client.
Provider selection remains separate: `provider_mix`, `quality_first`, or
`cost_optimized` still decides which approved Claude/OpenAI/Gemini/GLM/DeepSeek
candidate handles the chosen lane.

## Thinking Budget Policy

Bluey now carries provider-neutral thinking controls through the managed path:

- Desktop/daemon request: `reasoning_effort` and `thinking_budget_tokens`
- Cloud client: forwards those fields to `bluey-server`
- Server dispatcher: maps them to provider-safe knobs only when the selected
  model supports them

Default policy:

| Lane | Default thinking | Why |
| --- | --- | --- |
| `instant` | Off | Starts answers fastest; no hidden reasoning spend |
| `balanced` | Off | Keeps normal work snappy unless caller/operator opts in |
| `deep` | Medium, 4096 thinking tokens | Hard coding/system-design work benefits from explicit reasoning budget |
| `vision` | Off | Screen analysis currently routes through OpenAI Chat Completions; provider-specific vision reasoning is future work |

Server overrides:

```bash
BLUEY_THINKING_DEEP_EFFORT=high
BLUEY_THINKING_DEEP_TOKENS=8192
BLUEY_THINKING_BALANCED_EFFORT=low
BLUEY_THINKING_BALANCED_TOKENS=2048
```

Request overrides use the same concepts:

```json
{
  "reasoning_effort": "high",
  "thinking_budget_tokens": 8192
}
```

Provider mapping today:

- Anthropic `claude-opus-4-8` and `claude-sonnet-4-6` map to `thinking:
  {"type":"enabled","budget_tokens":...}` and reserves enough
  `max_tokens` for both thinking and visible answer text. The Haiku fallback
  also supports the same manual thinking payload when a caller explicitly asks
  for thinking on that route.
- OpenAI managed routes still use Chat Completions in this codebase, so
  `reasoning_effort` is accepted but not sent upstream yet. The GPT-5.4 models
  in the route table are Chat Completions compatible; switching OpenAI managed
  routes to the Responses API is the right future hook for explicit OpenAI
  reasoning controls.
- Gemini is wired as a managed text/vision candidate. We pass output token and
  temperature controls today; explicit thinking-budget controls are left to a
  future Gemini-specific pass once live quality/cost measurements are in.
- Z.AI `glm-5.2` and DeepSeek V4 routes use the OpenAI-compatible Chat
  Completions shape. Bluey sends `thinking: {"type":"disabled"}` on non-deep
  lanes and enables provider reasoning on deep lanes without exposing reasoning
  text in the overlay.

This gives us the operational knob the user asked for without making every
easy question slower or more expensive.

The server now resolves each lane to an ordered candidate list, not a single
hard dependency. If the first provider for that request is unavailable, over
quota, or temporarily busy, Bluey tries the next candidate before returning an
error.

Default `provider_mix` candidate order:

Enable explicitly with `BLUEY_ROUTE_POLICY=provider_mix`, or leave
`BLUEY_ROUTE_POLICY` unset. The top tier rotates by request id; the fallback
tail stays fixed so weaker/cheaper fallbacks do not become the first choice for
harder work.

| Lane | Rotated top tier | Fixed fallback tail |
| --- | --- | --- |
| `instant` | OpenAI `gpt-5.4-mini` / DeepSeek `deepseek-v4-flash` / Gemini `gemini-3.1-flash-lite` / Anthropic `claude-haiku-4-5-20251001` / Z.AI `glm-5.2` | Gemini `gemini-3-flash-preview` -> Anthropic `claude-sonnet-4-6` |
| `balanced` | Anthropic `claude-sonnet-4-6` / DeepSeek `deepseek-v4-flash` / Z.AI `glm-5.2` / Gemini `gemini-3.1-pro-preview` / OpenAI `gpt-5.5` | Gemini `gemini-3-flash-preview` -> OpenAI `gpt-5.4-mini` |
| `deep` | Anthropic `claude-opus-4-8` / Z.AI `glm-5.2` / DeepSeek `deepseek-v4-pro` / Gemini `gemini-3.1-pro-preview` / OpenAI `gpt-5.5` | Anthropic `claude-sonnet-4-6` -> DeepSeek `deepseek-v4-flash` -> Gemini `gemini-3-flash-preview` |
| `vision` | OpenAI `gpt-5.5` / Gemini `gemini-3.1-pro-preview` / Gemini `gemini-3-flash-preview` | OpenAI `gpt-5.4-mini` |

Optional `quality_first` candidate order:

Enable with `BLUEY_ROUTE_POLICY=quality_first` if an incident requires the older
static first-provider order.

| Lane | Candidate order |
| --- | --- |
| `instant` | OpenAI `gpt-5.4-mini` -> DeepSeek `deepseek-v4-flash` -> Gemini `gemini-3.1-flash-lite` -> Anthropic `claude-haiku-4-5-20251001` -> Gemini `gemini-3-flash-preview` -> Anthropic `claude-sonnet-4-6` |
| `balanced` | Anthropic `claude-sonnet-4-6` -> DeepSeek `deepseek-v4-flash` -> Z.AI `glm-5.2` -> Gemini `gemini-3.1-pro-preview` -> OpenAI `gpt-5.5` -> Gemini `gemini-3-flash-preview` -> OpenAI `gpt-5.4-mini` |
| `deep` | Anthropic `claude-opus-4-8` -> Z.AI `glm-5.2` -> DeepSeek `deepseek-v4-pro` -> Gemini `gemini-3.1-pro-preview` -> OpenAI `gpt-5.5` -> Anthropic `claude-sonnet-4-6` -> DeepSeek `deepseek-v4-flash` -> Gemini `gemini-3-flash-preview` |
| `vision` | OpenAI `gpt-5.5` -> Gemini `gemini-3.1-pro-preview` -> Gemini `gemini-3-flash-preview` -> OpenAI `gpt-5.4-mini` |

Optional `cost_optimized` candidate order:

Enable with `BLUEY_ROUTE_POLICY=cost_optimized` on the server. This is meant
for owner-controlled smoke/A-B testing and margin tuning, not an unannounced
quality downgrade. It only changes text lanes; vision stays on providers with
image support.

| Lane | Candidate order |
| --- | --- |
| `instant` | DeepSeek `deepseek-v4-flash` -> Gemini `gemini-3.1-flash-lite` -> OpenAI `gpt-5.4-mini` -> Anthropic `claude-haiku-4-5-20251001` -> Gemini `gemini-3-flash-preview` -> Anthropic `claude-sonnet-4-6` |
| `balanced` | Z.AI `glm-5.2` -> DeepSeek `deepseek-v4-flash` -> Anthropic `claude-sonnet-4-6` -> Gemini `gemini-3.1-pro-preview` -> OpenAI `gpt-5.5` -> Gemini `gemini-3-flash-preview` -> OpenAI `gpt-5.4-mini` |
| `deep` | Z.AI `glm-5.2` -> DeepSeek `deepseek-v4-pro` -> Anthropic `claude-opus-4-8` -> Gemini `gemini-3.1-pro-preview` -> OpenAI `gpt-5.5` -> Anthropic `claude-sonnet-4-6` -> DeepSeek `deepseek-v4-flash` -> Gemini `gemini-3-flash-preview` |
| `vision` | OpenAI `gpt-5.5` -> Gemini `gemini-3.1-pro-preview` -> Gemini `gemini-3-flash-preview` -> OpenAI `gpt-5.4-mini` |

Every managed LLM candidate above has a matching entry in
`server/src/pricing/mod.rs`; the dispatcher unit tests assert this so an
unpriced model cannot silently become a paid route.

Code references:

- `server/src/routing/dispatcher.rs::resolve_route_candidates`
- `crates/cue-router/src/policy.rs::ManagedPolicy`
- `crates/cue-llm/src/bluey_managed.rs`

## Capacity And Rate-Limit Policy

Bluey protects realtime work at three layers:

1. **HTTP edge per-IP buckets**: protects unauthenticated auth endpoints from
   abuse. Authenticated router edge buckets are disabled by default to avoid
   punishing legitimate customers behind the same office/VPN/NAT; operators can
   enable them during an incident with `BLUEY_LIMIT_ROUTER_*`.
2. **Provider/model buckets**: keeps OpenAI, Anthropic, Gemini, Z.AI, DeepSeek,
   Deepgram, and embedding calls inside configured capacity and lets LLM lanes
   fall back before failing.
3. **Provider/model/key health ledger**: if an upstream key returns a capacity
   response such as HTTP 429, Bluey cools down that exact provider/model/key for
   `Retry-After` and immediately tries the next approved key or route.
4. **Optional per-account emergency guardrails**: disabled by default. Turn
   them on only during abuse incidents, stolen-token response, or runaway-client
   mitigation. Normal paid usage is controlled by account-credit balance and provider
   availability, not by per-account throttling.

Default server knobs:

| Env var | Default | Purpose |
| --- | ---: | --- |
| `BLUEY_LIMIT_PROVIDER_OPENAI_LLM_PER_MIN` | 900/min, burst 180 | OpenAI chat/vision capacity |
| `BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN` | 300/min, burst 60 | Anthropic chat capacity |
| `BLUEY_LIMIT_PROVIDER_GEMINI_LLM_PER_MIN` | 600/min, burst 120 | Gemini text/vision capacity |
| `BLUEY_LIMIT_PROVIDER_DEEPSEEK_LLM_PER_MIN` | 600/min, burst 120 | DeepSeek text capacity |
| `BLUEY_LIMIT_PROVIDER_ZAI_LLM_PER_MIN` | 300/min, burst 60 | Z.AI GLM text capacity |
| `BLUEY_ANSWER_PLAN_ROUTING` | enabled | Set to `0` only for rollback; default server AnswerPlan promotes Auto requests to instant/deep/vision/research-aware behavior before provider routing |
| `BLUEY_ROUTE_POLICY` | `provider_mix` | Default rotates first attempts across configured providers. Set `quality_first` for the older static order or `cost_optimized` to prefer GLM/DeepSeek first for managed text lanes |
| `BLUEY_LIMIT_PROVIDER_OPENAI_EMBED_PER_MIN` | 900/min, burst 180 | OpenAI embedding capacity |
| `BLUEY_LIMIT_PROVIDER_DEEPGRAM_STT_PER_MIN` | 600/min, burst 120 | Deepgram STT capacity |
| `BLUEY_LIMIT_PROVIDER_OPENAI_STT_PER_MIN` | 600/min, burst 120 | OpenAI STT fallback capacity |
| `BLUEY_LIMIT_ROUTER_COMPLETE_PER_MIN` | unset/disabled | Optional emergency per-IP answer edge guardrail |
| `BLUEY_LIMIT_ROUTER_EMBED_PER_MIN` | unset/disabled | Optional emergency per-IP embed/RAG edge guardrail |
| `BLUEY_LIMIT_ROUTER_TRANSCRIBE_PER_MIN` | unset/disabled | Optional emergency per-IP chunked STT edge guardrail |
| `BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN` | unset/disabled | Optional emergency per-account answer guardrail |
| `BLUEY_LIMIT_ACCOUNT_EMBED_PER_MIN` | unset/disabled | Optional emergency per-account embeddings/RAG guardrail |
| `BLUEY_LIMIT_ACCOUNT_STT_PER_MIN` | unset/disabled | Optional emergency per-account chunked STT guardrail |

Each capacity env var also supports a `_BURST` suffix, for example
`BLUEY_LIMIT_PROVIDER_OPENAI_LLM_PER_MIN_BURST=240`.

Provider keys can be supplied either as single-key env vars (`OPENAI_API_KEY`,
`ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `DEEPSEEK_API_KEY`,
`ZAI_API_KEY`, `ZHIPU_API_KEY`, `DEEPGRAM_API_KEY`) or
as comma-separated, provider-approved key pools (`OPENAI_API_KEYS`,
`ANTHROPIC_API_KEYS`, `GEMINI_API_KEYS`, `GOOGLE_API_KEYS`,
`DEEPSEEK_API_KEYS`, `ZAI_API_KEYS`, `ZHIPU_API_KEYS`, `DEEPGRAM_API_KEYS`).
Bluey shards requests across the pool. This is for approved capacity across
projects, regions, or enterprise allocations; do not use it for provider-limit
evasion.

Set `BLUEY_REDIS_URL` in production so capacity buckets are shared across every
Bluey server instance. Optional knobs:

| Env var | Default | Purpose |
| --- | --- | --- |
| `BLUEY_REDIS_URL` | unset/local only | Enables shared capacity ledger |
| `BLUEY_REDIS_NAMESPACE` | `bluey` | Separates staging/prod Redis keys |
| `BLUEY_RATE_LIMIT_REDIS_STRICT` | `false` | If true, Redis errors deny instead of falling back locally |
| `BLUEY_PROVIDER_429_COOLDOWN_SECS` | `30` | Fallback key cooldown when provider sends 429 without Retry-After |
| `BLUEY_PROVIDER_MAX_COOLDOWN_SECS` | `300` | Caps provider-key cooldowns so one bad header cannot park capacity forever |

Without Redis, buckets are in-process and suitable only for local/dev or a
single-server alpha. With Redis, provider limits are enforced globally across
instances. The provider-key health ledger uses the same Redis namespace when
`BLUEY_REDIS_URL` is set; otherwise it falls back to a local in-process cooldown
map. Redis failures do not block realtime calls by default because the local
cooldown remains active and provider/model token buckets still protect the
server.

`/admin/metrics` exposes aggregate provider-health counters for cooldowns,
all-keys-cooling events, and Redis ledger errors. Keep these low during load
tests; a spike means Bluey needs more approved provider capacity or a route mix
change.

## Internal Developer/Offline Fallback Routing

`StaticPolicy` / `LocalFallbackPolicy` is used for local development,
BYOK-style testing, and internal offline fallback only. This is not a customer
model picker and must not appear in the paid product UI. Logged-in production
accounts use managed routing through `bluey-server`; provider keys stay on the
server.

| Lane | Provider | Model |
| --- | --- | --- |
| `instant` | OpenAI | `gpt-4o-mini` |
| `balanced` | Anthropic | `claude-3-5-sonnet-latest` |
| `deep` | Anthropic | `claude-3-7-sonnet-latest` |
| `vision` | OpenAI | `gpt-4o` |
| `local` | Ollama | `llama3.1` |

The direct Ollama provider default is `llama3.2`, but the router's explicit
local lane is currently pinned to `llama3.1`.

Direct BYOK providers and local Ollama are developer-gated by
`BLUEY_DEV_BYOK=1` in debug/dev builds only. Release binaries ignore that flag.
`BLUEY_OLLAMA_HOST` is ignored unless the dev gate is active. The managed
server intentionally returns no route candidates for the `local` lane and
`/router/complete` rejects `lane=local`.

## Speculative Routing

Bluey Auto classifies a task, picks a lane, and streams through the selected
provider. The normal product path is one stable answer card backed by true
server-side provider streaming:

```text
desktop -> bluey-server -> OpenAI / Anthropic upstream stream -> overlay
```

Parallel cheap-draft plus deep-final replacement is implemented but remains
opt-in through:

```bash
BLUEY_PARALLEL_DRAFTS=1
```

This keeps the default UX simple: one visible answer card that streams from the
selected lane. When the draft+deep experiment is enabled, the final Deep answer
replaces/refines the same card rather than appending a second random answer.

To disable the auto router path for debugging:

```bash
BLUEY_SPECULATIVE_ROUTING=0
```

## STT Routing

Customer desktop installs use Bluey managed STT through `bluey-server`.
Provider keys stay server-side.

Debug/dev builds can still exercise the older direct streaming STT factory,
which builds this ordered chain:

1. Deepgram `nova-3`
2. OpenAI Realtime `gpt-4o-mini-transcribe`
3. LocalWhisper / whisper.cpp on macOS

Environment gates:

| Provider | Key/flag |
| --- | --- |
| Deepgram | Debug/dev only: `DEEPGRAM_API_KEY` or `BLUEY_STT_API_KEY` |
| OpenAI Realtime fallback | Debug/dev only: `OPENAI_API_KEY` plus `BLUEY_STT_FALLBACK_OPENAI=1` |
| LocalWhisper fallback | Debug/dev only: `BLUEY_STT_LOCAL_WHISPER=1` |
| Force STT router wrapper | Debug/dev only: `BLUEY_STT_ROUTER=1` |
| Dev mock STT | Debug/dev only: `BLUEY_USE_MOCK_STT=1` |

Release binaries ignore the direct STT env path and route paid/live captions
through Bluey managed STT.

### VAD And Endpointing

Bluey has two speech-boundary layers:

1. **Local VAD / silence gate**: production continuous system-audio STT now
   runs a Send-safe RMS gate before `send_audio()`. It forwards speech and
   short trailing silence, then drops sustained silence to cut STT bandwidth
   and cost. The full WebRTC VAD stage remains in `cue-daemon::audio::vad`;
   its native handle is not `Send`, so it should be used from a thread-owned
   capture worker rather than a Tokio task.
2. **Provider endpointing**: Deepgram streaming URLs request
   `endpointing=300`, `utterance_end_ms=1000`, `vad_events=true`, and
   `smart_format=true` by default. Provider endpointing is the second signal
   for "speech ended" and is what lets realtime apps avoid waiting for large
   silent chunks.

Runtime tuning:

```bash
BLUEY_VAD_AGGRESSIVENESS=aggressive        # quality | low_bitrate | aggressive | very_aggressive
BLUEY_VAD_RMS_THRESHOLD=0.02
BLUEY_VAD_HANGOVER_MS=500

BLUEY_DEEPGRAM_ENDPOINTING_MS=300
BLUEY_DEEPGRAM_UTTERANCE_END_MS=1000
BLUEY_DEEPGRAM_VAD_EVENTS=1
BLUEY_DEEPGRAM_SMART_FORMAT=1
```

The default goal is "fast but not twitchy": finalization should be quick after
the user stops speaking, while quiet speech still gets through.
For VAD experiments, `BLUEY_DEEPGRAM_ENDPOINTING_MS=off` and
`BLUEY_DEEPGRAM_UTTERANCE_END_MS=off` explicitly remove those Deepgram query
parameters so the local RMS/WebRTC gate can be measured by itself.

## Model Selection Recommendation

The right customer UX is still **Auto**, not a wall of providers. Internally we
should keep multiple providers and route by task:

| Need | Best current strategy | Pros | Cons |
| --- | --- | --- | --- |
| Fast easy answer | OpenAI `gpt-5.4-mini` instant lane | Low latency with much stronger 2026-era baseline quality than the old 4o-mini route | Not ideal for deep reasoning |
| Human-like technical answer | Anthropic `claude-sonnet-4-6` balanced lane | Strong prose, coding, and reasoning style | Higher latency/cost than fast mini models |
| Deep coding/system design | Anthropic Opus lane with thinking budget, Gemini Pro/OpenAI fallback | Better multi-step structure and safer tradeoff analysis | More output/thinking tokens, costlier |
| Screen/image analysis | OpenAI `gpt-5.5` vision lane with Gemini Pro/Flash fallback | Strong current integration with text+image input and a second multimodal provider | Needs live smoke to tune quality/cost ordering |
| Internal offline fallback | Local Whisper/Ollama behind dev flags | Helps demos and outage drills | Not a customer mode; not reliable enough as primary paid experience |

Decision: **have all providers behind the router, expose Auto/Balanced/Deep as
simple UX concepts, and keep provider/model swaps server-side**. That lets us
move capacity, pricing, and quality without forcing customers to understand
provider names.

Customer UI stays **Auto** because provider and model names are an operations
control plane, not a customer workflow. The router can change candidate order,
shift traffic away from a throttled model, and update pricing/health policy
without making customers choose between vendor brands or learn which model
currently handles screenshots, quick questions, or hard reasoning. The product
can still expose simple intent controls like Auto, Balanced, and Deep; the
provider menu should stay server-owned.

### 2026-06-10 provider stance

Do not hardcode the marketing site or overlay to one provider family. The
server route table is the product control plane:

- **Keep OpenAI** for fast mini answers, accurate current vision, embeddings,
  and OpenAI STT fallback. The current managed OpenAI path is Chat
  Completions, so use Chat-compatible GPT-5.4 family models until the server
  has a Responses API path for explicit reasoning controls.
- **Keep Anthropic** for human-like technical/system-design answers and long
  structured reasoning. `claude-sonnet-4-6` is the balanced default;
  `claude-opus-4-8` owns the Deep lane when budget allows.
- **Keep Gemini as a measured server-side candidate** for cheap/fast text
  fallback, Pro-quality multimodal fallback, and provider-capacity resilience.
  It remains hidden behind Auto rather than becoming a visible customer vendor
  dropdown.
- **Keep Deepgram primary for live STT** and OpenAI Realtime/chunked
  transcription as cloud fallback. LocalWhisper stays hidden/offline/dev.
- **Do not expose Local** in paid UI. If cloud is unavailable, local fallback can
  produce a degraded answer, but billing should reconcile once online only if
  the cloud path actually ran.

Why this is still conservative:

- The server already supports provider/model failover, key pools, and health
  cooldowns. The remaining risk is product quality and cost, not just "newest
  model wins".
- Route changes must move with the pricing table, cost-label copy, and load
  tests. Unknown model names are intentionally filtered out of priced routes.
- OpenAI reasoning-era controls are best wired through a Responses-style
  managed path; the current managed OpenAI path still uses Chat Completions, so
  the route table uses GPT-5.4 models that the current endpoint supports.
- Anthropic Opus/Fable-class adaptive-thinking models may require request-shape
  changes around effort and sampling parameters. Keep Sonnet 4.6 as the safe
  Messages API route until those semantics are covered by dispatcher tests.

Recommended next implementation after the managed smoke is an admin/server-owned
route config table:

```text
lane -> ordered candidates -> pricing key -> health bucket -> feature flags
```

That lets us test "Gemini vision first", "Claude Opus/Fable for deep", or
"OpenAI newest reasoning model through Responses" without rebuilding desktop
customers.

Important scope note: the streaming STT factory covers the continuous
system-audio streaming path. The chunked REST transcription path uses
`/router/transcribe`, which now tries Deepgram first and falls back to OpenAI
`gpt-4o-mini-transcribe` if Deepgram is busy or unavailable.

LocalWhisper is a hidden reliability/dev fallback, not the primary paid STT
experience. The paid path should prefer cloud STT quality and provider failover;
local STT remains useful for offline demos, development, and graceful degradation
if the customer explicitly accepts lower accuracy.

Code references:

- `crates/cue-daemon/src/stt/factory.rs`
- `crates/cue-daemon/src/stt/deepgram.rs`
- `crates/cue-daemon/src/stt/openai.rs`
- `crates/cue-daemon/src/stt/whisper.rs`
- `server/src/routing/dispatcher.rs::transcribe`

## Embeddings And RAG

Cloud embedding is currently OpenAI `text-embedding-3-small`.

Code references:

- `server/src/routing/dispatcher.rs::embed`
- `server/src/api/router.rs::embed`

Production RAG should use cloud vector storage for cross-device/session
continuity, with a local SQLite cache for fast client writes and offline
resilience.

## Required Provider Keys

Minimum keys for managed answers, screen analysis, transcription, and RAG:

```bash
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
DEEPGRAM_API_KEY=...
BLUEY_JWT_SECRET=...
BLUEY_PUBLIC_URL=https://bluey.sh
```

Billing/live-balance keys:

```bash
STRIPE_SECRET_KEY=...
STRIPE_WEBHOOK_SECRET=...
```

Email/account keys:

```bash
BLUEY_SMTP_HOST=...
BLUEY_SMTP_USERNAME=...
BLUEY_SMTP_PASSWORD=...
BLUEY_SMTP_FROM="Bluey <hello@bluey.sh>"
```

## Not Wired Today

| Provider/system | Current status |
| --- | --- |
| Codex runtime model | Not used by Bluey runtime. Codex is the development/review agent. |
| Gemini | Not wired in managed routing. Candidate for future cheap vision/classifier fallback. |
| Groq/Cerebras | Mentioned in older strategy/reference docs, not active in the current managed route map. |

## Recommended Next Hardening

1. Add an admin-visible model-routing config endpoint so lane/model changes do
   not require a redeploy.
2. Add latency/cost telemetry by lane so we can tune Auto routing with real
   data.
3. Add Gemini only after the first managed test pass, as a measured fallback
   rather than another visible customer option.
