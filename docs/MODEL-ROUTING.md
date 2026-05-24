# Bluey Model Routing

Last updated: 2026-05-24

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
3. **Optional per-account emergency guardrails**: disabled by default. Turn
   them on only during abuse incidents, stolen-token response, or runaway-client
   mitigation. Normal paid usage is controlled by wallet balance and provider
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

Without Redis, buckets are in-process and suitable only for local/dev or a
single-server alpha. With Redis, provider limits are enforced globally across
instances. The next capacity step is provider-key health scoring in the same
shared ledger so every server avoids unhealthy keys.

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
BLUEY_SMTP_FROM="Bluey <no-reply@bluey.sh>"
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
