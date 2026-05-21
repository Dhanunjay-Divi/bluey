# Observability Round Plan

**Branch:** `feat/phase-3-round-12` → next round
**Author:** Kiro
**Date:** 2026-05-21
**Goal:** End-to-end traceability + production-grade support diagnostics across UI / daemon / cloud-client / server / providers.

---

## 1. Why This Round Exists

Codex flagged in the security review handoff:

> Logging is good but not production-support complete.

Today's logging gaps:

- **No correlation across hops.** A customer reports "answer was slow"; we have no trace_id linking the dashboard event → daemon dispatch → cloud-client request → server router → upstream provider. We can't reconstruct timing from logs.
- **Logs are stderr-only on the daemon.** No on-disk persistence, no rotation. If the customer restarts before reporting, the relevant logs are gone.
- **Frontend errors are silent.** A failing Tauri invoke (e.g., `request_cue` that throws) only logs in the JS console — not captured in Rust logs that operators can request.
- **Overlay lifecycle is invisible.** When/why the overlay window crashes, when sharingType was applied, when the daemon spawned the helper — none of it is in the daemon log timeline.
- **No `bluey doctor`.** When a customer hits an issue, we ask them to manually share log files. There's no one-command bundle-and-redact-and-share workflow.
- **Field naming is inconsistent.** Some logs use `request_id`, some `req_id`, some inline. No standard set across the codebase.

This round closes those gaps. **It is observability, not metrics** — Carnaval / equivalent metrics tooling is a separate operator-side gate per the prelaunch checklist.

---

## 2. Standard Log Fields

Every log line emitted by Bluey code (server, daemon, dashboard, overlay, CLI) should be able to carry these fields when they apply:

| Field | Type | Source | Notes |
|---|---|---|---|
| `component` | string | bake-in per crate | `cue-daemon`, `bluey-server`, `cue-dashboard`, `cue-overlay`, `cue-cli`, `cue-cloud-client` |
| `version` | string | `env!("CARGO_PKG_VERSION")` | From the build |
| `platform` | string | `std::env::consts::OS` + arch | `macos-aarch64`, `linux-x86_64`, etc. |
| `trace_id` | string (UUIDv4) | propagated | One per user-facing operation. Created at the UI/CLI entry point, threaded through daemon, cloud-client, server. |
| `request_id` | string (UUIDv4) | per-hop or per-request | One per HTTP/IPC call. Server middleware mints if missing. |
| `session_id` | string | meeting/session id | The Bluey session this operation belongs to (if any). |
| `account_id_hash` | string | SHA-256 prefix of account_id | **Hashed**, not raw, so logs aren't PII-laden by default. |
| `status` | string | `ok` / `err` / `degraded` | Final state of the operation. |
| `latency_ms` | u64 | measured | Wall-clock duration of the operation. |
| `provider` | string | `openai` / `anthropic` / `deepgram` / `bluey-managed` | Which provider handled this. |
| `model` | string | model id | E.g. `gpt-4o-mini`, `nova-3`. |
| `cost_cents_to_customer` | i64 | from billing event | Charged to the customer for this operation. |
| `cost_cents_to_bluey` | i64 | from billing event | What Bluey paid the upstream. |

**Implementation approach:** thin wrapper macros in `cue-core` that take a struct of fields + the message:

```rust
log::observe!(
    level: Level::Info,
    component: "cue-daemon",
    trace_id,
    session_id: session.id,
    status: "ok",
    latency_ms,
    "answer dispatched",
);
```

These expand to standard `tracing::event!` calls so `tracing_subscriber` JSON output works without further effort.

**`account_id_hash`:** SHA-256 of account_id, take first 12 hex chars (48 bits) — enough to disambiguate within a customer's logs without making logs a PII firehose. Operators can re-derive the hash for support if they have the account_id.

---

## 3. Trace ID Propagation

### Entry points that mint a fresh trace_id:

1. **CLI command start** (`bluey on`, `bluey login`, `bluey cloud sync`, etc.) — `cue-cli` mints UUIDv4, passes via env var `BLUEY_TRACE_ID` to subprocesses.
2. **Tauri invoke from UI** — `cue-dashboard` mints a trace_id per invoke and includes it in the IPC payload to the daemon.
3. **F19 hotkey trigger / overlay button press** — daemon-internal start, daemon mints.
4. **Inbound webhook** (Stripe, etc.) — server middleware mints from header or fresh.

### Propagation:

- **CLI → daemon (IPC)** — IPC frame carries `trace_id` field. Daemon uses received trace_id; logs all operations under it.
- **Daemon → cloud-client → server (HTTP)** — `cue-cloud-client` always sends `X-Bluey-Trace-Id` header. Server middleware reads it (or mints if missing) and threads through request handler context.
- **Server → upstream provider** — `X-Bluey-Trace-Id` continues to upstream calls when supported (OpenAI ignores; Anthropic accepts; Deepgram accepts via metadata). On responses, server logs include the trace_id alongside upstream provider request IDs.
- **UI → daemon (Tauri invoke)** — UI mints, passes via invoke args; daemon honors.

