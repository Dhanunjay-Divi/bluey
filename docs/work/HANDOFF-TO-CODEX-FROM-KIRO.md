# HANDOFF — R7 + R8 Review + Remaining Work (Codex Picks Up)

**Author:** kiro
**Date:** 2026-05-16
**Branches:** `feat/phase-3-round-7` (10 commits, 213 tests) + `feat/phase-3-round-8` (7 commits, 224 tests)
**Repo (on uno):** `/Users/uno/Downloads/cue/`

---

## 1. What this doc is for

Single comprehensive handoff to Codex covering three jobs:

1. **Review** Phase 3 Round 7 (3 themes, 10 commits, 213 tests).
2. **Review** Phase 3 Round 8 (2 themes, 7 commits, 224 tests).
3. **Implement** remaining pending items if time permits.

Expected flow:

```
codex reads this doc
   ↓
codex reviews R7 (verdict in REVIEW-PHASE-3-ROUND-7.md)
   ↓
codex reviews R8 (verdict in REVIEW-PHASE-3-ROUND-8.md)
   ↓
if time: codex implements pending items (separate branches OK)
   ↓
codex writes: docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md (overwrite existing stale one)
   ↓
kiro reads that doc, verifies, decides next action
```

---

## 2. Branch state on uno

```
main                              (R3-R6 merged: 6126b28, 201 tests)
  └── feat/phase-3-round-7       (10 commits, 213 tests, 🟢 pipeline green)
       └── feat/phase-3-round-8  (7 commits, 224 tests, 🟢 pipeline green)
```

Previous round branches (`feat/phase-3-round-{4,5,6}`) are merged to main and can be ignored.

---

## 3. Job 1a: Review R7

### Scope: 3 themes

| Theme | Commits | What it does |
|-------|---------|--------------|
| Live transcript UX | `40b9340`, `73c0fb0` | Daemon broadcast channel → file bridge → Tauri event → LiveTranscript route with rolling 200-segment buffer + auto-scroll |
| Local Whisper fallback | `fd8ab9a`, `d4d0800`, `b2c3991` | macOS Swift + Windows C helper stubs (NDJSON IPC); `LocalWhisperProvider` Rust impl; SttRouter 3-tier chain gated by env var |
| Distribution scaffolding | `5181a65`, `0e84903`, `2457314` | GitHub Actions release pipeline, Makefile, Homebrew formula, Scoop manifest, Tauri updater endpoint, INSTALL.md |
| Lint fix | `97aa759` | items-after-test-module reorder in router.rs |

### Key review points

1. **Live transcript:** Does the broadcast channel have bounded capacity? Is auto-scroll threshold (100px) reasonable? Does the poller clean up on unmount?
2. **Whisper provider:** Is the NDJSON protocol well-defined? Does `close()` properly kill the child process? Is `BLUEY_LOCAL_WHISPER_BINARY` env override sufficient as a test seam?
3. **Router 3-tier:** Is the failover test (Auth → Quota → success) exercising the real path? Is the env-var gate clean?
4. **Distribution:** Is `release.yml` syntactically valid? Are Makefile targets correct? Is the Homebrew formula valid Ruby? Is the Scoop manifest valid JSON?
5. **Stubs are honest:** The whisper helpers emit placeholder text via RMS silence detection — they do NOT do real transcription. This is intentional and documented.

### Verification (kiro ran on uno)

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 213 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check main..HEAD                       ✅ clean
```

### Review deliverable

Create: `docs/work/REVIEW-PHASE-3-ROUND-7.md` using `docs/work/TEMPLATE-REVIEW.md` skeleton.

Detailed per-commit checklist: `docs/work/PHASE-3-ROUND-7-HANDOFF-FOR-CODEX-REVIEW.md`

---

## 4. Job 1b: Review R8

### Scope: 2 themes

| Theme | Commits | What it does |
|-------|---------|--------------|
| Process masquerading | `268a138`, `e068044`, `862e480`, `237123d`, `e19c1b4` | `cue-stealth` crate with per-platform FFI (macOS argv[0], Linux prctl, Windows AUMID); startup apply + re-assertion timers; Tauri commands; Settings UI dropdown; placeholder icons |
| Settings regression fix | `972a92b` | Route was mounting `<Placeholder>` instead of real `<Settings />`; fixed |
| Formatting | `d00880a` | cargo fmt normalization across cherry-picks |

### Key review points

1. **FFI safety:** macOS argv[0] overwrite bounded by original strlen, null-padded. No UB.
2. **Re-assertion timers:** 200ms/1s/5s pattern mirrors natively-cluely. Thread does not block Tauri setup.
3. **Tauri commands:** `set_disguise` round-trips correctly; updates all open windows; persists to DB.
4. **Settings regression:** Codex should grep for any other `<Placeholder name="..."` that masks a real component.
5. **Icons:** 6 PNGs at 256×256, simple Pillow generation, documented replacement path.

### Verification (kiro ran on uno)

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 224 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-7..HEAD       ✅ clean
```

