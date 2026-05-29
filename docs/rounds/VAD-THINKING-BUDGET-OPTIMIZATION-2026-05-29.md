# VAD + Thinking Budget Optimization — 2026-05-29

## Why This Round Exists

The user asked whether Bluey already has VAD, whether thinking budgets can be
set across providers, and how realtime assistant apps optimize latency/cost.

The short answer before this round:

- Bluey had a strong two-stage VAD module, but the continuous system-audio STT
  path was not applying any local silence gate before `send_audio()`.
- Deepgram streaming was configured for Nova-3 basics, but not for the
  realtime endpointing knobs that shorten utterance-finalization.
- Managed LLM routing had no request/server field for reasoning effort or
  thinking-token budget.

## External Research Snapshot

Official provider docs confirm the knobs we need:

- Deepgram streaming supports endpointing/VAD concepts and documents
  `endpointing`, `utterance_end_ms`, `vad_events`, and `interim_results` for
  realtime utterance boundaries.
- Anthropic Messages supports extended thinking with
  `thinking: {"type":"enabled","budget_tokens": ...}` on supported Claude
  models; newer Claude families may prefer adaptive thinking, so Bluey only
  applies manual budgets to known-supported models today.
- Gemini exposes `thinkingConfig` / thinking budget controls, but Gemini is not
  wired into Bluey's managed route table yet.
- OpenAI exposes reasoning controls on newer reasoning/Responses-style APIs,
  but this server's managed OpenAI path still uses Chat Completions, so we
  accept the neutral fields without sending unsupported JSON upstream.

## Code Changes

### 1. Continuous System-Audio VAD Gate

Files:

- `crates/cue-daemon/src/audio/vad.rs`
- `crates/cue-daemon/src/app.rs`

Changes:

- Added `config_from_env()` for runtime VAD tuning.
- Added safe parsing for aggressiveness, RMS threshold, hangover frames, and
  hangover milliseconds.
- Continuous system-audio STT now runs a Send-safe `RmsGate` before
  `provider.send_audio()`.
- Sustained silence is dropped; speech and short trailing silence are still
  forwarded so providers can finalize utterances.

Important implementation detail:

- `webrtc-vad`'s native handle is not `Send`, so it cannot live inside the
  Tokio `spawn` task that owns the system-audio STT select loop. The full
  `TwoStageVad` remains correct for thread-owned capture workers. The async
  streaming path now uses local RMS gating plus provider endpointing.

Runtime knobs:

```bash
BLUEY_VAD_AGGRESSIVENESS=aggressive
BLUEY_VAD_RMS_THRESHOLD=0.02
BLUEY_VAD_HANGOVER_MS=500
```

### 2. Deepgram Realtime Endpointing

Files:

- `crates/cue-daemon/src/stt/deepgram.rs`
- `crates/cue-daemon/src/stt/factory.rs`

Changes:

- `DeepgramConfig` now supports:
  - `smart_format`
  - `endpointing_ms`
  - `utterance_end_ms`
  - `vad_events`
- Default streaming URL now includes:
  - `smart_format=true`
  - `endpointing=300`
  - `utterance_end_ms=1000`
  - `vad_events=true`
- Factory reads env overrides:

```bash
BLUEY_DEEPGRAM_MODEL=nova-3
BLUEY_DEEPGRAM_LANGUAGE=en-US
BLUEY_DEEPGRAM_ENDPOINTING_MS=300
BLUEY_DEEPGRAM_UTTERANCE_END_MS=1000
BLUEY_DEEPGRAM_VAD_EVENTS=1
BLUEY_DEEPGRAM_SMART_FORMAT=1
```

### 3. Managed Thinking Budget

Files:

- `server/src/routing/dispatcher.rs`
- `server/src/api/router.rs`
- `crates/cue-cloud-client/src/types.rs`
- `crates/cue-llm/src/lib.rs`
- `crates/cue-llm/src/bluey_managed.rs`
- downstream `LlmRequest` constructors

Changes:

- Added request fields:
  - `reasoning_effort`
  - `thinking_budget_tokens`
- Added server-side `ThinkingMode` and `ThinkingBudget`.
- Added lane defaults:
  - Instant: off
  - Balanced: off
  - Deep: medium / 4096 thinking tokens
  - Vision: off
- Added env overrides:

```bash
BLUEY_THINKING_DEEP_EFFORT=high
BLUEY_THINKING_DEEP_TOKENS=8192
```

- Anthropic `claude-3-7-sonnet-latest` now receives extended-thinking JSON
  when the resolved budget is enabled.
- Server entry-cost estimates now reserve output-token room for thinking
  tokens so deep requests are not under-estimated.

## What This Does Not Do Yet

- It does not move OpenAI managed calls from Chat Completions to Responses.
  That is the next clean step before OpenAI reasoning knobs can be sent
  upstream.
- It does not add Gemini routing. Gemini remains a candidate for future
  vision/classifier lanes.
- It does not apply WebRTC VAD inside the current Tokio system-audio task
  because the native handle is not `Send`.
- It does not change the customer UI. Customers should still see Auto and
  simple effort labels, not provider internals.

## Review Checklist For Kiro / Next Agent

- Confirm `cargo clippy --all-targets -- -D warnings` stays clean.
- Confirm Deepgram URL tests include endpointing fields.
- Confirm deep managed route requests reserve enough max output budget for
  Anthropic thinking.
- Verify a live Deepgram stream with `BLUEY_DEEPGRAM_ENDPOINTING_MS=300` and
  no local RMS clipping on quiet speech.
- If adding OpenAI reasoning, switch the managed OpenAI route to the Responses
  API first rather than sending unsupported fields to Chat Completions.

## What To Tell The Next Agent

Read this file and `docs/MODEL-ROUTING.md` first. Bluey now has:

- Local RMS silence gating for continuous system-audio STT.
- Deepgram realtime endpointing knobs.
- Provider-neutral managed thinking-budget fields.
- Anthropic 3.7 manual thinking budget for the Deep lane.

Next best optimization round: OpenAI Responses API support for reasoning
models, Gemini vision/classifier evaluation behind the same router abstraction,
and telemetry-driven default tuning after real call data exists.
