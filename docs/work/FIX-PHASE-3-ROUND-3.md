# FIX-PHASE-3-ROUND-3: Deepgram live connect() + codex nits

## Issue

Codex review (`docs/work/REVIEW-PHASE-3-ROUND-3.md`) returned 🔴 REQUEST
CHANGES for Round 3. Primary blocker: the round was scoped as "Deepgram
Nova-3 WebSocket STT provider," but only the parser / state-seam / helpers
shipped — no live `connect()` path, so no production code could open a
real Deepgram session. Four additional 🟡 findings covered stale docs, a
missed session config override, a non-ASCII-unsafe helper, and weak
integration-test payload checks.

User's follow-up clarification (2026-05-13) explicitly reaffirmed:
- live `connect()` **must** land this round
- overlay restart loop **stays** deferred to Round 4

## Root Cause

`crates/cue-daemon/src/stt/deepgram.rs` exposed only the `from_channels`
test seam as a public constructor. The round's documentation and handoff
described a full provider including connect / WS reader-writer tasks /
reconnect loop, but the code was glue-less: `tokio-tungstenite` was a
declared workspace dep with zero call sites, and `SttProvider::connect`
returned any fully-wired provider only via the test seam.

Secondary issues, all in `deepgram.rs` / `overlay.rs`:
- Module docs at `deepgram.rs:8` / `:262` referred to `connect()` and
  `spawn_transport` as existing symbols.
- `build_url` at `deepgram.rs:101` checked `DeepgramConfig::interim_results`
  only, ignoring the session-scoped `SttConfig::emit_partials`.
- `mask_api_key` at `deepgram.rs:121` sliced bytes on input of unknown
  encoding; could panic for non-ASCII keys longer than 4 bytes.
- `overlay.rs:17` described a restart-on-crash watcher loop that doesn't
  exist this round.
- `overlay.rs:150` `send()` doc claimed Err after `shutdown`, but
  `shutdown(self)` consumes the handle so the error is unreachable that way.
- `overlay_pipe_integration.rs:33` tests validated that a message decoded
  cleanly but never that `session_id` / `title` survived the round trip.

## Fix Summary

**Live `connect()` — full path, wired end-to-end:**

1. `DeepgramProvider::connect(cfg, stt_cfg, source)` — builds the URL via
   `build_url`, calls `url.as_str().into_client_request()` to get a
   proper `tungstenite::http::Request` with required WS handshake
   headers, then inserts `Authorization: Token <api_key>` with
   `request.headers_mut().insert(...)`.
2. `tokio_tungstenite::connect_async(request).await` performs the
   handshake. Errors map through `map_ws_error` which delegates to
   `map_handshake_status` for HTTP status codes.
3. `futures_util::StreamExt::split` divides the stream into reader and
   writer halves. They cooperate inside `tokio::select!`:
   - **writer branch:** drains `audio_rx`; `Vec<u8>::new()` (from
     `finalize()`) → `Message::text(r#"{"type":"CloseStream"}"#)`, all
     other bytes → `Message::binary(bytes)`.
   - **reader branch:** consumes `read.next()`. `Message::Text` →
     `parse_frame` → events pushed on `events_tx`. `Message::Close` with
     a non-Normal/Away code → retryable `SttError::Network`.
4. `run_supervisor` wraps the connection lifecycle: on retryable error it
   sleeps `reconnect_delay(attempt)`, bumps `attempt`, sets state to
   `Reconnecting { attempt }`, and reopens the connection. After
   `MAX_RECONNECT_ATTEMPTS` it surfaces the error on `events_tx` and sets
   state to `Failed`. On non-retryable error (Auth/Quota/AudioFormat) it
   surfaces immediately and stops — no infinite retry on fatal failures.
5. API key never logged raw: `mask_api_key` used in all tracing spans
   (`"api_key = %mask_api_key(&cfg.api_key)"`), and passed to the request
   header only once.

**Other codex findings:**
- Module docs in `deepgram.rs` rewritten to describe `run_supervisor` /
  `run_connection` (no more ghost references to `spawn_transport`).
- URL builder now uses `stt.emit_partials && deepgram.interim_results` —
  per-session `false` wins even when provider config is `true`.
- `mask_api_key` now iterates chars instead of byte-slicing; verified
  safe against multi-byte Unicode and emoji keys.
- Overlay module doc rewritten to state "does NOT relaunch the child
  this round" and flags `restart_delay` as scaffolding.
- `send()` doc clarified to say `Err` surfaces only when the writer task
  died mid-flight (not post-`shutdown`).
- `OverlayIpcCommand` gained `Echo { payload: String }`. Stub emits
  `Pong` then `Echo { payload: serde_json(msg) }` per inbound message.
  Integration test now asserts `assert_eq!(decoded, expected_msg)` on
  each echo, proving full-payload fidelity.

## Files Modified

