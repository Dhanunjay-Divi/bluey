# IMPL — Phase 3 Listening Upgrade (Round 1 — Foundations + Follow-ups)

**Scope**: Phase 3 Round 1. Follow-ups from Phase 2 + type/trait foundation for audio, VAD, STT, overlay IPC.
**Branch**: `feat/phase-3-listening-upgrade`
**Base**: `21b4b35` (main post-Phase-2-merge)
**Commits in this round**: 3

Phase 3 is scoped as ~3 weeks in the master plan. This round ships the **foundations** so subsequent rounds can drop concrete implementations (CPAL capture, RMS+WebRTC VAD, Deepgram Nova-3 WS, platform system-audio, daemon↔overlay IPC wiring) onto stable traits.

## Commits

| Hash | Title |
|---|---|
| `755dc18` | `fix(dashboard-ui): switch to HashRouter + wire New Session command palette action [P3-followup]` |
| `ad50a6e` | `feat(daemon+dashboard): persist active session id across restarts [P3-followup]` |
| `eef62b6` | `feat(core): add pcm/vad/stt/overlay_ipc modules as Phase 3 foundation [P3]` |

## Carried follow-ups from Phase 2 (all resolved this round)

### HashRouter switch
- **Why**: Tauri packages its webview without a backend server; `BrowserRouter` needs the server to serve `index.html` for every path. `HashRouter` works with a static bundle and supports deep-linking once we wire file-level URL schemes.
- **Change**: `crates/cue-dashboard/ui/src/App.tsx` — `BrowserRouter` → `HashRouter`. All routes and navigations (`navigate("/chats")`, `navigate("/session/:id")`) work identically; URLs change from `/session/abc` to `/#/session/abc`.

### Command palette "New session" actually creates
- **Why**: Previously it just navigated to `/chats`, same as "Go to chats" — misleading label.
- **Change**: `crates/cue-dashboard/ui/src/components/CommandPalette.tsx` — replaced the flat `{ label, action }` array with a typed `PaletteCommand` union. `run()` dispatches on `kind`: `"navigate"` goes to a route; `"new-session"` invokes `create_session` then navigates to `/session/:id`. Palette shows a "Creating session..." placeholder while the invoke is in flight and disables input to prevent double-submission.

### Active session persistence
- **Decision**: persist + best-effort recovery. Memory-only was fragile — daemon restarts (app quit, crash, update) would reset the active selection even though the session row still exists in SQLite. Persisted form is a single row in a new `app_state` key-value table.
- **New migration**: `infra/migrations/004_app_state.sql` — `CREATE TABLE IF NOT EXISTS app_state (key TEXT PRIMARY KEY, value TEXT, updated_at INTEGER NOT NULL)`. Extensible for future daemon state.
- **New Database methods**:
  - `get_app_state(key)` / `set_app_state(key, value)` — general key-value primitives
  - `load_active_session()` — reads `active_session_id`, validates the row exists, returns `Option<Uuid>`. Returns `None` (not error) on stale / corrupt id so the daemon starts cleanly.
  - `save_active_session(id: Option<Uuid>)` — writes or clears
