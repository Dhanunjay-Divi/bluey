# Bluey Model Routing

Last updated: 2026-05-29

This document is the source-of-truth snapshot for the model/provider routing
currently implemented in the Bluey codebase. It is intentionally operational:
it says what runs today, which keys are required, and which provider ideas are
not wired yet.

## Managed LLM Routing

When a user is logged in, the desktop talks to `bluey-server` through
`BlueyManagedProvider`. The server owns provider API keys and maps lanes to
actual upstream models.

| Bluey lane | Provider | Model | Primary use |
| --- | --- | --- | --- |
| `instant` | OpenAI | `gpt-4o-mini` | Easy questions, quick answers, optional cheap draft |
| `balanced` | Anthropic | `claude-3-5-sonnet-latest` | Default technical/general answer |
| `deep` | Anthropic | `claude-3-7-sonnet-latest` | Hard coding, system design, long reasoning |
| `vision` | OpenAI | `gpt-4o` | Analyse Screen, screenshots, image context |

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

- Anthropic `claude-3-7-sonnet-latest` maps to `thinking:
  {"type":"enabled","budget_tokens":...}` and reserves enough
  `max_tokens` for both thinking and visible answer text.
- OpenAI managed routes still use Chat Completions in this codebase, so
  `reasoning_effort` is accepted but not sent upstream yet. Switching OpenAI
  managed routes to the Responses API is the right future hook for explicit
  OpenAI reasoning controls.
- Gemini thinking controls are not wired today because Gemini is not in the
  managed route table yet.

This gives us the operational knob the user asked for without making every
easy question slower or more expensive.

The server now resolves each lane to an ordered candidate list, not a single
hard dependency. If the preferred provider is unavailable, over quota, or
temporarily busy, Bluey tries the next candidate before returning an error.

| Lane | Candidate order |
| --- | --- |
| `instant` | OpenAI `gpt-4o-mini` -> Anthropic `claude-3-5-sonnet-latest` |
| `balanced` | Anthropic `claude-3-5-sonnet-latest` -> OpenAI `gpt-4o-mini` |
| `deep` | Anthropic `claude-3-7-sonnet-latest` -> OpenAI `gpt-4o` -> Anthropic `claude-3-5-sonnet-latest` |
| `vision` | OpenAI `gpt-4o` |

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
2. **Provider/model buckets**: keeps OpenAI, Anthropic, Deepgram, and embedding
   calls inside configured capacity and lets LLM lanes fall back before failing.
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
`ANTHROPIC_API_KEY`, `DEEPGRAM_API_KEY`) or as comma-separated, provider-approved
key pools (`OPENAI_API_KEYS`, `ANTHROPIC_API_KEYS`, `DEEPGRAM_API_KEYS`). Bluey
shards requests across the pool. This is for approved capacity across projects,
regions, or enterprise allocations; do not use it for provider-limit evasion.

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
BYOK-style testing, and emergency offline fallback. This is not a customer
model picker and should not appear in the paid product UI. Logged-in production
accounts use managed routing through `bluey-server`; local fallback is selected
inside the daemon before a request reaches the cloud.

| Lane | Provider | Model |
| --- | --- | --- |
| `instant` | OpenAI | `gpt-4o-mini` |
| `balanced` | Anthropic | `claude-3-5-sonnet-latest` |
| `deep` | Anthropic | `claude-3-7-sonnet-latest` |
| `vision` | OpenAI | `gpt-4o` |
| `local` | Ollama | `llama3.1` |

The direct Ollama provider default is `llama3.2`, but the router's explicit
local lane is currently pinned to `llama3.1`.

Direct BYOK providers are developer-gated by `BLUEY_DEV_BYOK=1` once managed
tokens exist. Ollama can still be enabled through `BLUEY_OLLAMA_HOST` for local
fallback. The managed server intentionally returns no route candidates for the
`local` lane and `/router/complete` rejects `lane=local`.