| File | Change |
|------|--------|
| `Cargo.toml` | Enable `connect` feature on `tokio-tungstenite` |
| `crates/cue-core/src/overlay_ipc.rs` | Added `OverlayIpcCommand::Echo { payload }` variant |
| `crates/cue-daemon/src/stt/deepgram.rs` | Implemented `connect()`, `run_supervisor`, `run_connection`, `map_ws_error`, manual `Debug`; fixed `build_url` emit_partials handling; UTF-8-safe `mask_api_key`; rewrote module docs; added 6 live-connection tests |
| `crates/cue-daemon/src/overlay.rs` | Rewrote module + `send()` docs to match actual behavior |
| `crates/cue-daemon/src/bin/overlay_stub.rs` | Emit `Echo { payload }` after `Pong` |
| `crates/cue-daemon/tests/overlay_pipe_integration.rs` | Assert exact payload round-trip via Echo |
| `docs/work/PHASE-3-ROUND-3-HANDOFF-FOR-CODEX-REVIEW.md` | Updated to reflect `connect()` is now in, not deferred |
| `docs/work/PLAN-STT-FALLBACK-CHAIN.md` (new) | Captures user's primary/fallback/local-whisper design for Round 4+ |
| `docs/work/AGENT-ONBOARDING.md` (new, previously untracked) | Handoff file for new agent chat sessions |

## Edge Cases Handled

- **Empty API key** → `SttError::Auth` returned immediately from `connect()`
  before any socket open (test: `connect_rejects_empty_api_key_before_io`).
- **401 handshake** → `SttError::Auth`, state flips to `Failed`, no retry
  (test: `connect_maps_401_handshake_rejection_to_auth`).
- **429 handshake** → `SttError::Quota(...)`, no retry (test:
  `connect_maps_429_handshake_rejection_to_quota`).
- **5xx handshake** → `SttError::Network`, supervisor retries.
- **Abnormal WS close on open stream** (code 1011) → retryable `Network`,
  supervisor waits `reconnect_delay(0) = 250 ms`, reopens, test delivers
  transcript on second connection (test:
  `connect_reconnects_on_retryable_server_close`).
- **Normal close (1000)** / **Away (1001)** → clean return, supervisor
  exits loop, state flips to `Closed`.
- **finalize()** → sends `{"type":"CloseStream"}` as WS Text (Deepgram's
  documented flush signal); connection stays open for a subsequent
  `send_audio` or `close()`.
- **close()** → flips atomic `closed` flag; supervisor observes on next
  loop iteration, sends `Message::Close(None)`, returns cleanly.
- **Audio channel drop** (provider being torn down) → writer sends close
  frame and the connection exits without marking Failed.
- **Non-ASCII API key** → `mask_api_key("dg🔑🔑🔑🔑tail") == "****tail"`.
- **Protocol-level parse error mid-stream** → retryable, supervisor
  reopens the connection.
- **Provider-level `{"type":"Error"}` frame** → surfaced via events
  channel as `SttError::Provider(payload)` without tearing down the
  stream; next frame may recover.

## How to Test

Mock WS servers listening on `127.0.0.1:0` — zero Deepgram creds required.

```bash
ssh uno 'cd /Users/uno/Downloads/cue
  export PATH=/opt/homebrew/bin:/Users/uno/.cargo/bin:/usr/local/bin:$PATH
  cargo test -p cue-daemon --lib stt::deepgram'
```

Expected: 26 tests pass — all `parse_frame`, `build_url`, `mask_api_key`,
`map_handshake_status`, `reconnect_delay`, plus 7 live-connection tests:

- `connect_rejects_empty_api_key_before_io`
- `connect_sends_authorization_header` (captures handshake `Authorization` header value)
- `connect_delivers_final_transcript_from_server` (happy path)
- `connect_forwards_audio_as_binary_frame` (320 i16 samples → 640 LE bytes)
- `connect_maps_401_handshake_rejection_to_auth`
- `connect_maps_429_handshake_rejection_to_quota`
- `connect_reconnects_on_retryable_server_close` (proves backoff + reconnect)

Full workspace pipeline:

```bash
cargo fmt --all --check                     # ✅
cargo clippy --all-targets -- -D warnings   # ✅
cargo build --all-targets --release         # ✅
cargo test --all-targets                    # ✅ 130 pass, 1 ignored
cd crates/cue-dashboard/ui && npm run build # ✅
git diff --check main..HEAD                 # ✅
```

## Known Limitations

**Explicitly deferred to Round 4 per user direction:**
- Overlay restart-on-crash loop (`restart_delay` helper + `Restarting { attempt }`
  variant are scaffolded but not wired)
- Secondary STT provider + local whisper fallback — design captured in
  `docs/work/PLAN-STT-FALLBACK-CHAIN.md`; implementation is a future round
- System audio capture (ScreenCaptureKit / WASAPI loopback)
- Real Swift/C overlay code updates

**Residual by design, not a follow-up:**
- The `audio_rx` channel is unbounded. Real audio capture runs at fixed
  cadence (50 fps @ 20 ms frames), so backlog is bounded in practice. A
  bounded channel would require back-pressure plumbing in the framer,
  which is out of scope.
- The supervisor does not currently drain buffered audio from `audio_rx`
  when mid-reconnect — chunks sent during the backoff window are
  preserved but arrive at Deepgram only after the new WS handshake. For
  short (sub-second) reconnects this is fine; prolonged outages may
  cause a brief "wall of audio" on reconnect. If that becomes a problem,
  the fix is to drain-and-discard stale chunks during `Reconnecting`.
