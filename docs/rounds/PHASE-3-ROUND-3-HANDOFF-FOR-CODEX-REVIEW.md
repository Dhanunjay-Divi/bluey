# Phase 3 Round 3 — Handoff for Codex Review

**Branch**: `feat/phase-3-round-3`
**Base**: main tip post-Round-2-merge
**Author**: kiro

## Scope

Round 3 of Phase 3. Builds the first streaming STT provider (Deepgram Nova-3) and wires the daemon ↔ native-overlay-process pipe end-to-end, verified by an integration test against a stub overlay binary.

### What shipped

1. **Deepgram Nova-3 WebSocket STT provider** (`crates/cue-daemon/src/stt/deepgram.rs`)
   - `SttProvider` impl with channel-based seam from Round 2's trait
   - URL builder (model / encoding / sample_rate / channels / punctuate / interim / diarize / language)
   - Auth via `Authorization: Token <key>`; `mask_api_key` guards logging
   - JSON frame parser → `TranscriptEvent::{Partial,Final,SpeakerLabel}` with `WordTiming`
   - HTTP handshake status → `SttError` classification (Auth / Quota / AudioFormat / Network / Protocol)
   - `reconnect_delay` exponential backoff 250 ms → 10 s cap
2. **Native overlay IPC** (`crates/cue-daemon/src/overlay.rs`)
   - `NativeOverlayHandle` — spawn + writer + reader + watcher tasks, cleanly separated
   - NDJSON-encoded `OverlayMessage` to child stdin; parsed `OverlayIpcCommand` from child stdout
   - Graceful `shutdown()` via stdin close + bounded task join
3. **Stub overlay binary** (`crates/cue-daemon/src/bin/overlay_stub.rs`)
   - `[[bin]] name = "overlay-stub"` — auto-resolved via `CARGO_BIN_EXE_overlay-stub` in tests
   - Reads NDJSON, responds with `Pong`, exits on stdin EOF
4. **Integration tests** (`crates/cue-daemon/tests/overlay_pipe_integration.rs`)
   - `SessionSwitched` round-trips through real child process (stub)
   - `TranscriptPartial` round-trips
   - `shutdown` is graceful within 5 s

## Commits

```
(tip)   feat(daemon): Deepgram Nova-3 STT + native overlay IPC + overlay-stub bin + integration tests [P3.R3]
```

Plus this doc + the IMPL doc as their own commit.

## Verification — ALL GREEN (post-fix)

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets --release                  ✅
cargo test --all-targets                             ✅ 130 pass
                                                        45 core + 78 daemon lib + 3 overlay pipe
                                                        + 4 pipeline integration + 1 ignored (hw)
cd crates/cue-dashboard/ui && npm run build          ✅ 297 KB JS
git diff --check main..HEAD                          ✅ clean
```

Test count:

| Tier | Round 2 | Round 3 (initial) | Round 3 (final, post-fix) | Δ vs R2 |
|---|---|---|---|---|
| cue-core lib | 45 | 45 | 45 | — |
| cue-daemon lib | 48 | 71 | 78 | +30 (20 deepgram unit + 7 deepgram live + 3 overlay unit) |
| Integration: pipeline | 4 | 4 | 4 | — |
| Integration: overlay pipe | 0 | 3 | 3 | +3 |
| Ignored (hardware) | 1 | 1 | 1 | — |
| **Total running** | **97** | **123** | **130** | **+33** |

## Architecture diagram — Round 3 additions

```
                    ┌─────────────────────────────────────────────┐
                    │               cue-daemon                    │
                    │                                             │
Audio/Mic ──▶ Framer──▶ TwoStageVad──▶ SttProvider                │
                              (VAD pass)   │                      │
                                           │                      │
                                      DeepgramProvider            │
                                  ┌────────┴──────────────┐       │
                                  │   connect() (live)    │       │
                                  │   supervisor + backoff│       │
                                  │   WS reader / writer  │       │
                                  │   parse_frame         │       │
                                  │   build_url, auth hdr │       │
                                  └───────────────────────┘       │
                                                                  │
