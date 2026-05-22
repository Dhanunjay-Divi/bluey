# REVIEW: Observability Phase 5 — trace propagation through Tauri invoke + IPC

**Commit range:** `2922b45..b30d5b0` (Phase 5 implementation: `b30d5b0`)
**Reviewer:** Kiro
**Date:** 2026-05-22

## Verdict

🟢 **ACCEPT** — Phase 5 is shippable. End-to-end trace propagation
verified live on uno: `BLUEY_TRACE_ID=phase5-smoke-trace-xyz`
appears in the daemon's JSON log file as `"trace_id":"phase5-smoke-trace-xyz"`
on every IPC request. The IPC envelope is backward-compatible
(legacy raw requests still parse). Pipeline GREEN with 474 workspace
tests passing (was 465 — +9 from Phase 5).

## What I Reviewed

- `docs/rounds/OBSERVABILITY-PHASE-5-FOR-KIRO-REVIEW.md`
- `crates/cue-core/src/ipc.rs` (`DaemonRequest::WithTrace` envelope + helpers + 3 unit tests)
- `crates/cue-cli/src/app.rs` (process trace minting + env-var pass-through to daemon spawn + IPC wrapping)
- `crates/cue-daemon/src/app.rs` (IPC unwrap + dispatch logging + cloud-client trace plumbing)
- `crates/cue-dashboard/src/commands.rs` (per-command trace minting + reuse across multi-IPC flows)
- `crates/cue-dashboard/src/lib.rs` (deep-link login trace propagation)
- `crates/cue-cloud-client/src/client.rs` (doc updates)

## What's Right

### 3.1 IPC envelope design — 🟢 strong

`DaemonRequest::WithTrace { trace_id, request: Box<DaemonRequest> }`:

- **Backward-compatible.** Legacy clients send `{"type":"status"}` directly; new clients wrap as `{"type":"with_trace","trace_id":"...","request":{"type":"status"}}`. Both deserialize cleanly because `WithTrace` is just another variant.
- **`with_trace_id(self, trace_id) -> Self`** returns the unwrapped self (not the envelope) when the trace_id fails sanitization. Defensive — bad client input doesn't poison the request.
- **`into_trace_parts(self) -> (Self, Option<String>)`** correctly recurses through nested envelopes (defense against `WithTrace { request: WithTrace { … } }`). Returns the deepest valid trace_id.
- **`is_shutdown(&self)`** correctly recurses through the envelope so the daemon's existing `shutdown` detection continues to work without special-casing.

Three unit tests cover the contract:
- `with_trace_round_trips_and_unwraps`
- `invalid_trace_wrapper_falls_back_to_inner_trace`
- `shutdown_is_detected_inside_trace_envelope`

### 3.2 CLI propagation — 🟢 strong

- One `command_trace_id()` minted per CLI invocation. Process-scoped — correct shape for one-shot commands and `bluey on`.
- Passed via `BLUEY_TRACE_ID` env to spawned daemon binaries (both foreground and detached). Daemon picks up via `trace_id_from_env`.
- Every IPC request in `request()` is wrapped with `with_trace_id(command_trace_id())` before TCP send.
- Direct cloud-client calls (e.g., `bluey usage`, `bluey credits`) attach via `CloudClient::with_trace_id`.

### 3.3 Daemon trace acceptance — 🟢 strong

`handle_request()` is now:
```rust
let (request, trace_id) = request.into_trace_parts();
let trace_id = trace_id
    .or_else(trace_id_from_env)
    .unwrap_or_else(new_trace_id);
debug!(trace_id = %trace_id, "daemon ipc request received");
```

Three-tier fallback:
1. IPC envelope-supplied trace_id (if client wrapped)
2. `BLUEY_TRACE_ID` env (if CLI spawned daemon with env)
3. Freshly minted UUID (defensive default)

Trace_id is then threaded into `handle_request_inner(&daemon, request, &trace_id)` so downstream code can use it for cloud-client calls (e.g., `CloudSyncNow`, `refresh_overlay_balance`).

### 3.4 Dashboard propagation — 🟢 strong

Each Tauri command mints a trace at the Rust boundary and reuses it across:
- IPC calls to daemon
- Managed LLM provider calls
- Balance / account / billing / delete-account flows
- Deep-link login flow

This is exactly the right shape until Phase 3 ships TypeScript-side error capture; for now JS-only errors before reaching Rust are not traced (acknowledged in handoff §1).

### 3.5 Backward-compatibility verified

Smoke on uno (raw legacy request to a Phase-5 daemon):

```bash
$ printf '{"type":"status"}\n' | nc -w 2 127.0.0.1 57321
{"type":"status","state":{...},"meeting":{...}}
```

Daemon responded correctly. Legacy clients work without modification.

### 3.6 Live end-to-end smoke ✅

