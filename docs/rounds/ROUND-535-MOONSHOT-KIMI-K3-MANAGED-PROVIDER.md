# Round 535 - Moonshot Kimi K3 managed provider

Date: 2026-07-19

Status: implementation and local provider-contract verification complete; funded
Moonshot staging smoke and deployment remain explicit release gates

## Objective

Add Kimi K3 to Bluey's managed model APIs without exposing provider keys in the
desktop, portal, or browser and without introducing any operating-system
credential-store or Keychain behavior.

Kimi is an upstream model provider. It is separate from optional context
optimization systems such as Headroom and from browser-grounding systems such as
OmniParser.

## Implemented contract

Bluey registers the provider internally as `moonshot` and the exact model as
`kimi-k3`. The server calls `https://api.moonshot.ai/v1/chat/completions` with a
Bearer key from `MOONSHOT_API_KEYS` / `MOONSHOT_API_KEY`; `KIMI_API_KEYS` /
`KIMI_API_KEY` are supported aliases.

K3 has a provider-specific request contract:

- always send top-level `reasoning_effort: "max"`;
- never send the older K2.x `thinking` object;
- omit fixed `temperature` and sampling fields;
- use `max_completion_tokens` for the output limit;
- use base64 data URLs for image input; and
- request streaming usage while accepting usage in both documented stream
  placements.

Provider `reasoning_content` is treated as private progress. It can keep the
bounded first-output preflight alive, but it does not select the provider before
visible text. A provider error, disconnect, empty completion, or timeout before
visible text therefore remains eligible for fallback. Reasoning is never emitted
in the Bluey SSE response, persisted as the answer, or shown to the customer.
Only final `content` is released.

K3 also receives a provider-specific minimum total completion budget: 4,096
tokens reserved for mandatory reasoning plus at least 1,024 tokens for visible
output. Pre-dispatch cost reservation prices each configured fallback with its
own provider-aware ceiling: Kimi receives that 5,120-token floor while OpenAI,
Gemini, and other candidates retain their own limits. The selected route's
ceiling also drives the truncation guard.

## Routing and safety posture

Kimi K3 is a late fallback for `deep` and `vision` only. It is not placed in the
rotating `instant` or `balanced` top tiers because K3 always performs maximum
reasoning and should first be measured for real latency, quality, and cost.

The route participates in the existing missing-key skip, per-key health and
cooldown ledger, fallback loop, spend guard, usage ledger, and idempotency
boundary. Moonshot also has an isolated default capacity bucket of 120 requests
per minute with burst 30.

Billing uses the official cache-miss list price until Bluey persists provider
cache-hit token splits: $3 per million input tokens and $15 per million output
tokens, with the existing deep-model 150% customer markup. The lower $0.30 per
million cached-input price is documented but not assumed during reservation or
metering.

## Operations and secret handling

Deployment examples and preflight now recognize the optional Moonshot key pool.
Missing keys produce a truthful warning and skip Kimi routes; they do not make
the existing provider set fail readiness. Release-artifact secret scanning
recognizes all Moonshot and Kimi key env aliases.

No frontend settings, desktop BYOK provider, Keychain access, or customer-visible
provider selector was added. Provider ownership stays on `bluey-server`.

## Verification

Local automated verification covers:

- exact request serialization for K3;
- route ordering and pricing completeness;
- fixed reasoning and temperature behavior;
- private reasoning-stream activity and both usage shapes;
- isolated Moonshot capacity limiting;
- managed non-streaming completion through a Moonshot HTTP mock; and
- managed SSE completion where private reasoning is withheld and only final
  content plus billing are released.

The complete server library suite passed 493 tests, and the complete HTTP
integration suite passed 77 tests. `cargo clippy --all-targets -- -D warnings`,
Rust formatting, shell syntax checks, the release-artifact scanner self-test,
Python compilation, and `git diff --check` also passed.

A real Moonshot API call was not executed because this environment has no funded
Moonshot/Kimi key. Production activation must wait for a funded staging canary of
deep text and base64-image vision, followed by latency, content, usage, and cost
reconciliation. This round does not deploy or change production flags.

## Evidence and sources

Primary code:

- `server/src/config.rs`
- `server/src/routing/dispatcher.rs`
- `server/src/api/router.rs`
- `server/src/pricing/mod.rs`
- `server/src/rate_limit.rs`
- `server/tests/integration_e2e.rs`
- `ops/bluey-api.env.example`
- `scripts/bluey-cloud-preflight.sh`
- `scripts/bluey-scalable-readiness.sh`
- `scripts/check-release-artifact-contents.py`

Official provider contract:

- <https://platform.kimi.ai/docs/guide/kimi-k3-quickstart>
- <https://platform.kimi.ai/docs/api/chat>
- <https://platform.kimi.ai/docs/guide/utilize-the-streaming-output-feature-of-kimi-api>
- <https://platform.kimi.ai/docs/pricing/chat-k3>

## Release handoff

1. Add a funded staging-only `MOONSHOT_API_KEYS` value.
2. Validate account access to `kimi-k3` and execute one bounded deep call.
3. Execute one base64-image vision call; public image URLs are unsupported.
4. Confirm no `reasoning_content` appears in the customer stream or logs.
5. Reconcile provider-reported input/output usage and Bluey billing.
6. Compare first-final-content latency and answer quality with current deep and
   vision routes.
7. Keep Kimi late in fallback order until the canary evidence justifies a
   routing promotion.
