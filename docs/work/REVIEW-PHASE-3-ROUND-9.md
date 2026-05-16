# REVIEW: Phase 3 Round 9 — AI Features + RAG + R7 Fix Wave

**Commit range:** `feat/phase-3-round-8..feat/phase-3-round-9`
**Reviewer:** Codex
**Date:** 2026-05-16

## Per-Task Review

### R9.1 — R7 Fix Wave Carried Into R9

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/stt/factory.rs`, `native/windows/cue-whisper/main.c`, `.github/workflows/release.yml`, `infra/scripts/build-sha256-manifest.py`, `crates/cue-dashboard/ui/src/routes/LiveTranscript.tsx` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 The remaining R7 release workflow blocker is resolved in the stacked branch. The release job now checks out the repository before invoking `infra/scripts/build-sha256-manifest.py`.
- 🟢 STT factory construction now attempts enabled providers independently and allows local-only `LocalWhisper` chains where configured.
- 🟢 Live transcript duplicate handling moved to index-aware de-dupe, and later R10 work strengthens this further with `{session_id, index}` keys.

---

### R9.2 — cue-llm Router + Specialized LLMs

| Field | Value |
|-------|-------|
| Files | `crates/cue-llm/**`, `crates/cue-daemon/src/llm/**`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/routes/Responses.tsx` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 The LLM trait/router/provider shape is clean for alpha: failover is constrained to auth/quota errors, provider test seams exist, and the specialized Answer/Recap/Suggest wrappers are easy to reason about.
- 🟢 API keys are routed through keyring-backed helpers for dashboard commands rather than generic settings persistence.
- 🟡 Provider breadth and routing remain intentionally basic. R10/R11 add streaming, but multi-provider answer selection and richer tool/function calling are still future work.

---

### R9.3 — cue-rag

| Field | Value |
|-------|-------|
| Files | `crates/cue-rag/**`, `crates/cue-daemon/src/db/rag.rs`, `crates/cue-daemon/src/app.rs`, `crates/cue-daemon/tests/rag_integration.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 Chunking, dimension validation, session filtering, zero-norm handling, and cascade delete behavior are covered by tests.
- 🟢 Live indexing is fire-and-forget and disabled gracefully when the OpenAI embedding key is absent.
- 🟡 In-memory cosine search is acceptable for v0.1 alpha, but production-scale RAG still needs sqlite-vec or another ANN backend.

---

### R9.4 — Small Wins: Rate Limiter, Passthrough Setting, Keybinds

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/util/rate_limiter.rs`, `crates/cue-core/src/overlay_ipc.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/pages/Settings.tsx` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 Rate limiter arithmetic and concurrency behavior are covered, including the previous zero-refill safety issue.
- 🟢 Keybind persistence and reset behavior are DB-backed and covered.
- 🟡 Native overlay passthrough handlers are still deferred; the IPC/settings groundwork is acceptable as non-blocking infrastructure.

## Cross-Task Findings

- No blocking R9 issues remain on the current stacked branch.
- The R9 deferrals are explicit and mostly moved forward by R10/R11, especially streaming and live transcript de-dupe.

## Build & Test Verification

Verified on the current stacked branch `feat/phase-3-round-11`:

```bash
cargo fmt --all --check                              # ✅
cargo clippy --all-targets -- -D warnings            # ✅
cargo build --all-targets --release                  # ✅
cargo test --all-targets                             # ✅ 354 passed, 14 ignored
cd crates/cue-dashboard/ui && npm run build          # ✅
git diff --check                                     # ✅
```

## Overall Verdict

🟢 **ACCEPT** — Ready to merge as part of the v0.1 alpha stack.

## Follow-ups for Next Batch

- Replace in-memory cosine RAG with sqlite-vec or another ANN backend before larger-scale usage.
- Wire native passthrough behavior in the Swift/C overlays if the UI setting is exposed to users.
- Expand provider routing beyond OpenAI for production answer generation.
