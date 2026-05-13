# REVIEW: Phase 3 Round 3 — Deepgram + Native Overlay IPC

**Commit range:** `5fca21f..d0ef672`
**Reviewer:** Codex
**Date:** 2026-05-13

## Per-Task Review

### P3.R3.1 — Deepgram Nova-3 STT Provider

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/stt/deepgram.rs`, `crates/cue-daemon/src/stt/mod.rs`, `Cargo.toml`, `crates/cue-daemon/Cargo.toml` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ The previous blocker is resolved. `DeepgramProvider::connect` now builds the Deepgram URL, constructs a WebSocket client request, inserts `Authorization: Token <key>`, spawns a supervisor task, and opens the stream through `tokio_tungstenite::connect_async`.
- ✅ The live path is exercised by in-process mock WebSocket tests rather than real Deepgram credentials. Coverage includes auth header capture, final transcript delivery, binary audio forwarding, 401 → `SttError::Auth`, 429 → `SttError::Quota`, and retry/reconnect after an abnormal server close.
- ✅ `build_url` now honors the stricter `stt.emit_partials && deepgram.interim_results` rule, and the language/provider override behavior remains intact.
- ✅ `mask_api_key` no longer byte-slices UTF-8 input.
- 🟡 Minor: provider-level `{"type":"Error"}` frames are surfaced on the event channel while keeping the socket alive. That can be defensible, but the STT fallback plan later says `Provider(_)` should retry once and then fail over. When the router lands, make sure this behavior is intentional and observable.

### P3.R3.2 — Native Overlay IPC

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/overlay.rs`, `crates/cue-daemon/src/bin/overlay_stub.rs`, `crates/cue-daemon/src/lib.rs`, `crates/cue-core/src/overlay_ipc.rs` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ The module docs now correctly state that the watcher observes child exit but does not relaunch in this round.
- ✅ The `send` docs now match the consumed-by-`shutdown(self)` API shape.
- ✅ `OverlayIpcCommand::Echo { payload }` plus stub echoing improves the pipe tests from "variant decoded" to "payload preserved." This is a good test-only observability seam, and the schema change is small.
- ✅ The child-process boundary remains cleanly isolated. `kill_on_drop(true)` is set, writer flushes every line, malformed stdout logs and continues, and invalid executable paths fail at spawn.

### P3.R3.3 — Overlay Pipe Integration Tests

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/tests/overlay_pipe_integration.rs`, `crates/cue-daemon/src/bin/overlay_stub.rs` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ The exact `OverlayMessage` payload is now verified through `Echo { payload }`, covering both `SessionSwitched` with `Some(_)` and `None`, plus `TranscriptPartial`.
- ✅ Tests still use `env!("CARGO_BIN_EXE_overlay-stub")`, remain fast, and exercise a real child process pipe.

### P3.R3.4 — Work Docs

| Field | Value |
|-------|-------|
| Files | `docs/work/FIX-PHASE-3-ROUND-3.md`, `docs/work/PHASE-3-ROUND-3-HANDOFF-FOR-CODEX-REVIEW.md`, `docs/work/PLAN-STT-FALLBACK-CHAIN.md`, `docs/work/AGENT-ONBOARDING.md` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 `docs/work/AGENT-ONBOARDING.md` was committed with stale state. It still says Round 3 has "2 commits, awaiting codex review," "123 passing," and lists "Deepgram live connect()" as pending Round 4. Since this file is specifically meant for new-agent handoff, update it after merge or in the first Round 4 housekeeping commit.
- 🟡 `docs/work/PHASE-3-ROUND-3-HANDOFF-FOR-CODEX-REVIEW.md` is partially stale: the top-level verification block still says 123 pass, the architecture diagram still emphasizes `from_channels seam` rather than the live supervisor/connect path, and the integration-test description still says the stub responds with `Pong` only. This does not block the code, but it should be cleaned so future reviewers do not have to reconcile old and new claims.

## Cross-Task Findings

- The original 🔴 blocker is fixed. Round 3 now delivers a real Deepgram WebSocket provider path plus native overlay IPC.
- Overlay restart-on-crash remains correctly deferred to Round 4.
- STT fallback chain is not implemented, correctly captured as design-only in `docs/work/PLAN-STT-FALLBACK-CHAIN.md`.

## Build & Test Verification

```bash
cargo fmt --all --check                    # ✅
cargo clippy --all-targets -- -D warnings  # ✅
cargo build --all-targets                  # ✅
cargo test --all-targets                   # ✅ 130 passed, 1 ignored
cd crates/cue-dashboard/ui && npm run build # ✅
git diff --check main..HEAD                # ✅
```

## Overall Verdict

🟡 **ACCEPT WITH NITS** — Mergeable from a code perspective; clean the stale handoff/onboarding docs as immediate housekeeping.

## Follow-ups for Next Batch

- Update `docs/work/AGENT-ONBOARDING.md` to reflect the post-fix/post-merge state: Round 3 includes live Deepgram `connect()`, current test count is 130 passing / 1 ignored, and Round 4 should not list Deepgram connect as pending.
- Refresh `docs/work/PHASE-3-ROUND-3-HANDOFF-FOR-CODEX-REVIEW.md` verification counts and architecture diagram before treating it as canonical historical handoff.
- Round 4 can proceed with overlay restart loop + system audio capture. Keep STT fallback router for the later round described in `PLAN-STT-FALLBACK-CHAIN.md`.