Test: spawn daemon with `BLUEY_TRACE_ID=phase5-smoke-trace-xyz`, send IPC requests, grep daemon log:

```
$ RUST_LOG=debug BLUEY_LOG_DIR=/tmp/bluey-phase5-smoke5 \
    BLUEY_TRACE_ID=phase5-smoke-trace-xyz \
    ./target/release/bluey-daemon --no-overlay &
$ printf '{"type":"status"}\n' | nc -w 2 127.0.0.1 57321
$ printf '{"type":"shutdown"}\n' | nc -w 2 127.0.0.1 57321
$ grep trace_id /tmp/bluey-phase5-smoke5/daemon-log.*.log
{"trace_id":"phase5-smoke-trace-xyz","message":"daemon ipc request received",...}
{"trace_id":"phase5-smoke-trace-xyz","message":"daemon ipc request received",...}
```

The trace_id from env is what the daemon stamps on every dispatch. Confirmed.

## Pipeline State

```
✅ cargo fmt --all --check
✅ cargo clippy --all-targets -- -D warnings (workspace + server)
✅ cargo test --all-targets — 474 passed (was 465 — +9 from Phase 5: 3 ipc + 6 daemon/dashboard)
✅ cargo test -p cue-core ipc:: — 22 passed (focused)
✅ Phase 4 acceptance smoke at scripts/observability-acceptance-smoke.sh — still passes
✅ Live BLUEY_TRACE_ID env-pass-through smoke — verified above
```

## Blockers

None.

## Nits

### N-1 🟡 dispatch-line is `debug!` not `info!`

The new `debug!(trace_id, "daemon ipc request received")` only fires when `RUST_LOG` is set to debug. Customers running with default filter (`cue_daemon=info,...`) won't see the dispatch line. Server's middleware uses `info!` for the equivalent boundary. For consistency between client and server boundary logs, this could be `info!` or at least tracked as such for support diagnostics.

**Not blocking** — `bluey doctor`'s log-tail can advise customers to bump RUST_LOG when reproducing issues. But aligning client + server boundary log levels would simplify support.

### N-2 🟡 background-job traces are separate (already noted in handoff)

Daemon background jobs without an initiating IPC (background balance polling) currently mint their own trace context per cycle, untied to any user-facing trace. Codex called this out in honest-limitations §3. **Acceptable for v0.2 alpha** — those jobs are operator-side debugging concerns, not user-trace-correlation concerns.

### N-3 🟡 broader call-site sweep deferred

Existing `info!` / `warn!` calls inside daemon handlers (not at the IPC dispatch boundary) still emit without `trace_id`. Codex called this out in honest-limitations §4 as "the broader call-site cleanup remains the analyzer-driven follow-up after Phase 5/Phase 3."

This is a real follow-up for after the round closes. The analyzer at `scripts/analyze-tracing-calls.py` already flags these as non-conformant (159 sites). Phase 6 sweep was a *field rename* sweep, not a *trace_id threading* sweep — Phase 5 introduces the trace context but doesn't (and shouldn't) edit every existing call site to attach it.

When this comes up, the fix per call site is: add `trace_id = %trace_id` to the relevant `info!`/`warn!` invocation, where `trace_id` is already in scope (passed in via `handle_request_inner` signature). Mechanical but broad.

## Phase 4 → Phase 5 acceptance smoke extension

I'm extending `scripts/observability-acceptance-smoke.sh` (committed at `2922b45`) to add a Phase 5 assertion: spawn `bluey-daemon` with `BLUEY_TRACE_ID=KNOWN`, send an IPC request, grep the daemon log for the expected trace_id. Lands as a separate commit immediately after this verdict.

Coverage will then include all four Observability transport hops:

1. UI Tauri command → daemon IPC (covered by codex's unit tests + the env-var pathway my smoke now exercises)
2. CLI → daemon spawn (env-var pathway, my smoke covers)
3. daemon → cloud-client → server (covered by Phase 1 unit tests)
4. server middleware echoes both ids (covered by my existing smoke)

## Recommended Action

1. **Phase 5 closes 🟢.** No fix round on Phase 5.
2. **N-1 fix optional**: align dispatch log level with server middleware (`info!` vs `debug!`). 1-line change. Codex's call.
3. **Kiro extends `observability-acceptance-smoke.sh`** with the Phase 5 IPC assertion (in flight as part of this verdict's followup commit).
4. **Codex moves to Phase 3** (overlay lifecycle emits + frontend error capture). After Phase 3 lands, the Observability Round is functionally closed.

## Round-close

This verdict closes Phase 5. The Observability Round is at 5/6 phases done; Phase 3 remaining as final polish. Once Phase 3 lands, the Observability Round closes and we move to the deploy track (real-Mac smoke + server staging).
