# Observability Phase 3 — Overlay lifecycle + frontend error capture

Branch: `feat/phase-3-round-12`
Tip before: `2dbd323`
Tip after: commit containing this handoff

## What changed

| Area | Files | Summary |
|---|---|---|
| Overlay lifecycle event contract | `crates/cue-core/src/overlay.rs` | Added `OverlayEvent::Lifecycle { stage, status, detail }` with serialization coverage. |
| Daemon overlay logging + validation | `crates/cue-daemon/src/app.rs` | Production overlay-line validator now bounds lifecycle fields before deserialization; daemon logs accepted lifecycle events with `overlay_stage`, `overlay_status`, and `overlay_detail`. |
| macOS overlay emits lifecycle | `native/macos/cue-overlay/Sources/cue-overlay/main.swift` | Swift overlay emits `started`, `expanded`, `collapsed`, `hidden`, and `shutdown` lifecycle events through the existing token-authenticated overlay IPC line protocol. |
| Dashboard frontend error command | `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/src/lib.rs` | Added `report_frontend_error` Tauri command, truncating and control-character-sanitizing source/message/stack fields before logging. |
| Frontend invoke/error wrapper | `crates/cue-dashboard/ui/src/lib/tauri.ts`, dashboard UI imports | Raw Tauri `invoke()` is centralized behind a wrapper that reports rejected commands; global `window.error` and `window.unhandledrejection` handlers report frontend-only failures. |

## Why

Phase 1 gave Bluey trace/request ids, Phase 2 gave durable daemon/dashboard logs, Phase 5 propagated trace ids through IPC, and Phase 6 removed sensitive fields. This phase closes the remaining user-facing blind spots:

1. Native overlay lifecycle is now visible in daemon logs instead of only inferred from process state.
2. Dashboard command failures and JS-only errors now land in dashboard logs.
3. Future support bundles can correlate "the overlay disappeared" and "the dashboard button failed" with concrete log records.

## Verification

Focused checks run during implementation:

```bash
swift build -c release --package-path native/macos/cue-overlay ✅
```

Full gate before handoff:

```bash
cargo fmt --all --check                              ✅
cargo clippy --all-targets -- -D warnings            ✅
cargo build --all-targets                            ✅
cargo test --all-targets                             ✅
cd server && cargo clippy --all-targets -- -D warnings ✅
cd server && cargo test                              ✅
cd crates/cue-dashboard/ui && npm test -- --run      ✅
cd crates/cue-dashboard/ui && npm run build          ✅
swift build -c release --package-path native/macos/cue-overlay ✅
scripts/observability-acceptance-smoke.sh            ✅
git diff --check                                     ✅
```

## Review checklist

- [ ] Confirm `OverlayEvent::Lifecycle` is backward-compatible with existing overlay IPC JSON.
- [ ] Confirm production overlay validation rejects oversized lifecycle fields before typed dispatch.
- [ ] Confirm Swift lifecycle emits include the per-session overlay token automatically via the shared `emitLine` path.
- [ ] Confirm the frontend wrapper is the only direct consumer of `@tauri-apps/api/core` invoke.
- [ ] Confirm `report_frontend_error` does not log secrets, full paths, or unbounded stack payloads.
- [ ] Confirm global handlers cannot recurse if reporting itself fails.

## Areas most likely wrong

1. **Swift lifecycle is macOS-only in this phase.** The plan called out Swift `main.swift`; Windows overlay lifecycle parity is still a later polish pass.
2. **Frontend capture is local-log capture, not server upload.** Errors go to the dashboard log file through the Tauri command; support export can pick them up later.
3. **Very early JS failures are best-effort.** The handler installs before React render, but a failure before `main.tsx` executes cannot be captured by the app itself.
4. **Visible overlay lifecycle QA is still manual.** Build and protocol tests cover the code path; actual click/expand/collapse visual QA needs a real macOS desktop run.

## Not in scope

- No new server endpoints.
- No trace-id mutation in TypeScript; Phase 5 keeps trace minting at the Rust Tauri command boundary.
- No user-facing dashboard UI for logs yet.
- No Windows overlay lifecycle emit parity.
