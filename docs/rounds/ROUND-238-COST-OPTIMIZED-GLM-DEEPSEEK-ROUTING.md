# Round 238 - Cost Optimized GLM DeepSeek Routing

Date: 2026-06-29 19:38 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked why Bluey was not using GLM-5.2 and DeepSeek more often even
though they are cheaper than the current first-choice Anthropic/OpenAI text
routes.

## Root Cause

GLM-5.2 and DeepSeek were wired and priced, but the production route order was
quality-first:

- `balanced` started with Anthropic Sonnet
- `deep` started with Anthropic Opus
- GLM-5.2 and DeepSeek were mostly later fallback candidates

So Bluey would only use GLM/DeepSeek when an earlier provider was unavailable,
over capacity, or skipped by the API layer. The code was doing what the old
route table asked it to do; it just was not cost-optimized.

## Fix

Added an explicit server route policy:

```bash
BLUEY_ROUTE_POLICY=cost_optimized
```

`BLUEY_ROUTE_ORDER=cost_optimized` is also accepted as an alias.

Default behavior remains `quality_first` so production quality does not change
silently. When the cost policy is enabled:

- `instant` starts with DeepSeek Flash
- `balanced` starts with Z.AI GLM-5.2, then DeepSeek Flash
- `deep` starts with Z.AI GLM-5.2, then DeepSeek V4 Pro
- `vision` stays unchanged on OpenAI/Gemini because GLM/DeepSeek are only wired
  as text/chat routes in this codebase

The cost-optimized route still keeps Anthropic/OpenAI/Gemini as fallbacks, so a
provider outage or quality rollback does not strand user requests.

## Security Note

The previously pasted provider keys must be rotated before production. They
were not copied into repo docs. Rotated values should live only in server
environment/secrets:

```bash
ZAI_API_KEY=...
DEEPSEEK_API_KEY=...
```

or pooled:

```bash
ZAI_API_KEYS=...
DEEPSEEK_API_KEYS=...
```

## Docs Updated

- `docs/MODEL-ROUTING.md`
  - documented default `quality_first`
  - documented optional `cost_optimized`
  - added `BLUEY_ROUTE_POLICY`
- `docs/PRICING-MODEL.md`
  - clarified GLM/DeepSeek are first-choice candidates under cost policy, not
    only passive fallbacks

## Verification

Passed:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml routing::dispatcher::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml pricing -- --nocapture
cargo check --manifest-path server/Cargo.toml
git diff --check
```

## Current State

The code is ready for server-side smoke testing. To test cheaper text routing
on staging or a controlled production canary:

```bash
BLUEY_ROUTE_POLICY=cost_optimized
ZAI_API_KEYS=<rotated-pool>
DEEPSEEK_API_KEYS=<rotated-pool>
```

Then inspect usage rows for:

- `provider=zai`, `model=glm-5.2` on balanced/deep text answers
- `provider=deepseek`, `model=deepseek-v4-flash` on instant/simple text answers
- no GLM/DeepSeek for `vision` image/screen asks

## Remaining QA And Gates

- Rotate the exposed provider keys before any live deploy.
- Run live smoke tests for:
  - instant text
  - balanced text
  - deep/coding text
  - vision/screen
- Compare quality and latency against the quality-first route on a small owner
  eval set.
- If quality is acceptable, enable cost policy for a canary percentage or owner
  accounts first.
- Longer-term: add the richer `AnswerPlan` classifier so Bluey can choose cheap
  or flagship models by task, not only by broad lane.