### Review deliverable

Create: `docs/work/REVIEW-PHASE-3-ROUND-8.md` using `docs/work/TEMPLATE-REVIEW.md` skeleton.

Detailed per-commit checklist: `docs/work/PHASE-3-ROUND-8-HANDOFF-FOR-CODEX-REVIEW.md`

---

## 5. Job 2: Implement remaining pending work (if time permits)

### Still pending (user-prioritized order)

#### P0 — The "cue" differentiator (headline features)

1. **LLM router with 7 providers + provider chain** — OpenAI, Anthropic, Gemini, Groq, Ollama, Together, Fireworks. Failover + load balancing. API key per provider in Settings.

2. **20+ specialized LLMs** — AnswerLLM, AssistLLM, RecapLLM, SummaryLLM, ActionItemsLLM, FollowUpLLM, etc. Each wraps a system prompt + model selection. During session: rolling transcript → LLM for action items, 60s summary, follow-up suggestions. End-of-session: full summary + extracted action items + decisions. Custom "cues" (user-defined prompts).

3. **Local RAG** — SQLite + sqlite-vec for embedding storage. Semantic search over past sessions. Context injection into LLM prompts.

4. **Screenshot + cropper window** — capture screen region for context injection into LLM prompts.

#### P1 — Small wins

5. **Mouse passthrough toggle** — overlay click-through mode.
6. **User-rebindable keybinds** — settings UI for global shortcut customization.
7. **Token bucket rate limiter** — per-provider rate limiting for LLM API calls.

#### P2 — Infrastructure + reliability

8. **Real whisper.cpp integration** — replace stub helpers with actual whisper.cpp inference; bundle `tiny.en` model (~75MB).
9. **Structured logging + crash reporting** — `tracing-appender` file rotation + panic dump files.
10. **Long-session stress tests** — `#[ignore]` test running 5-minute synthetic pipeline.
11. **Bookmarks / highlights** — SQLite migration + keyboard shortcut + transcript markers.
12. **Session metadata** — title, tags, participants, notes.
13. **Mic device hot-swap mid-session** — detect disappearance, pause, emit UI event.

### Time guidance

- ~4 hours: item 1 (LLM router with provider chain)
- ~8 hours: add item 2 (specialized LLMs — at least 5 core ones)
- ~12 hours: add items 3-4 (RAG + screenshot)

---

## 6. Codex's final deliverable

Write (overwriting the existing stale file):

**`docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md`** with this structure:

```markdown
# Codex → Kiro: R7 + R8 Review + Implementation Handoff

## 1. R7 Verdict
(verdict + REVIEW-PHASE-3-ROUND-7.md path)

## 2. R8 Verdict
(verdict + REVIEW-PHASE-3-ROUND-8.md path)

## 3. What I Implemented
For each item completed:
- Item name
- Branch + commit hashes
- Files changed (table)
- Tests added
- Design notes
- Known limitations

## 4. What I Skipped and Why

## 5. Pipeline Status
(fmt / clippy / build / test counts per branch)

## 6. New Test Count

## 7. Branches Ready for Kiro Review
```

---

## 7. Standing rules (carry-forward)

- Do **not** push to any remote.
- Do **not** rewrite pushed history.
- Conventional Commits.
- Pipeline before each commit:
  - `cargo fmt --all --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo build --all-targets`
  - `cargo test --all-targets`
  - `cd crates/cue-dashboard/ui && npm run build`
  - `git -P diff --check main..HEAD`
- API keys never logged raw
- Native helpers stay child-process based (no Rust FFI)
- Subagents: use `git worktree add` for isolation

---

## 8. Reference docs on uno

- `docs/work/IMPL-PHASE-3-ROUND-7.md` — R7 detailed implementation record
- `docs/work/IMPL-PHASE-3-ROUND-8.md` — R8 detailed implementation record
- `docs/work/PHASE-3-ROUND-7-HANDOFF-FOR-CODEX-REVIEW.md` — R7 per-commit review checklist
- `docs/work/PHASE-3-ROUND-8-HANDOFF-FOR-CODEX-REVIEW.md` — R8 per-commit review checklist
- `docs/work/PLAN-STT-FALLBACK-CHAIN.md` — STT architecture design
- `docs/work/PLAN-DISTRIBUTION.md` — distribution design decisions
- `docs/work/AGENT-ONBOARDING.md` — full project context for new agents

Good luck.
