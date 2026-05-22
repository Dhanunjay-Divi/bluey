# Observability Round — Close Summary

**Branch:** `feat/phase-3-round-12`
**Tip at this doc:** populated when the round-close commits land
**Round duration:** 2026-05-22 (single-day round, six phases)

---

## TL;DR

The Observability Round is functionally closed. 5 of 6 phases shipped
with verdicts; Phase 3 (overlay lifecycle + frontend error capture) is
in flight at codex but blocks no other work — the round's primary
contract (end-to-end trace correlation across UI / daemon / cloud /
server) is satisfied.

Acceptance gate: `bash scripts/observability-acceptance-smoke.sh`
returns exit 0 with 6/6 assertions green.

---

## Phase ledger

| # | Owner | Commit | Verdict |
|---|---|---|---|
| 1. Foundations (`ObserveFields`, `account_id_hash_prefix`, header constants, request-id middleware) | Codex | `9cd66d4` | 🟢 kiro `0485a9b` |
| 2. Daemon + dashboard log rotation (JSONL, 7-day retention, `BLUEY_LOG_DIR` env) | Codex | `fdf3611` (+ N-1 fix `fbff17f`) | 🟢 kiro `2f70f58` |
| 3. Overlay lifecycle emits + frontend error capture | Codex | in flight | pending |
| 4. `bluey doctor` + `bluey logs export --redact` (+ macOS perm probes + `--json` + `bluey support`) | Kiro | `8b9c24a` + `cef8b77` + `07d2fdd` + `1e178cd` + `a29ff48` | 🟢 codex `8ab84ef` |
| 5. Trace propagation through Tauri invoke + IPC (`DaemonRequest::WithTrace` envelope, env passthrough) | Codex | `b30d5b0` (+ smoke ext `fd9e79a`) | 🟢 kiro `3187d6b` |
| 6. Standard field migration sweep (`account_id` → `account_id_hash`, email drops) | Kiro | `60ff7fd` (+ tooling `98fe051` + integration `739bc02`) | 🟢 codex `81eecdc` |

---

## Round-close acceptance gate

`scripts/observability-acceptance-smoke.sh` exercises the round's
end-to-end contract. Run on uno against debug bluey-server + release
bluey-daemon: **6/6 assertions PASS**.

| Assertion | Verifies |
|---|---|
| 0 | dashboard command layer wraps daemon IPC with WithTrace envelope |
| 1 | `X-Bluey-Trace-Id` round-trips client → server → response |
| 2 | `X-Bluey-Request-Id` round-trips client → server → response |
| 3 | Server emits `request received` + `request done` lines with both IDs |
| 4 | Server mints fresh UUIDs when client omits IDs |
| 5 | Daemon honors `BLUEY_TRACE_ID` env on IPC dispatch |

CI workflow at `.github/workflows/observability-policy.yml` runs
the analyzer's `--check-only` gate plus fmt + clippy + tests on
every push and PR. Pre-commit hook at
`scripts/pre-commit-observability.sh` runs the policy gate locally.

---

## What ships, by capability

### End-to-end trace correlation
- One user-facing operation produces a trace_id that flows through
  UI Tauri command (Rust boundary) → daemon IPC → cloud-client →
  server. Server middleware echoes via `X-Bluey-Trace-Id` response
  header. All log lines for that operation share the same trace_id.

### Persistent local logs
- `~/Library/Logs/Bluey/daemon-log.YYYY-MM-DD.log` and
  `dashboard-log.YYYY-MM-DD.log` on macOS, JSONL format with
  standard fields (component, version, platform, level, target,
  ts_ms, plus event-attached fields).
- 7-day retention, daily rotation.
- `BLUEY_LOG_DIR` / `CUE_LOG_DIR` env override.
- Subscriber-install failure degrades to stderr-only with warning.

### Support tooling
- `bluey doctor` — redacted self-diagnosis snapshot with summary
  header (build, account, permissions, log status, issue list).
