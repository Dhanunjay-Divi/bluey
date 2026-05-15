# REVIEW: Phase 3 Round 5 — Post-Fix Re-Review

**Commit range:** `2f164cd..77f5e9e` plus R5 fix commits on `feat/phase-3-round-6`
**Reviewer:** Codex
**Date:** 2026-05-15

## Per-Task Review

### P3.R5.F1 — SystemAudioCapture Retention

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/app.rs`, `crates/cue-daemon/tests/system_audio_integration.rs` |
| Verdict | 🟡 minor nit |

**Findings:**
- ✅ The daemon now retains the continuous `SystemAudioCapture` handle in `Daemon.system_audio` instead of dropping it immediately after startup (`crates/cue-daemon/src/app.rs:370`, `crates/cue-daemon/src/app.rs:470`).
- ✅ `shutdown_daemon` now takes and stops the retained capture handle (`crates/cue-daemon/src/app.rs:4710`).
- 🟡 The added test simulates `Option<SystemAudioCapture>` retention rather than exercising daemon startup/shutdown with `BLUEY_SYSTEM_AUDIO_CONTINUOUS=1`. This is acceptable as a narrow regression test, but the next integration pass should cover the real daemon field.

### P3.R5.F2 — Overlay Supervisor Cap + Shutdown

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/overlay.rs`, `crates/cue-daemon/tests/overlay_lifecycle.rs` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ `shutdown()` now marks the supervisor as shutting down, closes the sender, and waits for the supervisor task with a timeout.
- ✅ `run_one_child` treats closed send channels and shutdown requests as clean exits and waits for the child after dropping stdin.
- ✅ Restart attempts are capped and failure drains queued messages, preventing an unbounded crash loop.
- 🟡 Existing follow-up: `msgs_acked` still increments for every stdout line that decodes as an `OverlayMessage`, not only true acknowledgements. This is observability polish, not a blocker.

### P3.R5.F3 — System-Audio STT Event Drain

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/app.rs`, `crates/cue-daemon/tests/system_audio_integration.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The drain is still not attached to the provider receiving audio. The capture task builds `stt` and calls `provider.send_audio(&chunk)` inside the `sys_rx` loop (`crates/cue-daemon/src/app.rs:473` and `crates/cue-daemon/src/app.rs:487`). The new drain task calls `build_system_audio_stt_provider()` again and polls a separate provider instance (`crates/cue-daemon/src/app.rs:4658`). Result: audio is sent to provider A, while transcripts are read from provider B, so real system-audio transcripts still do not reach `add_audio_transcript_segment`.
- 🔴 The regression test does not catch this because `stt_event_drain_forwards_finals_and_handles_errors` manually drains a local `MockStt` it also manually seeds (`crates/cue-daemon/tests/system_audio_integration.rs:208`). It never exercises the production split between the sender task and `drain_system_audio_stt_events`.
- Required fix: create one provider per continuous system-audio session and run send + `next_event()` against that same provider. A common shape would be a single task with `tokio::select!` over `sys_rx.recv()` and `provider.next_event()`, or a shared provider handle that is explicitly designed for concurrent send/drain.

### P3.R5.F4 — FTS Delete Consistency

| Field | Value |
|-------|-------|
| Files | `infra/migrations/008_fts_cascade_fix.sql`, `crates/cue-daemon/src/db/search.rs`, `crates/cue-daemon/src/db/mod.rs` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ Migration 008 rebuilds `transcript_fts` with an explicit `transcript_id` and deletes by that stable key instead of relying on mismatched FTS rowids.
- ✅ `search_transcripts` uses snippet column index `2`, matching the rebuilt table shape (`transcript_id`, `session_id`, `text`, `source`, `ts`).
- ✅ DB tests cover direct transcript delete and session cascade behavior.

### P3.R5.F5 — Safe Search Snippet Rendering

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/routes/Search.tsx` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ Search results no longer use `dangerouslySetInnerHTML`; snippets are tokenized into React text nodes and `<mark>` nodes.
- ✅ This closes the original XSS path from transcript content.

### P3.R5.F6 — Onboarding Mount + Tray Settings Navigation

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/App.tsx`, `crates/cue-dashboard/src/lib.rs` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ The onboarding gate is mounted at app startup and persists through `onboarding_complete`.
- ✅ Tray Settings now shows/focuses the window and emits `navigate_to` with `/settings`; the React side listens and routes it.

## Cross-Task Findings

- The R5 post-fix branch resolves five of the six reviewed blockers, but the system-audio STT event-drain fix is still incorrect in the running daemon.
- Because R6 builds on this branch, any live listening work that depends on continuous system-audio STT should treat R5 as not yet mergeable.

## Build & Test Verification

```bash
cargo fmt --all --check                    # ✅
cargo clippy --all-targets -- -D warnings  # ✅
cargo build --all-targets                  # ✅
cargo test --all-targets                   # ✅ 185 passed, 2 ignored
cd crates/cue-dashboard/ui && npm run build # ✅
git diff --check                           # ✅
```

## Overall Verdict

🔴 **REQUEST CHANGES** — The remaining system-audio STT drain blocker must be resolved before R5 can merge.

## Follow-ups for Next Batch

- Fix continuous system-audio STT so one provider instance owns both `send_audio` and `next_event()` for the same session.
- Replace the current drain test with an integration test that feeds a capture chunk into the production wiring and asserts a transcript reaches the daemon transcript path.
- Keep the overlay ack-count observability nit for a later cleanup unless it becomes operationally confusing.