Active session change ────────▶ NativeOverlayHandle               │
                                  │                               │
                                  │ encode_ndjson(msg)            │
                                  ▼                               │
                                  stdin (piped)                   │
                    └──────────────┼──────────────────────────────┘
                                  ▼
                           ┌──────────────┐
                           │ overlay-stub │  (integration tests)
                           │              │
                           │  writes Pong │
                           └──────┬───────┘
                                  │ stdout (piped)
                                  ▼
                    ┌─────────────┼──────────────────────────────┐
                    │           Reader task                     │
                    │           decodes OverlayIpcCommand       │
                    │           pushes onto recv channel        │
                    └───────────────────────────────────────────┘
```

## Carried follow-ups from Round 2 (status)

- ✅ **Pre-wired deps either consumed or pruned** — `tokio-tungstenite` re-added (consumed by deepgram.rs via future connect path), `url` added (consumed by `build_url`)
- ✅ **MockStt + VAD/capture tests landed before Deepgram** — shipped in Round 2, Deepgram now builds alongside
- ✅ **`delete_session` tightened** — Round 2
- ✅ **Overlay SessionSwitched integration test** — shipped in this round

## Deferred to Round 4 (explicit)

1. **Overlay restart-on-crash loop** — `restart_delay` helper + `OverlayProcessState::Restarting { attempt }` variant are defined; watcher task currently observes exit but doesn't relaunch. Waiting until real Swift/C overlay exists to tune semantics.
2. **STT fallback chain** (Deepgram primary → secondary cloud provider → local whisper.cpp) — design captured in `docs/work/PLAN-STT-FALLBACK-CHAIN.md` per user direction 2026-05-13. Requires a router layer around `SttProvider`; deferred to a later round.
3. **System audio capture** (ScreenCaptureKit macOS, WASAPI loopback Windows) — Round 4
4. **Real Swift/C overlay code updates** — Round 4 (or later; depends on stealth work sequencing)

**NOT deferred (shipped in Round 3 after FIX commit):**
- `DeepgramProvider::connect()` live path — full `connect_async` + auth header + split reader/writer tasks + supervisor with exponential-backoff reconnect loop + error classification

## Known quirks

1. **`DeepgramProvider::source` field marked `#[allow(dead_code)]`** — intentional. The live `connect()` path threads `source` into its supervisor task which stamps it on emitted events; the field on the handle itself is retained so future extensions (e.g. per-request re-tagging) can access it without reshaping constructors.
2. **Overlay watcher is single-shot**, documented above. `restart_delay` and `Restarting { attempt }` are defined but unused this round.
3. **Stub binary builds as part of `cargo test`** — Cargo automatically builds bins that integration tests reference via `CARGO_BIN_EXE_<name>`. Runs transparently in CI.

## Review checklist for codex

### Deepgram correctness
- [ ] `build_url`: `channels=1` always set, `model=nova-3` default, `encoding=linear16` always set
- [ ] `build_url`: `SttConfig.language` wins over `DeepgramConfig.language` when both present
- [ ] `build_url` honors `base_url` override (enables tests against mock WS server later)
- [ ] `mask_api_key`: empty string and keys shorter than 5 chars both return `"****"` (no panic, no partial leak)
- [ ] `parse_frame`: `is_final: true` → `Final`, `is_final: false` → `Partial`
- [ ] `parse_frame`: word-level timing preserved (order, start/end times, confidence)
- [ ] `parse_frame`: diarization speaker label emitted as a SECOND event after Final/Partial
- [ ] `parse_frame`: empty transcript + no words returns `Vec::new()` (not an event with empty text)
- [ ] `parse_frame`: `"type": "Error"` returns `SttError::Provider(<raw payload>)` for debuggability
- [ ] `parse_frame`: invalid JSON returns `SttError::Protocol(_)` with serde error in message
- [ ] `map_handshake_status`: 401 AND 403 → Auth (not retryable, failover)
- [ ] `map_handshake_status`: 402 AND 429 → Quota (not retryable, failover)
- [ ] `map_handshake_status`: 400 → AudioFormat (not retryable, no failover — config bug)
- [ ] `map_handshake_status`: 500/502/503/504 → Network (retryable, no failover)
- [ ] `reconnect_delay` curve: 250ms, 500ms, 1s, 2s, 4s, 8s, 10s (cap), 10s, ...
- [ ] `DeepgramProvider::close` transitions state to `Closed` AND sets `closed: true` atomic
- [ ] `DeepgramProvider::send_audio` after close returns `SttError::NotActive` (not a silent drop)

