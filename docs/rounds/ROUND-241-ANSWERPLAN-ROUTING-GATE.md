# Round 241 - AnswerPlan Routing Gate

## Trigger

The owner approved adding AnswerPlan as the brain before managed provider
routing. The concrete pain points were:

- "Tell me about yourself" could look like system design in the canvas.
- Coding prompts could return vague explanation without full code.
- Follow-up coding prompts such as "I want the code in Python" needed to stay
  code/artifact-oriented.
- Bare public lookups such as "secret passage ranch" should be eligible for
  managed web-search planning when no saved context exists.
- The owner asked whether `BLUEY_ANSWER_PLAN_ROUTING=1` and
  `BLUEY_ROUTE_POLICY=cost_optimized` were still available.

## Fix

Added a server-side `BLUEY_ANSWER_PLAN_ROUTING=1` gate in
`server/src/api/router.rs`.

The planner is deterministic local rules first. It does not call a classifier
model and does not answer the user by itself. It classifies the request, decides
evidence needs, chooses an output shape, and can promote the requested managed
lane before the existing provider dispatcher runs.

New/expanded intents:

- `coding`
- `coding_followup`
- `behavioral`
- `system_design`
- `screen`
- `research`
- `missing_context`
- `writing`
- `meeting`
- `quick`
- `general`

The lane mapping is:

- quick -> `instant`
- coding / coding follow-up / system design -> `deep`
- screen/image -> `vision`
- behavioral / research / meeting / writing / missing context / general ->
  `balanced`

Provider routing remains separate. After AnswerPlan picks a lane, the existing
dispatcher still applies `BLUEY_ROUTE_POLICY`:

- unset or `provider_mix`: rotated provider mix across configured top-tier
  Claude/OpenAI/Gemini/GLM/DeepSeek routes
- `quality_first`: older static quality-first order
- `cost_optimized`: GLM/DeepSeek-first text order for owner-controlled smoke
  and margin testing

## Prompt Style

`prompt_with_answer_plan` now passes structured intent/output/lane guidance to
the selected model:

- code requests should include actual fenced code, not just explanation
- code follow-ups should produce the smallest useful delta while still showing
  code when asked
- behavioral prompts should answer like a polished interview response and never
  become system design
- unrelated new questions should not drag old canvas/session context forward
- research answers should cite managed web sources when available
- missing-context answers should name the missing item once and give a concrete
  next step

## Web Search Planning

The planner now treats bare public lookup phrases such as `secret passage
ranch` as research when no saved context, screen, code, behavioral, or system
design signal applies. Managed web search is still guarded by the existing
server-side search provider configuration, query sanitization, repeated-query
guard, trial limits, credit checks, and separate web-search usage rows.

## Verification

Passed:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml api::router::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml routing::dispatcher::tests -- --nocapture
cargo check --manifest-path server/Cargo.toml
rg -n "<redacted-pasted-key-fragments>" server docs scripts --glob '!target'
```

The secret-fragment scan returned no matches.

New router tests cover:

- self-intro routes as `behavioral`, not `system_design`
- LRU/code requests route to `deep` with `code_artifact`
- "I want the code in Python" routes as `coding_followup`
- system design routes to `deep` with `canvas_detail`
- bare public lookup phrases route as `research`
- AnswerPlan routing can override Auto/balanced when enabled
- vision requests remain `vision`

## Current State

`BLUEY_ROUTE_POLICY=cost_optimized` already existed and still works.

`BLUEY_ANSWER_PLAN_ROUTING=1` is now implemented as the server-side gate for
AnswerPlan lane promotion.

Mac and Windows parity: this change is server-side managed routing, so both
desktop clients benefit once they call the same deployed `bluey-server`.

## Remaining Gates

- Deploy with `BLUEY_ANSWER_PLAN_ROUTING=1` in staging first.
- Smoke these prompts live:
  - "Tell me about yourself"
  - "Build me LRU cache in Python"
  - "I want the code in Python"
  - "Design a scalable notification system"
  - "secret passage ranch"
  - a screenshot/screen question
- Compare selected lanes/provider logs against expected AnswerPlan logs.
- Only then enable broadly in production.
