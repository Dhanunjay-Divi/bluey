# IMPL — Phase 3 Listening Upgrade (Round 3 — Deepgram + Native Overlay IPC)

**Branch**: `feat/phase-3-round-3`
**Base**: `5fca21f` (main post-Round-2-merge)
**Tip**: pre-docs — this commit + handoff will be the final commits

## Scope

Round 3 of Phase 3. Adds the first concrete streaming STT provider and wires the daemon ↔ native-overlay process pipe end-to-end.

### What shipped

1. **Deepgram Nova-3 WebSocket STT provider** — `crates/cue-daemon/src/stt/deepgram.rs`
   - `SttProvider` impl with the channel-based seam from Round 2's trait
   - URL builder (model / encoding / sample_rate / channels / punctuate / interim_results / diarize / language)
   - `Authorization: Token <key>` auth; API key NEVER logged (helper `mask_api_key` only shows last 4 chars)
   - JSON frame parser (`parse_frame`) producing `TranscriptEvent::Partial` / `::Final` / `::SpeakerLabel` with word-level timing
   - HTTP handshake status → `SttError` classification (`map_handshake_status`)
   - `reconnect_delay` exponential backoff (250 ms → 10 s cap)
   - `DeepgramProvider::from_channels` test seam so we can unit-test state-machine and lifecycle without opening a real WS

2. **Native overlay IPC module** — `crates/cue-daemon/src/overlay.rs`
   - `NativeOverlayHandle` encapsulates the child process + writer + reader + watcher tasks
   - Spawns via `tokio::process::Command` with `kill_on_drop(true)`
   - Writer task: drains an mpsc of `OverlayMessage`, writes NDJSON to child stdin via `encode_ndjson`
   - Reader task: parses child stdout line-by-line into `OverlayIpcCommand`
   - Watcher task: observes child exit and updates `OverlayProcessState`
   - `shutdown()` is graceful: closes stdin (triggers child EOF exit) + waits on tasks with a 2 s per-task timeout
   - `restart_delay` exponential backoff (250 ms → 5 s cap) — reserved for a future enhancement (single-shot spawn for this round)

3. **Overlay stub binary** — `crates/cue-daemon/src/bin/overlay_stub.rs`
   - `[[bin]] name = "overlay-stub"` — Cargo auto-builds it for integration tests
   - Reads NDJSON `OverlayMessage`s from stdin, responds with `OverlayIpcCommand::Pong` on stdout
   - Exits cleanly on stdin EOF so `shutdown()` terminates the stub
   - Used ONLY in integration tests; no production runtime path

4. **Overlay-pipe integration tests** — `crates/cue-daemon/tests/overlay_pipe_integration.rs`
   - `session_switched_round_trips_through_overlay_pipe` — sends 3 `SessionSwitched` messages (with and without session id), receives 3 `Pong` commands
   - `transcript_partial_round_trips_through_overlay_pipe` — exercises the transcript message path
   - `overlay_shutdown_is_graceful` — `shutdown()` completes within 5 s

## Commits

| Hash | Title |
|---|---|
| `(new)` | `feat(daemon): Deepgram Nova-3 STT + native overlay IPC + overlay-stub bin + integration tests [P3.R3]` |

Plus a follow-up commit restoring the pruned `tokio-tungstenite` + `url` workspace deps (was pre-committed this round).

## Files created / modified

| File | Change |
|---|---|
| `Cargo.toml` | Restored `tokio-tungstenite` workspace dep; added `url` workspace dep |
| `crates/cue-daemon/Cargo.toml` | Added `tokio-tungstenite.workspace = true`, `url.workspace = true`; added `[[bin]] name = "overlay-stub"` |
| `crates/cue-daemon/src/lib.rs` | `pub mod overlay;` |
| `crates/cue-daemon/src/stt/mod.rs` | `pub mod deepgram;` |
| `crates/cue-daemon/src/stt/deepgram.rs` | `DeepgramConfig`, `build_url`, `mask_api_key`, `parse_frame`, `map_handshake_status`, `reconnect_delay`, `DeepgramProvider`, shared state types. 20 unit + async tests. |
| `crates/cue-daemon/src/overlay.rs` | `NativeOverlayHandle`, `OverlayProcessState`, `OverlaySpawnOptions`, `restart_delay`, shared state + tasks. 4 unit tests. |
| `crates/cue-daemon/src/bin/overlay_stub.rs` | Tiny NDJSON-echo binary for integration tests. |
| `crates/cue-daemon/tests/overlay_pipe_integration.rs` | 3 async integration tests driving the stub. |

