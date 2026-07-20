# IMPL: CONVERSATION-MEMORY — App-owned in-meeting conversation memory (Wave 1)

Bluey now stores every in-meeting Q&A turn in its OWN store and re-supplies the
running dialogue (rolling summary + verbatim recent turns) to the agent on each
ask. This is the foundation that lets "follow up on that" / "shorten your last
answer" work WITHOUT depending on the agent's own resumable session — the
prerequisite for driving agents ephemerally (Wave 3) so nothing accumulates in
the agent's on-disk chat history while its MCP connectors (the moat) survive
(they are config-scoped, verified independently).

Built on branch `agent/conversation-memory` across three parts: the pure-logic
core (`cue-core::conversation`), the retrieval-pipeline fixes the quality harness
proved necessary (`cue-rag`), and the daemon wiring (`cue-daemon`).

## Scope

**Does:**

- **`cue-core::conversation`** — pure, dependency-free memory logic:
  `ConvConfig` (all tunables, env-overridable), `estimate_tokens` (chars/4
  heuristic, zero deps), `context_window_for_model` (model→window lookup table +
  `BLUEY_MODEL_WINDOW` override), `ConvTurn`/`ConvRole`, `assemble_block`
  (rolling summary + newest-turns-that-fit-the-budget, returns overflow),
  `build_fold_prompt`, `bound_summary`.
