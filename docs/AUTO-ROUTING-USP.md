# Bluey Auto Routing — the USP

> Fastest useful answer first. Smarter answer when needed. Cost-aware managed routing.

## The pitch

Every other AI overlay forces the user to pick a model up-front. Bluey Auto picks for them — and picks **twice** when the question is hard:

- A small, fast, cheap model produces a streaming **draft** in <1s.
- A strong model produces a refined **final** answer when the question warrants it.
- The user sees the draft immediately and the final replaces it in-place when ready.

This is the product USP. It does not require the user to pick a model — it
stitches managed providers (`anthropic`, `openai`) into a routing layer that
classifies first, dispatches second, and keeps daemon-only local fallback out
of the customer-facing model picker.

## Why it matters

| Problem | Today | With Bluey Auto |
|---|---|---|
| User picks a model | "Should I use a fast model, Sonnet, Opus, or vision?" | Bluey decides per question. |
| Cheap model on a hard question | Wrong answer, fast. | Wrong draft, then auto-refined to the right answer. |
| Expensive model on a trivial question | Right answer, slow + costly. | Cheap fast answer; no escalation. |
| Local-only mode | All-or-nothing toggle. | Daemon-only fallback, not a managed customer lane. |
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

### Lanes -> providers (managed defaults, refreshed 2026-06-20)

| Lane | Primary | Fallbacks | Max tokens | Stream | Notes |
|---|---|---|---|---|---|
| `instant` | OpenAI `gpt-5.4-mini` | Gemini Flash-Lite, Claude Haiku, Gemini Flash, Claude Sonnet | 512 | yes | First-token latency and cost optimized |
| `balanced` | Anthropic `claude-sonnet-4-6` | Gemini Pro, OpenAI `gpt-5.5`, Gemini Flash, OpenAI mini | 2048 | yes | Default technical/general answer lane |
| `deep` | Anthropic `claude-opus-4-8` | Gemini Pro, OpenAI `gpt-5.5`, Claude Sonnet, Gemini Flash | 8192 | yes | Hard coding, architecture, and reasoning-heavy answers |
| `vision` | OpenAI `gpt-5.5` | Gemini Pro, Gemini Flash, OpenAI mini | 2048 | yes | Screen analysis / screenshot / multimodal context |
| `local` | Daemon only | not a managed cloud route | 2048 | yes | Offline/dev fallback before the server |

`StaticPolicy::local_only()` remains available for daemon/dev fallback. Managed
server routing returns no provider candidates for `local`, so paid customer
requests never dispatch to a hidden desktop model.

Gemini is server-side only, just like OpenAI and Anthropic. Customers still see
simple intent controls, not provider names or local keys.
- Gemini 3.5 Pro / Flash / Flash-Lite: attractive for multimodal/cost
  diversity, but Bluey does not yet have a Gemini server dispatcher, key pool,
  pricing row, or billing tests. Add as a separate provider-integration round.
- Deepgram Flux: likely a better live-conversation STT path than chunked
  Nova-3, but it belongs with the desktop→server→Deepgram WebSocket relay
  rather than this LLM routing refresh.

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

Normal product mode uses `speculative_when_deep: false`, so even Deep questions
stream as one visible answer card from the selected lane. Parallel draft mode
is retained for internal latency experiments only.

When the policy returns the `Deep` lane AND `speculative_when_deep: true`, the
router emits two streams concurrently:

```
Caller stream:
   t=0:    SpeculativeChunk::Draft { text: "Hello",  finished: false }     ← from Instant lane
   t=200ms SpeculativeChunk::Draft { text: " world", finished: true  }     ← Instant done
   t=2.4s  SpeculativeChunk::Final { text: "<full deep answer>" }          ← Deep done; replace card
```

The dashboard / overlay can treat `Final` as a card-body replacement. This is
not the default customer UX because replacement can feel like the answer is
changing under the user while they are speaking.

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
- Local Ollama / whisper.cpp stay as **developer-only fallback tooling** behind
  explicit dev flags that release binaries ignore. They are not customer modes,
  and the paid product routes cloud answers through `bluey-server`.

**This is exactly the plug-point design described above** — the
classifier is decoupled from the dispatcher precisely so the BYOK →
managed swap is a config change, not a rewrite.
