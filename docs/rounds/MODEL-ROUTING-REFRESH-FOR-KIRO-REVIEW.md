# Model Routing Refresh for Kiro Review

Date: 2026-06-17
Branch: `codex/bluey-ai-site`

## Intent

Refresh Bluey's managed model defaults so customer-facing lanes use the
current best fit per task, while preserving the managed-only desktop contract:

`overlay / CLI -> daemon -> bluey-server -> provider`

The desktop still sends Bluey lane names (`instant`, `balanced`, `deep`,
`vision`) rather than provider API keys or raw provider decisions. The server
owns provider choice, pricing, capacity, billing, and fallback order.

## Source Snapshot

Provider docs checked on 2026-06-17:

- OpenAI pricing: `https://openai.com/api/pricing/`
- Anthropic model overview/pricing: `https://docs.anthropic.com/en/docs/about-claude/models/overview`
- Gemini model overview: `https://ai.google.dev/gemini-api/docs/models`
- Deepgram model overview: `https://developers.deepgram.com/docs/models-languages-overview`

## New Managed Defaults

| Lane | Primary | Fallbacks | Why |
|---|---|---|---|
| `instant` | OpenAI `gpt-5.4-mini` | Claude Haiku 4.5, Claude Sonnet 4.6 | fastest useful first response without burning Opus/GPT-5.5 budget |
| `balanced` | Claude Sonnet 4.6 (`claude-sonnet-4-6-20260115`) | OpenAI `gpt-5.5`, OpenAI `gpt-5.4-mini` | strong technical/general default with cross-provider fallback |
| `deep` | Claude Opus 4.8 (`claude-opus-4-8-20260225`) | OpenAI `gpt-5.5`, Claude Sonnet 4.6, Claude Haiku 4.5 | hard coding, architecture, and reasoning-heavy answers |
| `vision` | OpenAI `gpt-5.5` | OpenAI `gpt-5.4-mini` | current flagship multimodal lane, cheaper OpenAI fallback |
| `local` | no managed provider | none | daemon-only offline/dev fallback; not a customer cloud route |

## What Changed

- `server/src/routing/dispatcher.rs`
  - `OPENAI_ACCURATE_MODEL` now points to `gpt-5.5`.
  - Anthropic Sonnet now uses the exact dated ID
    `claude-sonnet-4-6-20260115`.
  - Added `ANTHROPIC_DEEP_MODEL = claude-opus-4-8-20260225`.
  - Deep lane now starts with Opus 4.8.
  - Balanced and Instant gained additional priced fallback candidates.
  - Manual Anthropic thinking support recognizes Opus 4.8; Fable 5 remains
    unsupported because Anthropic documents no extended thinking support.

- `server/src/pricing/mod.rs`
  - Reconciled date updated to 2026-06-17.
  - Added OpenAI `gpt-5.5` pricing at $5/M input and $30/M output.
  - Added Anthropic `claude-opus-4-8-20260225` pricing at $15/M input and
    $75/M output.
  - Updated Sonnet pricing key to `claude-sonnet-4-6-20260115`.
  - Updated unit tests for route/pricing consistency and new deep-lane costs.

- `docs/PRICING-MODEL.md`
  - Updated provider snapshot date and cost table.
  - Updated Deep examples to Opus 4.8 and Vision examples to GPT-5.5.

- `docs/AUTO-ROUTING-USP.md`
  - Updated stale GPT-4o / Claude 3 lane table to current managed defaults.
  - Clarified that local is daemon-only, not a managed cloud lane.

## Evaluated But Not Enabled

- Anthropic `claude-fable-5-20260609`
  - Best raw Claude capability in the current model list, but higher cost than
    Sonnet and no extended thinking support. Keep for a future "max accuracy"
    lane instead of using it as the default Deep lane.

- Gemini 3.5 Pro / Flash / Flash-Lite
  - Promising for multimodal and cost diversity, but Bluey currently has no
    Gemini server dispatcher, provider-key pool, pricing rows, billing tests,
    or capacity-health integration. This should be a separate provider
    integration round rather than a silent model-table change.

- Deepgram Flux
  - Likely better for live conversational STT latency, but the right product
    path is desktop -> bluey-server WebSocket -> Deepgram WebSocket -> overlay.
    Current chunked `/router/transcribe` keeps Nova-3 until that relay lands.

## Verification

Run locally before handoff:

```bash
cargo fmt --all --check
cargo test --manifest-path server/Cargo.toml routing::dispatcher
cargo test --manifest-path server/Cargo.toml pricing
cargo test --manifest-path server/Cargo.toml --lib
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
git diff --check
```

## Reviewer Ask

Please verify:

- Every `resolve_route_candidates()` model has a pricing row.
- `gpt-5.5` uses GPT-5-compatible token limit fields.
- Opus 4.8 supports Anthropic thinking budgets in the current API path.
- Deep lane cost increase is acceptable for "best answer" behavior.
- Gemini remains intentionally deferred until dispatcher/pricing/billing
  support exists.
