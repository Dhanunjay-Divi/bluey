# Round 233 - Flagship Model Routes

Date: 2026-06-29 13:05 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked whether Bluey can add latest flagship models such as GLM-5.2
and DeepSeek.

## Sources Checked

- Z.AI pricing/model docs: `https://docs.z.ai/guides/overview/pricing`
- Z.AI GLM-5.2 docs: `https://docs.z.ai/guides/llm/glm-5.2`
- DeepSeek pricing docs: `https://api-docs.deepseek.com/quick_start/pricing`
- DeepSeek chat completion docs: `https://api-docs.deepseek.com/api/create-chat-completion`

## Fix

- Added managed server key-pool slots:
  - `DEEPSEEK_API_KEYS` / `DEEPSEEK_API_KEY`
  - `ZAI_API_KEYS` / `ZAI_API_KEY`
  - `ZHIPU_API_KEYS` / `ZHIPU_API_KEY` as GLM/Z.AI aliases
- Added DeepSeek and Z.AI as first-class managed provider names instead of
  routing them through `openai`.
- Added OpenAI-compatible chat dispatch for:
  - DeepSeek `deepseek-v4-pro`
  - DeepSeek `deepseek-v4-flash`
  - Z.AI `glm-5.2`
- Added provider-specific endpoints:
  - DeepSeek: `https://api.deepseek.com/chat/completions`
  - Z.AI: `https://api.z.ai/api/paas/v4/chat/completions`
- Added provider-specific test URL overrides:
  - `BLUEY_TEST_DEEPSEEK_URL`
  - `BLUEY_TEST_ZAI_URL`
- Added route candidates:
  - `instant`: DeepSeek `deepseek-v4-flash` after OpenAI fast
  - `balanced`: DeepSeek `deepseek-v4-flash`, then Z.AI `glm-5.2`
  - `deep`: Z.AI `glm-5.2`, then DeepSeek `deepseek-v4-pro`, plus DeepSeek
    flash as a later fallback
  - `vision`: unchanged because these new routes are text/chat routes here
- Added conservative cache-miss pricing rows:
  - Z.AI `glm-5.2`: `$1.40/1M` input, `$4.40/1M` output, `150%` markup
  - DeepSeek `deepseek-v4-pro`: `$0.435/1M` input, `$0.87/1M` output,
    `150%` markup
  - DeepSeek `deepseek-v4-flash`: `$0.14/1M` input, `$0.28/1M` output,
    `200%` markup
- Added dedicated provider capacity buckets:
  - `BLUEY_LIMIT_PROVIDER_DEEPSEEK_LLM_PER_MIN`
  - `BLUEY_LIMIT_PROVIDER_ZAI_LLM_PER_MIN`
- DeepSeek/Z.AI thinking behavior:
  - non-deep lanes send `thinking: {"type":"disabled"}` to avoid hidden latency
    and spend
  - deep lane can enable provider reasoning
  - reasoning text is not surfaced in the overlay
- If an OpenAI-compatible stream omits final usage despite requesting it, Bluey
  now uses the server's input estimate plus a conservative output character
  estimate instead of charging zero.

## Security / Abuse Notes

- New providers use the same managed server-side dispatch path as other paid
  LLM calls, so desktop clients do not receive provider API keys.
- Missing DeepSeek/Z.AI keys are skipped as unconfigured routes; they do not
  become customer-facing errors while other candidates remain available.
- Pricing rows are required by tests, so an unpriced paid route cannot silently
  enter managed routing.
- Provider capacity is isolated by provider/model and can be backed by Redis in
  production like the existing OpenAI/Anthropic/Gemini buckets.
- Account credit balance, reserve-before-dispatch, provider health cooldowns,
  and idempotency still wrap these routes.

## Mac / Windows Parity

This is a server-side managed-routing change. macOS and Windows both reach it
through the existing Bluey managed provider path; no native overlay parity code
was needed.

## Verification

Passed:

- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo fmt --all`
- `cargo check --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml pricing -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml routing::dispatcher -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml rate_limit -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml config::tests -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml`
  - `179` unit tests passed
  - `41` integration tests passed
  - doc-tests passed

## Remaining QA / Gates

- Configure real production/staging keys before expecting these routes to run:
  - `DEEPSEEK_API_KEY` or `DEEPSEEK_API_KEYS`
  - `ZAI_API_KEY` or `ZAI_API_KEYS`
- Run live smoke tests for:
  - `instant` with DeepSeek flash available
  - `balanced` fallback with DeepSeek/Z.AI keys available
  - `deep` with Z.AI GLM-5.2 and DeepSeek V4 Pro available
- Check provider dashboards after smoke tests to confirm token usage and billed
  cost match Bluey's usage ledger.
- Future improvement: add cache-hit/cache-miss token accounting so DeepSeek/Z.AI
  cached input can be billed more precisely instead of using conservative
  cache-miss pricing.
