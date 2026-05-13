# REVIEW: Phase 3 Listening Upgrade — Round 1

**Commit range:** `21b4b35..c7b9236`
**Reviewer:** Codex
**Date:** 2026-05-13

## Per-Task Review

### P3 Follow-Ups — HashRouter, Command Palette Action, Active Session Persistence

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/App.tsx`, `crates/cue-dashboard/ui/src/components/CommandPalette.tsx`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/src/lib.rs`, `crates/cue-daemon/src/db/mod.rs`, `infra/migrations/004_app_state.sql` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `BrowserRouter` is replaced with `HashRouter`, preserving the existing route table while making the static Tauri bundle safer (`crates/cue-dashboard/ui/src/App.tsx:1-24`).
- 🟢 The command palette now distinguishes navigation commands from a real `new-session` command, invokes `create_session`, navigates to `/session/:id`, closes on success, and disables input while busy (`crates/cue-dashboard/ui/src/components/CommandPalette.tsx:15-96`).
- 🟢 `app_state` migration is small and general-purpose (`infra/migrations/004_app_state.sql:6-10`).
- 🟢 `get_app_state`, `set_app_state`, `load_active_session`, and `save_active_session` cover the persisted active-session path (`crates/cue-daemon/src/db/mod.rs:214-270`).
- 🟢 Dashboard startup restores persisted active session defensively and falls back to `None` on recovery failure (`crates/cue-dashboard/src/lib.rs:43-51`).
- 🟢 `set_active_session` validates target existence, updates memory, best-effort persists, drops locks before emitting, and emits only on actual change (`crates/cue-dashboard/src/commands.rs:120-163`).
- 🟡 `delete_session` still returns an error if reacquiring the DB lock for persistence fails after the session was already deleted and active memory was cleared (`crates/cue-dashboard/src/commands.rs:83-93`). This is only a poisoned-lock edge case, but Round 2 can make the "best effort" guarantee stricter by logging lock/persistence failures and still emitting the in-memory change.

---

### P3 Core PCM Types

| Field | Value |
|-------|-------|
| Files | `crates/cue-core/src/pcm.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `SampleRate::new(0)` rejects zero, and `hz()` exposes the raw rate.
- 🟢 `AudioChunk` carries source, sample rate, PCM16 samples, and first-sample capture timestamp.
- 🟢 Duration and byte-length helpers are straightforward and covered for 16 kHz and 48 kHz chunks.
- 🟢 The module-level docs clearly separate PCM primitives from the older `audio` pipeline status/config types.

---

### P3 Core VAD Types

| Field | Value |
|-------|-------|
| Files | `crates/cue-core/src/vad.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `FrameAction::should_forward()` correctly forwards `Send` and `SendSilence`, and drops only `Drop`.
- 🟢 `VadAggressiveness::as_u8()` matches the WebRTC 0-3 scale.
- 🟢 Defaults are sensible for the first implementation pass: aggressive mode, 0.02 RMS threshold, 25-frame hangover.

---

### P3 Core STT Trait Foundation

| Field | Value |
|-------|-------|
| Files | `crates/cue-core/src/stt.rs`, `crates/cue-core/Cargo.toml`, root `Cargo.toml` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `TranscriptEvent` separates partial, final, and speaker-label events with stable `kind` tagging.
- 🟢 `WordTiming` is present for Deepgram/AssemblyAI-style word-level timings.
- 🟢 `ConnectionState::Reconnecting { attempt }` round-trips through serde and gives the UI a useful state surface.
- 🟢 `SttError::is_retryable()` and `should_failover()` make retry/failover classification explicit and mutually sensible.
- 🟢 `SttProvider` is runtime-agnostic and streaming-oriented.
- 🟡 Several workspace deps are pre-wired but unused in this round (`cpal`, `ringbuf`, `webrtc-vad`, `bytemuck`, `futures-util`, `tokio-tungstenite`, `parking_lot`). The handoff explains this clearly; Round 2 should either consume them or prune any that prove unnecessary.

---

### P3 Overlay IPC Types

| Field | Value |
|-------|-------|
| Files | `crates/cue-core/src/overlay_ipc.rs` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `OverlayMessage::SessionSwitched` carries both optional session id and title, enough for native overlay display without a round-trip.
- 🟢 Listening-state and transcript partial/final variants establish the next overlay wire path.
- 🟢 `OverlayIpcCommand` is separate from existing `cue-core::overlay::OverlayCommand`, avoiding name collision.
- 🟢 `encode_ndjson` appends exactly one newline, and `decode_ndjson` accepts newline-terminated input.
- 🟢 Unknown message types fail to decode instead of silently defaulting.

---

### Docs and Handoff

| Field | Value |
|-------|-------|
| Files | `docs/work/IMPL-PHASE-3-ROUND-1.md`, `docs/work/PHASE-3-ROUND-1-HANDOFF-FOR-CODEX-REVIEW.md` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 Implementation notes match the code shape and the round boundary.
- 🟢 Known out-of-scope work is correctly deferred to later Phase 3 rounds.
- 🟡 The handoff's "chronological" commit list is reversed relative to `git log --reverse main..HEAD`, and the `Tip` field names `eef62b6` even though the review range includes the doc commit `c7b9236`. Documentation-only nit.
- 🟡 `docs/work/REVIEW-PHASE-2-SESSION-UX.md` is added in this Phase 3 branch because it was not present on `main`. Fine to merge as review history, but it is not Phase 3 implementation.

## Cross-Task Findings

- The foundation layer is cleanly scoped: it adds shared PCM/VAD/STT/overlay IPC contracts without pretending concrete capture or Deepgram STT exists yet.
- Phase 2 follow-ups are handled well enough to unblock real listening work.
- No blocker-level correctness issues found in the changed code paths.

## Build & Test Verification

```bash
cargo fmt --all --check                   # ✅
cargo clippy --all-targets -- -D warnings # ✅
cargo build --all-targets --release       # ✅
cargo test --all-targets                  # ✅ 68 passed, 0 failed
cargo test -p cue-core                    # ✅ 45 passed, 0 failed
cargo test -p cue-daemon                  # ✅ 23 passed, 0 failed
cd crates/cue-dashboard/ui && npm run build # ✅
git diff --check main..HEAD               # ✅
```

## Overall Verdict

🟢 **ACCEPT** — Ready to merge.

## Follow-ups for Next Batch

- Round 2 should consume or prune the pre-wired audio/STT workspace dependencies.
- Tighten the best-effort persistence path in `delete_session` so persistence lock/write failures never turn an already-applied in-memory active-session clear into a user-facing command error.
- Add concrete MockStt and VAD/capture tests against these traits before the Deepgram provider lands.
- When overlay IPC is wired, add an integration test proving `SessionSwitched` is emitted through the native overlay process pipe, not only represented as a type.
