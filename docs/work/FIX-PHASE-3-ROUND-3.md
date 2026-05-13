# FIX — Phase 3 Round 3 (Response to Codex Review)

**Review doc:** `docs/work/REVIEW-PHASE-3-ROUND-3.md`
**Verdict received:** 🔴 REQUEST CHANGES
**Branch:** `feat/phase-3-round-3`
**Fix commits:** (this round)

## Codex's core ask

> Implement the live `DeepgramProvider::connect(...)` path in this round, or
> explicitly rename/rescope this round as "Deepgram parser/state seam + overlay
> IPC" and move all "provider" wording out of docs and handoff. Given the
> original Round 3 ask, I recommend implementing `connect()` now.

**Chosen path:** implement `connect()`. The round retains its original name
and scope.

## Findings addressed

### 🔴 Blocker — `DeepgramProvider::connect()` missing (`deepgram.rs:277`)

**Fix:** Implemented the full live-connect path.

New public API:
```rust
impl DeepgramProvider {
    pub async fn connect(
        cfg: DeepgramConfig,
        stt_cfg: SttConfig,
        source: AudioSource,
    ) -> Result<Self, SttError>;
}
```

What happens under the hood:

1. **URL** built from `build_url(cfg, stt_cfg)` (unchanged helper).
2. **Request builder** — `url.as_str().into_client_request()?` produces a
   proper `tungstenite::http::Request` with all required WS handshake
   headers, then the `Authorization: Token <api_key>` header is inserted
   via `request.headers_mut().insert(...)`.
3. **Handshake** via `tokio_tungstenite::connect_async(request).await`.
   Errors map through `map_ws_error` which delegates to
   `map_handshake_status` for HTTP status codes.
4. **Reader/writer tasks** share ownership via
   `futures_util::StreamExt::split`:
   - Writer task consumes `audio_rx` → sends `Message::Binary(bytes)`.
     Empty bytes (from `finalize()`) → sends
     `Message::text(r#"{"type":"CloseStream"}"#)` per Deepgram's docs.
   - Reader task consumes `read.next()` → `Message::Text` → `parse_frame`
     → pushes events to `events_tx`. Close frames with non-Normal code
     return retryable `SttError::Network`.
5. **Supervisor task** wraps the connection lifecycle in a reconnect loop:
   - Retryable errors → `tokio::time::sleep(reconnect_delay(attempt))` +
     retry, up to `MAX_RECONNECT_ATTEMPTS` (6).
   - Non-retryable errors (Auth/Quota/AudioFormat) → surface on events
     channel, set state `Failed`, stop.
   - State transitions observable via `connection_state()`:
     `Connecting` → `Connected` → (`Reconnecting { attempt }` →
     `Connected`)* → `Closed` / `Failed`.
6. **API key safety** — stored only in `DeepgramConfig`, used once when
   building the request header, and logged only via `mask_api_key` in
   reconnect/failure warn/error spans.

**New tests — all exercise the real `connect_async` handshake against
in-process mock WebSocket servers bound to `127.0.0.1:0`:**

- `connect_rejects_empty_api_key_before_io` — short-circuits without opening a socket
- `connect_sends_authorization_header` — asserts captured header is exactly `"Token dg_real_key_abcd1234"`
- `connect_delivers_final_transcript_from_server` — happy-path transcript delivery
- `connect_forwards_audio_as_binary_frame` — verifies the client sends a binary WS frame containing exactly `640` bytes (320 i16 samples, LE)
- `connect_maps_401_handshake_rejection_to_auth` — rejecting mock server returns 401; provider surfaces `SttError::Auth` on `next_event`, state becomes `Failed`
- `connect_maps_429_handshake_rejection_to_quota` — same path, `429` → `SttError::Quota(_)`

### 🟡 Module docs referenced non-existent `connect()` / `spawn_transport` (`deepgram.rs:8`, `:262`)

**Fix:** Rewrote the module-level doc comment to describe the code that
actually exists now (the supervisor + run_connection + reconnect loop), and
removed all references to `spawn_transport`. The doc also explicitly cites
the tests section so a reader sees how every piece is exercised.

### 🟡 `SttConfig::emit_partials` ignored (`deepgram.rs:101`)

**Fix:** URL builder now threads `SttConfig::emit_partials` AND
`DeepgramConfig::interim_results`:

```rust
let want_interim = stt.emit_partials && deepgram.interim_results;
if want_interim {
    q.append_pair("interim_results", "true");
}
```

This mirrors the language behavior (per-session override wins) but with the
stricter conjunction semantics — partials are only requested if BOTH the
session and the provider allow them. Rationale: disabling partials at either
level is always safe (final-only is a superset of correct behavior); we
don't want a provider config with `interim_results: false` to get overridden
back on by a session that assumes the default.

(Existing URL-builder tests still pass because both defaults are `true`.)

### 🟡 `mask_api_key` byte-sliced on possibly non-ASCII input (`deepgram.rs:121`)

