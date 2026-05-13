# REVIEW: Phase 3 Round 3 — Deepgram + Native Overlay IPC

**Commit range:** `5fca21f..1b56b09`
**Reviewer:** Codex
**Date:** 2026-05-13

## Per-Task Review

### P3.R3.1 — Deepgram Nova-3 STT Provider

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/stt/deepgram.rs`, `crates/cue-daemon/src/stt/mod.rs`, `Cargo.toml`, `crates/cue-daemon/Cargo.toml` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 `DeepgramProvider` is not yet a real WebSocket provider. The round request asks for a Deepgram Nova-3 WebSocket provider with persistent connection, auth, backoff reconnect, partial/final handling, and word timing. The implementation has useful pieces (`build_url`, `parse_frame`, `map_handshake_status`, `reconnect_delay`, and a `from_channels` seam), but there is no `connect()`, no `connect_async`, no auth header construction, no reader/writer WS task, and no reconnect loop. The only constructor is the test seam at `crates/cue-daemon/src/stt/deepgram.rs:277`, so production code cannot create a live Deepgram session. This is too large to defer if Round 3 is meant to deliver "Deepgram + native overlay IPC".
- 🟡 The module docs describe behavior that does not exist yet. `crates/cue-daemon/src/stt/deepgram.rs:8` says `connect()` opens the WS, owns reader/writer tasks, reconnects, and closes the socket, and `crates/cue-daemon/src/stt/deepgram.rs:262` says the full loop lives in `spawn_transport`; neither `connect()` nor `spawn_transport` exists. This makes the next implementation round more error-prone because the docs read as finished production behavior.
- 🟡 `SttConfig::emit_partials` is ignored in URL construction. `crates/cue-daemon/src/stt/deepgram.rs:101` only checks `DeepgramConfig::interim_results`, so a per-session request to disable partials cannot win. The language setting correctly lets `SttConfig` override provider config; partial behavior should follow the same pattern or be documented as provider-level only.
- 🟡 `mask_api_key` slices by byte offset at `crates/cue-daemon/src/stt/deepgram.rs:121`. Deepgram keys are normally ASCII, so this is not urgent, but the public helper can panic on non-ASCII input longer than four bytes. Prefer `key.chars().rev().take(4)` or document ASCII-only.

### P3.R3.2 — Native Overlay IPC

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/overlay.rs`, `crates/cue-daemon/src/bin/overlay_stub.rs`, `crates/cue-daemon/src/lib.rs` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 The implementation is single-shot, but the module-level docs still say the watcher attempts to relaunch the child with backoff at `crates/cue-daemon/src/overlay.rs:17`. The handoff explicitly defers restart to Round 4 and the code comment at `crates/cue-daemon/src/overlay.rs:295` is honest, so this is documentation drift rather than a code blocker.
- 🟡 `NativeOverlayHandle::send` docs say it returns `Err` "if the handle has been shut down" at `crates/cue-daemon/src/overlay.rs:150`, but `shutdown(self)` consumes the handle, so callers cannot actually call `send` after graceful shutdown. The current error path is really "writer receiver has closed." Small wording fix, but worth tightening before other modules depend on it.
- 🟢 The child-process boundary is cleanly isolated. `spawn`, writer, reader, watcher, state, and shutdown are in one module; `kill_on_drop(true)` is set; writer flushes every NDJSON line; malformed stdout logs and continues; invalid executable paths return from `spawn`.

### P3.R3.3 — Overlay Pipe Integration Tests

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/tests/overlay_pipe_integration.rs`, `crates/cue-daemon/src/bin/overlay_stub.rs` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 The tests verify that a valid `OverlayMessage` reaches the stub, but not that the exact message payload was preserved. `overlay_stub` ACKs every decoded message with `Pong`, so `session_switched_round_trips_through_overlay_pipe` at `crates/cue-daemon/tests/overlay_pipe_integration.rs:33` would still pass if `session_id` or `title` were lost but the variant remained decodable. For a stronger integration test, make the stub echo a debug/JSON payload or add a test-only command variant that includes the decoded message.
- 🟢 The tests are fast, cross-platform in shape via `CARGO_BIN_EXE_overlay-stub`, and cover spawn, write, read, and graceful shutdown through a real child process.

## Cross-Task Findings

- 🔴 The round splits into one solid half and one incomplete half: overlay IPC is mergeable with nits, but Deepgram is not production-usable yet. Because Phase 3 Round 3 is named and scoped as "Deepgram + native overlay IPC", accepting this would let main claim Deepgram support before any code can actually connect to Deepgram.
- 🟡 `tokio-tungstenite` is added but unused by implementation code. That is expected if `connect()` lands immediately next, but it reinforces that the live provider path is still absent.

## Build & Test Verification

```bash
cargo fmt --all --check                    # ✅
cargo clippy --all-targets -- -D warnings  # ✅
cargo build --all-targets                  # ✅
cargo test --all-targets                   # ✅ 123 passed, 1 ignored
cd crates/cue-dashboard/ui && npm run build # ✅
git diff --check main..HEAD                # ✅
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved.

The requested fix is focused: either implement the live `DeepgramProvider::connect(...)` path in this round, or explicitly rename/rescope this round as "Deepgram parser/state seam + overlay IPC" and move all "provider" wording out of docs and handoff. Given the original Round 3 ask, I recommend implementing `connect()` now.

## Follow-ups for Next Batch

- Implement `DeepgramProvider::connect(config, stt_config)` using `tokio_tungstenite::connect_async` or equivalent request builder with `Authorization: Token <key>`.
- Bind WS reader/writer tasks to the existing `from_channels` seam: binary PCM from `send_audio`, empty binary frame from `finalize`, text frames through `parse_frame`.
- Map handshake failures through `map_handshake_status`; classify IO/socket failures as retryable `SttError::Network`.
- Exercise `connect()` with a local mock WS server so auth header, URL, partial/final frames, EOF, and retry/fail paths are tested without Deepgram credentials.
- Clean up stale docs in `deepgram.rs` and `overlay.rs` so they describe the code that actually exists.
