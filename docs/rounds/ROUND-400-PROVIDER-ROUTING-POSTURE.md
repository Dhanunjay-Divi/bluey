# ROUND-400-PROVIDER-ROUTING-POSTURE

Date: 2026-07-06
Branch: `main`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Explain why live Bluey answers appear to route mostly to OpenAI/Claude, why
image/screen work leans on Gemini/OpenAI, and why DeepSeek/Z.AI GLM show up less
often than expected.

## Production Posture Checked

Checked only non-secret production environment posture on the droplet.

- `BLUEY_ROUTE_POLICY=provider_mix`
- `BLUEY_ANSWER_PLAN_ROUTING=1`
- OpenAI, Anthropic, Gemini, Z.AI, and DeepSeek provider pools are configured.
- No provider secret values were printed.

## Current Routing Behavior

`provider_mix` is the production default. It rotates the first candidate by
request id inside a lane, then falls back when a provider key is missing, cooling
down, over capacity, or fails.

Text route candidates:

- `instant`: OpenAI mini, DeepSeek flash, Gemini lite, Anthropic fast, Z.AI GLM
- `balanced`: Anthropic Sonnet, DeepSeek flash, Z.AI GLM, Gemini Pro, OpenAI
- `deep`: Anthropic Opus, Z.AI GLM, DeepSeek Pro, Gemini Pro, OpenAI

Vision route candidates:

- Gemini Flash
- OpenAI flagship
- Gemini Pro
- OpenAI mini

DeepSeek and Z.AI GLM are not used for image/screen requests in the current
managed route because the implemented vision lane only uses image-capable
OpenAI/Gemini routes.

## Why OpenAI Shows Up Often

OpenAI can be selected for three reasons:

1. The request id rotation puts OpenAI first for that lane.
2. A previous candidate is cooling down or over capacity.
3. The request is vision/screen related, where the safe image-capable pool is
   OpenAI/Gemini.

Recent production logs showed Gemini selected first for some requests, then
cooling down after HTTP 429, after which Bluey fell back to OpenAI. That makes
the final usage row show OpenAI even though the original route was not OpenAI.

## Why GLM/DeepSeek Show Up Less

They are configured and present in the candidate lists, but production is not
using `cost_optimized`. In `provider_mix`, they are peers in the rotation, not
hard-preferred first providers.

Observed live eval behavior:

- Z.AI GLM handled several text prompts directly.
- OpenAI handled prompts where it was first by rotation or where Gemini fell
  back after cooldown.
- DeepSeek did not appear in the small eval sample, which can happen with
  request-id rotation and a small number of requests.

## Options

Keep current behavior:

- Best for quality and avoiding single-provider 429 hot spots.
- Uses GLM/DeepSeek, but does not force them first.

Switch production to `cost_optimized`:

- Text lanes prefer Z.AI/DeepSeek first.
- Vision still stays on OpenAI/Gemini.
- Better margin, but should be monitored for answer quality and latency.

Build a weighted policy:

- More precise than simple rotation.
- Example: balanced text could be 35% GLM, 30% DeepSeek, 20% Claude, 10% OpenAI,
  5% Gemini, with automatic cooldown fallback.
- This is likely the best long-term product control because it can tune cost,
  latency, and quality without a binary all-or-nothing policy.

## Recommendation

Do not assume all traffic is going to OpenAI. The final provider can be OpenAI
because Gemini or another provider was tried first and cooled down. For clearer
owner visibility, add a provider-routing dashboard/report that shows:

- planned lane
- first candidate
- final provider
- fallback reason
- first token latency
- total latency
- customer cost
- Bluey estimated upstream cost

For routing itself, the best next step is a weighted provider policy for text
lanes, while keeping vision on OpenAI/Gemini until additional image-capable
routes are implemented.
