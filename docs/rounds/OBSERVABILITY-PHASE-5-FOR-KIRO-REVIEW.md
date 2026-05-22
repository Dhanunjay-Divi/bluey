Branch:     feat/phase-3-round-12
Tip before: fbff17f
Tip after:  commit containing this handoff

# Observability Phase 5 — trace propagation through Tauri invoke + IPC

## What changed

| Area | Files | Summary |
|---|---|---|
| Backward-compatible daemon IPC trace envelope | `crates/cue-core/src/ipc.rs` | Added `DaemonRequest::WithTrace { trace_id, request }`, plus helpers to wrap, unwrap, sanitize, and detect shutdown through the envelope. Existing raw request JSON remains accepted. |
| CLI trace propagation | `crates/cue-cli/src/app.rs` | Mint one process-level trace id per CLI invocation, pass it to spawned daemon processes via `BLUEY_TRACE_ID`, wrap every daemon IPC request, and attach it to direct cloud-client calls. |
| Daemon trace acceptance | `crates/cue-daemon/src/app.rs` | Unwrap IPC trace ids, fall back to env or a freshly minted trace id, log dispatch with `trace_id`, and thread the trace into daemon-owned cloud clients for sync and request-triggered balance refreshes. |
| Dashboard trace propagation | `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/src/lib.rs` | Mint a trace id per dashboard command/deep-link flow, wrap daemon IPC calls, reuse the same trace across multi-IPC commands, and attach it to managed-provider/cloud-client calls. |
| Cloud-client doc cleanup | `crates/cue-cloud-client/src/client.rs` | Updated `with_trace_id` docs now that Phase 5 is implemented. |

## Why

Phase 1 gave us trace ids and cloud-client header support, Phase 2 gave us persistent logs, and Phase 6 cleaned sensitive fields. This phase connects the runtime path so a support trace can follow:

1. CLI or dashboard action
2. daemon IPC dispatch
3. daemon cloud-client call
4. server middleware via `X-Bluey-Trace-Id`

The IPC envelope keeps older clients compatible while newer clients get end-to-end correlation.

## Verification

Focused checks run during implementation:

```bash
cargo test -p cue-core ipc::tests -- --nocapture                                      ✅
cargo test -p cue-daemon app::tests::cloud_client_with_optional_trace -- --nocapture  ✅
cargo clippy --all-targets -- -D warnings                                             ✅
```

Full gate before handoff:

```bash
cargo fmt --all --check                              ✅
cargo test --all-targets                             ✅
cd server && cargo clippy --all-targets -- -D warnings ✅
cd server && cargo test                              ✅
cd crates/cue-dashboard/ui && npm test -- --run      ✅
cd crates/cue-dashboard/ui && npm run build          ✅
git diff --check                                     ✅
```

## Areas most likely wrong

1. **Dashboard trace ids are minted at the Rust Tauri command boundary, not in TypeScript.** The product effect is still "one trace per dashboard invoke / flow", but JS-only errors before a command reaches Rust remain Phase 3 frontend-error-capture work.
2. **CLI uses one process-level trace id.** This is ideal for one-shot commands and `bluey on`; long-running live mode will group all daemon IPC from that CLI process under one trace.
3. **Daemon background jobs without an initiating IPC request still mint/omit their own trace context.** Request-triggered cloud sync and audio-stop balance refresh use the incoming trace. Background balance polling remains a daemon background flow.
4. **Log inheritance is explicit at dispatch/cloud boundaries.** Existing inner `info!` / `warn!` calls are not all rewritten to carry `trace_id`; that broader call-site cleanup remains the analyzer-driven follow-up after Phase 5/Phase 3.

## Review checklist

- [ ] Confirm raw legacy daemon requests still deserialize and work.
- [ ] Confirm traced requests deserialize as `WithTrace` and unwrap to the intended inner request.
- [ ] Confirm shutdown detection works through the trace envelope so `bluey off` still exits the daemon.
- [ ] Confirm `CloudClient::with_trace_id` is used for dashboard managed providers and CLI cloud commands.
- [ ] Confirm daemon `CloudSyncNow` and request-triggered balance refresh pass the received trace id to `build_cloud_client`.

## Honest limitations

- No TypeScript invoke wrapper yet; frontend-only errors are Phase 3.
- No server changes in this phase; server already reads/echoes `X-Bluey-Trace-Id`.
- No overlay lifecycle trace events yet; those are Phase 3.
