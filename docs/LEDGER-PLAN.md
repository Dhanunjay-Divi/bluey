# Decisions Ledger — Implementation Plan

> Every N transcript turns, a **stateless, cheap-lane** LLM call extracts the meeting's
> decisions / constraints / owners into a running ledger. That ledger is rendered as a
> **pinned context block** prepended to every real answer — so the agent always knows what
> was decided, regardless of how far back it scrolled.

---

## 0. Design principles (locked from discussion)

1. **No second bundled model.** The ledger reuses whatever LLM backend Bluey already has.
   No candle / mistral.rs / gguf. (Granite testing was exploratory only; nothing shipped.)
2. **Temporary / throwaway session.** The extraction call must NOT read or append to the
   user's answer conversation. This is *already the default* — daemon LLM calls are
   stateless (`provider_messages()` builds a fresh `[system, user]` every call; no history
   store exists anywhere). The ledger simply never writes its result back to a session.
3. **Cheap model lane, don't waste credits.** Force the **Instant** lane
   (`gpt-4o-mini` BYOK / `bluey-managed-instant` managed / local Ollama). ~200-token output,
   fires every N turns (minutes apart). In local mode it costs nothing.
4. **Anti-hallucination is non-negotiable.** Every extracted item carries a verbatim `quote`;
   we drop any item whose quote is not a literal substring of the transcript window.
5. **Graceful degradation.** Follow whatever cheap lane is configured. If no provider is
   configured at all, the ledger is simply disabled (heuristics already run as the floor).

---

## 1. The seams (verified against the code)

| Seam | Location | Role |
|---|---|---|
| `%N` trigger | `cue-core/src/intelligence.rs:57` (`meeting.transcript.len() % 5 == 4`) | where we hook the ledger tick |
| One-shot LLM call | `cue-daemon/src/app.rs:7292` `call_chat_provider(config, payload, None)` | stateless HTTP call, reused as-is |
| Payload (all fields pub) | `cue-core/src/ai.rs:912` `ProviderRequestPayload` | build directly — cheap `model`, our prompt |
| Provider config | `cue-daemon/src/app.rs:7710` `provider_client_config(&ProviderSelector)` | resolves endpoint + key for a lane |
| Cheap default model | `app.rs:7804` OpenAi→`gpt-4.1-mini`; `policy.rs:44` Instant→`gpt-4o-mini` | the cheap lane |
| Answer context | `cue-core/src/ai.rs` `AnswerContext { kind, content, title, source }` | how the pinned block is injected |

**Key fact:** `ProviderRequestPayload` has all-public fields, so I can construct a bare
extraction payload without going through `from_request` (which is coupled to `AnswerRequest`
+ session metadata). That keeps the ledger call fully decoupled from the answer path.

---

## 2. Architecture

```
transcript loop
   │  every N turns (N ≈ 15, env BLUEY_LEDGER_INTERVAL_TURNS)
   ▼
ledger_tick(daemon)                          ← background task, NOT on audio path
   │  window = last N turns w/ speaker labels ("Speaker 2: …")
   ▼
build ProviderRequestPayload
   │    model  = cheap lane (gpt-4o-mini / bluey-managed-instant / local)
   │    system = EXTRACTION_PROMPT (verbatim-quote contract)
   │    context= [ AnswerContext { transcript window } ]
   │    stream = false, max_output_tokens = 512
   │    NO session id, NO meeting write
   ▼
call_chat_provider(cfg, payload, None)       ← EXISTING fn, unchanged
   ▼
raw JSON text
   ▼
verify_harness(json, window):                ← PURE LOGIC, unit-testable, no model
   │  1. parse JSON (repair: on parse fail, one bounded retry, else drop pass)
   │  2. for each item: quote MUST be a literal substring of window → else DROP
   │  3. speaker MUST be a label present in the window → else null it
   │  4. dedup vs running ledger (normalized text match)
   ▼
LedgerState (on Daemon)                       ← accumulates; capped (e.g. 40 items, LRU)
   │
   ▼  rendered as AnswerContext(kind=Context, title="Meeting ledger")
prepend to the NEXT real answer's context    ← at resolve_answer_route context assembly
```