## Build + test

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets --release                  ✅ 55.59s
cargo test --all-targets                             ✅ 123 pass
                                                        (45 core + 71 daemon lib + 3 overlay pipe + 4 pipeline integration + 1 ignored hardware)
cd crates/cue-dashboard/ui && npm run build          ✅ 297 KB JS
git diff --check main..HEAD                          ✅ clean
```

Test count change:
- Round 2 end: 97 total (45 core + 48 daemon lib + 4 integration)
- Round 3 end: 123 total (+26)
  - +23 daemon lib (20 deepgram + 3 overlay unit)
  - +3 integration tests (overlay_pipe_integration)

## Design decisions worth calling out

### Deepgram provider split — connect path deferred

`DeepgramProvider` has TWO constructor seams:

- `from_channels(source, initial_state, events_rx, audio_tx)` — used this round. Lets tests and the eventual `connect()` path share the same lifecycle + state machine. No actual WS is opened.
- A real `connect()` — deferred. The URL / auth / handshake / task loop is all in the file (see `build_url`, `map_handshake_status`, `reconnect_delay`, `parse_frame`), but wiring them into a live `tokio-tungstenite` `connect_async` call + error mapping + reconnect loop is a Phase 3 Round 4 item. Reason: hitting a real Deepgram server in tests is fragile; the parser, URL builder, and state machine are the correctness-critical parts and they're fully unit-tested now. The glue code reads a WS stream and calls `parse_frame` for each text message, calls `send(Message::Binary(...))` for each audio chunk — mechanically simple but not worth reviewing alongside everything else in this round.

Every piece the connect path will need is already exercised by tests:
- URL shape — verified by 3 separate tests
- Auth header value — NOT verified here but trivial ("Token <key>" — no other form)
- Frame parsing — 7 unit tests covering partial, final, word-timing, diarization, empty, error-type, malformed
- Error classification — 5 tests covering the HTTP status map
- Reconnect backoff — 1 test covering the growth curve

### Overlay IPC watcher is single-shot this round

The watcher task observes child exit and updates `OverlayProcessState`, but does NOT currently relaunch the child on unexpected exit. The `restart_delay` helper + `OverlayProcessState::Restarting { attempt }` variant are defined so a future round can extend this to a proper restart loop without reshaping the module. The reason this lands single-shot today:

1. The immediate value — "the daemon can talk to the overlay" — doesn't require restart logic.
2. The Swift/C overlay doesn't yet exist. We can't tune restart semantics without a real child to crash.
3. Integration tests already assert clean shutdown; restart tests would need deliberate crash injection which is worth its own round.

### Stub overlay is a real bin, not a script

Cargo sets `CARGO_BIN_EXE_<name>` automatically for integration tests in the same crate. The stub is a `[[bin]]` (not `[[example]]`) so:

- It builds implicitly on `cargo test` (cargo builds bin targets the tests depend on)
- The test gets a stable absolute path via `env!("CARGO_BIN_EXE_overlay-stub")`
- It's released by Cargo — tests don't need to know the OS's executable extension (`.exe` on Windows)

### API-key masking explicit

`mask_api_key("dg_secret_abcd1234")` returns `"****1234"`. This is the ONLY way any tracing log line is allowed to reveal any part of the key. Never log the full key. The integration-test stub also never sees a real key — the daemon only uses it to build the WS `Authorization` header in the (future) `connect()` call.

### Send_audio uses bytemuck zero-copy

`AudioChunk::samples` is `Vec<i16>` already little-endian (we guarantee this at capture time). `bytemuck::cast_slice::<i16, u8>` is a zero-copy reinterpret, then we `.to_vec()` once to get an owned buffer the mpsc channel can consume.

## Known quirks / next-round bridge

1. **`connect()` glue** — the live WebSocket open + rx/tx task that binds the seam `from_channels` exposes has not landed. Round 4 work. The seam is complete; only the pipe-to-pipe wiring is missing.
2. **`source` field on `DeepgramProvider` is `#[allow(dead_code)]`** — annotated explicitly because tests use `from_channels` which doesn't currently stamp the field onto emitted events. The future `connect()` will use it when constructing events from parsed frames.
3. **Native overlay restart loop** — single-shot, documented above.
4. **No real Swift/C overlay yet** — integration tests use the stub. The Swift/C overlay code will be added in Round 4 or later; the daemon side is ready to talk to whatever speaks NDJSON on stdin/stdout.

