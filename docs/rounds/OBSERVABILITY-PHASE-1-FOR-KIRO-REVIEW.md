# Round Handoff: Observability Phase 1 — trace foundations

```
Branch:     feat/phase-3-round-12
Tip before: 8b9c24a
Tip after:  9cd66d4
Author:     Codex
Reviewer:   Kiro
Round of:   Observability Round, Phase 1
Round shape: shared observability contract + cloud-client headers + server middleware
```

## 1. What changed

| File / Area | Change |
|---|---|
| `crates/cue-core/src/observability.rs` | NEW shared observability primitives: `ObserveFields`, `observe!` compile-smoke, `account_id_hash_prefix`, trace/request header constants, id sanitization, UUID helpers. |
| `crates/cue-core/src/lib.rs` | Exports observability helpers and adds the `observe!` macro. |
| `crates/cue-core/Cargo.toml` | Adds `tracing` + `sha2` for standard event emission and account hashing. |
| `crates/cue-cli/src/doctor.rs` | Reuses `cue_core::account_id_hash_prefix` so doctor and runtime logs share the same account-hash function. |
| `crates/cue-cloud-client/src/client.rs` | Adds `ClientConfig.trace_id`, `CloudClient::with_trace_id`, per-request `X-Bluey-Request-Id`, and optional `X-Bluey-Trace-Id` from config or `BLUEY_TRACE_ID`. |
| `crates/cue-cloud-client/Cargo.toml` | Adds `cue-core` dependency for shared header constants and trace helpers. |
| `server/src/api/middleware/request_id.rs` | NEW Axum middleware: reads or mints request/trace ids, injects `RequestId`/`TraceId` extensions, echoes response headers, logs request start/end with latency. |
| `server/src/api/mod.rs` | Replaces generic `TraceLayer::new_for_http()` with the custom request-id middleware. |
| `server/Cargo.toml` | Adds `cue-core` dependency for shared constants and id helpers. |
| `server/src/api/stt.rs`, `server/src/api/sync.rs`, `server/src/db/mod.rs`, `server/src/db/sync.rs`, `server/tests/integration_e2e.rs` | `cargo fmt` normalization from running the server workspace gate. No behavior change. |

Related but separate commit:

- `8ab84ef docs(reviews): accept observability phase 4`

## 2. Why

Phase 1 establishes the IDs and field names the later observability phases need. Before this, the daemon/cloud/server path had request ids in payloads but no standard HTTP trace/request headers and no shared account-hash helper. Now cloud-client requests carry a per-hop request id, can propagate a caller-supplied trace id, and the server guarantees both ids are available as extensions and echoed back to clients.

## 3. Verification

Focused tests before the full gate:

```bash
cargo test -p cue-core observability
cargo test -p cue-cloud-client
cd server && cargo test api::middleware::request_id
```

Commit gate:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo test --all-targets
cd server
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo test
git diff --check
```

Observed counts:

- Main workspace: 459 passed, 14 ignored.
- Server workspace: 90 passed, 0 ignored.
- `git diff --check`: clean before commit.

## 4. Areas Most Likely Wrong

1. **Trace id source is intentionally narrow.** `CloudClient` propagates `ClientConfig.trace_id` or `BLUEY_TRACE_ID`; it does not mint a trace id by itself. Phase 5 should explicitly mint per UI/Tauri invoke and call `with_trace_id(...)`.
2. **Server middleware does not include `account_id_hash` yet.** It runs before auth has attached `AuthedAccount`. Handler-level Phase 6 migration should add account hashes where authenticated account context exists.
3. **`observe!` is intentionally thin.** It standardizes core fields but does not yet support arbitrary extra fields like `method` or `path`; server middleware logs those directly for now.
4. **Server formatting churn is included.** Because the server workspace is now part of the Phase 1 gate, `cargo fmt` normalized a few pre-existing long lines outside the middleware files.

## 5. Honest Limitations

- No daemon IPC trace threading yet. That is Observability Phase 5.
- No Tauri invoke trace minting yet. That is Observability Phase 5.
- No persistent log rotation yet. That is Observability Phase 2.
- No frontend error capture or overlay lifecycle emits yet. That is Observability Phase 3.
- No full standard-field sweep of all existing logs yet. That is Observability Phase 6.

## 6. Reviewer Checklist

- Verify `cue-core::account_id_hash_prefix` matches the Phase 4 doctor hash contract.
- Verify cloud-client sends `X-Bluey-Request-Id` on auth and public requests.
- Verify cloud-client only sends `X-Bluey-Trace-Id` when configured or env-provided.
- Verify server middleware echoes both headers and injects extensions.
- Decide whether replacing `TraceLayer::new_for_http()` with custom middleware is acceptable now, or whether you want both during the transition.

## 7. Verdict Request

Write the verdict at:

`docs/reviews/REVIEW-OBSERVABILITY-PHASE-1-BY-KIRO.md`

Verdict options per the collaboration contract:

- 🟢 ACCEPT
- 🟡 ACCEPT WITH FOLLOWUPS
- 🔴 BLOCKER
