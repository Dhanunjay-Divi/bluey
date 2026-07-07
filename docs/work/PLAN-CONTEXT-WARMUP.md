# PLAN — Context Warm-Up (Pre-Staging + Live Memory + Question Detection)

> The spine of the product per `docs/STRATEGY-LOCAL-FIRST-AND-PRESTAGING.md`:
> gather context *around* the meeting → summarize into a warm-up brief → prime
> the agent's session once → keep it current with distilled live deltas → so
> every answer is instant, grounded, and private.
>
> This doc is the master checklist. We complete it **set by set**. Nothing here
> is committed as code yet — it is the agreed plan of record.

---

## The target flow (how it's supposed to work)

**Before the meeting:** pull calendar invite + Slack/email/tickets/PRs → summarize
into a **prestage brief** → **prime the agent's session** with it (agent is warm,
knows AUTH-12 / the thread / the ticket before a word is spoken). Warm-up buys
BOTH latency (instant answers) and accuracy (agent resolves mangled STT like
"off twelve" → "AUTH-12" because it knows what's in play).

**During the meeting:** live transcript → **rolling summary** + **decisions
ledger** (distilled running memory, not the raw firehose) → fed as deltas to the
already-warm session.

**On a question:** detector decides *when* (and *whether it's for the user*) →
agent answers instantly because it is already warm + current → only the user
sees it (invisible overlay).

**Principle:** the daemon owns the distilled memory (brief + ledger + summary)
and primes the agent once, re-injecting the delta when the agent's state is
untrusted/stale. Bluey stays a **conduit, never a holder** — pre-context is
fetched *through the agent's own MCP connectors* (Path 2), not by Bluey pulling
Slack/email itself.

---

## What EXISTS today (verified 2026-07)

| Piece | Status | Evidence |
|---|---|---|
| Meeting store (JSON files) + `MeetingRecord` | ✅ solid | `crates/cue-daemon/src/storage.rs`, `crates/cue-core/src/meeting.rs:329` |
| Whole conversation (every Q&A turn), persisted | ✅ live, on | `meeting.conversation`, `push_conversation_turn` (meeting.rs:340,467) |
| Prime-once mechanism (heavy first turn → deltas) | ✅ works | `attached_context_primed`, `answer_context_from_meeting_within` (app.rs:10088) |
| Decisions ledger (quote-verified, LLM-extracted, live) | ⚠️ built but **OFF by default** | `maybe_fire_ledger` (app.rs:6858); `ledger::enabled()` needs `BLUEY_LEDGER=1` |
| Conversation summary | ⚠️ **template + end-of-meeting**, not a live rolling LLM summary | `generate_recap` (intelligence.rs:156); `meeting.summary` set at auto-end (app.rs:6776) |
| `PrestageInput` + `build_prestage_brief` | ✅ struct + builder | `crates/cue-core/src/prestage.rs:25,55` |
| Pre-context INGESTION (Slack/email/calendar/tickets) | ❌ **nothing** — `attendees`/`agenda` hardcoded empty | app.rs:~10021 |
| Pre-meeting TRIGGER (warm before audio) | ❌ **none** — priming fires on first question, inside the meeting | |
| Question detection | ⚠️ regex only (`is_question`, intelligence.rs:279) | measured F1 ~0.55 real / classifier lifts to ~0.76 |
| "For me" routing (name gate + suggest-don't-force) | ✅ partial | `detect_for_me_question` + `auto_trigger_enabled` (defaults to suggest) |
| RAG vector store (semantic recall) | ✅ exists but **needs OpenAI key** | `rag_vectors.db` SQLite; disabled without key |

**Data model note:** no SQL except one SQLite DB for RAG embeddings. Everything
else is JSON files (`~/.local/share/bluey/…`). There is **no schema field for
pre-context** — that must be added.

---

## THE PLAN — set by set

### SET 0 — Fix the live-memory plumbing (DO FIRST)
The warm-agent flow depends on live memory actually running. Today it half-does.

- [ ] **0.1 — Ledger on by default (or a real setting).** It's fully built but
      gated behind `BLUEY_LEDGER=1`. Decide: default-on, or a `CueSettings`
      toggle. Understand WHY it was gated first (cost? latency? verify).
- [ ] **0.2 — Live rolling summary.** Today `meeting.summary` is a template
      assembled mostly at meeting-end. Add a **live, periodic LLM summary** that
      refreshes during the meeting (like the ledger tick) so the warm session
      stays current. Reuse the ledger's interval/tick pattern.
- [ ] **0.3 — Confirm prime-once feeds the live ledger+summary as the delta.**
      `answer_context_for_question` already inserts the ledger + recent
      transcript; make sure the new live summary is in the delta too.

### SET 1 — Question detection (the ONNX classifier)
- [ ] **1.1 — Export the 45MB question classifier to ONNX.** (Off-the-shelf
      `shahrukhx01/question-vs-statement-classifier` measured: wh 92%, yes/no
      51%, false-fire 5%. Fine-tune later to fix yes/no.)
- [ ] **1.2 — Load + run it via `ort`** (already shipped for Parakeet — zero new
      deps; follow the `model_setup.rs` fetch-on-first-run pattern).
- [ ] **1.3 — Swap `is_question()` → classifier** inside `detect_for_me_question`
      (name gate stays unchanged; classifier just replaces the regex condition).
- [ ] **1.4 — (later) Fine-tune** the classifier on MRDA + tech-meeting data to
      lift yes/no recall.
- [ ] **1.5 — Standup / tech-meeting eval** to validate before shipping.

### SET 2 — Routing ("is it for the user?") — no gaze/prosody available
- [ ] **2.1 — Keep name-mention as the high-precision signal** (exists).
- [ ] **2.2 — "Don't force it": suggest-don't-auto-fire is already the default**
      (`auto_trigger_enabled`). Confirm the classifier feeds this gate.
- [ ] **2.3 — Context judgment for no-name questions:** for detected questions
      lacking a name, optionally ask the attached LLM "is this directed at the
      user, given the last N turns?" → suggest (never auto-fire) when uncertain.

### SET 3 — Manual fallback button (escape hatch when detection misses)
- [ ] **3.1 — A button that sends the assembled context** (ledger + rolling
      summary + recent transcript — NOT raw last-N) to the agent as an ask.
      Reuses `answer_context_for_question` + the `AskRequested` path.
- [ ] **3.2 — Decide UX:** quick-send vs. pre-fill-to-edit; placement
      (composer vs. suggestion card). (Open — user decides.)

### SET 4 — Pre-context ingestion (the big new build; Path 2 = via agent MCP)
- [ ] **4.1 — Add pre-context fields** to `PrestageInput`/`MeetingRecord`
      (attendees, agenda, per-source summaries) + persist them (`#[serde(default)]`).
- [ ] **4.2 — Pre-meeting trigger:** meeting-start hook (manual "prep this
      meeting" button first; calendar poll later) that fires BEFORE audio.
- [ ] **4.3 — Path 2 ingestion:** at meeting-start, prompt the WARM agent to pull
      its own Slack/ticket/PR/calendar context via ITS MCP connectors, summarize,
      and hold. Bluey never touches the raw data → conduit promise intact.
- [ ] **4.4 — Summarize pulled pre-context → prestage brief** (reuse the
      summarizer from SET 0.2 / `build_prestage_brief`).
- [ ] **4.5 — Prime the session with the brief** (reuse `attached_context_primed`).

### SET 5 — Context coverage surface (from CONTEXT-INTELLIGENCE-LAB.md)
- [ ] **5.1 — Coverage meter** (audio/screen/docs/repo/agent/memory states).
- [ ] **5.2 — Safe upgrade prompts** ("meeting mentions a ticket → connect it").
- [ ] **5.3 — Source cards** (every answer shows the sources it used).

---

## Dependency order (what unblocks what)
```
SET 0 (live memory)  ──►  SET 4 (pre-context needs the summarizer + prime path)
                     └─►  SET 3 (button sends the assembled memory)
SET 1 (detection)    ──►  SET 2 (routing gates the detected question)
SET 5 depends on 0 + 4 (it visualizes what context exists)
```
**Start: SET 0.** Everything warm-agent depends on live memory actually running.

---

## Open decisions (user to resolve as we hit them)
- SET 0.1: ledger default-on vs. setting — and *why* it was gated.
- SET 2.3: how aggressive the no-name routing is (suggest-only recommended).
- SET 3.2: button behavior + placement.
- SET 4.2: manual "prep" trigger first vs. calendar integration.
- SET 4.3: confirm Path 2 (agent self-fetches via MCP) over Path 1 (Bluey pulls).

---

# APPENDIX A — The Memory Architecture (production-grade, researched + benchmarked)

## Who this is for (target user + meeting types)
Software engineers using coding agents (Cursor, Claude Code, Codex, Copilot,
Gemini). Meetings: **standups, design/architecture reviews, sprint planning,
incident/on-call, code reviews, retros, interviews.** Conversation is messy,
disfluent, jargon-heavy — decisions buried in rambling, ticket IDs, thinking out
loud. (Confirmed: `docs/PRODUCT-STRATEGY.md` "not an interview assistant — the
live layer between meetings and coding agents".)

## The two-tier memory model (industry-standard: Mem0 / Zep / Letta)
- **Working memory (local, per-meeting):** raw transcript + rolling summary +
  this-meeting ledger + Q&A. Resets each meeting. = the "hot path".
- **Long-term memory (cross-meeting, persistent):** extracted facts/decisions,
  embedded + semantically searchable across ALL past meetings + the prestage
  brief inputs. = the "cold path".

## Update rules (from Mem0 + Zep, validated by LongMemEval)
- **Facts/decisions → per-item ADD / UPDATE / DELETE / NOOP** (an LLM compares
  each new candidate vs existing memories). NOT our current append-only+dedup.
  (Mem0's core loop.)
- **Contradictions/stale facts → SUPERSEDE, don't delete.** Close the old fact's
  validity window (`valid_from`/`valid_to`), keep it queryable, surface the new
  one. ("We chose Postgres" (Jan-Mar) → "switched to Mongo" (Mar+).) (Zep's
  temporal pattern — scores 100% on LongMemEval "knowledge updates".)
- **Rolling summary → REPLACE (regenerate, bounded).** The ledger (append/supersede)
  is the durable record; the summary is the disposable current-narrative view.

## When to summarize/consolidate (production defaults)
- Trigger on **% of context window (50-85%)**, NOT a fixed turn count. Our current
  "every 15 turns" is cruder; use a token-budget cap + turn-count proxy.
- Keep ~10% recent raw window verbatim (recency buffer); summary is 60-80% shorter.
- Run it **after the turn, in the background** (fire-and-forget) — never block the
  loop. (We already do this for the ledger.)

## THE CRITICAL RULE (measured on our content): extract facts, don't embed raw transcript
Tested `all-MiniLM-L6-v2` / `bge-small` / `gte-small` on realistic eng-meeting memory:
- **Clean extracted facts** ("DECISION: shard by tenant, not region") → **100% top-3.**
- **Raw transcript chunks** ("yeah so um the sharding thing…") → **80%, confident
  WRONG answers** (can't tell "chose tenant" from "rejected region").
- => Our current RAG indexes *raw transcript* — that is the wrong thing to embed.
  **Extract facts FIRST (the ledger), then embed the facts.** Extraction quality
  matters more than embedder size for our messy content.

## Model choice (measured, not guessed)
- **Embedder: `bge-small-en-v1.5` (~33MB)** — 90% top-1 / 100% top-3 on extracted
  facts. Runs via `ort` (same as Parakeet + the question classifier). Fetch on
  first run. Fixes RAG being OpenAI-only → semantic memory works offline, keyless.
- (`all-MiniLM-L6-v2` 22MB also passed at 100% top-3; bge is a touch better for
  free. Bigger models did NOT help on messy short text.)

## Benchmark yardstick (validate against real evals, not our synthetic tests)
- **LongMemEval** (`xiaowu0162/longmemeval` — HF, ungated, 916★ repo): the standard
  multi-session chat-memory benchmark. 5 abilities: info-extraction, single-session,
  multi-session, knowledge-updates, abstention. Mem0 scores: LoCoMo 92.5%,
  LongMemEval 94.4% at ~7K tokens/query (vs 25K full-context).
- **The hard part it reveals: multi-session (cross-meeting) recall = 70.7%** even
  for Mem0. So cross-meeting is where the difficulty is — budget for it.
- **Gaps in ALL public benchmarks (so we build our own on top):** (a) the *write/
  extraction* step is barely measured — our extraction quality needs its own eval;
  (b) none are *meeting*-shaped (all chat) — add an MRDA/real-transcript meeting
  eval; (c) multi-user isolation untested — relevant to "does meeting A leak into B".

---

# APPENDIX B — How this integrates into OUR system (replace vs extend)

## Build vs. buy: BUILD the patterns on our stack
- **No production-grade Rust memory library exists.** `mem0-rust`/`rustmem`/
  `mempalace-rs` are all hobby (10-144 downloads, one author, unmaintained) AND
  force a **DB server** (Postgres/Qdrant) — worse than Python for our lean-binary.
- **Mem0 (Python, 60k★)** is the right *design*, wrong *runtime* (Python + it
  becomes a data-holder → breaks the conduit promise).
- **We already have 80% of the machinery:** SQLite (`rag_vectors.db`, cue-rag),
  vector search (`store.rs`), ONNX runtime (`ort`), LLM fact-extraction (ledger).
  => **Port the PATTERNS onto our stack.** `mem0-rust` is MIT — its
  `memory/prompts.rs` is a free reference for the ADD/UPDATE/DELETE prompts.

## What we REPLACE vs EXTEND, file by file
| Piece | Today | Action |
|---|---|---|
| RAG embedder (`cue-rag/src/embedder.rs`) | **OpenAI-only** (cloud, keyed) | **ADD** a local ONNX `bge-small` `EmbeddingProvider` (the trait already has a TODO for it). Keep OpenAI as optional. |
| RAG index (`cue-rag/src/store.rs`) | indexes **raw transcript** | **CHANGE** to index **extracted facts**, and query **cross-meeting** (drop/loosen the per-session filter). |
| Ledger merge (`ledger.rs::merge`) | append + dedup | **UPGRADE** to ADD/UPDATE/DELETE/NOOP + supersede (validity windows). |
| Ledger extractor (`app.rs:9180 ledger_extract_once`) | **cloud cheap-lane** (skips Local, needs API key) | **REPLACE** the caller: use the USER'S ATTACHED AGENT to extract/summarize (see Appendix C). Keep cloud as fallback only. |
| Ledger trigger (`ledger.rs::should_fire`) | every 15 turns | **EXTEND** with a token-budget/% trigger. |
| Summary (`generate_recap`) | template + at-end | **ADD** a live rolling LLM summary (driven by the agent). |
| Meeting store (`storage.rs` / `MeetingRecord`) | no pre-context / no long-term fields | **EXTEND** schema: prestage fields + a cross-meeting facts store (new SQLite table or file). |
| Prime-once (`attached_context_primed`) | works | **REUSE** as-is. |

---

# APPENDIX C — Use the CODING AGENT to summarize, via a THROWAWAY session

## Correction (verified 2026-07)
`apply_continuation_tier` (app.rs:8157 → `continuation/tier.rs`) is **NOT** a
live-conversation summarizer. It is **session-RESUME compaction**: when a user
reopens a PRIOR session that can't resume-by-id (the Replay tier), it loads that
old transcript and compacts it to replay as context. Different purpose. (Earlier
draft wrongly cited it as the summarize-via-agent precedent.)

## The right model: two DISTINCT uses of the agent
| Use | Session | Persist? |
|---|---|---|
| **ANSWER** the user's question | the warm answering session (primed w/ pre-context, continued for follow-ups) | ✅ yes |
| **SUMMARIZE / EXTRACT facts** from the conversation | a **THROWAWAY one-shot drive** — fire, take output, discard | ❌ **no — delete/ignore after** |

**Why throwaway:** summarization must NOT pollute the user's real answering
session (would corrupt its context + burn its rate limit) and must NOT appear in
their session list. It's a pure side-computation.

## The mechanism EXISTS (primitive form)
Every agent has **`oneshot_args`** in `drive/cli.rs` (`claude -p "{prompt}" -s`,
`gemini -p "{prompt}"`, `cursor-agent -p … --output-format json`, etc.) driven
with **`resume: None`** = a **headless print-mode one-shot** that does NOT continue
the user's session. `drive_and_collect(agent, q, DriveMode::Answer)` with a fresh
`AgentQuestion{ resume: None }` is exactly this call.

## DECISION (locked): one-shot, accumulate in OUR store
We use a **stateless throwaway one-shot drive per summary tick** — NOT a persistent
side-session. The "keep summarizing on top of the previous transcript" benefit is
real, but the accumulation lives in **OUR rolling summary (SQLite)**, which we
re-feed each tick — NOT in a live agent session. Per tick:
```
input  = [our current rolling summary] + [new transcript segments since last tick]
output = [updated rolling summary] + [new/updated facts]
store both; next tick feeds the NEW summary + the next delta.
```
Why one-shot over a persistent summarizer session:
- The rolling summary accumulates in OUR durable store → survives agent
  restart/compaction (a persistent agent session is fragile; if it drops context
  mid-meeting, memory is lost — the Architecture-A fragility we already rejected).
- No second long-lived session to manage/tear-down, no its-own-compaction problem,
  no session-list pollution to clean up.
- We still get the coherence ("builds on previous") because we feed our own growing
  summary back in. Best of both.

## The plan
Route ledger extraction + rolling summary through a **throwaway one-shot drive of
the user's attached agent** — NOT the answering session, NOT a Bluey/cloud LLM.
- Replaces `ledger_extract_once`'s current **cloud cheap-lane** (which needs an API
  key + skips `Local` → breaks the conduit/local-first thesis).
- Fallback order: **throwaway agent one-shot → local model (`ort`) → cloud** —
  inverting today's cloud-first order.
- All AI (answer AND memory) then runs through the user's own agent. On-thesis,
  zero Bluey-held data.

## MUST-VERIFY before building (do not assume)
1. **Does a one-shot `-p` run PERSIST a session file on disk** (per agent)? If some
   agents persist even one-shot drives, a throwaway summarizer would leak sessions
   into the user's list → we must **delete them after** (or find a truly-ephemeral
   flag per agent). Verify empirically per agent (claude/codex/cursor/gemini/copilot).
2. **Cost/latency of driving the agent every summary tick** (rate limits). If too
   heavy for high-frequency ledger ticks, use a tiny LOCAL summarizer (`ort`) for
   the frequent ticks and reserve the agent one-shot for the heavier rolling summary
   + the prestage brief. **Measure first.**

---

# APPENDIX E — Mem0 → Rust rewrite spec (deep, implementation-level)

> Reverse-engineered from the Mem0 paper (arxiv 2504.19413) + actual source
> (`mem0/configs/prompts.py`, `mem0/configs/base.py`, deepwiki storage docs).
> This is enough to reimplement natively. **We port the ALGORITHM, on our stack**
> (SQLite + `ort` + the user's agent) — no Python, no server, no cloud LLM.

## E.1 — The two-phase pipeline (exact)

**Phase 1 — EXTRACTION** (per new exchange):
- Input `P = (S, {m_{t-10}...m_{t-2}}, m_{t-1}, m_t)`:
  the conversation **summary S** + the **last m=10 messages** + the new pair.
- Feed to an LLM with the extraction prompt → JSON `{"facts": [...]}` (a list of
  short atomic facts). Empty list is valid (chit-chat → no facts).
- The summary S is regenerated **asynchronously**, out of band.
- **For us:** the "messages" are transcript segments; the LLM is the **user's
  attached agent** (`drive_and_collect`, Appendix C), not a cloud call.

**Phase 2 — UPDATE / CONSOLIDATION** (per extracted fact ω):
- For each ω, retrieve **top s=10** semantically-similar existing memories (vector).
- Feed (ω + those 10) to the LLM with the **update prompt** → it returns, per item,
  one of **ADD / UPDATE / DELETE / NONE** via a JSON schema.
- Apply the ops to the store.

## E.2 — The ACTUAL prompts (port these near-verbatim)

**Extraction** (`FACT_RETRIEVAL_PROMPT`): "extract relevant atomic facts... return
JSON `{"facts": [...]}`... empty list if nothing relevant." Few-shot examples map
input utterances → fact lists. **We adapt the "types to remember" from personal-
preference categories → ENGINEERING categories** (decisions, owners, tickets/PRs,
SLAs, blockers, action items, config/architecture choices).

**Update** (`DEFAULT_UPDATE_MEMORY_PROMPT`): four ops with exact rules —
- **ADD** — new info not in memory → new id.
- **UPDATE** — same topic, more/different info → keep same id, keep the richer text;
  if it conveys the *same* thing, NONE (don't churn).
- **DELETE** — new info **contradicts** existing → remove (we do SUPERSEDE instead
  — see E.4). 
- **NONE** — already present / irrelevant.
Returns JSON: `{"memory":[{"id","text","event","old_memory"?}]}`.

## E.3 — Data model (port `MemoryItem` → Rust struct)
```
MemoryItem { id, memory (text), hash (MD5 of text — dedup),
             metadata (scope: user/agent/meeting/project), score,
             created_at, updated_at }
```
Plus the **history log** (SQLite table, every op): `memory_id, old_memory,
new_memory, event(ADD|UPDATE|DELETE), timestamp` — the audit trail (fits our
quote-verified ethos).

## E.4 — Our adaptations to Mem0 (deliberate deltas)
- **DELETE → SUPERSEDE** (Zep pattern): don't remove a contradicted fact; set
  `valid_to`, keep it queryable. LongMemEval "knowledge updates" = 100% this way.
- **Embed EXTRACTED facts, not raw transcript** (our measured rule — 100% vs 80%).
- **LLM = the user's attached agent** (Appendix C), not GPT-4o-mini/cloud.
- **Embedder = local `bge-small` via `ort`**, not `text-embedding-3-small`.
- **Scope tags** (meeting/project) for the multi-user/leak isolation the benchmarks
  don't cover.
- **Trigger** = %-of-window + turn-count proxy, not fixed.

## E.5 — Numbers (Mem0's, our starting defaults)
| Param | Mem0 | Notes |
|---|---|---|
| Recency window m | 10 msgs | segments for us |
| Similar retrieved s | 10 | for the update-decision |
| Retrieval top-k (answer) | small (1-2 in paper) | we send top few facts |
| Search latency | p50 0.15s / p95 0.20s | target |
| Tokens/query | ~1.7-7k | vs 26k full-context |

## E.6 — Component → our-stack mapping (nothing new to buy)
| Mem0 component | Our equivalent |
|---|---|
| Vector store (Qdrant/pgvector) | **SQLite** `rag_vectors.db` (cue-rag) — extend |
| Embedder (OpenAI) | **local `bge-small` via `ort`** (new provider, trait exists) |
| Extract/Update LLM (GPT-4o-mini) | **user's attached agent** (`drive_and_collect`) |
| History (SQLite) | **SQLite** (already used) |
| Graph variant (Neo4j) | **SKIP for v1** — SQLite + supersede covers our needs; revisit only if entity-graph reasoning is needed. `sqlite-graph` (bi-temporal edges) is the Rust option if so. |

## E.7 — Build order for the memory layer (maps into SET 0 + SET 4)
1. Local `bge-small` embedder (`EmbeddingProvider` impl via `ort`).  ← prerequisite, do first
2. Extraction via the attached agent (replace `ledger_extract_once`'s cloud call).
3. Update/consolidation (ADD/UPDATE/DELETE→SUPERSEDE/NONE) on the ledger's merge.
4. Index **extracted facts** (not transcript) into SQLite; query cross-meeting.
5. Two-tier scope (working = per-meeting, long-term = cross-meeting facts).
6. Eval against LongMemEval + a meeting-shaped set.

---

# APPENDIX D — Question detection & routing (measured, for reference)
- Question classifier (SET 1): off-the-shelf 45MB `shahrukhx01/question-vs-statement`
  measured on real MRDA meeting speech (STT-shaped): **wh 92%, yes/no 51%,
  open 72%, false-fire 5%.** Fine-tune to fix yes/no. Runs via `ort`.
- Every SLM tested (Gemma 3 270M/1B, Qwen3 0.6B/1.7B, SmolLM2, LFM2) either scored
  BELOW regex or was too heavy (1.7B=640ms/line). **No small SLM does both well.**
  So: classifier for detection (fast, per-line) + agent/cheap-lane for the hard
  "for me" judgment (rare). Details in the conversation log.

---

# APPENDIX F — Session mechanics we ALREADY have (the foundation, do NOT rebuild)

The memory + warm-up layer sits ON TOP of Bluey's existing agent-session engine.
These are built, tested, and documented — REFERENCE them, don't reimplement.
(Full detail: `docs/work/PLAN-AGENT-BRIDGE.md`, `docs/AGENT-MODEL-SPEED-CONTROL.md`.)

| Capability | Where | Use in the memory layer |
|---|---|---|
| **Continue a session** (true resume / fork / replay tiers) | `cue-agent-bridge/src/continuation/` (`apply_tier`, `resolve_session`) | Resume the WARM answering session for follow-ups |
| **Start a new session** | `app.rs:11079 start_new_session` | Open the warm answering session at meeting start |
| **Select model / speed** | `cue-agent-bridge/src/model_resolve.rs`, `app.rs model_override` | Pick the answering model; a cheap model for the summarizer one-shot |
| **Drive: one-shot vs resume** | `cue-agent-bridge/src/drive/cli.rs` (`oneshot_args` / `resume_args`), `drive_and_collect` | **one-shot (`resume:None`)** = the throwaway summarizer (App C); resume = answering |
| **Per-agent CLI map** (claude/codex/cursor/gemini/copilot) | `drive/cli.rs` | Both answering and summarizer drives go through this |
| **Prime-once context** | `attached_context_primed`, `answer_context_from_meeting_within` | Send the prestage brief + memory as the heavy first turn, deltas after |

**So the memory layer adds only:** (1) the local ONNX embedder, (2) fact
extraction/consolidation (ADD/UPDATE/DELETE→SUPERSEDE) via the throwaway drive,
(3) the SQLite facts store (extend `cue-rag`), (4) cross-meeting scope, (5)
pre-context ingestion. Everything about DRIVING the agent already exists.

---

# APPENDIX G — END GOALS (definition of done — for Fable 5 to execute against)

> The target state. Fable 5 builds toward THIS. Each SET is done when its goal
> below is true, verified (fmt+clippy+tests per `/CLAUDE.md`), on a real build.

## The product end-state (what "complete" means)
A user opens a meeting. **Before it starts**, their agent is warm — it already
knows the meeting's tickets/Slack/thread (pre-context, summarized in). **During**
the meeting, Bluey keeps a live rolling summary + a decisions ledger (facts
ADD/UPDATE/SUPERSEDE'd correctly), all built by throwaway one-shot drives of the
user's OWN agent (Bluey runs no AI, holds no data). **Cross-meeting**, extracted
facts are embedded locally (bge-small via ort) and semantically recalled across
past meetings. **A question** is detected (classifier), routed ("for me?" via name
+ context), and answered instantly by the warm agent — or the user taps the manual
button to ask about the last N + context. Everything local, private, one lean
binary.

## Definition of done, per SET
- **SET 0 (live memory):** ledger runs by default (or a real setting), driven by
  the user's agent one-shot (NOT cloud); a live rolling summary refreshes during
  the meeting and accumulates in OUR store; prime-once feeds ledger+summary as the
  delta. DONE when a live meeting shows a growing summary + ledger with zero cloud
  calls and no API key.
- **SET 1 (detection):** bge/classifier via ort; `is_question` → classifier in
  `detect_for_me_question`; measured lift over regex on a meeting eval. DONE when
  the classifier ships in the meeting build and beats regex recall on the eval.
- **SET 2 (routing):** name gate + suggest-don't-force + optional context judge.
  DONE when a for-me question surfaces a suggestion (never auto-spams) and a
  not-for-me question stays silent.
- **SET 3 (manual button):** sends assembled memory (summary+ledger+transcript).
  DONE when tapping it produces a grounded answer with no typing.
- **SET 4 (pre-context):** PrestageInput populated from real sources via the
  agent's own MCP (Path 2); pre-meeting trigger; brief primes the session before
  audio. DONE when, given a calendar/Slack source, the agent answers a first
  question using pre-meeting context it was never told in the meeting.
- **SET 5 (coverage surface):** coverage meter + upgrade prompts + source cards.
  DONE when the overlay shows what context it has and every answer cites sources.

## The memory layer (Mem0-in-Rust) definition of done
Implements App E: two-phase (extract → ADD/UPDATE/DELETE/NONE→SUPERSEDE) on
SQLite, local bge-small embeddings of EXTRACTED FACTS (not raw transcript), two
tiers (working=per-meeting, long-term=cross-meeting), driven by the throwaway
agent one-shot. DONE when it scores competitively on **LongMemEval** (knowledge-
updates + multi-session) AND on a meeting-shaped eval, at low token cost, fully
local.

## What is LEFT after all sets (the honest scope)
Per the target: once SET 0-3 + the memory layer land, **only SET 4 (pre-context
ingestion + pre-meeting trigger) is the remaining big build** — exactly as hoped.
SET 5 (coverage surface) is polish on top.

## Non-goals (explicit, so Fable 5 doesn't scope-creep)
- NO Bluey-hosted AI or cloud memory (conduit/local-first is inviolable).
- NO Python runtime, NO database server (lean one-binary constraint).
- NO graph DB for v1 (SQLite + supersede; revisit only if entity-graph needed).
- NO gaze/prosody routing (no camera; STT is text-only).
- NO scraping/cookies for pre-context — only user-authorized sources via the
  agent's MCP.
