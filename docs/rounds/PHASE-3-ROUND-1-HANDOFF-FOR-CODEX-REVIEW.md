# Phase 3 Listening Upgrade — Round 1 Handoff for Codex Review

**Branch**: `feat/phase-3-listening-upgrade`
**Base**: `21b4b35` (main post-Phase-2-merge)
**Tip**: `eef62b6` (this round's last feature commit; IMPL+handoff docs added on top)

## Scope of this round

Phase 3 is a 3-week scope in the master plan. This round ships **Round 1 of 2-3**: foundations + Phase 2 follow-ups. Concrete capture/VAD/STT implementations land in subsequent rounds on top of these types.

**What shipped:**

1. **P3A — Phase 2 follow-ups (all three carried forward):**
   - Dashboard UI switched from `BrowserRouter` to `HashRouter` (Tauri packaging / deep-linking friendly)
   - Command palette "New session" actually creates + navigates (was dead nav to `/chats`)
   - Active session id now persisted across daemon restarts via new `app_state` key-value table
2. **P3B — PCM audio types** (`cue-core::pcm`): `AudioSource`, `SampleRate`, `AudioChunk` with capture-timestamp propagation
3. **P3C — VAD type surface** (`cue-core::vad`): `FrameAction`, `VadAggressiveness`, `VadConfig`
4. **P3D — STT trait foundation** (`cue-core::stt`): `SttProvider` trait, `TranscriptEvent`, `ConnectionState`, `SttError` with retry/failover classification
5. **P3E — Daemon↔overlay IPC types** (`cue-core::overlay_ipc`): `OverlayMessage` with `SessionSwitched`/`TranscriptPartial`/`TranscriptFinal`/`ListeningStateChanged`/`Ping`, NDJSON encode/decode helpers

## Commits (chronological)

```
eef62b6 feat(core): add pcm/vad/stt/overlay_ipc modules as Phase 3 foundation [P3]
ad50a6e feat(daemon+dashboard): persist active session id across restarts [P3-followup]
755dc18 fix(dashboard-ui): switch to HashRouter + wire New Session command palette action [P3-followup]
```

Plus IMPL + this handoff doc committed separately.

## Verification

```
cargo fmt --all --check                            ✅ pass
cargo clippy --all-targets -- -D warnings          ✅ pass
cargo build --all-targets --release                ✅ 41.65s
cargo test --all-targets                           ✅ 68 pass (45 core + 23 daemon)
                                                        was 49 — added 19 core + 3 daemon
cd crates/cue-dashboard/ui && npm run build        ✅ 297 KB JS (94 KB gz)
git diff --check main..HEAD                        ✅ clean
python3 tomllib .codex/agents/*.toml               ✅ all 7 valid
```

## Test count breakdown

| Crate | Before round | After round | Added |
|---|---|---|---|
| cue-core | 26 | 45 | +19 (4 pcm + 3 vad + 5 stt + 7 overlay_ipc) |
| cue-daemon | 20 | 23 | +3 (app_state round-trip, active-session valid, active-session stale recovery) |
| **Total** | **46** | **68** | **+22** |

## Layer changes

### INFRA
- New workspace deps: `cpal`, `ringbuf`, `webrtc-vad`, `bytemuck`, `async-trait`, `futures-util`, `tokio-tungstenite`, `parking_lot`, `thiserror`. Only `async-trait` + `thiserror` are imported by any crate this round — the rest are pre-wired for the next round's concrete impls.
- New migration `infra/migrations/004_app_state.sql` — small key-value table for persisted daemon state.

### DAEMON (`crates/cue-daemon/`)
- New `Database::get_app_state / set_app_state` key-value helpers
- New `Database::load_active_session / save_active_session` convenience methods
- Migration 004 wired via `include_str!` + `execute_batch`
- 3 new regression tests for app_state

### DASHBOARD Rust (`crates/cue-dashboard/`)
- `commands.rs`:
  - `set_active_session` now persists to DB after in-memory update (best-effort, warn on DB failure)
  - `delete_session` clears persisted active when deleted session was active
- `lib.rs`:
  - On startup: `db.load_active_session()` restores previous selection
  - `ActiveSessionState` initialized with restored value instead of `None`

### DASHBOARD UI (`crates/cue-dashboard/ui/`)
- `App.tsx` — `BrowserRouter` → `HashRouter`
- `components/CommandPalette.tsx` — typed `PaletteCommand` union, async `run()` dispatcher, `create_session` invoke for "New session", busy-state UI

### CORE (`crates/cue-core/`)
- New modules: `pcm`, `vad`, `stt`, `overlay_ipc` (registered in `lib.rs`)
- Dep additions to `Cargo.toml`: `async-trait`, `thiserror` via workspace

## Design decisions worth calling out

1. **Separate `pcm` module instead of extending existing `audio` module.** `cue-core::audio` already has a rich type set (`AudioPipelineStatus`, `AudioBackend`, etc.) representing the current product's listening-pipeline config. The new Phase 3 types are PCM-primitive-level (single chunk of samples), so they get their own module. Eventually both converge as concrete capture implementations land, but keeping them distinct now avoids collision and clarifies ownership.

2. **Separate `overlay_ipc` module instead of extending existing `overlay` module.** `cue-core::overlay` holds the UI-level `OverlayCommand` (`Show`/`Hide`/`PushCard`/etc.) for in-process control. The new `overlay_ipc` module holds the daemon↔overlay-process wire protocol (`OverlayMessage`). Two distinct channels, two distinct consumers, two modules. `OverlayIpcCommand` renamed from original draft `OverlayCommand` specifically to avoid overloading the name.

3. **Persistence is best-effort, not blocking.** `set_active_session` does not fail when the DB write errors — memory is the source of truth during the request; DB catches up. User actions must never be held up by persistence.

4. **Active-session recovery is defensive.** `load_active_session()` returns `Ok(None)` on malformed UUIDs and on ids pointing at deleted sessions. Explicit test coverage proves no stale data is resurrected into the UI.

5. **`AudioChunk.captured_at_ms`** is a first-class field specifically for the end-to-end latency instrumentation (`L6` task in the master plan). Every stage preserves it.

## Known quirks

1. **Warnings from `tauri-nspanel`** still present (8 from `panel_delegate!` macro on the `cocoa` deprecations). Same as previous phases — suppressed via `[lints.rust] unexpected_cfgs = allow ...` in `cue-dashboard/Cargo.toml`. Not new in this round.

2. **`tokio-tungstenite` in workspace but unused** this round. Declared so the next round (Deepgram Nova-3 WebSocket provider) can use it via `.workspace = true` without another Cargo manifest edit.

3. **`cue-core` doesn't depend on `tokio`.** Deliberately — the `SttProvider` trait uses `#[async_trait]` but that's runtime-agnostic. Implementations in `cue-daemon` pull in tokio themselves.

## Next rounds

Round 2 (concrete implementations, ~1-1.5 weeks):
- `cue-daemon::audio::capture::MicrophoneCapture` — CPAL-backed, lock-free ring buffer, sample-rate detection, 20 ms chunks emitting `AudioChunk`
- `cue-daemon::audio::vad::RmsGate` + `WebRtcVad` — the two-stage VAD running on `AudioChunk` streams, emitting `FrameAction`
- `cue-daemon::stt::mock::MockStt` — test double implementing `SttProvider` with deterministic event sequences (unlocks integration tests)

Round 3 (Deepgram + overlay wire-up, ~1 week):
- `cue-daemon::stt::deepgram::DeepgramNova3` — persistent WebSocket, auth, reconnect with backoff, partial+final decoding, word-level timing
- Daemon side of overlay IPC: spawn overlay as child, pipe `encode_ndjson` messages, listen for `OverlayIpcCommand`
- End-to-end integration test: mic → VAD → MockStt → transcript events routed to dashboard AND overlay

Round 4 (system audio + platform impls, ~1-1.5 weeks):
- macOS ScreenCaptureKit loopback
- Windows WASAPI loopback
- Native Swift/C overlay code updates to consume `OverlayMessage::SessionSwitched`

## Review checklist for codex

### Correctness
- [ ] `SampleRate::new(0)` → `None`; `new(16_000)` → `Some(16000)`; `hz()` returns raw value
- [ ] `AudioChunk::duration_ms()` returns 0 when sample rate is 0 Hz (impossible to construct but defensive)
- [ ] `FrameAction::should_forward()` true for `Send` + `SendSilence`, false for `Drop`
- [ ] `SttError::is_retryable()` true for Network/Protocol/Provider; false for Auth/Quota/AudioFormat
- [ ] `SttError::should_failover()` true for Auth/Quota only
- [ ] `TranscriptEvent` JSON shape uses `"kind"` as tag field (not `"type"` which is overloaded)
- [ ] `OverlayMessage` and `OverlayIpcCommand` use separate tag fields / serde layouts
- [ ] `encode_ndjson` appends exactly one `\n`, never more
- [ ] `decode_ndjson` of unknown `"type"` returns `Err`, not `Ok` with some default

### Persistence
- [ ] `load_active_session()` does not panic on corrupt UUID string
- [ ] `load_active_session()` returns `None` for well-formed UUID pointing at deleted session (regression test verifies)
- [ ] `save_active_session(None)` clears the row — subsequent load returns `None` (tested)
- [ ] Startup recovery code in `lib.rs` logs warning on DB error and initializes with `None` rather than panicking

### Dashboard follow-ups
- [ ] HashRouter change does not break any route — verify `/chats`, `/session/:id`, `/settings`, `/prompts` all navigate correctly with `/#/...` URL form
- [ ] Command palette "New session" actually calls `create_session` (not dead nav) — check the `invoke("create_session", {title: null})` call
- [ ] Command palette closes after successful creation; stays open on error
- [ ] "Creating session..." placeholder shows during in-flight invoke
- [ ] Palette input is disabled during busy state

### Tests
- [ ] 22 new tests (19 core + 3 daemon) all pass
- [ ] 46 existing tests (26 core + 20 daemon) all still pass
- [ ] `test_active_session_persistence_recovers_from_stale_id` actually exercises the deleted-session-while-active case, not a happy path

### Style
- [ ] `cargo fmt --all --check` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] No new `unwrap()` in non-test code
- [ ] Module doc comments explain each module's purpose (not just what the code is)
- [ ] `#[serde(rename_all = ...)]` used consistently — `snake_case` for Rust-faced enums

### Next-round readiness
- [ ] `SttProvider` trait signature sufficient for Deepgram Nova-3 streaming WS (partial + final, reconnect state, auth error surfacing, word-level timing in Final events)
- [ ] `AudioChunk` carries everything CPAL capture needs to produce (source, sample rate, samples, capture timestamp)
- [ ] `OverlayMessage::SessionSwitched` carries session id + title so the native overlay can render without a round-trip
- [ ] Workspace deps for next round (`cpal`, `ringbuf`, `webrtc-vad`, `tokio-tungstenite`, etc.) pre-wired

## Verdict request

Codex: review the 3 commits + new modules + tests + doc. Write `docs/work/REVIEW-PHASE-3-ROUND-1.md` with verdict.

If 🟢 ACCEPT → merge to main and I start Round 2 (CPAL capture + real VAD + MockStt).
If nits → I fold them into Round 2.
If 🔴 REQUEST CHANGES → I write `docs/work/FIX-PHASE-3-ROUND-1.md` using `TEMPLATE-FIX.md`.
