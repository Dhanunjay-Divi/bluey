# HANDOFF — R7 Review + Remaining Work (Codex Picks Up)

**Author:** kiro
**Date:** 2026-05-15
**Branch:** `feat/phase-3-round-7` (10 commits ahead of main)
**Repo (on uno):** `/Users/uno/Downloads/cue/`

---

## 1. What this doc is for

Single comprehensive handoff to Codex covering two jobs:

1. **Review** Phase 3 Round 7 (3 themes, 10 commits, 213 tests).
2. **Implement** remaining pending items if time permits.

Expected flow:

```
codex reads this doc
   ↓
codex reviews R7 (verdict in REVIEW-PHASE-3-ROUND-7.md)
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
  └── feat/phase-3-round-7       (10 commits, 213 tests, 🟢 pipeline green, awaiting review)
```

Previous round branches (`feat/phase-3-round-{4,5,6}`) are merged to main and can be ignored.

---

## 3. Job 1: Review R7

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

Detailed per-commit checklist is in: `docs/work/PHASE-3-ROUND-7-HANDOFF-FOR-CODEX-REVIEW.md`

---

## 4. Job 2: Implement remaining pending work (if time permits)

Items completed in R7 (remove from pending):
- ~~Live transcript UX~~ ✅
- ~~Local whisper fallback (stub)~~ ✅
- ~~Distribution scaffolding~~ ✅
- ~~Wire OpenAI into SttRouter~~ ✅ (done in R7 as part of 3-tier chain)

### Still pending (priority order)

#### P0 — Required for alpha

1. **Real whisper.cpp integration** — replace stub helpers with actual whisper.cpp inference. Bundle `tiny.en` model (~75MB). macOS: link whisper.cpp via SPM or vendored source. Windows: link via CMake.

2. **LocalWhisperProvider crash restart loop** — supervisor pattern: if helper process dies, wait 1s, respawn up to 3 times, then return permanent error to router.

#### P1 — Important quality / reliability

3. **Structured logging + log rotation** — `tracing-appender` `RollingFileAppender`, daily rotation, 7-day retention. Path: `dirs::data_local_dir().join("bluey/logs")`. `--log-level` CLI flag.

4. **Crash reporting** — on panic, write stack trace + recent log tail to `bluey/crashes/<timestamp>.log`. Dashboard shows "previous session crashed" toast if fresh crash file exists.

5. **Long-session stress test** — `#[ignore]` test running pipeline for 5 minutes with synthetic audio. Assert no panic, no monotonic memory growth above threshold.

6. **Bookmarks / highlights** — SQLite migration `009_bookmarks.sql`. Tauri commands. Keyboard shortcut during live session. Markers in transcript view.

7. **Session metadata** — title (auto-suggest from first transcript), tags, participants, notes. New migration. Edit UI in session detail.

8. **Mic device hot-swap mid-session** — detect device disappearance, pause session, emit Tauri event for UI prompt.

#### P2 — Differentiators (the "cue" in the product)

9. **AI features** — LLM API key in Settings. During session: rolling transcript → LLM for action items, 60s summary, follow-up suggestions. End-of-session: full summary + extracted action items + decisions. Custom "cues" (user-defined prompts).

10. **Auto-update endpoint signing keys** — `tauri signer generate`, store private key as GitHub Secret, commit public key.

### Time guidance

- ~2 hours: items 1-2 (real whisper + crash restart)
- ~4 hours: add items 3-4 (logging + crash reporting)
- ~8 hours: add items 5-7 + starter AI features (end-of-session summary)

---

## 5. Codex's final deliverable

Write (overwriting the existing stale file):

**`docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md`** with this structure:

```markdown
# Codex → Kiro: R7 Review + Implementation Handoff

## 1. R7 Verdict
(verdict + REVIEW-PHASE-3-ROUND-7.md path)

## 2. What I Implemented
For each item completed:
- Item name
- Branch + commit hashes
- Files changed (table)
- Tests added
- Design notes
- Known limitations

## 3. What I Skipped and Why

## 4. Pipeline Status
(fmt / clippy / build / test counts per branch)

## 5. New Test Count

## 6. Branches Ready for Kiro Review

## 7. Pending Followups
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

- `docs/work/IMPL-PHASE-3-ROUND-7.md` — detailed implementation record
- `docs/work/PHASE-3-ROUND-7-HANDOFF-FOR-CODEX-REVIEW.md` — per-commit review checklist
- `docs/work/PLAN-STT-FALLBACK-CHAIN.md` — STT architecture design
- `docs/work/PLAN-DISTRIBUTION.md` — distribution design decisions
- `docs/work/AGENT-ONBOARDING.md` — full project context for new agents

Good luck.
