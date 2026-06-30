# Round 239 - Provider Mix Anti 429 Routing

Date: 2026-06-29 19:57 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked Bluey to use all configured flagship providers as needed, with
an initial mix of answers across Claude, OpenAI, Gemini, GLM, and DeepSeek so a
burst does not keep hitting one provider and trigger avoidable 429s.

## Root Cause

Round 238 added a `cost_optimized` policy, but normal managed answer routing was
still effectively static unless an operator set a server env var. That meant the
first candidate for each lane could still concentrate traffic on one upstream:

- `balanced` would usually start on Anthropic Sonnet
- `deep` would usually start on Anthropic Opus
- `vision` would usually start on OpenAI
- GLM/DeepSeek/Gemini would be used later unless the route was explicitly changed
  or an earlier provider was unavailable

Fallbacks and provider health cooldowns already existed, but they only helped
after the first route was skipped, over capacity, or failed.

## Fix

Added default `provider_mix` routing:

- Managed answer requests now pass `request_id` into route selection.
- The dispatcher deterministically rotates the top-tier candidate list by that
  request id.
- Text lanes can now start across Anthropic, DeepSeek, Gemini, OpenAI, and Z.AI
  GLM when those keys are configured.
- Vision rotates only across image-capable OpenAI/Gemini routes.
- Fixed fallback tails remain in place so weaker fallbacks do not become the
  first choice for hard work.
- `quality_first` remains available for the old static order:

```bash
BLUEY_ROUTE_POLICY=quality_first
```

- `cost_optimized` remains available for owner-controlled GLM/DeepSeek-first
  smoke or margin tuning:

```bash
BLUEY_ROUTE_POLICY=cost_optimized
```

The API layer now logs the resolved first route at debug level with request id,
lane, first provider/model, and candidate count. It does not log prompt text.

## 429 And Abuse Behavior

This change does not remove existing safety rails. It works alongside:

- provider/model rate buckets
- provider/model/key health cooldowns after 429/529 or Retry-After
- key-pool sharding for approved provider capacity
- balance and upstream-spend guards before dispatch
- route fallback when a key pool is cooling down

The goal is not provider-limit evasion. The goal is to avoid self-inflicted
hot-spotting by spreading legitimate first attempts across the providers Bluey
already has configured.

## Files Changed

- `server/src/routing/dispatcher.rs`
  - added `RoutePolicy::ProviderMix`
  - made provider mix the default policy
  - added request-seeded candidate rotation
  - added tests for text-provider spread and vision safety
- `server/src/api/router.rs`
  - passes `request_id` into priced route resolution for streaming and
    non-streaming answers
  - logs first resolved LLM route candidate at debug level
- `docs/MODEL-ROUTING.md`
  - documents provider mix as the default
  - documents `quality_first` and `cost_optimized` overrides
- `docs/PRICING-MODEL.md`
  - clarifies GLM/DeepSeek participation under provider mix

## Verification

Passed:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml routing::dispatcher::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml api::router::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml pricing -- --nocapture
cargo check --manifest-path server/Cargo.toml
git diff --check
```

Also scanned the changed server/docs tree for the previously pasted provider key
fragments and found no matches.

## Current State

Provider mix is code-ready locally. Once deployed with rotated provider keys,
normal managed text answers should naturally spread first attempts across the
configured providers while still falling back on capacity errors.

## Remaining QA And Gates

- Rotate the provider keys that were pasted in chat before any production deploy.
- Configure real server secrets only through env/secret manager, never repo docs.
- Run live staging smoke tests with trace ids for:
  - instant text
  - balanced text
  - deep/coding text
  - vision/screen
- Confirm usage rows show a reasonable provider spread across request ids.
- Watch provider-health logs for any repeated 429/cooldown loops after deploy.
