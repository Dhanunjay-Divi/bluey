# Phase 1 Dashboard Shell — Handoff for Codex Review

**Branch**: `feat/phase-1-dashboard-shell`
**Base**: `fa19623` (tip of `feat/phase-0-foundation`)
**Tip**: `a064630`
**Date**: 2026-05-12
**Author**: kiro (Phase 1 implementation agent)

---

## Scope

Phase 1 of the bluey master plan — **Dashboard Shell**. End-to-end wiring from Tauri backend (Database state, commands, global shortcut) through React frontend (routing, sidebar, session list, command palette, event channel).

---

## Commits (3 total, oldest to newest)

```
eaadb4f [D1.1] feat(dashboard): install npm deps and configure Tailwind 4
f7d6399 [D1.2+D1.3+D1.4] feat(dashboard): integrate DB state, Tauri commands, global shortcut
a064630 [D1.5+D1.6+D1.7+D1.8] feat(dashboard-ui): React shell with routing, sessions, command palette, events
```

## Per-task commit map

| Task | Commit | Description |
|------|--------|-------------|
| D1.1 | eaadb4f | npm install + Tailwind 4 + vite plugin |
| D1.2 | f7d6399 | Database in Tauri State via Mutex |
| D1.3 | f7d6399 | 5 Tauri commands: list/create/get/archive/delete sessions |
| D1.4 | f7d6399 | Global shortcut Cmd+Shift+D toggle |
| D1.5 | a064630 | React Router + DashboardLayout + Sidebar with 9 routes |
| D1.6 | a064630 | /chats page with session CRUD |
| D1.7 | a064630 | Command palette Cmd+K with cmdk |
| D1.8 | a064630 | useSessionEvents hook for event channel |

---

## Build + test verification

```bash
# npm build
$ cd crates/cue-dashboard/ui && npm run build
  1697 modules transformed
  dist/index.html                   0.39 kB
  dist/assets/index-B3rzTlbj.css   12.51 kB
  dist/assets/index-BSCM8SI_.js   289.85 kB
  built in 1.34s
# PASS

# cargo fmt (dashboard)
$ cargo fmt -p cue-dashboard -- --check
# PASS (no diff)

# cargo build --release (full workspace)
$ cargo build --release
  Finished release profile in 1m 36s
# PASS (8 pre-existing nspanel warnings only)

# cargo test --all-targets
$ cargo test --all-targets
  test result: ok. 14 passed; 0 failed; 0 ignored
# PASS

# cargo build -p cue-dashboard --release (with real ui/dist)
$ cargo build -p cue-dashboard --release
  Finished release profile
# PASS
```

---

## Layer-by-layer change summary

### DASHBOARD (Rust backend)

- **Cargo.toml**: Added cue-daemon, cue-core, dirs, uuid, tauri-plugin-global-shortcut deps
- **src/lib.rs**: DbState with Mutex-wrapped Database, global shortcut registration, plugin init
- **src/commands.rs**: 5 tauri::command functions bridging invoke to Database methods
- **capabilities/default.json**: Added global-shortcut allow-register/unregister

### DASHBOARD (React frontend)

- **package.json**: Added react-router-dom, lucide-react, cmdk, tailwindcss, @tailwindcss/vite
- **vite.config.ts**: Tailwind 4 vite plugin
- **src/App.tsx**: BrowserRouter with 9 routes
- **src/components/**: DashboardLayout, Sidebar with 9 NavLinks + icons, CommandPalette with cmdk
- **src/pages/**: Chats with session CRUD, Placeholder for generic routes
- **src/hooks/**: useSessionEvents event listener for backend events

### INFRA

- **Cargo.lock**: Updated with new dependencies

---

## Known quirks

1. Pre-existing nspanel warnings (8 total) from objc macro in tauri-nspanel. Not introduced by Phase 1.
2. Pre-existing fmt diffs in cue-core/cue-daemon from Phase 0. Not touched by Phase 1.
3. Event channel is listen-only. Backend does not emit session:created yet — Phase 2 scope.
4. No React tests. No test framework set up for UI — out of Phase 1 scope.

---

## Review checklist for codex

- [ ] **Correctness**: Tauri commands correctly wrap Database methods with proper error handling
- [ ] **Safety**: Mutex lock is held only for the duration of each command (no deadlock risk)
- [ ] **Tests**: All 14 existing tests still pass; no regressions
- [ ] **Docs**: IMPL doc covers all tasks, deviations documented
- [ ] **Style**: Rust code passes cargo fmt; TypeScript uses consistent patterns
- [ ] **Next-batch readiness**: Phase 2 can build on this — event channel wired, commands exposed, UI shell navigable

---

## Verdict request

Please review the 3 commits on `feat/phase-1-dashboard-shell` (base `fa19623`, tip `a064630`). Confirm:
1. All 8 tasks (D1.1-D1.8) are implemented per plan
2. Build and test verification passes
3. No regressions to Phase 0 code
4. Ready for Phase 2 (Chat UI + streaming)

---

## Continuity note for Phase 2

Phase 2 should:
- Emit session:created events from the Rust backend after create_session
- Build chat UI on /chats/:id route (already has router infrastructure)
- Add streaming response display using the event channel
- Consider adding vitest for React component testing
