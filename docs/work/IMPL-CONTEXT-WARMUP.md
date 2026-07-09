# IMPL — CONTEXT-WARMUP (SET 0–3 + the Mem0-in-Rust memory layer)

> Implementation record for the PLAN-CONTEXT-WARMUP build phase, per
> `/CLAUDE.md` workflow. Branch: `agent/meeting-build-separation`.
> Plan: `docs/work/PLAN-CONTEXT-WARMUP.md` (appendices A–G govern scope).

## What was built

### SET 0 — live memory (commit 0b8c225 + review fixes)
- `live_memory_enabled` setting (default ON, env `BLUEY_LEDGER` override).
- Ledger extraction driven by the **attached agent's throwaway one-shot**
  (`memory_oneshot_via_agent` → `drive_and_collect`, `DriveMode::Answer`,
  `resume: None`); legacy cloud cheap-lane is fallback only. Verified
  empirically: `claude -p` persists NO session — accumulation lives in OUR
  store, never in an agent session (Appendix C decision).
- Rolling summary (`crates/cue-daemon/src/summary.rs`): fires every 24
  segments (env `BLUEY_SUMMARY_INTERVAL_SEGMENTS`, min 8), bounded prompt +
  bounded result (≤1800 chars), REPLACES `meeting.summary` under the meeting
  lock, only if the same meeting is still active. Sent every answer turn as
  the "live rolling summary" context block.
- Review fixes (adversarial agent findings, all confirmed real):
  - inflight guard cleared via `Drop` guard → panic-safe;
  - no `save_active` off the meeting lock (lost-update clobber);
  - meeting-id guard on late ledger merges (cross-meeting bleed).

### Memory layer — Mem0-in-Rust (commit 1b2f511 + update phase)
- `cue-rag/src/local_embed.rs`: `LocalBgeEmbedder` — bge-small-en-v1.5 int8
  ONNX via `ort =2.0.0-rc.12`, CLS pooling + L2 normalize, bge query prefix,
  384-dim. Model downloads once (~34MB, Xenova export) into
  `data_dir/models/bge-small-en/`.
- `cue-rag/src/facts.rs`: `FactsStore` (SQLite) — extracted-facts-only
  (measured rule: facts ≈100% top-3 recall; raw transcript confidently
  mismatches), supersede-not-delete validity windows, `facts_history` audit
  table (mem0 history-log pattern), agent-op appliers
  (`insert_fact`/`supersede_fact`/`invalidate_fact`) + similarity-heuristic
  fallback (`add_fact`).
- `cue-daemon/src/memory.rs`: `FactsMemory` orchestration — `prepare_update`
  (embed candidates, pre-agent exact-hash dedup, per-candidate top-10 similar
  union capped at 10 presented) → update-decision prompt (near-verbatim port
  of mem0's `DEFAULT_UPDATE_MEMORY_PROMPT`, engineering-flavored, integer
  display-id indirection) → `apply_agent_ops` (tolerant parse; unknown
  ids/events skipped) / `apply_heuristic` fallback. Wired into
  `maybe_fire_ledger`'s verified output.
- **Architecture decision (source-verified 2026-07-08):** mem0 OSS at HEAD
  (v3) deleted the paper's update phase (additive-only + MD5 dedup; graph
  memory removed entirely). We port the PAPER loop because meeting decisions
  get REVERSED — additive-only keeps stale + new decisions both "current".
  v3's hardening (id indirection, pre-LLM hash dedup, empty-store fast-path)
  is adopted. Full source map in the session's workflow record.

### SET 1+2 — two-stage question detection
- `scripts/export-qdetect-onnx.sh`: one-time export of
  `shahrukhx01/question-vs-statement-classifier` → int8 ONNX (~11MB) with an
  int8-vs-pytorch parity gate (6/6 required). Pinned to torch 2.6 legacy
  exporter (torch 2.12's dynamo export produced a graph that fails shape
  inference during quantization).
- `cue-daemon/src/qdetect.rs`: `QuestionClassifier` via ort; model dir
  resolution `BLUEY_QDETECT_MODEL_DIR` → `data_dir/models/qdetect-en` →
  exe-adjacent `models/qdetect-en` (canonicalized — installer launches via
  symlink). No network download; absent model → regex-only.
- `cue-core` seam: `is_question_shaped` + `detect_for_me_question_given`
  (speaker/name gating unchanged, cue-core stays the single authority).
- Wiring: classifier runs ONLY on lexical rejects, never on own speech or
  sub-substance lines. `package-airdrop.sh` ships `dist/models/qdetect-en`
  into the tarball `bin/models/`.

