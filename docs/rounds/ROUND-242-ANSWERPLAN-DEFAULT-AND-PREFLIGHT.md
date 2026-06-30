# Round 242 - AnswerPlan Default And Preflight

## Trigger

The owner asked whether these needed to be manually set:

```bash
BLUEY_ANSWER_PLAN_ROUTING=1
BLUEY_ROUTE_POLICY=cost_optimized
```

Then asked for the senior-engineering version: make the setup better so the
server is easy to test without hidden assumptions, while keeping safe rollback.

## Decision

AnswerPlan should be a product default because it fixes intent quality:

- code prompts should route as code and request actual code
- behavioral prompts should not become system design
- screen prompts should stay vision
- public unknown lookups should enter research/web-search planning when enabled

Cost-optimized provider order should remain an operator-controlled route policy
because it changes first-provider quality/economics. The safe default stays
`provider_mix`, which already rotates across configured Claude/OpenAI/Gemini/
GLM/DeepSeek candidates to avoid 429 hot spots.

## Fix

Changed server behavior:

- `BLUEY_ANSWER_PLAN_ROUTING` is now default-on.
- Set `BLUEY_ANSWER_PLAN_ROUTING=0`, `false`, `no`, or `off` only for rollback.
- `BLUEY_ROUTE_POLICY` still defaults to `provider_mix`.
- `BLUEY_ROUTE_POLICY=cost_optimized` remains available for owner canary/testing.

Updated deploy/operator visibility:

- `ops/bluey-api.env.example` now explicitly includes:
  - `BLUEY_ANSWER_PLAN_ROUTING=1`
  - `BLUEY_ROUTE_POLICY=provider_mix`
  - commented `ZAI_API_KEYS` and `DEEPSEEK_API_KEYS` slots
- `scripts/bluey-cloud-preflight.sh` now prints routing posture:
  - confirms AnswerPlan is enabled/default-on
  - warns if AnswerPlan is disabled
  - normalizes and validates `BLUEY_ROUTE_POLICY`
  - fails if `cost_optimized` is selected without any Z.AI or DeepSeek key pool
- `scripts/bluey-scalable-readiness.sh` now reports Z.AI and DeepSeek provider
  env readiness as optional route capacity.
- deploy docs now explain:
  - AnswerPlan default-on with env rollback
  - `provider_mix` as the safe default
  - `cost_optimized` only after configuring Z.AI/DeepSeek keys

## Verification

Passed:

```bash
cargo fmt --manifest-path server/Cargo.toml
bash -n scripts/bluey-cloud-preflight.sh
bash -n scripts/bluey-scalable-readiness.sh
cargo test --manifest-path server/Cargo.toml api::router::tests -- --nocapture
cargo check --manifest-path server/Cargo.toml
```

Pending final pre-commit hygiene:

```bash
git diff --check
```

## Current State

For normal deployed Bluey:

```bash
# Optional explicit posture; server behaves this way even if unset.
BLUEY_ANSWER_PLAN_ROUTING=1

# Safe default.
BLUEY_ROUTE_POLICY=provider_mix
```

For owner testing of GLM/DeepSeek-first routing:

```bash
BLUEY_ROUTE_POLICY=cost_optimized
ZAI_API_KEYS=...
DEEPSEEK_API_KEYS=...
```

Mac and Windows parity: this is server-side managed routing/preflight, so both
clients get the same behavior once they talk to the same deployed server.

## Compaction

The compaction continuity model remains:

- numbered round docs for every work round
- this handoff doc updated after each round
- current branch and backup thread id preserved in the handoff
- latest round number advanced so the next continuation starts with the correct
  source of truth instead of old chat memory

## Remaining Gates

- Deploy the new server binary to staging/droplet.
- Run `scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env`.
- If testing cost optimization, set `BLUEY_ROUTE_POLICY=cost_optimized` only
  after configuring at least one of `ZAI_API_KEYS` or `DEEPSEEK_API_KEYS`.
- Live-smoke the AnswerPlan prompts from Round 241.
