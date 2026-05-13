# IMPL: Phase 1 — Dashboard Shell

## Task IDs

| ID | Size | Title |
|----|------|-------|
| D1.1 | M | Install dashboard npm deps + verify React runs |
| D1.2 | M | Integrate daemon Database into Tauri State |
| D1.3 | M | Tauri commands bridging dashboard to daemon |
| D1.4 | M | Global shortcut Cmd+Shift+D |
| D1.5 | L | React dashboard shell (routing, sidebar, layout) |
| D1.6 | M | Session list page (/chats) |
| D1.7 | S | Command palette stub (Cmd+K) |
| D1.8 | M | Dashboard to daemon event channel |

## Scope

**Does:**
- Installs all npm dependencies (react-router-dom, lucide-react, cmdk, tailwindcss, @tailwindcss/vite)
- Configures Tailwind 4 via Vite plugin
- Integrates cue-daemon Database into Tauri managed state (Mutex wrapped)
- Opens DB at ~/Library/Application Support/bluey/sessions.db on startup
- Exposes 5 Tauri commands: list_sessions, create_session, get_session, archive_session, delete_session
- Registers Cmd+Shift+D global shortcut to toggle window visibility
- Builds React shell with 9 routes, sidebar navigation, active route highlighting
- Implements /chats page with session CRUD via invoke
- Adds Cmd+K command palette using cmdk
- Wires event channel (useSessionEvents hook listening for session:created)

**Does NOT:**
- Emit events from backend (channel wired, not fired — Phase 2+)
- Implement actual session chat UI (just list/create/archive)
- Add tests for React components (no test framework in UI yet)
- Add Rust integration tests for Tauri commands (requires Tauri test harness)

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| crates/cue-dashboard/Cargo.toml | Modified | Add cue-daemon, dirs, uuid, tauri-plugin-global-shortcut deps |
| crates/cue-dashboard/src/lib.rs | Modified | DbState, global shortcut registration, plugin wiring |
| crates/cue-dashboard/src/commands.rs | Created | 5 Tauri commands bridging UI to Database |
| crates/cue-dashboard/src/macos.rs | Modified | fmt fix (line wrap) |
| crates/cue-dashboard/capabilities/default.json | Modified | Add global-shortcut permissions |
| crates/cue-dashboard/ui/package.json | Modified | Add react-router-dom, lucide-react, cmdk, tailwindcss |
| crates/cue-dashboard/ui/package-lock.json | Created | Lockfile |
| crates/cue-dashboard/ui/vite.config.ts | Modified | Add tailwindcss vite plugin |
| crates/cue-dashboard/ui/src/index.css | Modified | Tailwind 4 import |
| crates/cue-dashboard/ui/src/App.tsx | Modified | BrowserRouter + 9 routes |
| crates/cue-dashboard/ui/src/components/DashboardLayout.tsx | Created | Flex layout: sidebar + outlet + command palette |
| crates/cue-dashboard/ui/src/components/Sidebar.tsx | Created | 9 NavLinks with lucide icons |
| crates/cue-dashboard/ui/src/components/CommandPalette.tsx | Created | Cmd+K modal with cmdk |
| crates/cue-dashboard/ui/src/pages/Placeholder.tsx | Created | Generic placeholder for unimplemented routes |
| crates/cue-dashboard/ui/src/pages/Chats.tsx | Created | Session list with create/archive |
| crates/cue-dashboard/ui/src/hooks/useSessionEvents.ts | Created | Event listener for session:created |

## Build and Test

```bash
# npm build
cd crates/cue-dashboard/ui && npm run build
# PASS: 1697 modules transformed, dist/ produced (289KB JS, 12KB CSS)

# cargo fmt (dashboard)
cargo fmt -p cue-dashboard -- --check
# PASS: no diff

# cargo build --release (full workspace)
cargo build --release
# PASS: Finished release profile in 1m36s (8 pre-existing nspanel warnings)

# cargo test --all-targets
cargo test --all-targets
# PASS: 14 passed; 0 failed

# cargo build -p cue-dashboard --release (with ui/dist)
cargo build -p cue-dashboard --release
# PASS
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| D1.2+D1.3+D1.4 combined in one commit | Tightly coupled: DB state needed for commands, shortcut needed plugin in same Cargo.toml change |
| D1.5+D1.6+D1.7+D1.8 combined in one commit | All UI work forms a cohesive unit; splitting would leave broken intermediate states |
| Event channel is listen-only (no emit) | Per plan: "Not required to fire in this phase; just wire the channel" |

## Known Follow-ups

- Backend event emission (Phase 2): window.emit after create_session
- React test framework (vitest) — not in Phase 1 scope
- Tauri command integration tests — requires tauri-test or similar
- Session detail view (clicking a session to chat UI) — Phase 2+
- Command palette "New session" should actually create session, not just navigate

## Review Checklist (for reviewer)

- [ ] All 5 Tauri commands compile and match the Session type from cue-core
- [ ] Global shortcut uses CmdOrCtrl+Shift+D (works on macOS)
- [ ] React Router has all 9 routes from plan
- [ ] Sidebar shows active route highlighting
- [ ] /chats page calls invoke correctly with proper error handling
- [ ] Command palette opens on Cmd+K and navigates
- [ ] useSessionEvents hook properly cleans up listener
- [ ] No new clippy warnings introduced (only pre-existing nspanel ones)
- [ ] npm run build produces valid dist/
- [ ] cargo build --release passes for full workspace