- **`cue-rag` fixes** (proven necessary by the retrieval harness over 5,000 real
  chunks of this machine's agent history): the `Chunker` UTF-8 panic fix
  (byte-offset slicing crashed on multibyte text — 34 confirmed panics),
  `hash_dedup` (exact-duplicate chunks were 46.5% of the corpus; dedup measured
  +32pts recall@1), and `is_self_prompt` (filters Bluey's own headless prompts
  leaked into agent session stores — for Wave 2's indexer).
- **`conversation_turns` table** (migration 012) + DB accessors (`conv_append`,
  `conv_turns`, `conv_delete_oldest`, `conv_prune`, `conv_clear`), FK to
  `sessions(id)` with the `ensure_meeting_session` discipline (commit 338f1e9).
- **Daemon wiring**: record each successful in-meeting exchange (skip warm-up),
  assemble the conversation block into the answer envelope AFTER the rolling
  summary and BEFORE the transcript (compaction-order discipline), and fold
  overflow into an in-memory rolling summary via the existing stateless one-shot
  with a panic-safe single-flight guard. Reset per meeting at all 5 ledger-reset
  sites.

**Does NOT:**

- Change the drive to ephemeral (Wave 3 — flag-gated, default OFF; `--resume`
  stays the default until the buffer is validated live).
- Add session-history retrieval / the `search_agent_history` MCP tool (Wave 2 —
  consumes the `cue-rag` fixes; engine already proven at 78% recall@1 / 12ms in
  the harness).
- Persist the rolling conversation summary to disk (in-memory for Wave 1 — the
  raw turns are the durable source of truth and re-foldable after a restart).
- Add any new dependency. Token counting is a heuristic, not a tokenizer.

## Config surface (the "easily customizable" requirement)

All knobs in one place (`ConvConfig::from_env()`), each env-overridable with a
min-clamp, following the existing `ledger::interval_words()` pattern:

| Knob | Env var | Default | Min | Meaning |
|------|---------|---------|-----|---------|
| `tail_tokens` | `BLUEY_CONV_TAIL_TOKENS` | 2000 | 200 | verbatim recent-turns budget |
| `summary_tokens` | `BLUEY_CONV_SUMMARY_TOKENS` | 500 | 100 | running conv-summary cap |
| `max_stored_turns` | `BLUEY_CONV_MAX_TURNS` | 200 | 20 | per-meeting stored-turn cap (FIFO prune) |
| `window_frac` | `BLUEY_CONV_WINDOW_FRAC` | 0.02 | 0.005 | fraction of model context window the tail may use |
| model window | `BLUEY_MODEL_WINDOW` | (table) | — | override the model→context-window lookup |

Effective tail budget = `min(tail_tokens, window_frac × model_window)`. The
model window comes from a substring lookup table (opus/sonnet/haiku → 200k,
gemini → 1M, unknown → 100k conservative), overridable per the env var.

Reserved for Wave 2 (documented as struct comments, not implemented):
`BLUEY_RETRIEVAL_TOP_K`, `BLUEY_RETRIEVAL_TOKENS`.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-core/src/conversation.rs` | Created | Pure memory logic (config, assemble, fold prompt, token estimate, window table) + 14 tests |
| `crates/cue-core/src/lib.rs` | Modified | `pub mod conversation;` + root re-exports; `ConvRole::as_str`/`from_str_lossy` |
| `crates/cue-rag/src/chunker.rs` | Modified | UTF-8 boundary fix + `hash_dedup` + regression tests |
| `crates/cue-rag/src/filter.rs` | Created | `is_self_prompt` + tests |
| `crates/cue-rag/src/lib.rs` | Modified | Re-exports |
| `infra/migrations/012_conversation_turns.sql` | Created | The turn-log table |
| `crates/cue-daemon/src/db/mod.rs` | Modified | Register migration 012 |
| `crates/cue-daemon/src/db/conversation.rs` | Created | Turn-log accessors + 5 tests |
| `crates/cue-daemon/src/conversation.rs` | Created | Daemon orchestration (record / assemble / fold / reset) |
| `crates/cue-daemon/src/lib.rs` | Modified | `pub mod conversation;` |
| `crates/cue-daemon/src/app.rs` | Modified | Daemon fields + 5 reset sites + envelope insertion + turn recording + placement test; `pub(crate)` on 2 helpers |

## Build & Test

```
cargo build -p cue-daemon --target aarch64-apple-darwin                       # ✅
cargo build -p cue-daemon --target aarch64-apple-darwin --features cloud-calendar  # ✅
cargo test  -p cue-core -p cue-rag -p cue-daemon --lib                        # ✅ 475 passed
cargo clippy -p cue-core -p cue-rag -p cue-daemon -- -D warnings              # ✅ clean
cargo clippy -p cue-daemon --features cloud-calendar -- -D warnings           # ✅ clean
cargo fmt --check                                                             # ✅ clean
```

New tests: cue-core 14 (config env/clamps, token estimate, window table,
assemble budget/overflow/ordering, fold prompt, bound), cue-rag 21 (UTF-8
CJK/emoji/accented/mixed, ASCII-unchanged, dedup, self-prompt incl. negative),
cue-daemon 6 (5 db round-trip/prune/delete/clear + envelope placement).

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Envelope insertion done in the async `answer_context_for_question` (not the sync `_within`) by finding the transcript index | The block needs the DB + summary lock (async); the sync builder can't reach them. Placement invariant still guarded by a test. |
| Migration numbered 012 (not 011) | 011 (keybinds) is an inline `ensure_keybinds_table`, not in `run_migrations`; 012 avoids collision. |
| `ConvRole::as_str`/`from_str_lossy` added to cue-core | The type owner is the right home for its DB string form; keeps the daemon accessor clean. |

## Known Follow-ups

- **Wave 2**: session-history retrieval index + `search_agent_history` MCP tool
  (consumes `cue-rag::hash_dedup` + `is_self_prompt`; the harness proved the
  engine at 78% recall@1 / 12ms). Note Agent 1's flag: the fold prompt opens
  with "You maintain the running summary of the conversation…", distinct from
  the meeting summarizer — add that marker to `is_self_prompt` before indexing
  if fold one-shots can leak into agent session files.
- **Wave 3**: ephemeral drive (`--ephemeral` / `--no-session-persistence`, drop
  `--resume`) behind a default-OFF flag, only after live validation below.
- **Persistence**: promote `conv_summary` to disk if restart-mid-meeting summary
  loss proves noticeable (turns already survive; only the fold is lost).

## Needs LIVE validation (the last proof)

Wave 1 is unit-proven; the end-to-end behavior needs one real meeting:

1. Ask two related questions in a meeting; the second ("follow up on that")
   must show the agent recalling its OWN prior answer — proving the block is
   supplied and used.
2. After ~15+ asks (tail overflows ~2000 tokens), confirm a fold fires: a
   `conversation fold` debug line, the rolling summary populates, and the oldest
   turns are deleted from `conversation_turns`.
3. Confirm the ordering in a captured envelope: summary → conversation →
   transcript.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria
- [x] Code style matches CLAUDE.md rules
- [x] No TODOs without linked task IDs (Wave 2/3 are tracked here)