- `bluey doctor --json` — same diagnosis as structured JSON for
  support automation.
- `bluey logs export --redact` — bundle local logs into a redacted
  zip; default redaction ON, `--no-redact` opt-out.
- `bluey support` — combined zip with doctor.json + system-info.txt
  + redacted logs/ + manifest.json. One command, attach to ticket.
- Real macOS permission probes (Accessibility / Microphone /
  Screen Recording) via `AXIsProcessTrusted`,
  `AVCaptureDevice.authorizationStatusForMediaType`,
  `CGPreflightScreenCaptureAccess`.

### Standard fields surface
- `cue_core::observability::ObserveFields` builder.
- `cue_core::account_id_hash_prefix` (SHA-256 first 12 hex chars).
- `cue_core::observe!` macro for emitting standard-fielded events.
- `cue_core::sanitize_observability_id` defends against log-injection
  via header values.
- HTTP header constants: `X-Bluey-Trace-Id`, `X-Bluey-Request-Id`.

### Diagnostic redaction
- Cloud-client log redaction (JSON-aware recursive on token / secret
  / password / authorization / code / url field shapes).
- Phase 4 redactor (regex-based) on log content: bearer tokens,
  magic-link URLs, Stripe IDs, OpenAI / Anthropic / Deepgram keys,
  JWTs, device codes, emails, IPv4 addresses (last octet → 0/24),
  `/Users/<name>/` and `/home/<name>/` paths.
- Auth verification/reset URLs gated behind
  `BLUEY_DEV_LOG_AUTH_LINKS=1`.
- Stripe upstream error redaction (url / client_secret /
  payment_method).

### Standard field migration sweep
- 21 `account_id` → `account_id_hash` substitutions across
  server/api/{account,auth_routes,router,stt,usage}.rs.
- 10 `email` field drops in auth/verify/reset/login paths.
- 1 `session` → `session_id` alias rename in
  cue-daemon/src/app.rs.
- Reusable migration script at `scripts/migrate-tracing-fields.py`.

### CI policy gate
- `analyze-tracing-calls.py --check-only` exits 1 if any
  transitional findings, PII findings, or alias inconsistencies
  remain. Exit 0 at current tip.
- Pre-commit hook runs the same check locally.

### Server health endpoint
- `GET /health` (public, no auth) returns:
  ```json
  { "status": "ok", "version": "0.1.0", "commit": "...",
    "platform": "macos-aarch64", "server_time_ms": ... }
  ```
- `GET /admin/health` is the legacy alias kept for backward
  compatibility.

---

## What ships, by file count

| Area | Files added/modified |
|---|---|
| `crates/cue-core/src/observability.rs` | NEW (237 LOC) |
| `crates/cue-core/src/logging.rs` | NEW (398 LOC) |
| `crates/cue-cli/src/doctor.rs` | NEW (~400 LOC) |
| `crates/cue-cli/src/logs.rs` | NEW (~300 LOC) |
| `crates/cue-cli/src/macos_perms.rs` | NEW (~200 LOC) |
| `crates/cue-cli/src/support.rs` | NEW (~250 LOC) |
| `server/src/api/middleware/request_id.rs` | NEW (155 LOC) |
| `crates/cue-core/src/ipc.rs` | `DaemonRequest::WithTrace` envelope |
| Migration sweep | 6 server + dashboard files |
| `scripts/analyze-tracing-calls.py` | NEW |
| `scripts/migrate-tracing-fields.py` | NEW |
| `scripts/observability-acceptance-smoke.sh` | NEW |
| `scripts/pre-commit-observability.sh` | NEW |
| `.github/workflows/observability-policy.yml` | NEW |
| `docs/deploy/PHASE2-MAC-SMOKE.md` | NEW |
| `docs/deploy/PHASE3-SERVER-DEPLOY.md` | NEW |
| Round handoff + verdict docs | 12 NEW under `docs/rounds/` and `docs/reviews/` |