- **Wiring**: `cue-dashboard::lib.rs` setup calls `db.load_active_session()` before managing `ActiveSessionState`, restoring the selection. `set_active_session` and `delete_session` commands now call `db.save_active_session(...)` when the active selection changes — best-effort (warn on failure, don't fail the command).
- **3 new tests**: `test_app_state_round_trip`, `test_active_session_persistence_valid`, `test_active_session_persistence_recovers_from_stale_id` (the last one exercises the recovery-after-deletion case).

## New Phase 3 modules in `cue-core`

### `pcm` module (audio primitive types)

**Why a new module name**: `crates/cue-core/src/audio.rs` already exists with an extensive pre-3 pipeline-config type set (`AudioPipelineStatus`, `AudioSourceKind`, `AudioBackend`, etc.). Creating a separate `pcm` module avoids shadowing those types and makes the new PCM-bit handling surface explicit.

**Types**:
- `AudioSource` — enum `Microphone | System` for which source a chunk came from
- `SampleRate` — newtype wrapper, rejects 0 Hz, constants for 16k + 48k
- `AudioChunk` — `{ source, sample_rate, samples: Vec<i16>, captured_at_ms: u64 }`. The `captured_at_ms` field carries the original capture timestamp forward through every pipeline hop for end-to-end latency instrumentation (Phase 3 `L6` task per the master plan).

4 unit tests.

### `vad` module

**Types** (runtime implementations land in `cue-daemon::audio::vad` where they can pull in `webrtc-vad`):
- `FrameAction` — enum `Send | SendSilence | Drop` with `should_forward() -> bool` helper for quick filtering
- `VadAggressiveness` — enum wrapping the 0-3 values the WebRTC VAD expects (`Quality` / `LowBitrate` / `Aggressive` / `VeryAggressive`)
- `VadConfig` — `{ aggressiveness, rms_threshold_start, silence_hangover_frames }`. `rms_threshold_start` expressed as fraction of `i16::MAX` so it scales with sample format. `silence_hangover_frames` defaults to 25 (500 ms at 20 ms frames) — matches the reference-repo baseline.

3 unit tests.

### `stt` module

Core trait + types for all STT providers. Deliberate design choices:

- **Streaming-first** — every provider sends partial transcripts via `next_event()`; one-shot REST is modeled as a single `Final` event on `finalize()`.
- **Partial vs final explicit** — `TranscriptEvent::Partial` carries provisional text that may be replaced; `TranscriptEvent::Final` is committed. This is the exact surface the `L3` stable-partial detector needs.
- **Connection state surfaced** — `ConnectionState` enum with `Reconnecting { attempt: u32 }` so the dashboard can show a "reconnecting" banner with attempt count.
- **Error classification** — `SttError` with `is_retryable()` (retry same provider) and `should_failover()` (switch to next provider). Routers use these to decide.
- **Async trait** — `#[async_trait] SttProvider: Send + Sync` so providers can hold persistent WebSocket connections inside `parking_lot::Mutex` inside a Tauri managed state.

**Types exported**:
- `TranscriptEvent` (Partial / Final / SpeakerLabel)
- `WordTiming` (for providers that supply word-level timing — Deepgram, AssemblyAI)
- `ConnectionState`, `SttError`
- `SttConfig` — language, vocabulary hints, `emit_partials`, source, sample rate
- `SttProvider` trait — `name`, `connection_state`, `send_audio`, `finalize`, `next_event`, `close`

5 unit tests covering error classification, serde round-trip, defaults.

### `overlay_ipc` module (distinct from existing `overlay` module)

**Why a new module name**: `crates/cue-core/src/overlay.rs` already defines `OverlayCommand` + `OverlayPosition` for the UI-level overlay control (show/hide/position/push-card). That's a different concern from the **daemon↔native-overlay-process** JSON-over-stdin wire protocol. The two modules coexist; each serves a distinct audience.

**Types**:
- `OverlayMessage` — daemon → overlay. `SessionSwitched { session_id, title }`, `ListeningStateChanged { state }`, `TranscriptPartial { source, text }`, `TranscriptFinal { source, text }`, `Ping`. Tagged `#[serde(tag = "type", rename_all = "snake_case")]` for a stable, self-describing JSON form.
- `ListeningState` — `Idle | Connecting | Listening | Paused | Failed`
- `OverlayIpcCommand` — overlay → daemon. `Pong`, `RequestSync`. Minimal because overlays are display-only.
- `encode_ndjson(&msg) -> String` — produces single-line NDJSON (newline-terminated). The native receiver reads stdin line-by-line.
- `decode_ndjson(line) -> OverlayMessage` — robust to trailing newline.

7 unit tests: session-switched with id + title, session-switched with both `None`, transcript partial round-trip, ping decode with trailing newline, `OverlayIpcCommand` round-trip, listening state JSON format, unknown-type rejection.

## Workspace dependencies added

```toml
# Phase 3 — audio + STT
cpal = "0.15"
ringbuf = "0.4"
webrtc-vad = "0.4"
bytemuck = "1"
async-trait = "0.1"
futures-util = "0.3"
tokio-tungstenite = { version = "0.24", default-features = false, features = ["rustls-tls-native-roots"] }
parking_lot = "0.12"
thiserror = "1.0"
```

Only `async-trait` and `thiserror` are imported by `cue-core` in this round (used by the STT trait). The rest are reserved for `cue-daemon` in the next round when concrete capture/VAD/STT implementations land.

## Not in scope for this round (next Phase 3 rounds)

- **Concrete CPAL microphone capture** — lock-free ring buffer, platform-aware device selection, sample-rate detection + resampling via rubato.
- **Actual RmsGate + WebRtcVad wrappers** in `cue-daemon::audio::vad` built on these `cue-core::vad` types.
- **Deepgram Nova-3 WebSocket provider** — persistent WS lifecycle, reconnect with backoff, auth, partial+final event decoding.
- **System audio capture** on macOS (ScreenCaptureKit) and Windows (WASAPI loopback).
- **Daemon ↔ overlay process IPC wiring** — the types are defined; spawning the overlay as a child process and piping `encode_ndjson` messages to its stdin lands next round alongside the Swift/C overlay code updates.
- **End-to-end integration test** — mic → VAD → STT → transcript event arrives in dashboard.

These are all explicitly called out as unblocked by the foundations in this round. Each can be reviewed and merged independently.

## Build + test

```
cargo fmt --all --check                 ✅ pass
cargo clippy --all-targets -- -D warnings ✅ pass
cargo build --all-targets --release     ✅ 41.65s (Cargo.lock picks up 9 new workspace deps, nothing broken)
cargo test --all-targets                ✅ 68 pass (45 cue-core, 23 cue-daemon)
                                           was 49 total — added 19 cue-core tests for new modules
                                           + 3 cue-daemon tests for app_state
cd crates/cue-dashboard/ui && npm run build ✅ 297KB JS (94KB gz)
git diff --check main..HEAD             ✅ clean
```

## Deviations from plan

- **Module naming**: the master plan's "audio" scope went into `pcm` (types) + `vad` (types) + eventual `cue-daemon::audio::capture` + `cue-daemon::audio::vad` (implementations) instead of a single `cue-core::audio` (which already exists with different types).
- **Overlay IPC** lives in `cue-core::overlay_ipc` (not `overlay_proto` as originally drafted) to clearly distinguish the daemon↔process wire protocol from the in-process UI-level `OverlayCommand` that already exists.
- **D0.2 V2 deferred nits** — these were already covered in Phase 2's regression tests (`test_invalid_uuid_in_db_row_surfaces_error`, `test_unarchive_clears_archived_at`, `test_duplicate_turn_index_rejected_by_unique_constraint`). This round's new tests are exclusively for Phase 3 additions (app_state, audio types, VAD types, STT trait, overlay IPC).

## Review checklist for codex

### Foundations correctness

- [ ] `SampleRate::new(0)` returns `None` (guards against /0 in `AudioChunk::duration_ms`)
- [ ] `AudioChunk::duration_ms()` and `byte_len()` match across sample rates (test covers 16k + 48k)
- [ ] `AudioChunk.captured_at_ms` is documented as first-sample timestamp and preserved through the pipeline
- [ ] `FrameAction::should_forward()` correctly includes both `Send` and `SendSilence`
- [ ] `VadAggressiveness::as_u8()` returns values matching the upstream `webrtc-vad` crate API (0-3)
- [ ] `SttError::is_retryable()` and `should_failover()` are mutually exclusive by design (auth/quota → failover but NOT retry; network/protocol → retry but NOT failover)
- [ ] `TranscriptEvent` serde uses `#[serde(tag = "kind", rename_all = "snake_case")]` so the JSON form has stable type discrimination
- [ ] `ConnectionState::Reconnecting { attempt }` round-trips through JSON (externally tagged form carries the attempt count)
- [ ] `encode_ndjson` always ends with `\n` (no missing newline edge case)
- [ ] `decode_ndjson` accepts with-or-without trailing newline
- [ ] `OverlayMessage` and `OverlayIpcCommand` use distinct type tags — they're separate wire channels
- [ ] Module `cue-core::overlay_ipc` does not re-export a type named `OverlayCommand` (to prevent clash with existing `cue-core::overlay::OverlayCommand`)

### Persistence correctness

- [ ] `load_active_session()` does NOT panic when the stored id is malformed — returns `Ok(None)`
- [ ] `load_active_session()` returns `Ok(None)` when the id is well-formed but the session was deleted (tested)
- [ ] `save_active_session(None)` actually clears the row's value (tested)
- [ ] `set_active_session` command persists AFTER the in-memory update (order matters for crash safety — memory is authoritative during the call, DB catches up)
- [ ] `delete_session` clears BOTH in-memory AND persisted active selection when the deleted session was active
- [ ] DB write failures inside persistence hooks are logged as warn-level, not errors — active-session persistence is best-effort and should never fail a user action

### Follow-ups

- [ ] HashRouter does not break any existing route — all Phase 2 routes (`/`, `/chats`, `/session/:id`, `/settings`, etc.) still work
- [ ] Command palette's "New session" creates + navigates + closes the palette in the right order
- [ ] Command palette disables input during the in-flight invoke so the user can't double-submit

### Tests
- [ ] 19 new cue-core tests pass (4 pcm, 3 vad, 5 stt, 7 overlay_ipc)
- [ ] 3 new cue-daemon tests pass (app_state round-trip, active-session valid, active-session recovery-from-stale)
- [ ] Existing 26 cue-core + 20 cue-daemon tests all still pass

### Style
- [ ] `cargo fmt --check` clean
- [ ] `cargo clippy -- -D warnings` clean
- [ ] No new `unwrap()` in non-test code
- [ ] New modules include module-level doc comments explaining purpose

### Next-round readiness
- [ ] `SttProvider` trait shape is sufficient to implement Deepgram Nova-3 (streaming WS, partial+final, reconnect, auth errors, word-level timing)
- [ ] `AudioChunk` shape is sufficient for CPAL microphone capture (`source`, `sample_rate`, `samples`, `captured_at_ms`)
- [ ] `OverlayMessage::SessionSwitched` has the fields needed for the Swift/C overlay to render the active session indicator
- [ ] `VadConfig` defaults are reasonable for the first implementation pass