## Speculative Routing

Bluey Auto classifies a task, picks a lane, and streams through the selected
provider. Parallel cheap-draft plus deep-final replacement is available but
currently opt-in through:

```bash
BLUEY_PARALLEL_DRAFTS=1
```

This keeps the default UX simple: one visible answer card that may stream and
be updated in place, not random appended drafts.

To disable the auto router path for debugging:

```bash
BLUEY_SPECULATIVE_ROUTING=0
```

## STT Routing

The streaming STT factory builds this ordered chain:

1. Deepgram `nova-3`
2. OpenAI Realtime `gpt-4o-mini-transcribe`
3. LocalWhisper / whisper.cpp on macOS

Environment gates:

| Provider | Key/flag |
| --- | --- |
| Deepgram | `DEEPGRAM_API_KEY` or `BLUEY_STT_API_KEY` |
| OpenAI Realtime fallback | `OPENAI_API_KEY` plus `BLUEY_STT_FALLBACK_OPENAI=1` |
| LocalWhisper fallback | `BLUEY_STT_LOCAL_WHISPER=1` |
| Force STT router wrapper | `BLUEY_STT_ROUTER=1` |
| Dev mock STT | `BLUEY_USE_MOCK_STT=1` |

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
| Fast easy answer | OpenAI fast mini lane | Lowest latency, good first-token speed | Not ideal for deep reasoning |
| Human-like technical answer | Anthropic Sonnet lane | Strong prose and reasoning style | Higher latency/cost than mini models |
| Deep coding/system design | Anthropic deep lane with thinking budget | Better multi-step structure, safer tradeoff analysis | More output/thinking tokens, costlier |
| Screen/image analysis | OpenAI vision lane today; evaluate Gemini vision later | Simple current integration | Gemini may be cheaper/better for some image workloads but is not wired |
| Offline fallback | Local Whisper/Ollama hidden fallback | Helps demos and outage resilience | Not reliable enough as primary paid experience |

Decision: **have all providers behind the router, expose Auto/Balanced/Deep as
simple UX concepts, and keep provider/model swaps server-side**. That lets us
move capacity, pricing, and quality without forcing customers to understand
provider names.

### 2026-05-29 provider stance

Do not hardcode the marketing site or overlay to one provider family. The
server route table is the product control plane:

- **Keep OpenAI** for fast mini answers, embeddings, current vision, and OpenAI
  STT fallback.
- **Keep Anthropic** for human-like technical/system-design answers and long
  structured reasoning.
- **Add Gemini only as a measured server-side candidate** after managed smoke:
  likely first for vision and cheap/fast multimodal fallback, not as a visible
  customer dropdown item.
- **Keep Deepgram primary for live STT** and OpenAI Realtime/chunked
  transcription as cloud fallback. LocalWhisper stays hidden/offline/dev.
- **Do not expose Local** in paid UI. If cloud is unavailable, local fallback can
  produce a degraded answer, but billing should reconcile once online only if
  the cloud path actually ran.

Why we are not switching every model name in code immediately:

- The server already supports provider/model failover, key pools, and health
  cooldowns. The remaining risk is product quality and cost, not just "newest
  model wins".
- Route changes must move with the pricing table, cost-label copy, and load
  tests. Unknown model names are intentionally filtered out of priced routes.
- OpenAI reasoning-era models are best wired through a Responses-style managed
  path; the current managed OpenAI path still uses Chat Completions.
- Claude newer-than-3.7 routes may need different thinking semantics than the
  current manual `thinking` payload. Keep the current safe route until the API
  request shape is verified by tests.

Recommended next implementation after the managed smoke is an admin/server-owned
route config table:

```text
lane -> ordered candidates -> pricing key -> health bucket -> feature flags
```

That lets us test "Gemini vision first", "Claude newest Sonnet for balanced",
or "OpenAI newest reasoning model for deep" without rebuilding desktop
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
