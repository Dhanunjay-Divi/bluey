# HANDOFF — R10 Review (Codex Picks Up)

**Author:** kiro
**Date:** 2026-05-16
**Branch:** `feat/phase-3-round-10` (12 commits, 299 tests)
**Repo (on uno):** `/Users/uno/Downloads/cue/`

---

## 1. What this doc is for

Single comprehensive handoff to Codex covering:

1. **Review** Phase 3 Round 10 (4 themes, 12 commits, 299 tests).
2. **Implement** remaining pending items if time permits.

**Context:** R7 (fix-3 recheck at `a5991b2`) and R8 fixes are still awaiting your final verdict. R9 is also pending review. R10 is the newest piece.

Expected flow:

```
codex reads this doc
   ↓
codex reviews R10 (verdict in REVIEW-PHASE-3-ROUND-10.md)
   ↓
if time: codex gives final verdicts on R7-fix-3 + R9
   ↓
if time: codex implements pending items (separate branches OK)
   ↓
codex writes: docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md (overwrite existing)
   ↓
kiro reads that doc, verifies, decides next action
```

---

## 2. Branch state on uno

```
main                                    (R3-R6 merged: 6126b28, 201 tests)
  └── feat/phase-3-round-7             (10 commits, 213 tests, fix-3 recheck pending)
       └── feat/phase-3-round-8        (7 commits, 224 tests, 🟡 accepted with nits)
            └── feat/phase-3-round-9   (16 commits, 284 tests, pending review)
                 └── feat/phase-3-round-10  (12 commits, 299 tests, 🟢 pipeline green)
```

---

## 3. Job 1: Review R10

### Scope: 4 themes

| Theme | Commits | What it does |
|-------|---------|--------------|
| AI hookup completion | `75e5df7`, `94bf079`, `32528dc`, `4bcbeb8` | Cmd+Shift+A → AnswerLLM via question-detect; auto-recap on session end; whisper-stub e2e test; live transcript Map dedup |
| Streaming LLM | `0aa3962`, `2cd7ca2`, `849257e` | `complete_stream()` trait method + `LlmChunk`; Anthropic SSE, OpenAI SSE, Ollama NDJSON streaming impls; dashboard chunk subscriber with typing indicator |
| Hardening basics | `a00d0b6`, `6b04f3c` | Anti-debug (PT_DENY_ATTACH / IsDebuggerPresent / TracerPid); obfstr for API URLs + auth headers |
| Real whisper.cpp | `bf7a351`, `010a884` | macOS SwiftWhisper integration (whisper_full C API); Windows stub documents model env + defers |
| Reconciliation | `5943e5a` | cargo fmt + doc sync |

### Key review points

1. **Streaming trait design:** Is `LlmChunkStream` (Pin<Box<dyn Stream>>) the right abstraction? Does the default fallback correctly wrap `complete()`? Does the router's failover work on first-chunk errors?
2. **SSE/NDJSON parsing:** Are the three provider parsers robust to malformed events? Do they handle connection drops gracefully?
3. **Hotkey path:** Is loading last ~10 segments sufficient context? Is `ends_with_question()` (naive `trim().ends_with('?')`) acceptable for v0.1?
4. **Auto-recap:** Is fire-and-forget safe (no dangling futures on app exit)? Is the graceful skip on missing provider correct?
5. **Anti-debug:** Is PT_DENY_ATTACH called early enough? Is the Windows watchdog truly daemon (won't prevent exit)? Is TracerPid parsing robust?
6. **obfstr:** Are all documented strings covered? Is the `strings` grep verification trustworthy?
7. **whisper.cpp:** Is SwiftWhisper version pin sufficient? Is the RMS gate threshold (0.01) reasonable? Is the NDJSON ABI truly unchanged?

### Verification (kiro ran on uno)

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 299 pass, 10 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-9..HEAD       ✅ clean
strings target/release/cue-dashboard | grep "wss://api.deepgram.com"   ✅ 0 matches
```

### Review deliverable

Create: `docs/work/REVIEW-PHASE-3-ROUND-10.md` using `docs/work/TEMPLATE-REVIEW.md` skeleton.

Detailed per-commit checklist: `docs/work/PHASE-3-ROUND-10-HANDOFF-FOR-CODEX-REVIEW.md`

---

## 4. Job 2: Implement remaining pending work (if time permits)

### Still pending (user-prioritized order)

1. **sqlite-vec swap** — replace in-memory cosine with native ANN search in VectorStore.
2. **Native overlay passthrough handlers** — macOS Swift `window.ignoresMouseEvents` + Windows C `WS_EX_TRANSPARENT`.
3. **Multi-provider embedding** — Ollama, local ONNX models alongside OpenAI.
4. **Structured logging + crash reporting** — `tracing-appender` file rotation + panic dump files.
5. **Long-session stress tests** — `#[ignore]` test running 5-minute synthetic pipeline.
6. **Mic device hot-swap mid-session** — detect disappearance, pause, emit UI event.

---

## 5. Codex's final deliverable

Write (overwriting the existing file):

**`docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md`** with this structure:

```markdown
# Codex → Kiro: R10 Review + Implementation Handoff

## 1. R10 Verdict
(verdict + REVIEW-PHASE-3-ROUND-10.md path)

## 2. R7-fix-3 Final Verdict
(if addressed: final accept/reject for the release workflow checkout blocker)

## 3. R9 Verdict
(if addressed: verdict + REVIEW-PHASE-3-ROUND-9.md path)

## 4. What I Implemented
For each item completed:
- Item name
- Branch + commit hashes
- Files changed (table)
- Tests added
- Design notes
- Known limitations

## 5. What I Skipped and Why

## 6. Pipeline Status
(fmt / clippy / build / test counts per branch)

## 7. Branches Ready for Kiro Review
```

---

## 6. Standing rules (carry-forward)

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

## 7. Reference docs on uno

- `docs/work/IMPL-PHASE-3-ROUND-10.md` — R10 detailed implementation record
- `docs/work/PHASE-3-ROUND-10-HANDOFF-FOR-CODEX-REVIEW.md` — R10 per-commit review checklist
- `docs/work/IMPL-PHASE-3-ROUND-9.md` — R9 detailed implementation record
- `docs/work/PHASE-3-ROUND-9-HANDOFF-FOR-CODEX-REVIEW.md` — R9 per-commit review checklist
- `docs/work/REVIEW-PHASE-3-ROUND-7.md` — R7 review (recheck-3 pending final verdict)
- `docs/work/REVIEW-PHASE-3-ROUND-8.md` — R8 review (🟡 accepted with nits)
- `docs/work/AGENT-ONBOARDING.md` — full project context for new agents

Good luck.
