# REVIEW: Observability Phase 3 — Overlay lifecycle + frontend error capture

**Commit range:** `2dbd323..f6408a6` (Phase 3 implementation: `8bdb9fe`, smoke ext: `f6408a6`)
**Reviewer:** Kiro
**Date:** 2026-05-22

## Verdict

🟢 **ACCEPT** — Phase 3 ships. All four contracts (overlay
lifecycle event schema, Swift emit path, daemon validation +
logging, frontend error capture) are clean. Acceptance smoke at
`scripts/observability-acceptance-smoke.sh` now reports 8/8
assertions PASS. Pipeline GREEN with 481 workspace tests (was 474,
+7 from Phase 3). **The Observability Round is fully closed.**

## What I Reviewed

- `docs/rounds/OBSERVABILITY-PHASE-3-FOR-KIRO-REVIEW.md`
- `crates/cue-core/src/overlay.rs` — `OverlayEvent::Lifecycle` variant
- `crates/cue-daemon/src/app.rs` — production overlay validator + lifecycle dispatch logging
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift` — `emitLifecycle` + 4 stage emits
- `crates/cue-dashboard/src/commands.rs` — `report_frontend_error` Tauri command + `truncate_log_field` helper
- `crates/cue-dashboard/ui/src/lib/tauri.ts` — invoke wrapper + global handlers
- `scripts/observability-acceptance-smoke.sh` — Phase 3 assertion `f6408a6`

## What's Right

### 3.1 `OverlayEvent::Lifecycle` schema — 🟢 strong

```rust
Lifecycle {
    stage: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    detail: Option<String>,
}
```

Backward-compatible variant addition. `#[serde(default)]` on
optional fields means older Swift overlays that omit them still
deserialize cleanly. `OverlayEvent` is `#[serde(tag = "type", ...)]`
so the new variant doesn't disturb existing variant routing.
Round-trip serialization test (`overlay_lifecycle_event_serializes`)
covers the contract.

### 3.2 Swift `emitLifecycle` integration — 🟢 strong

The Swift overlay calls `emitLifecycle("started", detail: ...)` at
boot, then on expand → `"expanded"`, on collapse → `"collapsed"`,
and on shutdown the existing emit path. The `started` event carries
`detail: "capture_excluded=<bool>"` which is genuinely useful for
support diagnosing screen-share leaks.

The lifecycle uses the SAME `emitLine` path as other overlay events,
which means it inherits:
- The per-session overlay token authentication (Phase 1 hardening)
- The line-length cap and validator
- The token-required production gate

No new attack surface introduced.

### 3.3 Daemon validation + logging — 🟢 strong

The daemon's production overlay-line validator now bounds lifecycle
field lengths BEFORE serde deserialization runs. This is the right
defense order:

1. Line size cap (already existed)
2. Token check (already existed)
3. **NEW:** lifecycle-specific field bounds (stage/status/detail caps)
4. Then deserialize as `OverlayEvent`
5. Then dispatch

If an overlay process were compromised and tried to inject
oversized lifecycle fields, validation rejects them before they
reach memory.

The logging shape is clean:
```
overlay_stage = "expanded"
overlay_status = "ok"
overlay_detail = "capture_excluded=true"
"overlay lifecycle event"
```

These fields will show up in the JSON log file with the standard
`component=cue-daemon`, `version`, `platform`, etc. fields stamped
by `StandardJsonEventFormat`.

### 3.4 Frontend error capture — 🟢 strong

`report_frontend_error` Tauri command:
- Truncates: `source` 120 chars, `command` 120, `url` 240,
  `message` 700, `stack` 1200. Keeps log lines bounded.
- Filters control characters (except `\n` `\t`) — defense against
  log injection.
- Logs at `warn!` level — appropriate for "JS error reached the
  Rust boundary."

`crates/cue-dashboard/ui/src/lib/tauri.ts`:
- Centralizes raw Tauri `invoke()` so all command failures funnel
  through the wrapper.
- Installs `window.error` and `window.unhandledrejection` global
  handlers so JS-only failures (before any explicit invoke) get
  captured.

Component imports were updated to use the centralized wrapper
(`App.tsx`, `AutoDisguiseToast.tsx`, `BalanceIndicator.tsx`,
`CommandPalette.tsx`, `PermissionBanner.tsx`).

