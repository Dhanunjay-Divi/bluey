# PHASE-3-ROUND-12-PLAN.md

**Status:** Planning. v0.1 alpha shipped from `feat/phase-3-round-11` → main on 2026-05-16.

This round is post-rollout cleanup. None of these are blockers for v0.1 alpha, but each was flagged by codex during the R11 final 🟢 chain review as "follow up later" or by Kiro during implementation as "deferred for now."

## Residual nits (codex final 🟢 review carry-over)

### R12.1 — Harden Responses.tsx for non-empty finished chunks

**Source:** codex final chain review.

**Today:** `crates/cue-dashboard/ui/src/routes/Responses.tsx` accumulates with `(existing?.text || "") + chunk.partial_text`. Now that the daemon emits deltas (R11-B1 fix), this is correct for in-progress chunks. But the "finished" chunk is currently always empty in practice — if the daemon ever emits a non-empty final chunk (e.g. punctuation, trailing whitespace, retry-finalization), the accumulator handles it correctly already, but there is no test proving the contract.

**Fix:** Add a Vitest reducer test in `crates/cue-dashboard/ui/src/routes/Responses.test.tsx` covering:
1. Sequence of deltas → cumulative text matches concatenation.
2. Final chunk with non-empty `partial_text` AND `finished: true` is appended (not dropped).
3. Final chunk with empty `partial_text` AND `finished: true` does not corrupt the accumulator.
4. Out-of-order chunks (response_id sequencing) are handled deterministically.

**Estimate:** 30 min, single PR, no daemon changes.

---

### R12.2 — Simplify or fully wire `overlay_ui_state`

**Source:** codex final chain review.

**Today:** `Daemon::overlay_ui_state: parking_lot::Mutex<OverlayUiState>` exists in `app.rs`. Spawn-overlay clones the *initial* `Idle` state into the reader thread's `Arc<Mutex<OverlayUiState>>`. Two consequences:
1. The Daemon's own `overlay_ui_state` field is read once at spawn but never written to by handlers, so transitions in app.rs are not visible to the production reader thread.
2. The `NativeOverlayHandle` path (used by the cmd-line `cue` flow) maintains its own independent `OverlayUiState` and handlers there *do* mutate it.

The state machine works correctly today because the relaxed gating only blocks the inner submit events (`AttachFilesRequested`, `InstructionsUpdated`) which are emitted only when the user actually opened the panel. But the architecture is split-brained.

**Fix options (decide one):**
- **A. Simplify:** drop `Daemon::overlay_ui_state`. Pass `Arc<Mutex<OverlayUiState>>` from a single owner (either Daemon or the overlay process) and have transitions written from handlers in `app.rs` so the production reader actually consults the live state.
- **B. Fully wire:** keep both copies but synchronize them. When the dashboard sends `open_attach_ui` Tauri command, both `Daemon::overlay_ui_state` AND the reader-thread Arc are updated in lockstep.

Option A is simpler. Recommend A.

**Estimate:** 2 hours.

---

### R12.3 — True 32 random bytes for overlay session token

**Source:** codex final chain review.

**Today:** `crates/cue-daemon/src/overlay.rs::generate_session_token()` concatenates two `Uuid::new_v4().as_simple()` values:
```rust
pub fn generate_session_token() -> String {
    let id = uuid::Uuid::new_v4();
    let id2 = uuid::Uuid::new_v4();
    format!("{}{}", id.as_simple(), id2.as_simple())  // 64 hex chars = 32 bytes
}
```

Each UUID v4 has 122 bits of randomness (6 bits are version/variant), so 244 bits total. That is well past the security threshold but technically not 256 bits. The doc comment claims "32 bytes" — should be true 32 bytes from `getrandom` or `rand::rngs::OsRng`.

**Fix:**
```rust
pub fn generate_session_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
```

Add `rand` and `hex` deps if not already present (rand probably already is via uuid's transitive). Update the doc comment to accurately say "256 bits from OsRng / getrandom."

**Estimate:** 15 min.

---

### R12.4 — Migrate RAG to sqlite-vec / ANN

**Source:** codex final chain review.

**Today:** `crates/cue-rag/` does linear-scan cosine similarity over an embeddings table in sqlite. Fine at <10k docs, will start to hurt over 100k. v0.1 alpha users won't hit this, but pre-beta should switch to a proper vector index.

**Fix:** Adopt [sqlite-vec](https://github.com/asg017/sqlite-vec) or [usearch](https://github.com/unum-cloud/usearch) for ANN search. Migrate the schema with a one-shot DB upgrade.

**Estimate:** 1 day. Defer to dedicated round.

---

### R12.5 — Windows real whisper.cpp

**Source:** codex (deferred from R10).

**Today:** Windows whisper helper at `native/windows/cue-whisper/main.c` is a stub that documents the `BLUEY_WHISPER_MODEL` env var and emits a "not implemented" error JSON. macOS has the real SwiftWhisper/whisper.cpp impl shipped in R10.

**Fix:** Port the SwiftWhisper Swift code to a C/C++ implementation that links whisper.cpp directly on Windows. Use the same JSON IPC format the daemon already speaks to the macOS variant.

**Estimate:** 2-3 days (Windows toolchain + whisper.cpp build). Schedule once we have a Windows test machine in rotation.

---

## Optional Round 12 polish

These are not from codex but came up during R11 implementation:

- **Documentation:** `docs/work/AGENT-ONBOARDING.md` is outdated — mentions cue-audio but not cue-whisper.
- **Telemetry:** the production overlay reader thread logs warnings on rejected events but has no metric counter. Add a counter so we can see how often token/length/state rejections happen in real use.
- **Test cleanup:** `cue-stealth::tests::TEST_ENV_LOCK` should ideally be moved into a shared `test_helpers` module if more tests start mutating env vars.

## Order of work

1. R12.3 (token bytes) — 15 min, mechanical, ship first.
2. R12.1 (Responses.tsx test) — 30 min, no production changes.
3. R12.2 (overlay_ui_state simplify) — 2 hr, touches `app.rs` and `overlay.rs`.
4. R12.4 (sqlite-vec) — separate round, schedule when there is time for a focused day.
5. R12.5 (Windows whisper) — separate round, blocked on Windows test bench.
