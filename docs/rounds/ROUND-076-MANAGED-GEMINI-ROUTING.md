# Round 076 - Managed Gemini Routing

Date: 2026-06-20
Branch: `codex/bluey-ai-site`

## Summary

Bluey managed routing now has Gemini as a real server-side provider, not a
future note. Desktop/browser clients still never receive provider API keys; all
OpenAI, Anthropic, Gemini, and Deepgram keys stay on `bluey-server`.

## What Changed

- Added Gemini text + image completion through the Gemini Generate Content API.
- Added Gemini SSE streaming through `streamGenerateContent?alt=sse`.
- Added Gemini route candidates for `instant`, `balanced`, `deep`, and `vision`.
- Added Gemini key pools:
  - `GEMINI_API_KEYS`
  - `GEMINI_API_KEY`
  - `GOOGLE_API_KEYS`
  - `GOOGLE_API_KEY`
- Added the provider capacity bucket:
  - `BLUEY_LIMIT_PROVIDER_GEMINI_LLM_PER_MIN`
  - `BLUEY_LIMIT_PROVIDER_GEMINI_LLM_PER_MIN_BURST`
- Updated pricing for stable Claude route IDs and Gemini route IDs.
- Updated model-routing, pricing, provider-capacity, and prelaunch docs.

## Managed Route Order

| Lane | Candidate order |
| --- | --- |
| `instant` | OpenAI `gpt-5.4-mini` -> Gemini `gemini-3.1-flash-lite` -> Anthropic `claude-haiku-4-5-20251001` -> Gemini `gemini-3-flash-preview` -> Anthropic `claude-sonnet-4-6` |
| `balanced` | Anthropic `claude-sonnet-4-6` -> Gemini `gemini-3.1-pro-preview` -> OpenAI `gpt-5.5` -> Gemini `gemini-3-flash-preview` -> OpenAI `gpt-5.4-mini` |
| `deep` | Anthropic `claude-opus-4-8` -> Gemini `gemini-3.1-pro-preview` -> OpenAI `gpt-5.5` -> Anthropic `claude-sonnet-4-6` -> Gemini `gemini-3-flash-preview` |
| `vision` | OpenAI `gpt-5.5` -> Gemini `gemini-3.1-pro-preview` -> Gemini `gemini-3-flash-preview` -> OpenAI `gpt-5.4-mini` |

## Pricing Snapshot

Provider price snapshot date: 2026-06-20.

- Gemini `gemini-3.1-pro-preview`: conservative high-context tier,
  $0.90/1M input tokens and $5.40/1M output tokens.
- Gemini `gemini-3-flash-preview`: $0.50/1M input tokens and $3.00/1M
  output tokens.
- Gemini `gemini-3.1-flash-lite`: $0.25/1M input tokens and $1.50/1M
  output tokens.
- Claude `claude-opus-4-8`: $5.00/1M input tokens and $25.00/1M output
  tokens.
- Claude `claude-sonnet-4-6`: $3.00/1M input tokens and $15.00/1M output
  tokens.

## Security Notes

- No Gemini key is accepted from desktop, web UI, or customer request bodies.
- Gemini keys use the same server-side pool and key-health path as OpenAI and
  Anthropic.
- Streaming Gemini responses must end with final usage metadata before Bluey
  emits `Done`; truncated streams return an upstream error instead of billing or
  caching partial output.
- Gemini image input accepts only base64 data URLs with supported image MIME
  types.

## Verification

Commands run locally:

```bash
cargo fmt --all --check
cargo test --manifest-path server/Cargo.toml --lib
cargo test --manifest-path server/Cargo.toml
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
git diff --check
```

Result:

```text
format clean
server lib tests: 140 passed
server tests: 140 lib + 33 integration/standalone tests passed
server clippy clean
diff whitespace clean
```

Still required before deployment:

```bash
funded-key live smoke against the deployed server environment
```

## Areas Most Likely Wrong

- Gemini Pro live cost may need tuning after real prompt-size telemetry. The
  current table intentionally uses the conservative high-context price.
- Gemini streaming is unit-tested with parser fixtures; live provider smoke
  still needs funded keys in the deployed server environment.
- Gemini thinking-budget controls are not mapped yet. Today Gemini receives
  temperature and output-token limits only.