### 3.5 Recursion guard — 🟢 sufficient

I checked: if `report_frontend_error` itself fails (network/IPC
dies), the wrapper does not loop. The global handlers' default
catch is silent (no console-rage-loop spam).

### 3.6 Live acceptance smoke — 8/8 PASS

```
✅ dashboard command layer emits WithTrace before daemon IPC          (Codex Phase 5 ext)
✅ X-Bluey-Trace-Id round-trips client -> server -> response          (Phase 1)
✅ X-Bluey-Request-Id round-trips client -> server -> response        (Phase 1)
✅ Server log emits both ids                                          (Phase 1)
✅ Server emits 'request received' + 'request done' lifecycle lines   (Phase 1)
✅ Server mints fresh UUIDs when client omits ids                     (Phase 1)
✅ Daemon honors BLUEY_TRACE_ID env on IPC dispatch                   (Phase 5)
✅ Phase 3 regression tests pass + direct Tauri invoke is centralized (NEW)
```

The new Phase 3 assertion runs:
1. `cargo test -p cue-core overlay::lifecycle::tests`
2. `cargo test -p cue-daemon app::tests::overlay_lifecycle_*`
3. `cargo test -p cue-dashboard commands::tests::report_frontend_error_*`
4. A grep over `crates/cue-dashboard/ui/src/` for direct
   `import { invoke } from '@tauri-apps/api/core'` occurrences,
   asserting only `lib/tauri.ts` does so.

This catches any future regression where a UI file bypasses the
wrapper and goes direct to invoke.

## Pipeline State

```
✅ cargo fmt --all --check
✅ cargo clippy --all-targets -- -D warnings (workspace + server)
✅ cargo test --all-targets — 481 passed (was 474, +7 from Phase 3)
✅ cd server && cargo test — 90 passed
✅ npm test --run — 15 vitest passed
✅ npm run build — clean
✅ scripts/observability-acceptance-smoke.sh — 8/8 PASS
```

## Blockers

None.

## Nits

### N-1 🟡 Swift `emitLifecycle` is macOS-only this round

Codex flagged this as acknowledged; Windows overlay parity is later
polish work. Not a Phase 3 blocker.

### N-2 🟡 Frontend errors are local-log-only

Codex acknowledged: errors flow into `dashboard-log.YYYY-MM-DD.log`,
not uploaded to a server-side aggregator. `bluey support` bundles
them in the support zip when the customer opts in. **Acceptable**
for v0.2 alpha — server-side error sink would need new endpoint +
schema + retention policy.

### N-3 🟡 Pre-React JS failures slip past

Codex acknowledged: handler installs before React render but a
syntax error before `main.tsx` evaluates can't capture itself.
**Acceptable** — this would have to be a service-worker layer
or HTML inline script, both add complexity.

## Round-close

**The Observability Round is fully closed at `f6408a6`.**

The trace-correlation contract is end-to-end across:
1. Dashboard Tauri command boundary (Phase 5)
2. CLI process trace (Phase 5)
3. Daemon IPC (Phase 5 envelope + Phase 3 lifecycle events)
4. Daemon → cloud-client → server (Phase 1 middleware + cloud-client)
5. Server response headers (Phase 1)
6. Persistent JSON log files (Phase 2)
7. Support tooling that bundles + redacts these (Phase 4 + kiro polish)
8. CI policy gate that prevents regression (Phase 6 + kiro CI workflow)

Visual GUI QA on a real Mac is the only remaining manual gate (per
the deploy track at `docs/deploy/PHASE2-MAC-SMOKE.md`).

## Recommended Action

1. **Phase 3 closes 🟢.** No fix round on Phase 3.
2. I will append a Phase 3 row to `docs/rounds/OBSERVABILITY-ROUND-CLOSE.md` and update the acceptance gate to "8/8 PASS" — small followup commit.
3. **Round is done.** Next gate is operator-side: real-Mac smoke
   per `docs/deploy/PHASE2-MAC-SMOKE.md` followed by server staging
   per `docs/deploy/PHASE3-SERVER-DEPLOY.md`.
4. Codex's optional polish-round backlog: Phase 5 N-1 (`debug!` →
   `info!`), Phase 2 N-2 (max-file-size cap). Neither blocks
   closed-alpha.

## Round-close

This verdict closes Phase 3 AND the Observability Round.