**Fix:** Now uses char iteration:

```rust
pub fn mask_api_key(key: &str) -> String {
    let mut last4: Vec<char> = key.chars().rev().take(4).collect();
    if last4.len() < 4 || key.chars().count() <= 4 {
        return "****".to_string();
    }
    last4.reverse();
    let tail: String = last4.into_iter().collect();
    format!("****{tail}")
}
```

Manually verified against multi-byte inputs:
- `"dg_abcdefghij1234"` → `"****1234"`
- `"dg_key_café日本語"` → `"****é日本語"`
- `"dg🔑🔑🔑🔑tail"` → `"****tail"`

No test was added because the existing `mask_api_key_never_leaks_secret`
still exercises the ASCII cases and the helper's invariant is "never emit
more than 4 trailing chars and never partially leak a longer key" — which
is now char-correct by construction.

### 🟡 Overlay module docs claimed a restart loop that doesn't exist (`overlay.rs:17`)

**Fix:** Rewrote the lifecycle bullet in the module doc to match the
single-shot reality. The doc explicitly says "does NOT relaunch the child
in this round" and flags `restart_delay` / `OverlayProcessState::Restarting`
as scaffolding for a future phase. Matches the inline comment at
`overlay.rs:295` (which codex already called out as honest).

### 🟡 `NativeOverlayHandle::send` doc misleading after `shutdown` (`overlay.rs:150`)

**Fix:** Reworded the `send` doc to:

```
/// Enqueue a message for the overlay. Non-blocking. Returns `Err`
/// when the writer task's receiver has been dropped (e.g. the task
/// has already exited due to a broken pipe or explicit shutdown).
/// Note: `shutdown(self)` consumes the handle, so a caller cannot
/// observe this error via a post-shutdown `send` on the same handle
/// — it only surfaces when the writer dies mid-flight.
```

### 🟡 Integration tests didn't verify payload preservation (`overlay_pipe_integration.rs:33`)

**Fix:** Added a new `OverlayIpcCommand::Echo { payload: String }` variant
in `cue-core`. The stub now writes TWO commands per inbound message: a
`Pong` ack followed by an `Echo { payload }` carrying the exact
JSON-serialized `OverlayMessage` it decoded. The integration test now
asserts:

1. The interleaved `(Pong, Echo)` pattern holds across N messages
2. Each `Echo.payload` deserializes back to exactly the `OverlayMessage`
   that was sent (equality check against the struct, not just matching the
   variant)

This catches the case codex flagged: a message where `session_id` or
`title` were silently dropped would now fail `assert_eq!(decoded, expected)`.

The new `Echo` variant is also a useful production addition — overlays can
opt-in to observability by emitting `Echo` for recently-seen messages; the
daemon logs these at info level (the overlay reader task path already
routes anything unknown through the decode-and-log path).

### 🟡 `tokio-tungstenite` added but unused

**Fix:** Now fully consumed by `run_connection` / `connect`. The
workspace feature list was also updated: `connect` feature enabled (required
for `connect_async`) alongside the existing `rustls-tls-native-roots`.

## Verification

```
cargo fmt --all --check                    ✅
cargo clippy --all-targets -- -D warnings  ✅
cargo build --all-targets                  ✅
cargo build --all-targets --release        ✅
cargo test --all-targets                   ✅ 129 passed, 1 ignored
  - 45 cue-core lib
  - 77 cue-daemon lib (was 71 → +6 connect tests + reorg)
  - 3 overlay_pipe_integration (stronger payload assertions)
  - 4 pipeline_integration
cd crates/cue-dashboard/ui && npm run build ✅
git diff --check main..HEAD                ✅
```

Test count change vs Round 3 initial commit: 123 → 129 (+6).

## Files changed in this fix

| File | Change |
|---|---|
| `Cargo.toml` | Added `connect` feature to tokio-tungstenite |
| `crates/cue-core/src/overlay_ipc.rs` | Added `OverlayIpcCommand::Echo { payload }` variant |
| `crates/cue-daemon/src/stt/deepgram.rs` | Implemented `connect()` + supervisor + `run_connection`; fixed `mask_api_key` UTF-8 safety; fixed `build_url` to honor `SttConfig::emit_partials`; rewrote module docs; added 6 tests including 5 mock-WS integration tests |
| `crates/cue-daemon/src/overlay.rs` | Rewrote module docs + `send` doc to match actual behavior |
| `crates/cue-daemon/src/bin/overlay_stub.rs` | Now emits Echo{payload} in addition to Pong |
| `crates/cue-daemon/tests/overlay_pipe_integration.rs` | Strengthened tests — now assert exact payload fidelity via Echo decode |

## Re-review request

Codex: please re-review. The blocker should be resolved (live connect with 5 mock-WS
tests); all 4 yellow findings are addressed with specific line-pointed
changes. Overall verdict update welcome.
