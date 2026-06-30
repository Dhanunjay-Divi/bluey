# Round 247 - AnswerPlan AI Fallback

## Trigger

The owner approved the next routing step: keep deterministic AnswerPlan rules
for obvious cases, but add a tiny AI classifier fallback when local rules are
unsure. The goal is a more human-feeling Bluey route decision without making
every answer slower or moving provider routing into the desktop.

Backup thread id remains `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause

Round 241 made AnswerPlan routing default-on, but it was fully rule-based. That
fixed clear failures such as self-intro prompts routing as system design and
code prompts missing the code lane, but ambiguous phrases could still inherit a
too-generic local rule.

The owner asked whether a classifier should be local model or AI call. The
decision for this round is hybrid:

- rules first for speed, privacy, and deterministic hard overrides
- tiny managed AI fallback only for low-confidence or mixed-signal cases
- provider routing still remains separate underneath AnswerPlan

## Fix

- Added `resolve_answer_plan_for_request()` before managed route selection.
- Added an optional default-on fallback controlled by:
  - `BLUEY_ANSWER_PLAN_AI_FALLBACK=0` to disable it
  - `BLUEY_ANSWER_PLAN_AI_CONFIDENCE_THRESHOLD`
  - `BLUEY_ANSWER_PLAN_AI_TIMEOUT_MS`
  - `BLUEY_ANSWER_PLAN_AI_MAX_TOKENS`
- The fallback uses the managed `instant` lane with a small JSON-only routing
  prompt and a short timeout.
- The classifier prompt includes only:
  - the user question
  - requested lane
  - image count
  - local-rule intent/output/lane/confidence
- The classifier prompt excludes:
  - RAG chunks
  - attached document text
  - screenshots
  - private prompts
  - provider secrets
- Added hard overrides so an AI fallback cannot turn:
  - `tell me about yourself` into system design
  - code requests into vague compact answers
  - image/screen requests into text-only routes
- Invalid JSON, sensitive-looking text, unsafe lane/output combinations, local
  lane requests, vision requests, and direct image requests fall back to local
  rules.
- Added tracing fields on managed requests:
  - `answer_plan_source`
  - `answer_plan_ai_attempted`
  - `answer_plan_ai_reason`
- Recorded classifier usage as kind `answer_plan_classifier` with customer cost
  `0`, so owner cost is auditable without charging users for hidden planning.
- Updated deploy docs, env example, and cloud preflight output.

## Verification

Passed:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml answer_plan_ -- --nocapture
cargo test --manifest-path server/Cargo.toml api::router::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml routing::dispatcher::tests -- --nocapture
cargo check --manifest-path server/Cargo.toml
```

Focused tests now cover:

- ambiguous low-confidence requests are eligible for AI fallback
- obvious code requests skip the AI fallback
- `BLUEY_ANSWER_PLAN_AI_FALLBACK=0` disables only the fallback
- fenced JSON classifier output parses
- behavioral hard overrides beat unsafe classifier suggestions
- generic ambiguous text can refine to research/web-search planning

## Current State

Managed answer flow is now:

1. Server receives the overlay request.
2. Local AnswerPlan rules classify intent, evidence, output, and lane.
3. If rules are confident, Bluey routes immediately.
4. If rules are low-confidence or mixed-signal, the tiny AI classifier may
   refine the plan.
5. Provider routing then applies `provider_mix`, `quality_first`, or
   `cost_optimized` to the chosen lane.
6. Streaming, cooldown fallback, web-search planning, canvas behavior, and
   billing continue through existing server paths.

This benefits Mac and Windows together because the behavior is server-side.

## Remaining QA/Gates

- Deploy to staging before production.
- Run cloud preflight and confirm the AI fallback status is visible.
- Live-smoke ambiguous prompts and confirm logs show:
  - `answer_plan_source=rules` for obvious prompts
  - `answer_plan_source=ai_refined` only for truly ambiguous prompts
  - no customer charge rows for `answer_plan_classifier`
- Watch latency and owner cost for fallback calls; disable with
  `BLUEY_ANSWER_PLAN_AI_FALLBACK=0` if the first canary shows regressions.