### SET 3 — manual ask button (overlay UI)
- `Composer.tsx`: optional `onAskRecent` button (✦, same `iconBtn` style),
  disabled while an answer streams. `AskScreen.tsx`: canonical
  `ASK_RECENT_QUESTION` routed through the SAME `runAsk` pipeline the typed
  composer uses. tsc clean.

## Verification (all real, no mocks)
- Unit: 140 cue-core + 276 cue-daemon (local-memory) + 14 cue-rag, clippy
  `-D warnings` on both feature combos, `--target aarch64-apple-darwin`.
- `facts_memory_real.rs` (#[ignore], real model + real `claude -p`): semantic
  recall of paraphrased questions; the update phase superseding a REVERSED
  sharding decision end-to-end (agent returned UPDATE with `old_memory`).
- `qdetect_real.rs` (#[ignore], real exported model): direct disfluent
  questions caught, ZERO false positives on statements (the hard gate);
  indirect forms are the documented ~51% band.
- **Live daemon end-to-end** (release build, WAV test hook, isolated dirs):
  - VoxConverse `aepyx.wav` (real multi-speaker broadcast): 225 real Parakeet
    STT segments; live rolling summary (1103 chars) written mid-meeting by
    real claude one-shots, content-accurate; meeting archived with
    recap-upgraded title. Ledger correctly extracted NOTHING (news audio has
    no meeting decisions — the verbatim-quote gate refusing to fabricate).
  - Real-speech standup meetings (macOS TTS → real WAV → real STT), measured
    2026-07-08: meeting B extracted 3 quote-verified facts live (SLA
    constraint + 2 owners, history ADD×3); meeting C spoke the REVERSAL and
    the update phase superseded live in-daemon (`Raj — payment service
    migration` closed, `Priya — payment service migration` current,
    `Raj — billing rewrite` added; history ADD×4 UPDATE×1); a third boot with
    a fresh empty meeting asked "who owns the payments migration now" through
    the REAL answer path and answered **Priya** from cross-meeting memory —
    the stale fact never surfaced, and topics that never survived extraction
    were answered "not found" rather than hallucinated.
  - Live-found production fix: quote verification failed for sentences split
    across choppy STT finals (interleaved `Label:` lines break verbatim
    containment) — `parse_and_verify` now also checks the label-stripped
    window content; fabrication still rejected (unit test
    `quote_spanning_choppy_stt_segments_still_verifies`).
  - Known quality follow-up (not architecture): extraction recall under
    heavy STT chop — the sharding decision made both rolling summaries but
    not the facts store; tune window sizing / extraction prompting against a
    meeting-shaped eval alongside the LongMemEval run.

### Hybrid retrieval — the mem0 v3 search port (follow-up build, 2026-07-08)
- Motivation: an honest source audit (5-reader workflow over mem0 HEAD
  22f70d5) showed our read path was cosine-only while mem0 v3 ships hybrid
  search — BM25 + adaptive sigmoid + entity boosts. Ported exactly:
  - `cue-rag/src/hybrid.rs`: Okapi BM25 over Snowball-stemmed fact text
    (stemming stands in for spaCy lemmatization — same role, consistent on
    both sides; the `-ing` original-form rule kept), mem0's
    `get_bm25_params`/`normalize_bm25`/`score_and_rank` with PARITY TESTS
    pinned to values from running mem0's own scoring.py, `entity_boost`
    (`sim * 0.5 * 1/(1+0.001(n-1)²)`), and a POS-free port of the entity
    extractor (IDENTIFIER regex, PROPER capitalized spans with connectors +
    generic-word filters, QUOTED; spaCy-only NER/TOPIC not ported — mem0
    without spaCy extracts nothing, so this exceeds its own fallback).
  - `facts.rs` schema v2: `text_stemmed` column (transparent backfill
    migration), `fact_entities` table (`normalized UNIQUE`, embedding,
    `linked_ids` JSON), `upsert_entity` (exact-normalized → merge; semantic
    ≥0.95 → merge; else insert), `entity_matches` (≥0.5 floor),
    `hybrid_query` (semantic pool over-fetched at `max(k*4,60)`, BM25 over
    the current corpus, fusion; threshold gates SEMANTIC before fusion —
    `FactHit` now carries both `score` (combined, ranking) and `semantic`
    (gating); `render_hits` floors on `semantic`).
  - `memory.rs`: `link_entities` on every fact write (ledger kind label
    stripped first), query-entity boosts in `search` (cap 8, dedup), plus
    `search_semantic_only` kept as the measured baseline surface.
- **Measured** (real bge-small, 15 meeting facts, 12 labeled queries):
  hybrid hit@1 = cosine hit@1 = 11/12, zero regressions. The eval CAUGHT a
  real defect pre-ship: bge-small clusters short acronyms ("SLA"≈"SSO"
  cosine >0.5), so a query entity falsely boosted the wrong fact past a
  0.763-semantic right answer. Fix (documented delta from mem0, whose
  OpenAI-tuned floor never sees this): non-exact entity matches where
  either side is ≤4 chars require 0.85 (`SHORT_ENTITY_MATCH_FLOOR`);
  exact-normalized matches keep mem0's 0.5.
- Known residual deltas from mem0 v3 search: no ANN index (O(N) scan — fine
  at v1 scale), stale fact ids accumulate inside `linked_ids` (harmless for
  ranking — only CURRENT rows are ranked — but they dilute
  `memory_count_weight` over time; revisit with entity GC), and the
  `last-10-messages` extraction-context table is not ported (our extraction
  context is the transcript window, which serves the same role).

## Deliberate scope notes
- Ledger kinds stay Decision/Constraint/Owner (quote-verified). News-style
  audio correctly yields zero items.
- FactsStore comparisons are O(current-facts) per insert under a short lock —
  fine at v1 scale (facts accrue slowly); revisit with an ANN index only if
  stores grow past tens of thousands.
- Retrieval floor 0.45 for the cross-meeting context block — to be tuned
  against a meeting-shaped eval + LongMemEval (memory-layer DoD, still open).
- SET 4 (pre-context ingestion) and SET 5 (coverage surface) intentionally
  not started (plan order).

---

## The MCP-backend pivot (Batches 0–5, 2026-07-08)

**The locked design:** Bluey = a local MCP server the agent PULLS meeting
memory from; the calendar-fired warm session IS the meeting's backend
session; no fallback LLM (the attached agent is the only answerer).

- **Batch 0 — spike (all five CLIs, live):** loopback Streamable-HTTP +
  bearer token fires headlessly with zero prompts. claude
  `--allowed-tools "mcp__<name>"`; gemini `--allowed-mcp-server-names` +
  `GEMINI_CLI_TRUST_WORKSPACE=true`; copilot `--allow-tool` (node≥24);
  cursor `.cursor/mcp.json` + scriptable `mcp enable` + `--force`
  (breadth caveat tracked); codex `mcp add --url --bearer-token-env-var`
  (auth+list verified; call blocked only by account model limits). The
  "GUI rows can't be backends" concern was wrong — GUI surfaces route to
  their CLI sibling (`continuation_via`).
- **Batch 1 — `cue-mcp`:** hand-rolled over hyper (zero new transitive
  deps), 4 read-only tools, strict schemas + server-side caps, per-meeting
  rotating token, Host allow-list. Acceptance: the real claude CLI answered
  a question grounded ONLY in the fixture tools.
- **Batch 2 — daemon as source:** `MeetingMemorySource` impl with
  short-lock clones (never `.await` under the meeting lock — the STT sink
  invariant), server mounted at startup, `BLUEY_MCP_PORT/TOKEN` dev hooks.
  Acceptance: MID-MEETING the tools served the live transcript + rolling
  summary while STT kept flowing; claude answered "what has been decided"
  from pulls alone; bad token → 401.
- **Batch 3 — warm session:** `mcp_register.rs` (write-side registration,
  surgical JSON merges, corrupt-config refusal) + `WarmupStart/Stop`.
  Acceptance: registered into the real claude config, warm brief produced
  via bluey-memory pulls, session pinned, mid-meeting ask RESUMED the
  warmed session and answered the reversals correctly, deregistered on
  stop. Known follow-up: `--resume` replays prior turns into stream-json
  (parser concatenates a stale prefix) — fix in the drive-parser pass.
- **Batch 4 — coverage meter:** `SourceCoverage` IPC + overlay chips with
  guided connect hints (vendor-documented endpoints only). Acceptance:
  honest classification of the real ~/.claude.json.
- **Batch 5 — calendar trigger:** poll + once-per-occurrence idempotency +
  typed `WarmupOutcome` with retry-on-refusal. Acceptance deliberately
  reproduced the trigger-before-attach race: 5 refusals, then exactly one
  success after attach; registered/minted/pinned/deregistered verified.

**Decision for Batch 6 (from live-testing feedback):** hybrid envelope —
PUSH the small always-needed slice (rolling summary + recent transcript,
bounded) WITH each question so the common case costs zero tool round-trips;
PULL everything else (past meetings, decisions, deeper transcript) via the
tools. Batch 6 therefore deletes the HEAVY push machinery (prime-once brief,
back-history, Q&A replay, primed/delta state) and keeps one lean envelope.

**Remaining:** Batch 6 (delete heavy push + hybrid envelope + ACP
`session/new` mcp_servers injection + resume-replay parser fix), EventKit
source behind the `calendar` feature (needs the packaged app's TCC usage
string), tarball rebuild.