## Review checklist for codex

### Deepgram correctness
- [ ] `build_url` never emits `language` when both `SttConfig.language` and `DeepgramConfig.language` are `None`
- [ ] `build_url` prefers `SttConfig.language` over `DeepgramConfig.language` (per-session beats per-provider)
- [ ] `build_url` always sets `channels=1` (we hand Deepgram mono audio)
- [ ] `mask_api_key("")` returns `"****"` without panicking
- [ ] `mask_api_key(<4 char)` returns `"****"` — not `"****<short>"`
- [ ] `parse_frame` with `"is_final": true` emits `TranscriptEvent::Final` (not `Partial`)
- [ ] `parse_frame` with `"type": "Error"` returns `SttError::Provider(_)` (raw payload preserved for debugging)
- [ ] `parse_frame` returns empty Vec for empty-transcript frames (no spurious events)
- [ ] `parse_frame` with word-level diarization emits TWO events (Final + SpeakerLabel), in that order
- [ ] `map_handshake_status` returns `SttError::Auth` for 401 AND 403
- [ ] `map_handshake_status` returns `SttError::Quota` for 402 AND 429
- [ ] `map_handshake_status` returns `SttError::Network` (retryable) for 5xx
- [ ] `reconnect_delay(0) == 250 ms`, `delay(1) == 500 ms`, capped at 10 s
- [ ] `DeepgramProvider::close` sets `closed=true` AND flips `ConnectionState::Closed`
- [ ] `DeepgramProvider::send_audio` after `close` returns `SttError::NotActive`
- [ ] `DeepgramProvider::finalize` sends an empty binary frame (DG-documented CloseStream signal)

### Overlay IPC correctness
- [ ] `NativeOverlayHandle::send` returns `Err` when the handle has been shut down
- [ ] `NativeOverlayHandle::next_command` awaits the next line from stdout and returns `None` on channel close
- [ ] `NativeOverlayHandle::try_next_command` is non-blocking — returns `None` if empty
- [ ] `NativeOverlayHandle::shutdown` does NOT send SIGKILL — it closes stdin and waits
- [ ] The writer task flushes after every line (so the stub / real overlay doesn't buffer indefinitely)
- [ ] The reader task logs-and-continues on malformed JSON (doesn't kill the pipe)
- [ ] `restart_delay(N)` caps at 5 s (shorter than `reconnect_delay`'s 10 s because overlay is local)
- [ ] `OverlayProcessState::Running` is set only AFTER `spawn_child` succeeds

### Integration tests
- [ ] 3 tests all pass within the 3 s timeout budget per message
- [ ] `session_switched_round_trips_through_overlay_pipe` sends messages with BOTH `session_id: Some(...)` and `None` cases
- [ ] `overlay_shutdown_is_graceful` completes within 5 s
- [ ] Tests reference the stub binary via `env!("CARGO_BIN_EXE_overlay-stub")` (stable across macOS/Linux/Windows)

### Security / secrets
- [ ] No call site logs `deepgram.api_key` directly — only via `mask_api_key`
- [ ] `mask_api_key` is the ONLY way any part of the key appears in any tracing span
- [ ] The integration test does not mention or require any API key

### Style / hygiene
- [ ] `cargo fmt --check` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean (no new allows beyond the documented `dead_code` on `DeepgramProvider::source` which is intentional)
- [ ] No `unwrap()` in non-test code other than `.expect("stdin piped")` / `.expect("stdout piped")` which are guaranteed by our `Stdio::piped()` call
- [ ] Module-level doc comments explain what and why

### Next-round readiness
- [ ] `DeepgramProvider::from_channels` seam is stable — a future `connect()` can build on it without trait changes
- [ ] `parse_frame` covers enough of Deepgram's live frame shapes that the connect path doesn't need new parser code
- [ ] `NativeOverlayHandle` is a clean abstraction — dashboard or other consumers can hold one and call `send(OverlayMessage::...)` without knowing about tasks