### Standard header: `X-Bluey-Trace-Id`

Server response header echoes the trace_id back so the client can correlate. Same for `X-Bluey-Request-Id` (per-hop).

---

## 4. Server Request-Id Middleware

New middleware `server/src/api/middleware/request_id.rs`:

- On request entry: read `X-Bluey-Request-Id` (incoming) or mint UUIDv4. Store in axum extension `RequestId(String)`.
- Read `X-Bluey-Trace-Id` (incoming) or mint UUIDv4. Store as `TraceId(String)`.
- Spans created via `tracing::info_span!("request", request_id, trace_id, ...)` for the request duration.
- On response: set `X-Bluey-Request-Id` and `X-Bluey-Trace-Id` headers.
- Always log: `request received { method, path, account_id_hash, request_id, trace_id }` at start, `request done { status, latency_ms }` at end.

**Existing logger replacement:** there's likely a basic `tower_http::trace::TraceLayer` already. Replace with a custom layer that emits the standard fields.

---

## 5. Persistent Local Logs

### Daemon (highest priority)

Use `tracing-appender` with a daily rotating file in `~/Library/Logs/Bluey/daemon-YYYY-MM-DD.log` (Linux: `~/.local/state/bluey/log/`).

- Format: JSON lines. One line = one event.
- Retention: 7 daily files. Older auto-deleted.
- Always log to BOTH file AND stderr in dev; file only in production.

### Dashboard (Tauri)

Use a Tauri-side `tracing-appender` to `~/Library/Logs/Bluey/dashboard-YYYY-MM-DD.log`. Smaller volume than daemon — 7 daily retention is fine.

### Overlay (Swift)

