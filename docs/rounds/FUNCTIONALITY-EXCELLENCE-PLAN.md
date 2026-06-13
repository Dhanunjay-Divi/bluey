# Functionality Excellence Plan — Fast, Smooth, Reliable

> Goal: make the Bluey end-to-end experience fast, smooth, and reliable.
> Author: Kiro
> Date: 2026-06-13
> Status: proposal — pick a track to start.

## Parked for now (operator/compliance gates)

These remain tracked as the open `[ ]` items in
`docs/PRELAUNCH-CHECKLIST.md` (42 open / 90 done). They are deliberately
parked while we focus on functionality:

- GDPR export/delete verification (CLI-driven; can resume anytime)
- DNS/DNSSEC, Square production+sandbox, SMTP live, monitoring/alerting,
  legal pages, clean-Mac smoke.

The single operator action that unblocks the most functionality work is
**funding the OpenAI + Anthropic accounts** (checklist item ~90: deployed
keys report insufficient quota). Until that lands, end-to-end latency
cannot be measured and the Mac smoke steps 4-10 cannot complete.

---

## What I actually read (hot path)

`crates/cue-dashboard/src/commands.rs::request_cue` — the question → answer
entry point. Sequence before the first token:

1. `AppPaths::discover()` + `MeetingStore::new()` + `store.load_active()`
   (local FS/JSON, synchronous, re-done per request)
2. `build_llm_provider_from_env(&db)` + `ProviderRegistry::from_env_and_secrets(&db)`
   (two DB/secret reads)
3. `classify_for_router()` + `classify_only_for_router()` — two heuristic
   classifications on the same `recent` text. **Both are cheap in-process
   heuristics (microseconds)** — redundant but NOT a latency contributor.
4. `recent` = last ~10 transcript segments joined (bounded — good for TTFB)
5. `try_speculative_dispatch(...)` → `/router/complete/stream`
6. Server side: RAG enrichment (`f829d61`) runs before the upstream call,
   so RAG retrieval latency is on the server's pre-stream critical path.

The latency-critical path is therefore: local setup (fast, warm) →
network to server → **server RAG enrichment** → network to upstream →
**upstream TTFB** → stream back → render. The two dominant unknowns are
server RAG retrieval time and upstream TTFB.

---

## Track A — Fast (latency). MEASURE FIRST. Gated on funded keys.

Optimizing latency without measurement is guessing. The harness already
exists (`scripts/bluey-latency-eval.py`, thresholds for health /
first_event / first_token / complete).

1. **Baseline.** Once provider keys are funded, run the eval harness against
   staging to get real p50/p95 for first_token + complete, per lane.
2. **Per-lane telemetry** (codex MODEL-ROUTING "next hardening" #2). Attribute
   the pre-first-token time into: routing, RAG retrieval, daemon→server,
   server→upstream, upstream TTFB. You cannot fix what you cannot attribute.
3. **Attack the biggest contributor with data.** Likely candidates, to be
   confirmed by measurement:
   - server-side RAG retrieval blocking the upstream call (see B1)
   - upstream model choice for the fast lane (gpt-5.4-mini is already the
     instant lane)
   - cold-start / connection reuse to upstreams

## Track B — Smooth + Reliable. CODE-LEVEL. Doable NOW (no keys needed).

These are logic/architecture improvements verifiable by reading + tests,
not by live measurement.

### B1 — RAG must never delay the first token (highest value)

Confirm the server-side RAG enrichment runs with a tight timeout and/or
concurrently with the idempotency reserve, so a slow or empty RAG store
never adds latency before the upstream stream starts. If RAG retrieval is
currently awaited inline with no deadline, add a short budget (e.g. 150-300ms)
after which the answer proceeds without RAG context. **Investigate
`server/src/api/router.rs` + the cloud RAG path first.**

### B2 — First-token deadline → fallback (highest reliability value)

A provider that is slow-but-not-erroring is worse for UX than one that
errors fast: the customer just waits. Today the fallback chain
(OpenAI→Anthropic; Deepgram→OpenAI→Whisper) triggers on hard errors. Verify
whether there is a first-token DEADLINE that trips fallback when an upstream
accepts the request but stalls before the first token. If not, add one.
This is the single biggest "feels reliable" win and is reviewable now.

### B3 — Warm the per-request setup

Minor smoothness on rapid repeated asks: the meeting store + provider
registry are re-opened per `request_cue`. Cache/reuse them. Collapse the
double classify call. Small, safe, measurable-by-microbench.

---

## Recommendation

Start **B2 (first-token deadline → fallback)** now — it's the highest-
leverage reliability+perceived-speed item, it's doable without funded keys
(pure control-flow review + a deadline + a test), and it directly serves
"smooth and reliable." Then **B1** (RAG never blocks first token). Track A
(measured latency tuning) begins the moment the provider accounts are funded.

Each item ships as its own pipeline-gated commit with a round handoff +
cross-review, same contract as before.