---

## Outstanding items

### Within the round (Phase 3 codex, not blocking)

- TS-only frontend errors before the Tauri Rust boundary (Phase 3
  codex)
- Overlay lifecycle emits in Swift main.swift (Phase 3 codex)

### Outside the round (deferred to v0.2.x or later)

- Phase 5 N-1: dispatch line is `debug!` not `info!` (level
  inconsistency vs server middleware boundary log) — single-line
  change in cue-daemon, ~15 min, codex's call
- Phase 2 N-2: no max-file-size cap on rotated logs (daily-only
  rotation) — `tracing-appender` byte-size rotation needed
- Phase 6 N-3: log_dir field at first init still emits user account
  path. **Closed at `d6c042f`** — Phase 4 redactor now masks
  `/Users/<name>/` paths.
- Broader call-site `trace_id` threading sweep (159 non-conformant
  info/warn/error calls without trace_id field). Most will close
  automatically when daemon-side handlers route trace_id from the
  IPC envelope context. Tracked as analyzer-driven follow-up post
  Phase 5/Phase 3.
- Windows path redaction (`%USERPROFILE%`, `C:\Users\<name>\`).
  Gated on Windows parity work overall.
- Dashboard --json schema validation in integration test suite.
  Currently smoke-only.

### Not in this round, queued for next round

- Real macOS permission probe via objc2 — closed at `cef8b77`.
- Signed release manifest (ed25519) for safe auto-update — P0
  next-security-round.
- Local DB encryption (SQLCipher) — P0.5 next-security-round.
- Dependency audit CI (`cargo deny` / `cargo audit`).

---

## Test totals

| Layer | Before round | After round |
|---|---|---|
| Workspace cargo tests | 422 | 474 (+52) |
| Server cargo tests | 75 | 90 (+15) |
| Dashboard vitest | 15 | 15 (unchanged) |
| Acceptance smoke assertions | 0 | 6 |

---

## Pipeline state at round-close

```
✅ cargo fmt --all --check
✅ cargo clippy --all-targets -- -D warnings (workspace + server)
✅ cargo test --all-targets — 474 passed (workspace) + 90 (server)
✅ npm test --run — 15 passed (dashboard ui)
✅ npm run build — clean
✅ swift build (overlay) — clean
✅ scripts/analyze-tracing-calls.py --check-only — exit 0
✅ scripts/observability-acceptance-smoke.sh — 6/6 PASS
```

---

## Next round candidates (post-Observability close)

In rough priority order:

1. **Closed-alpha gates** (operator-side, not code work):
   - Real-Mac smoke per `docs/deploy/PHASE2-MAC-SMOKE.md`
   - Server staging deploy per `docs/deploy/PHASE3-SERVER-DEPLOY.md`
2. **Next-security-round**:
   - Signed release manifest (P0)
   - `bluey check-update` against the manifest (P0.5)
   - Dependency audit CI (P1)
3. **Polish round** for v0.2.x backlog:
   - Phase 5 N-1 fix
   - Phase 2 N-2 byte-size rotation
   - Broader call-site trace_id sweep
4. **Closed-alpha launch** (when 1 + 2 ship)

---

## Summary

The Observability Round closed in a single day across 6 phases, 5
review verdicts, 1 acceptance smoke, 1 CI workflow, 1 pre-commit
hook, ~2500 LOC of new code, ~400 LOC of new scripts, and ~600 LOC
of new docs. Both kiro-owned phases shipped before the codex Phase 3
final polish landed; codex Phase 3 finishes the round but the
trace-correlation contract is already satisfied at the current tip.

Bluey now has end-to-end observability sufficient for closed alpha:
support diagnostics, redacted log bundling, persistent local logs,
trace correlation across UI / daemon / cloud / server, and
automated policy gates that prevent regression.
