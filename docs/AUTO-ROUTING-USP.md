# Bluey Auto Routing — the USP

> Fastest useful answer first. Smarter answer when needed. Cost-aware managed routing.

## The pitch

Every other AI overlay forces the user to pick a model up-front. Bluey Auto picks for them — and picks **twice** when the question is hard:

- A small, fast, cheap model produces a streaming **draft** in <1s.
- A strong model produces a refined **final** answer when the question warrants it.
- The user sees the draft immediately and the final replaces it in-place when ready.

This is the product USP. It does not require new model capabilities — it stitches existing providers (`anthropic`, `openai`, `ollama`) into a routing layer that classifies first, dispatches second.

## Why it matters

| Problem | Today | With Bluey Auto |
|---|---|---|
| User picks a model | "Should I use GPT-4o-mini or Claude 3.7?" | Bluey decides per question. |
| Cheap model on a hard question | Wrong answer, fast. | Wrong draft, then auto-refined to the right answer. |
| Expensive model on a trivial question | Right answer, slow + costly. | Cheap fast answer; no escalation. |
| Local-only mode | All-or-nothing toggle. | Same classifier, lane forced to `Local`. |
| Vision input | User remembers to switch models. | Auto-detected; vision-capable provider always wins. |

## Architecture

```
User input
   │
   ▼
ClassifierInput { prompt, has_transcript, has_page, file_attachment_count, has_screenshot, local_only }
   │
   ▼
TaskClassifier (LayeredClassifier: heuristic → tiny-model on low confidence)
   │
   ▼
TaskClassification { task_type, difficulty, needed_context, latency_lane, confidence }
   │
   ▼
RoutingPolicy::route()  (StaticPolicy: fixed lane → provider mapping)
   │
   ▼
ProviderRoute { lane, provider_name, model, max_tokens, temperature, stream }
   │
   ▼
SpeculativeRouter (optional): if Deep, also fire Instant in parallel
   │
   ▼
cue_llm::LlmProvider (anthropic / openai / ollama)
```

### Models

- **`TaskType`** — `general` | `code` | `system_design` | `meeting` | `writing` | `vision`
- **`Difficulty`** — `easy` | `medium` | `hard`
- **`ContextNeeds`** — flags for transcript / page / files / screenshot / memory
- **`LatencyLane`** — `instant` | `balanced` | `deep`
- **Confidence** — `f32` in `[0.0, 1.0]`

### Lanes → providers (StaticPolicy defaults)

| Lane | Provider | Model | Max tokens | Stream | Notes |
|---|---|---|---|---|---|
| `instant` | OpenAI | `gpt-4o-mini` | 512 | yes | First-token latency optimised |
| `balanced` | Anthropic | `claude-3-5-sonnet-latest` | 2048 | yes | Default |
| `deep` | Anthropic | `claude-3-7-sonnet-latest` | 8192 | no | Quality + budget |
| `vision` | OpenAI | `gpt-4o` | 2048 | yes | Multimodal |
| `local` | Ollama | `llama3.1` | 2048 | yes | Privacy / offline fallback |

`StaticPolicy::local_only()` flips `force_local: true`; routing then always returns the Local lane regardless of classification.

## Heuristic classifier — what it sees

| Signal | Source | Weight |
|---|---|---|
| Lexical task-type keywords | prompt text | high (multiple hits = high confidence) |
| Code fences (` ``` `) | prompt text | +2 hits to `Code` |
| Length: <60 chars | prompt text | bumps `Easy` |
| Length: >500 chars | prompt text | bumps `Hard` |
| Screenshot attached | input | overrides task type → `Vision` |
| Active transcript | input | adds `transcript` to context, +1 to `Meeting` keyword count |
| Page captured | input | adds `page` to context |
| File attachments | input | adds `files` to context |

**Confidence scoring:**

- 0.95 — Vision by attachment (unambiguous).
- 0.90 — Top keyword bucket has ≥3 hits, ≥2 ahead of runner-up.
- 0.75 — Top bucket has ≥2 hits.
- 0.55 — Some signal but weak.
- 0.40 — No keyword hits at all.

Below `escalation_threshold` (default 0.6), the `LayeredClassifier` defers to the tiny-model classifier if one is configured.

## Speculative mode

Only fires when the policy returns the `Deep` lane AND `speculative_when_deep: true`. The router emits two streams concurrently:

```
Caller stream:
   t=0:    SpeculativeChunk::Draft { text: "Hello",  finished: false }     ← from Instant lane
   t=200ms SpeculativeChunk::Draft { text: " world", finished: true  }     ← Instant done
   t=2.4s  SpeculativeChunk::Final { text: "<full deep answer>" }          ← Deep done; replace card
```

The dashboard / overlay treats `Final` as a card-body replacement (the existing `OverlayCommand::UpdateCard` already supports this). If the Deep lane errors, the user keeps the draft.

**Cost guardrail:** the Instant lane is configured to the cheapest streaming option, so a wasted parallel call is bounded to a few cents per question even at scale.

## Tests (current)

`crates/cue-router/`:

- 8 heuristic tests — easy/hard/code/design/vision/meeting/writing/follow-up routing, plus empty + ambiguous prompts.
- 4 policy tests — Instant/Deep lane mapping, Vision override, force-local.
- 3 speculative router tests — single-lane streaming, draft+final speculation, Deep without speculation.
- 2 follow-up tests — transcript context pickup on vague prompts, local-only doesn't alter classification.

**17 tests, 0 failures.**

## What's next

| Item | Status |
|---|---|
| Local heuristic classifier | ✅ shipped (R13.x) |
| `RoutingPolicy` + `StaticPolicy` | ✅ shipped |
| `SpeculativeRouter` | ✅ shipped (structural; needs daemon wiring) |
| Tiny-model classifier (managed) | 📋 stub trait, no implementation yet — needs Bluey routing endpoint |
| Daemon integration via `request_cue` (Auto Router classifier + speculative dispatch on Hard) | ✅ shipped |
| Dashboard: surface lane choice + draft→final replacement visually | 📋 next round |
| Cost telemetry + per-lane spend caps | 📋 future round |

## Why not just always use the deep model?

- 10× cost.
- 5× latency.
- The user perceives Bluey as slow.
- The Hard questions are the minority of asks.

Auto routing keeps Bluey "fast for 80% of questions, smart for the 20% that need it" without making the user choose.


## Post-decision update (2026-05-19): no-BYOK

The 2026-05-19 product decision (`DECISIONS.md`) makes Bluey
**managed-only**. The Auto Router classifier and policy stay exactly
the same; what changes is the dispatcher:

- v0.1 BYOK code path (`StaticPolicy::defaults` + `OpenAiProvider`
  reading `OPENAI_API_KEY`) is **dev-mode only**.
- v0.2 production default is `ManagedPolicy` + `BlueyManagedProvider`
  which dispatches through `bluey-server`. Bluey owns the keys; the
  customer pays Bluey.
- Local Ollama / whisper.cpp stay as the **offline / privacy fallback**.
  The classifier and the `SpeculativeRouter` work identically against
  fallback providers; only the underlying transport changes.

**This is exactly the plug-point design described above** — the
classifier is decoupled from the dispatcher precisely so the BYOK →
managed swap is a config change, not a rewrite.