### Overlay IPC correctness
- [ ] `NativeOverlayHandle::spawn` returns `Err` on invalid executable path; does NOT leak a task
- [ ] `NativeOverlayHandle::send` after `shutdown` returns `Err(OverlayMessage)` so caller can retry
- [ ] `NativeOverlayHandle::next_command` returns `None` once the pipe is fully closed (channel drained)
- [ ] `NativeOverlayHandle::try_next_command` is non-blocking and returns `None` when empty
- [ ] `NativeOverlayHandle::shutdown` closes stdin (trigger for stub EOF exit) — does not kill -9
- [ ] `shutdown` handles a task that doesn't exit within 2 s (moves on, doesn't deadlock)
- [ ] Writer task flushes after every line (stub sees each message immediately)
- [ ] Reader task logs-and-continues on malformed JSON (not propagated as panic or channel close)
- [ ] Watcher task sets `OverlayProcessState::Failed` on non-zero exit status
- [ ] `kill_on_drop(true)` on Command builder (child dies if handle is forgotten)

### Security
- [ ] Grep `tracing::` + `deepgram.api_key` shows no direct logging of the full key anywhere
- [ ] Grep confirms `mask_api_key` is the only path that ever touches the key for logging purposes
- [ ] Integration tests do NOT require any Deepgram API key to run

### Integration tests
- [ ] 3 overlay-pipe tests all complete in <10 s total wall time
- [ ] Each message generates exactly one Pong (no amplification or loss)
- [ ] Tests reference the stub via `env!("CARGO_BIN_EXE_overlay-stub")` (cross-platform stable)
- [ ] `overlay_shutdown_is_graceful` exercises the handle-consumed-by-shutdown path
- [ ] `session_switched_round_trips_through_overlay_pipe` covers BOTH `session_id: Some(_)` and `None`

### Style / hygiene
- [ ] `cargo fmt --check` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] No new `unwrap()` in non-test code
- [ ] Module-level doc comments on all new files explain what and why
- [ ] `#[allow(dead_code)]` attributes are minimal and have an inline comment explaining retention

### Next-round readiness (Round 4)
- [ ] `DeepgramProvider::from_channels` seam is stable — a real `connect()` can build on it without trait changes
- [ ] `parse_frame` covers enough of Deepgram's live frame shapes that the connect path won't need parser changes
- [ ] `NativeOverlayHandle` is a clean surface — dashboard or other consumers can hold one
- [ ] Restart loop extension point (`restart_delay`, `Restarting { attempt }`) is ready

## Verdict request

Codex: review the commits + new modules + integration test + IMPL doc. Write `docs/work/REVIEW-PHASE-3-ROUND-3.md` with verdict.

- 🟢 ACCEPT → merge to main, start Round 4 (overlay restart loop + system audio capture + Swift/C overlay code updates, whichever we sequence first)
- 🟡 ACCEPT WITH NITS → fold into Round 4
- 🔴 REQUEST CHANGES → I update `docs/work/FIX-PHASE-3-ROUND-3.md` (already present from the first fix cycle) and re-hand