Swift overlay can use `os.log` (Apple's unified logging) — no rotation needed; macOS handles it. For now, keep `os.log` and add a small "lifecycle" emit at startup (window created, sharingType applied, capture-visible mode, expanded vs collapsed transitions) so daemon-side logs can be cross-referenced.

### CLI

CLI prints to stdout/stderr (don't change). One-shot invocations don't benefit from rotation.

---

## 6. `bluey doctor` + `bluey logs export --redact`

### `bluey doctor`

New CLI subcommand that gathers a self-diagnosis snapshot:

```
bluey doctor
```

Outputs:
- Bluey version + build commit
- macOS version + arch
- Account login status (logged in / logged out — NO token)
- Cloud API URL (`account.api_url`)
- SMTP configured? (yes/no — server-side, derived from account status response)
- F19 hotkey registered? (probe AppKit)
- Accessibility permission granted? (probe `AXIsProcessTrusted`)
- Microphone permission granted? (probe `AVCaptureDevice.authorizationStatus`)
- Disk permissions on `~/Library/Application Support/Bluey/` (sanity check 0700)
- Last 20 daemon log lines (passed through the redactor)
- Last 5 cue_responses (counts only, no body) — verifies SQLite is functional

Designed for the customer to paste the output into a support ticket.

### `bluey logs export --redact [--days N]`

Bundles the recent log files into a single `.zip` after running them through a redactor:

- Redactor strips: bearer tokens, magic-link URLs, raw email addresses (replaced with `<email>` placeholder), Stripe customer IDs (`cus_xxx`), Deepgram keys, OpenAI/Anthropic keys, full IP addresses (replaced with `/24` prefix mask).
- Account-id-hash and session_id are PRESERVED (those are the join keys for support).
- Output goes to `~/Bluey-logs-export-YYYYMMDD.zip` by default; `-o` flag for custom path.
- Defaults to 7 days; `--days N` for shorter or longer range.

This is the customer-facing version of "send us your logs" without the customer manually grepping for tokens.

---

## 7. Frontend Error Capture

`crates/cue-dashboard/ui/`:

1. Add a global window error handler in the React entry point that posts to a Tauri command `report_frontend_error(error, stack, route, user_action)`.
2. The Tauri command receives + logs via `tracing::error!` with `component: "cue-dashboard.ui"`, includes the trace_id of the most recent invoke if available.
3. Failed `invoke()` calls also funnel through the same error reporter via a `wrappedInvoke()` helper.

This means a JS-land "TypeError: undefined is not an object" appears in `dashboard-YYYY-MM-DD.log` with stack and route, instead of being lost.

---

## 8. Overlay Lifecycle Logs

Daemon-side, log:
- Helper spawn → captured stderr from the helper process is piped to a local file under `~/Library/Logs/Bluey/overlay-YYYY-MM-DD.log` AND copied into the daemon log at level `info`.
- Helper exit → log cause (clean / killed / segfault).
- Helper unresponsive → if no IPC heartbeat for >10s, log warn + kill+respawn.

Swift-side, emit one-line events at:
- Window created (sharingType, frame, capture-visible state)
- Disguise mode change applied (mode, label, icon path)
- Pill ↔ expanded transition
- Card pushed (kind, has_artifact, length)
- User action (composer send, hide, close, opacity changed)

These give a per-second timeline of the visible overlay state when correlating against a customer-reported moment.

---

## 9. Implementation Phasing

This is a big surface. Phase to keep each step shippable.

### Phase 1: Foundations (1 commit)
- Standard fields struct + `observe!` macro in `cue-core`
- `account_id_hash` helper
- Trace ID propagation in cloud-client (`X-Bluey-Trace-Id` header)
- Server request-id middleware

### Phase 2: Daemon + dashboard rotation (1 commit)
- `tracing-appender` for daemon → `~/Library/Logs/Bluey/daemon-YYYY-MM-DD.log` (7-day retention)
- `tracing-appender` for dashboard → `~/Library/Logs/Bluey/dashboard-YYYY-MM-DD.log`
- Wire all existing log call sites to include component + version + platform automatically

### Phase 3: Overlay lifecycle + frontend error capture (1 commit)
- Overlay startup/lifecycle emits in Swift main.swift
- Global JS error handler + `report_frontend_error` Tauri command
- Wrap all `invoke()` call sites with error capture

### Phase 4: bluey doctor + bluey logs export --redact (1 commit)
- New CLI subcommands
- Log redactor as a `cue-cloud-client` helper (reuses existing `is_sensitive_log_key`)
- ZIP bundling

### Phase 5: Trace propagation through Tauri invoke + IPC (1 commit)
- Dashboard mints trace_id per invoke, includes in IPC payload
- Daemon honors received trace_id, threads through dispatch
- All daemon → cloud-client → server requests carry the same trace_id

### Phase 6: Standard field migration (1 commit, last)
- Sweep all existing `tracing::info!` / `tracing::warn!` call sites
- Replace inline fields with the `observe!` macro
- Field-name dedup (request_id everywhere, not req_id)

Each phase ships as one commit, pipeline-gated, with codex review between phases.

---

## 10. Acceptance Criteria

End-to-end test scenario (manual smoke):

1. Customer hits F19 to ask Bluey a question.
2. The question, dispatch, server call, OpenAI call, response, and persistence all log lines that share the SAME `trace_id`.
3. The dashboard "Show Logs" button pulls the JSON log entries for that trace_id.
4. `bluey doctor` runs and produces a redacted summary including the last 20 daemon log lines.
5. `bluey logs export --redact` produces a zip; `unzip + grep -i "key\|secret\|token"` returns ZERO matches in the redacted contents.
6. Server response headers include `X-Bluey-Trace-Id` and `X-Bluey-Request-Id`.
7. Failed Tauri invoke shows up in `dashboard-YYYY-MM-DD.log` with stack.
8. Overlay window-create, disguise change, card push are timestamped in the overlay log.

---

## 11. What Is OUT of Scope

- **Carnaval / metrics dashboards** — operator-side, separate gate.
- **APM / distributed tracing UI** — Honeycomb / Tempo / Jaeger integration would be P1 once we have a traffic shape.
- **Account-id un-hashing** — PII boundary; if support needs to find a specific account, they get the hash from the customer's `bluey doctor` output and re-derive.
- **Reverse-proxy access logs** — Caddy already logs these on the server host; out of scope here.
- **Client-side performance metrics** (FPS, paint times) — not what this round is for.

---

## 12. Out-of-Round Followups (track separately)

After Phase 6 lands:

- Add `tracing-flame` or `tracing-tracy` to the dev profile for local performance investigation.
- Consider OpenTelemetry export from the server as the v1.0 production path.
- Build a small operator dashboard that ingests daemon logs from `bluey doctor` uploads (anonymous if customer opts in).

---

## 13. Estimated Effort

- Phase 1: ~4-6 hours of focused work
- Phase 2: ~3-4 hours
- Phase 3: ~4-6 hours (Swift + Tauri-side glue)
- Phase 4: ~3-4 hours
- Phase 5: ~2-3 hours (mostly threading)
- Phase 6: ~4-6 hours (sweep is mechanical but broad)

**Total: ~20-30 hours of focused implementation across 6 commits.** Each phase is independently shippable and codex-reviewable.

---

## 14. Recommended Trigger

After the user fixes B-1 (the security hardening regression) and commits the Stage 25 + Security Hardening + Pinky leak parity batch.

This round is the right shape to start the moment v0.2 alpha goes through its first real-customer smoke — that's when missing observability will hurt the most.