---

## 3. Files to add / change

### New
- `crates/cue-core/src/ledger.rs` — pure types + harness (no I/O, fully tested):
  - `LedgerItem { kind: Decision|Constraint|Owner, text, quote, speaker: Option<String> }`
  - `LedgerState { items: Vec<LedgerItem>, cap: usize }` + `merge()`, `render() -> String`
  - `parse_and_verify(raw: &str, window: &str) -> Vec<LedgerItem>` (the harness)
  - `EXTRACTION_PROMPT: &str` (the verbatim-quote contract)
  - unit tests: quote-not-substring dropped; speaker-not-present nulled; dedup; JSON repair

### Changed
- `crates/cue-daemon/src/ledger.rs` (new daemon module) — orchestration:
  - `enabled()` (env `BLUEY_LEDGER=1`, default **off** until validated live)
  - `interval_turns()` (env `BLUEY_LEDGER_INTERVAL_TURNS`, default 15)
  - `cheap_provider() -> ProviderSelector` (Instant lane, follows configured mode)
  - `async fn ledger_tick(daemon)` → builds payload, calls `call_chat_provider`,
    runs `parse_and_verify`, merges into `daemon.ledger`
- `crates/cue-daemon/src/app.rs`:
  - `Daemon` gets `pub(crate) ledger: Arc<Mutex<LedgerState>>`
  - transcript loop: every N turns, `tokio::spawn(ledger_tick(...))` (off the hot path)
  - answer context assembly (`resolve_answer_route`): prepend `daemon.ledger.render()`
    as an `AnswerContext` when non-empty
- `crates/cue-daemon/src/lib.rs`: `pub mod ledger;`
- `CHANGELOG.md`: entry under `[Unreleased]`

**No new crate, no new dependency, no bundled model, no daemon refactor.**

---

## 4. The extraction prompt (anti-hallucination contract)

Same contract proven in testing (0 fabrications): FIRST copy the exact transcript sentence
that proves the item (`quote`), THEN the normalized fields. Output ONLY JSON:

```json
{
  "decisions":   [{"quote":"…","text":"…","speaker":"Speaker 2"}],
  "constraints": [{"quote":"…","text":"…","speaker":"Speaker 3"}],
  "owners":      [{"quote":"…","owner":"…","task":"…"}]
}
```
Rules: quote verbatim; speaker must be one who actually said it; extract only what's
explicitly stated; empty arrays if nothing qualifies. **The harness enforces these
mechanically after the model returns — the prompt is a request, the code is the guarantee.**

---

## 5. Cadence & model choice

- **N = 15 turns** default (env-overridable). Rationale: long enough that a pass is
  meaningful and cheap-per-minute; short enough the ledger stays current. Tunable live.
- **Model = cheap/Instant lane**, following configured mode:
  - managed → `bluey-managed-instant`
  - BYOK → `gpt-4o-mini`
  - local → Ollama (free, on-device — the local-first path)
- Extraction is mechanical → a small model is sufficient (validated: cheap models hit
  0 fabrications on this task; mis-bucketing is caught by the harness).

---

## 6. Testing

- **Unit (cue-core, no model):** harness drops non-substring quotes; nulls absent speakers;
  dedups; repairs/gives-up on bad JSON; `render()` format stable.
- **Integration (daemon):** `ledger_tick` with a stub provider returning canned JSON →
  asserts `LedgerState` merges verified items only; disabled-by-default respected.
- **Live smoke (manual, opt-in):** `BLUEY_LEDGER=1 bluey listen` on a real meeting →
  confirm pinned block appears and is faithful.

---

## 7. Rollout / flags

- Ships **off by default** (`BLUEY_LEDGER` unset). Heuristic context (existing `%5`
  Context card) stays as the zero-cost floor.
- Turn on with `BLUEY_LEDGER=1`. Tune with `BLUEY_LEDGER_INTERVAL_TURNS`.
- No effect on the answer model, no session pollution, no credit use when local or disabled.
