# Bluey Model Routing

Last updated: 2026-05-23

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

Code references:

- `server/src/routing/dispatcher.rs::resolve_route`
- `crates/cue-router/src/policy.rs::ManagedPolicy`
- `crates/cue-llm/src/bluey_managed.rs`

## Local/Developer Fallback Routing

`StaticPolicy` / `LocalFallbackPolicy` is used for local development,
BYOK-style testing, and offline fallback. Production customer accounts should
prefer managed routing.

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
fallback.

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
`/router/transcribe`, which currently supports Deepgram only. A production
server-side fallback from Deepgram to OpenAI transcription is the next clean
STT hardening task.

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
| Server-side OpenAI STT fallback | Not wired into `/router/transcribe` yet. Daemon streaming fallback exists. |

## Recommended Next Hardening

1. Add OpenAI transcription fallback to server `/router/transcribe`.
2. Add an admin-visible model-routing config endpoint so lane/model changes do
   not require a redeploy.
3. Add latency/cost telemetry by lane so we can tune Auto routing with real
   data.
4. Add Gemini only after the first managed test pass, as a measured fallback
   rather than another visible customer option.
