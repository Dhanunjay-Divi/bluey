# bluey (cue) Master Port Plan V2 — Option 4 Architecture

**Architecture locked**: Native overlay (keep) + cue-daemon (extend) + Tauri+React dashboard (new)
**Source**: 4 design docs (CUE-DESIGN-01–04) + current-state audit + reference patterns
**Execution**: codex implements, I review line-by-line, user approves before advancing
**Reference pattern**: Pinky's CODEX-ACTION-LIST.md (batched, reviewed, shipped)
**Date**: 2026-05-12

---

## Architecture Diagram

```
┌────────────────────────────────────────────────────────────────────────┐
│  bluey on user's machine (one installed app)                           │
│                                                                        │
│  [1] Native overlay HUD  — KEEP (already built)                        │
│      - Swift on macOS (2463 LOC existing)                              │
│      - C on Windows (996 LOC existing)                                 │
│      - Always-on-top, content-protected                                │
│      - Displays live AI response + listen status                       │
│      - Talks to cue-daemon via JSON-over-stdin/stdout                  │
│                                                                        │
│  [2] cue-daemon  — EXTEND (Rust, 3 crates)                             │
│      - crates/cue-core    (shared types)                               │
│      - crates/cue-cli     (CLI shell)                                  │
│      - crates/cue-daemon  (background process)                         │
│      - Owns: audio capture, STT, LLM, RAG, SQLite,                     │
│              keychain, session state                                   │
│      - Both overlay AND Tauri dashboard talk to it                     │
│                                                                        │
│  [3] Tauri + React dashboard  — NEW                                    │
│      - Separate window, opens on Cmd+Shift+D                           │
│      - Settings / chats / session-switching / prompts                  │
│      - Talks to cue-daemon via Tauri invoke/events                     │
│      - React 19 + Radix + Tailwind + shadcn                            │
│      - Session routes: /session/:id, /session/new                      │
│                                                                        │
│  NO web server, NO browser tab, NO localhost HTTP                      │
│  Tauri dashboard and daemon share same binary/process                  │
│  Native overlay is separate process, IPC via stdio                     │
└────────────────────────────────────────────────────────────────────────┘
```

## Legend

• 🔴 Critical — blocks user-visible functionality or other tasks
• 🟡 High-value — significant UX or reliability improvement
• 🟢 Nice-to-have — polish, DX, or edge-case coverage
• [S] < 1 day  • [M] 1–3 days  • [L] > 3 days
• ✅ DONE  • 🟡 PARTIAL  • ❌ NOT STARTED
• Layers: **NATIVE** | **DAEMON** | **DASHBOARD** | **CROSS** | **INFRA**

## Key Architecture Decisions (Option 4 Specifics)

1. **Single binary**: Tauri wraps cue-daemon. The `cue-daemon` crate becomes the Tauri `src-tauri/` backend. Dashboard React app is bundled inside the Tauri binary.
2. **No HTTP server**: Dashboard talks to daemon via `#[tauri::command]` functions (direct Rust calls). No localhost, no REST API.
3. **Native overlay stays separate**: The Swift/C overlay remains a child process spawned by the daemon, communicating via JSON-over-stdin/stdout. This preserves content protection and platform-native rendering.
4. **Session model**: Sessions live in SQLite (daemon-owned). Both overlay and dashboard reflect current session via events.
5. **CLI preserved**: `cue-cli` still connects via TCP IPC for power users / scripting.

---

## Dependency Graph (Top-Level Critical Path)

```
                    ┌─────────────────┐
                    │  D0.1 Tauri     │
                    │  Scaffold       │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
              ▼              ▼              ▼
     ┌────────────┐  ┌────────────┐  ┌────────────┐
     │ C0.1       │  │ D0.2       │  │ B7.5       │
     │ Session-ID │  │ Tauri↔Daemon│  │ SQLite     │
     │ Model      │  │ IPC        │  │ Schema     │
     └─────┬──────┘  └─────┬──────┘  └─────┬──────┘
           │                │               │
           └────────┬───────┘               │
                    ▼                       │
           ┌────────────────┐              │
           │ PHASE 1        │              │
           │ Dashboard Shell │◀─────────────┘
           └────────┬───────┘
                    │
        ┌───────────┼───────────┐
        ▼           ▼           ▼
  ┌──────────┐ ┌──────────┐ ┌──────────┐
  │ PHASE 2  │ │ PHASE 3  │ │ PHASE 5  │
  │ Session  │ │ Listening │ │ Memory   │
  │ UX       │ │ Upgrade   │ │ (RAG)    │
  └────┬─────┘ └────┬─────┘ └────┬─────┘
       │             │            │
       └──────┬──────┘            │
              ▼                   ▼
        ┌──────────┐       ┌──────────┐
        │ PHASE 4  │       │ PHASE 6  │
        │ Reasoning│       │ Dashboard│
        │ Upgrade  │       │ Polish   │
        └────┬─────┘       └────┬─────┘
             │                  │
             └────────┬─────────┘
                      ▼
              ┌──────────────┐
              │ PHASE 7 Ops  │
              └──────┬───────┘
                     ▼
              ┌──────────────┐
              │ PHASE 8 Dev  │
              └──────────────┘

  PARALLEL TRACKS (can run alongside Phases 1-4):
  ┌──────────────────────────────────────────┐
  │ B10.1-B10.8 Dev Discipline (anytime)     │
  │ B8.1-B8.7 Observability (after Phase 0)  │
  └──────────────────────────────────────────┘
```

---

## PHASE 0 — Foundation (3–5 days)

### Entry Criteria
- bluey repo cloned, `cargo build` passes for existing 3 crates
- Node.js 20+ and pnpm available for React tooling

### Exit Criteria
- `cargo build --release` produces a single Tauri binary that opens an empty window
- Daemon modules callable from Tauri commands (no HTTP)
- SQLite DB created on first launch with session table
- Native overlay still spawns and communicates via stdin/stdout
- `cargo test` passes all existing + new tests

### PR Title
`feat(foundation): add Tauri 2 scaffold with session model and daemon IPC [Phase 0]`

### CHANGELOG Entry
```
## [Unreleased] - Phase 0: Foundation
### Added
- Tauri 2 integration into cue-daemon workspace
- SQLite database with sessions table (via rusqlite + sqlite-vec)
- Tauri ↔ daemon direct IPC (no HTTP)
- Session-ID model with overlay protocol extension
```

### Verification
```bash
cargo build --release
cargo test --workspace
# Manual: launch binary, verify empty Tauri window opens
# Manual: verify native overlay still spawns and shows cards
# Manual: verify SQLite DB created at ~/.local/share/bluey/bluey.db
```

### Estimated Days: 4 (solo engineer)

### Review Checklist
- [ ] Cargo.toml workspace includes tauri crate
- [ ] tauri.conf.json has contentProtected: true, macOSPrivateApi: true
- [ ] No localhost HTTP server anywhere
- [ ] SQLite migrations run on first launch
- [ ] Overlay stdin/stdout protocol unchanged (backward compat)
- [ ] All existing `cargo test` still pass

---

#### D0.1 🔴 [M] Add Tauri 2 to cue workspace
**Layer**: INFRA + DAEMON
**Status**: ❌ NOT STARTED
**Source**: Derived from Option 4 architecture decision
**Summary**: Add Tauri 2 as a dependency to the cue-daemon crate. Set up `src-tauri/` structure within cue-daemon, add `tauri.conf.json`, configure React frontend scaffold with Vite + React 19 + Tailwind + shadcn. The daemon's `main()` becomes the Tauri app entry point.
**Deps**: None (first task)
**Code sketch**:
```toml
# crates/cue-daemon/Cargo.toml additions
[dependencies]
tauri = { version = "2", features = ["macos-private-api"] }
tauri-plugin-shell = "2"

[build-dependencies]
tauri-build = "2"
```
```json
// crates/cue-daemon/tauri.conf.json
{
  "productName": "bluey",
  "identifier": "com.bluey.app",
  "build": { "frontendDist": "../dashboard/dist" },
  "app": {
    "windows": [{
      "label": "dashboard",
      "title": "bluey",
      "width": 900, "height": 650,
      "visible": false,
      "contentProtected": true
    }],
    "macOSPrivateApi": true
  }
}
```
**Acceptance**:
- `cargo build --release` produces a Tauri binary
- Binary launches without errors
- Dashboard window hidden by default (opened via hotkey)
- Existing CLI and overlay functionality unaffected

---

#### D0.2 🔴 [M] Tauri ↔ cue-daemon shared-process IPC
**Layer**: DAEMON + DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: Option 4 — "Tauri dashboard and daemon share same binary/process"
**Summary**: Expose daemon functionality as `#[tauri::command]` functions. The dashboard React app calls these via `invoke()`. Events flow from daemon to dashboard via `app.emit()`. No HTTP, no WebSocket — direct Rust function calls.
**Deps**: D0.1
**Code sketch**:
```rust
// crates/cue-daemon/src/commands.rs
use crate::Daemon;
use tauri::State;
use std::sync::Arc;

#[tauri::command]
pub async fn get_daemon_status(daemon: State<'_, Arc<Daemon>>) -> Result<DaemonStatusDto, String> {
    Ok(daemon.status().await.into())
}

#[tauri::command]
pub async fn get_current_session(daemon: State<'_, Arc<Daemon>>) -> Result<SessionDto, String> {
    daemon.current_session().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ask_question(daemon: State<'_, Arc<Daemon>>, question: String) -> Result<(), String> {
    daemon.ask(&question).await.map_err(|e| e.to_string())
}
```
**Acceptance**:
- Dashboard can call `invoke("get_daemon_status")` and receive JSON
- Daemon emits events (`app.emit("transcript-update", ...)`) received by React
- No HTTP listener on any port for dashboard communication
- TCP IPC (port 57321) preserved for CLI compatibility

---

#### D0.3 🔴 [S] Single-binary architecture confirmation
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: Option 4 decision — "Tauri dashboard and daemon share same binary/process"
**Summary**: Confirm and document that the Tauri binary IS the daemon. On launch: (1) Tauri initializes, (2) daemon modules start (audio, STT, LLM, SQLite), (3) TCP IPC listener starts for CLI, (4) native overlay spawned as child process, (5) dashboard window created but hidden. Document this in ARCHITECTURE.md.
**Deps**: D0.1
**Code sketch**:
```rust
// crates/cue-daemon/src/main.rs
fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let daemon = Arc::new(Daemon::new(app.handle().clone())?);
            // Start TCP IPC for CLI
            daemon.start_tcp_listener();
            // Spawn native overlay
            daemon.spawn_overlay();
            // Store in Tauri state
            app.manage(daemon);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_daemon_status,
            commands::get_current_session,
            commands::ask_question,
            // ... all commands
        ])
        .run(tauri::generate_context!())
        .expect("error running bluey");
}
```
**Acceptance**:
- Single binary in `target/release/bluey` (or `bluey.app` on macOS)
- `ps aux | grep bluey` shows ONE process (plus child overlay)
- ARCHITECTURE.md documents the startup sequence

---

#### C0.1 🔴 [M] Session-ID model in daemon DB
**Layer**: DAEMON + CROSS
**Status**: ❌ NOT STARTED
**Source**: Derived — sessions are core to multi-conversation UX
**Summary**: Add SQLite database with `sessions` table. Each session has an ID, title, created_at, last_active_at, and state (active/archived). The daemon tracks `current_session_id`. Overlay protocol extended with `SessionChanged { id, title }` event. Dashboard shows session list.
**Deps**: D0.1 (needs Tauri for app data dir)
**Code sketch**:
```sql
-- migrations/001_sessions.sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT 'New Session',
    created_at INTEGER NOT NULL,
    last_active_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active', 'archived')),
    metadata TEXT -- JSON blob for extensibility
);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system', 'transcript')),
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    provider TEXT,
    model TEXT,
    cost_usd REAL DEFAULT 0
);
```
```rust
// crates/cue-core/src/overlay.rs — extend protocol
#[derive(Serialize)]
pub enum OverlayCommand {
    // ... existing variants ...
    SessionChanged { id: String, title: String },
}
```
**Acceptance**:
- SQLite DB created on first launch
- `sessions` and `messages` tables exist
- Daemon creates a default session on startup if none exists
- Overlay receives `SessionChanged` on session switch
- `cargo test` includes session CRUD tests

---

#### D0.4 🟡 [S] Native overlay protocol upgrade for sessions
**Layer**: NATIVE + DAEMON
**Status**: ❌ NOT STARTED
**Source**: Derived — overlay must reflect which session is active
**Summary**: Add `SessionChanged` command to the overlay stdin protocol. Overlay displays session title in header. Add `SessionSwitchRequest` event from overlay (user taps session indicator to request switch via dashboard).
**Deps**: C0.1
**Code sketch**:
```swift
// native/macos/cue-overlay/main.swift — handle new command
case "SessionChanged":
    if let id = payload["id"] as? String,
       let title = payload["title"] as? String {
        updateSessionIndicator(id: id, title: title)
    }
```
**Acceptance**:
- Overlay shows current session title in header area
- Switching sessions in dashboard updates overlay header
- No regression in existing overlay functionality

---

#### N0.1 🟢 [S] Audit native overlay for Tauri-overlap
**Layer**: NATIVE
**Status**: ❌ NOT STARTED
**Source**: Derived — ensure no feature duplication between overlay and dashboard
**Summary**: Review all overlay features. Document which features stay in overlay (real-time card display, composer, hotkeys) vs which move to dashboard (settings, session management, prompt library). Ensure overlay correctly reflects state changes initiated from dashboard.
**Deps**: D0.2
**Acceptance**:
- OVERLAY-SCOPE.md document listing overlay-only vs dashboard-only vs shared features
- No dead code paths in overlay after scope clarification

---

## PHASE 1 — Dashboard Shell (4–7 days)

### Entry Criteria
- Phase 0 complete: Tauri binary builds, SQLite works, IPC proven

### Exit Criteria
- Cmd+Shift+D opens/closes dashboard window
- Dashboard shows daemon status (listening state, current session, provider)
- Sidebar navigation with placeholder pages
- Two-way IPC proven: dashboard → daemon commands, daemon → dashboard events
- Theme toggle (dark/light) synced with overlay

### PR Title
`feat(dashboard): add Tauri shell with sidebar nav and daemon status [Phase 1]`

### CHANGELOG Entry
```
## [Unreleased] - Phase 1: Dashboard Shell
### Added
- Dashboard window (Cmd+Shift+D to toggle)
- Sidebar navigation: Sessions, Chats, Prompts, Shortcuts, Settings, Dev
- Real-time daemon status display
- Theme sync between dashboard and overlay
- Two-way Tauri IPC (invoke + events)
```

### Verification
```bash
cargo build --release
cargo test --workspace
cd dashboard && pnpm test && pnpm build
# Manual: Cmd+Shift+D opens dashboard
# Manual: dashboard shows "Listening" / "Idle" status
# Manual: theme toggle updates both dashboard and overlay
```

### Estimated Days: 5 (solo engineer)

### Review Checklist
- [ ] Dashboard window hidden on close (not destroyed)
- [ ] Cmd+Shift+D registered as global shortcut
- [ ] React Router with lazy-loaded route components
- [ ] Tailwind + shadcn configured
- [ ] No console errors in dashboard webview
- [ ] Content protection enabled on dashboard window

---

#### B6.3 🔴 [M] Dashboard window with sidebar navigation
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B6.3 — "Dashboard window — pre-create on startup, sidebar nav"
**Summary**: React app with sidebar navigation. Routes: `/sessions`, `/chats`, `/prompts`, `/shortcuts`, `/settings`, `/dev`. Window pre-created on startup (hidden), shown on Cmd+Shift+D. Hide-on-close behavior (window.on('close-requested', hide)).
**Deps**: D0.1, D0.2
**Code sketch**:
```tsx
// dashboard/src/App.tsx
import { Routes, Route } from 'react-router-dom'
import { Sidebar } from './components/Sidebar'
import { SessionsPage } from './pages/Sessions'
import { ChatsPage } from './pages/Chats'
// ...

export function App() {
  return (
    <div className="flex h-screen">
      <Sidebar />
      <main className="flex-1 overflow-auto">
        <Routes>
          <Route path="/" element={<SessionsPage />} />
          <Route path="/sessions" element={<SessionsPage />} />
          <Route path="/session/:id" element={<SessionDetailPage />} />
          <Route path="/chats" element={<ChatsPage />} />
          <Route path="/prompts" element={<PromptsPage />} />
          <Route path="/shortcuts" element={<ShortcutsPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/dev" element={<DevPage />} />
        </Routes>
      </main>
    </div>
  )
}
```
**Acceptance**:
- Sidebar renders with icons and labels
- Route transitions work without full page reload
- Window hides on close (not destroyed)
- Window remembers position/size across hide/show cycles

---

#### B6.1 🟡 [S] Hotkey system — register Cmd+Shift+D for dashboard
**Layer**: DAEMON
**Status**: ✅ DONE (existing hotkeys in native overlay) → 🟡 PARTIAL for dashboard
**Source**: CUE-DESIGN-04 B6.1 — "Hotkey system — register default bindings"
**What's done**: Native overlay has Cmd+Shift+B (toggle) and Cmd+Shift+H (hide) in Swift.
**What's needed**: Register Cmd+Shift+D in the Tauri layer to toggle dashboard window visibility.
**Deps**: D0.1
**Code sketch**:
```rust
// crates/cue-daemon/src/shortcuts.rs
use tauri_plugin_global_shortcut::GlobalShortcutExt;

pub fn register_shortcuts(app: &tauri::AppHandle) -> tauri::Result<()> {
    app.global_shortcut().on_shortcut("CmdOrCtrl+Shift+D", |app, _| {
        if let Some(window) = app.get_webview_window("dashboard") {
            if window.is_visible().unwrap_or(false) {
                let _ = window.hide();
            } else {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    })?;
    Ok(())
}
```
**Acceptance**:
- Cmd+Shift+D toggles dashboard visibility from any app
- Existing overlay hotkeys (Cmd+Shift+B, Cmd+Shift+H) still work
- No conflict between overlay and dashboard shortcuts

---

#### B6.4 🟡 [S] Theme management — dark/light/system with sync
**Layer**: DASHBOARD + CROSS
**Status**: ✅ DONE in overlay → extend to dashboard
**Source**: CUE-DESIGN-04 B6.4 — "Theme management — sync localStorage + IPC"
**What's done**: Native overlay has dark/light toggle persisted to UserDefaults.
**What's needed**: Dashboard React app reads theme preference, applies Tailwind dark mode class. Theme changes in dashboard emit event to daemon which forwards to overlay.
**Deps**: D0.2
**Acceptance**:
- Dashboard respects system theme by default
- Manual toggle in dashboard updates overlay theme
- Theme persisted to SQLite settings table

---

#### B6.5 🟡 [M] Streaming buffer — useStreamBuffer hook
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B6.5 — "rAF streaming buffer with startTransition"
**Summary**: React hook that buffers incoming Tauri events (transcript updates, AI streaming tokens) and flushes to state at 60fps using requestAnimationFrame. Prevents React re-render storm during fast streaming.
**Deps**: D0.2 (needs Tauri events working)
**Acceptance**:
- AI streaming tokens render smoothly at 60fps
- No dropped frames during fast token emission
- React DevTools shows batched renders (not per-token)

---

#### B6.6 🟢 [S] React.memo MessageRow with custom comparator
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B6.6 — "React.memo MessageRow — custom comparator"
**Summary**: Chat message component wrapped in React.memo with comparator that only re-renders when content or streaming state changes. Prevents re-render of entire message list when new message arrives.
**Deps**: B6.3
**Acceptance**:
- Adding a new message doesn't re-render existing messages
- Streaming message updates only its own row

---

#### B7.8 🔴 [S] CSP configuration for Tauri webview
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B7.8 — "CSP configuration — tauri.conf.json security headers"
**Summary**: Configure Content Security Policy in tauri.conf.json. Allow only local assets and specific API domains (for STT/LLM providers that need direct browser access, if any). Block inline scripts, eval, and external resources.
**Deps**: D0.1
**Code sketch**:
```json
{
  "app": {
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'"
    }
  }
}
```
**Acceptance**:
- No CSP violations in browser console during normal use
- External resource loading blocked
- Inline event handlers blocked

---

#### B7.9 🟢 [S] Error boundaries on route components
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B7.9 — "react-error-boundary on all route components"
**Summary**: Wrap each route in an error boundary that catches render errors and shows a recovery UI instead of crashing the entire dashboard.
**Deps**: B6.3
**Acceptance**:
- A crash in one page doesn't break the entire dashboard
- Error boundary shows "Something went wrong" with retry button
- Error details logged to daemon via invoke

---

## PHASE 2 — Session UX (5–8 days)

### Entry Criteria
- Phase 1 complete: dashboard opens, sidebar works, IPC proven

### Exit Criteria
- User can create new sessions, switch between them, archive old ones
- Session list page shows all sessions with timestamps
- Session detail page shows conversation history
- Overlay reflects current session (title in header)
- Messages persisted to SQLite across app restarts

### PR Title
`feat(sessions): add session management with create/switch/archive flows [Phase 2]`

### CHANGELOG Entry
```
## [Unreleased] - Phase 2: Session UX
### Added
- Session list page with create/archive actions
- Session detail page with conversation history
- Session switching (dashboard + overlay sync)
- Message persistence in SQLite
- Session title auto-generation from first question
```

### Verification
```bash
cargo build --release
cargo test --workspace
cd dashboard && pnpm test
# Manual: create 3 sessions, switch between them
# Manual: verify overlay header updates on switch
# Manual: restart app, verify sessions persist
# Manual: archive a session, verify it disappears from active list
```

### Estimated Days: 6 (solo engineer)

### Review Checklist
- [ ] Sessions table has proper indexes
- [ ] Message ordering is deterministic (created_at + rowid)
- [ ] Session switch emits event to overlay
- [ ] No data loss on crash (WAL mode enabled)
- [ ] Session title auto-generated from first user message

---

#### B6.3.1 🔴 [M] Session list page
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: Derived from B6.3 + C0.1
**Summary**: React page showing all active sessions. Each row: title, last message preview, timestamp, message count. Actions: new session button, click to open, swipe/button to archive. Sorted by last_active_at descending.
**Deps**: C0.1, B6.3
**Acceptance**:
- Sessions load from SQLite via invoke
- New session button creates session and navigates to it
- Archive action moves session to archived state
- Empty state shown when no sessions exist

---

#### B6.3.2 🔴 [M] Session detail page
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: Derived from B6.3
**Summary**: Shows conversation history for a session. Messages rendered as chat bubbles (user right, assistant left, transcript dimmed). Real-time updates via Tauri events when session is active. Scroll-to-bottom on new messages.
**Deps**: B6.3.1, B6.5, B6.6
**Acceptance**:
- Messages load from SQLite on page open
- New messages appear in real-time during active session
- Markdown rendered in assistant messages (code blocks, lists, headers)
- Scroll position preserved when switching away and back

---

#### B6.3.3 🟡 [S] Session switching logic
**Layer**: DAEMON + CROSS
**Status**: ❌ NOT STARTED
**Source**: Derived from C0.1
**Summary**: When user switches session: (1) daemon updates current_session_id, (2) emits SessionChanged to overlay, (3) emits session-changed event to dashboard, (4) loads conversation context for the new session into the LLM context window.
**Deps**: C0.1, D0.4
**Acceptance**:
- Switching sessions updates overlay header
- LLM context reflects the switched-to session's history
- No cross-contamination of messages between sessions

---

#### B5.5 🟡 [M] InterviewTranscriptBuffer with session persistence
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — conversation buffer exists (80-turn cap), no summarization
**Source**: CUE-DESIGN-03 B5.5 — "InterviewTranscriptBuffer (Q&A memory + summarization)"
**What's done**: `MeetingRecord.conversation` with 80-turn cap and `push_conversation_turn()`.
**What's needed**: Persist turns to SQLite messages table. Add summarization trigger when buffer exceeds threshold (summarize oldest 40 turns into a single summary message, keep recent 40 verbatim).
**Deps**: C0.1 (SQLite), B6.3.2 (display)
**Acceptance**:
- Conversation turns saved to messages table in real-time
- Buffer summarization triggers at 80 turns
- Summary message visible in session detail page
- Context window uses summary + recent turns (not all 80)

---

#### B10.8 🟡 [S] Graceful shutdown with session save
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — shutdown exists, no in-flight tracking
**Source**: CUE-DESIGN-04 B10.8 — "Graceful shutdown — track in-flight handlers"
**What's done**: `shutdown_daemon()` kills overlay and removes state file.
**What's needed**: On quit: (1) flush pending messages to SQLite, (2) wait for in-flight STT/LLM requests (max 2s timeout), (3) update session last_active_at, (4) close SQLite connection cleanly.
**Deps**: C0.1
**Acceptance**:
- No message loss on Cmd+Q
- In-flight AI response saved (partial content with "[interrupted]" suffix)
- SQLite WAL checkpoint on shutdown

---

## PHASE 3 — Listening Upgrade (10–15 days)

### Entry Criteria
- Phase 0 complete (SQLite, Tauri IPC)
- Can run in parallel with Phase 2

### Exit Criteria
- VAD prevents sending silence to STT (saves API costs)
- At least 3 STT providers working behind trait
- Speaker ID distinguishes user from interviewer
- Audio supervisor recovers from device disconnects
- Sample rate properly detected and resampled

### PR Title
`feat(audio): add VAD, multi-provider STT trait, speaker ID, and audio supervisor [Phase 3]`

### CHANGELOG Entry
```
## [Unreleased] - Phase 3: Listening Upgrade
### Added
- Two-stage VAD (RMS + WebRTC) — stops billing silence
- SttProvider trait with Deepgram, OpenAI Whisper, and local whisper-rs
- Speaker identification via ECAPA-TDNN ONNX
- Audio supervisor with device recovery and sleep/wake handling
- Dynamic sample rate detection + rubato resampling
### Changed
- Audio pipeline refactored from monolithic to modular architecture
```

### Verification
```bash
cargo build --release
cargo test --workspace
# Manual: speak into mic, verify VAD activates (log shows "speech detected")
# Manual: stop speaking, verify VAD suppresses within 300ms
# Manual: switch STT provider in settings, verify transcription continues
# Manual: unplug mic, verify recovery message + auto-reconnect
# Manual: sleep/wake laptop, verify audio resumes
```

### Estimated Days: 12 (solo engineer)

### Review Checklist
- [ ] VAD has configurable sensitivity (settings page)
- [ ] STT trait is async and provider-agnostic
- [ ] Speaker ID model bundled as ONNX asset (< 20MB)
- [ ] Audio supervisor logs all recovery events
- [ ] No audio glitches during provider hot-swap
- [ ] rubato resampler handles 44.1k, 48k, 96k → 16k

---

#### B2.4 🔴 [M] Two-stage VAD (RMS + WebRTC ML)
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-02 B2.4 — "Two-stage VAD with hangover FSM"
**Summary**: Stage 1: adaptive RMS threshold (EMA noise floor × multiplier). Stage 2: WebRTC VAD ML model at 16kHz. Both must agree before sending to STT. Hangover FSM preserves trailing consonants. States: Active → Hangover → Suppressed.
**Deps**: B2.3 (DSP loop)
**Acceptance**:
- Typing/fan noise does NOT trigger STT
- Speech detected within 20ms of onset
- Trailing consonants preserved (hangover ~300ms)
- Noise floor adapts to environment within 5s

---

#### B2.1 🔴 [L] Platform-abstracted SystemAudioStream trait
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — native helpers exist (Swift SCK 212 LOC, C WASAPI 325 LOC) but no Rust trait
**Source**: CUE-DESIGN-02 B2.1 — "SystemAudioStream trait + macOS SCK + WASAPI + PulseAudio"
**What's done**: Standalone Swift/C binaries pipe PCM to stdout, consumed by daemon.
**What's needed**: Rust trait `SystemAudioStream: Stream<Item = f32>` with platform backends. Keep existing native helpers as the actual capture mechanism (spawned as child processes), but wrap them in the trait interface for uniform consumption.
**Deps**: None
**Acceptance**:
- `SystemAudioStream` trait defined in cue-core
- macOS implementation wraps existing cue-audio Swift helper
- Windows implementation wraps existing cue-audio C helper
- Trait provides `sample_rate()` and `stop()`

---

#### B2.2 🟡 [M] CPAL microphone capture with stream recreation
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — mic capture exists in native helpers, no CPAL
**Source**: CUE-DESIGN-02 B2.2 — "CPAL microphone with stream recreation, atomic sample rate"
**What's done**: Native helpers capture mic audio.
**What's needed**: Pure Rust CPAL-based mic capture in daemon. Stream recreated on every start() to fix silent crash bug. Atomic sample rate tracking for STT configuration.
**Deps**: None
**Acceptance**:
- Mic capture works without native helper binaries
- start() → stop() → start() cycle works without crash
- Actual hardware sample rate reported via atomic

---

#### B2.3 🟡 [S] Zero-copy DSP loop refactor
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — f32→i16 conversion exists but naive
**Source**: CUE-DESIGN-02 B2.3 — "Zero-copy DSP: f32→i16, bytemuck, channel-based emission"
**What's done**: `wav_from_f32le_48k_mono_to_i16_16k()` in daemon.
**What's needed**: Replace with proper DSP loop using bytemuck for zero-copy byte reinterpretation. Emit frames via tokio mpsc channel to STT. Process in 20ms chunks.
**Deps**: B2.1, B2.2
**Acceptance**:
- No memory allocation per audio frame (bytemuck cast)
- 20ms chunk processing at 48kHz = 960 samples/chunk
- Channel-based emission to STT consumer

---

#### B2.5 🔴 [L] SttProvider trait + 3 initial providers
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — only OpenAI Whisper REST exists
**Source**: CUE-DESIGN-02 B2.5 — "SttProvider trait + Deepgram WS + OpenAI Realtime WS + REST"
**What's done**: `transcribe_audio_file()` sends multipart to OpenAI Whisper.
**What's needed**: Async trait `SttProvider` with `write()`, `start()`, `stop()`, `state()`. Implement: (1) Deepgram WebSocket (streaming), (2) OpenAI Whisper REST (batch per utterance), (3) Groq REST. Factory function creates provider by name.
**Deps**: B2.3 (DSP emits frames)
**Acceptance**:
- Trait defined with all methods
- Deepgram WS receives streaming audio, emits partial + final transcripts
- OpenAI REST batches audio on speech_ended, returns transcript
- Provider switchable at runtime without restart

---

#### B2.7 🟡 [S] Local Whisper via whisper-rs
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-02 B2.7 — "Local Whisper impl via whisper-rs (offline fallback)"
**Summary**: Implement SttProvider using whisper-rs (Rust bindings to whisper.cpp). Runs inference locally on CPU/GPU. Used as offline fallback when no API keys configured or network unavailable.
**Deps**: B2.5 (trait)
**Acceptance**:
- whisper-rs model downloaded on first use (~75MB base model)
- Transcription works without network
- Latency < 3s for 10s audio chunk on M1 Mac
- Falls back to this provider when others fail

---

#### B2.8 🟡 [S] STT state machine with error classification
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — minimal error handling exists
**Source**: CUE-DESIGN-02 B2.8 — "STT state machine: error classification, exponential backoff"
**What's done**: `warned_stt_error` flag and error card push.
**What's needed**: 3-state machine (Connected/Reconnecting/Failed). Error classification: auth (fatal), quota (fatal), transient (retry). Exponential backoff 1s→30s cap. State broadcast to dashboard via Tauri event.
**Deps**: B2.5
**Acceptance**:
- Auth errors immediately show "check API key" in dashboard
- Transient errors retry with backoff (visible in dashboard status)
- 5 consecutive failures → Failed state → user notification
- First successful transcript resets to Connected

---

#### B2.13 🟡 [S] Sample rate detection + rubato resampler
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — hardcoded 48k→16k downsample by factor 3
**Source**: CUE-DESIGN-02 B2.13 — "Sample rate detection + rubato resampler"
**What's done**: Naive downsample (skip every 3rd sample).
**What's needed**: Read actual sample rate from capture device. Use rubato crate for proper sinc-interpolation resampling to 16kHz (what STT providers expect).
**Deps**: B2.1, B2.2
**Acceptance**:
- Works with 44.1kHz, 48kHz, 96kHz input devices
- Output is clean 16kHz (no aliasing artifacts)
- rubato configured for low-latency (small chunk size)

---

#### B2.10 🟡 [L] Speaker ID via ECAPA-TDNN ONNX
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-02 B2.10 — "Speaker ID: ECAPA-TDNN ONNX enrollment + runtime inference"
**Summary**: Load ECAPA-TDNN model via ort (ONNX Runtime for Rust). Enrollment: user speaks 5s, extract embedding, save to SQLite. Runtime: for each STT segment, extract embedding, cosine similarity against enrolled user. If similarity > threshold → label as "user", else "interviewer".
**Deps**: B2.5 (STT segments), B2.13 (16kHz audio)
**Acceptance**:
- Enrollment flow: user speaks, embedding saved
- Runtime: segments labeled user/interviewer with >90% accuracy
- Inference < 50ms per segment on M1 Mac
- Model file < 20MB (ONNX quantized)

---

#### B2.11 🟡 [S] Question extractor with noise filter
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — basic `is_question()` exists
**Source**: CUE-DESIGN-02 B2.11 — "Question extractor: noise filter + coding question detection"
**What's done**: Checks if text ends with `?` or starts with question words.
**What's needed**: Add noise filter (reject greetings, filler, < 5 words). Add coding question heuristic (mentions code, algorithm, data structure, system design keywords).
**Deps**: None
**Acceptance**:
- "How are you?" filtered as greeting
- "Um, yeah, so..." filtered as filler
- "Implement a binary search tree" detected as coding question
- "What's the time complexity of quicksort?" detected as coding question

---

#### B2.12 🟡 [M] Dual-channel pipeline with hot-swap
**Layer**: DAEMON
**Status**: ✅ DONE (dual-channel) → extend with hot-swap
**Source**: CUE-DESIGN-02 B2.12 — "Dual-channel: system + mic, channel-keyed STT, hot-swap"
**What's done**: `real_audio_loop` iterates sources, separate system/mic sequences.
**What's needed**: Hot-swap support — change STT provider for one channel without interrupting the other. Channel-keyed session IDs for providers that need them (e.g., Deepgram).
**Deps**: B2.5
**Acceptance**:
- Switching system audio STT provider doesn't interrupt mic STT
- Each channel has independent state machine
- Channel label (system/mic) propagated to transcript events

---

#### B2.14 🟡 [M] Audio supervisor with recovery
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-02 B2.14 — "Audio supervisor: recovery, device watcher, sleep/wake, TCC"
**Summary**: Background task that monitors audio health. Detects: device disconnect (restart capture on new default), sleep/wake (restart after wake), TCC permission denied (show user prompt), capture thread crash (restart with backoff).
**Deps**: B2.1, B2.2
**Acceptance**:
- Unplugging mic → auto-switch to new default within 2s
- Sleep → wake → audio resumes within 1s
- TCC denied → user-facing notification with "Open System Preferences" button
- Capture crash → restart with 1s/2s/4s backoff

---

## PHASE 4 — Reasoning Upgrade (10–15 days)

### Entry Criteria
- Phase 0 complete (SQLite, Tauri IPC)
- B2.5 STT trait done (so LLM can receive transcripts)

### Exit Criteria
- Multi-provider LLM trait with 5+ providers working
- Streaming responses render in overlay at 60fps
- Prompt composition system replaces hardcoded strings
- Fallback chains with proper backoff
- Rate limiting prevents 429 errors

### PR Title
`feat(reasoning): add multi-provider LLM trait, prompt composition, and streaming upgrade [Phase 4]`

### CHANGELOG Entry
```
## [Unreleased] - Phase 4: Reasoning Upgrade
### Added
- LlmProvider trait with OpenAI, Anthropic, Groq, Cerebras, Ollama support
- ModelVersionManager for background model discovery
- Prompt composition system (CORE_IDENTITY + mode + context blocks)
- Rate limiting via governor crate
- Streaming with 60Hz batching and cancellation
### Changed
- Fallback chains now use exponential backoff between attempts
```

### Verification
```bash
cargo build --release
cargo test --workspace
# Manual: ask question, verify streaming response in overlay
# Manual: set provider to Anthropic, verify response works
# Manual: set invalid API key, verify fallback to next provider
# Manual: rapid-fire 10 questions, verify rate limiter kicks in
# Manual: cancel mid-stream (Escape), verify stream stops
```

### Estimated Days: 12 (solo engineer)

### Review Checklist
- [ ] LlmProvider trait is async + Send + Sync
- [ ] All providers handle streaming and non-streaming
- [ ] Prompt composition is testable (unit tests for each mode)
- [ ] Rate limiter configurable per provider
- [ ] Cancellation token properly propagated
- [ ] No hardcoded API keys anywhere

---

#### B3.1 🔴 [L] Multi-provider LLM router with async trait
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — routing exists with fallback chain, but no trait
**Source**: CUE-DESIGN-03 B3.1 — "Multi-provider LLM router with async trait dispatch"
**What's done**: `resolve_answer_route()` + `provider_client_config()` with match + loop.
**What's needed**: Extract into `LlmProvider` async trait. Implement for: OpenAI, Anthropic, Groq, Cerebras, Ollama. Factory creates provider by name. Router iterates fallback chain calling trait methods.
**Deps**: None
**Code sketch**:
```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn generate(&self, req: &LlmRequest) -> anyhow::Result<LlmResponse>;
    async fn stream(&self, req: &LlmRequest) -> anyhow::Result<Pin<Box<dyn Stream<Item = Result<String>>>>>;
    fn name(&self) -> &'static str;
    fn supports_vision(&self) -> bool;
    fn supports_structured(&self) -> bool;
}
```
**Acceptance**:
- 5 providers implement the trait
- Router tries providers in order, skips on error
- Each provider configurable via settings (API key, model, endpoint)
- Unit tests mock the trait for router logic testing

---

#### B3.2 🟡 [M] ModelVersionManager
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B3.2 — "ModelVersionManager — background polling + vision tiers"
**Summary**: Background task that polls provider `/models` endpoints every 5 minutes. Maintains a local cache of available models per provider. Categorizes models into tiers: fast (small), standard, vision-capable, structured-output-capable. Dashboard model picker reads from this cache.
**Deps**: B3.1
**Acceptance**:
- Model list refreshes every 5 min (configurable)
- Cache persisted to SQLite (survives restart)
- Dashboard model picker shows available models grouped by provider
- Vision-capable models tagged for screen analysis routing

---

#### B3.3 🟡 [M] Fallback chains with exponential backoff
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — fallback iteration exists, no backoff
**Source**: CUE-DESIGN-03 B3.3 — "Fallback chains with exponential backoff + Ollama terminal"
**What's done**: `resolve_answer_route()` iterates route steps with fallback.
**What's needed**: Add exponential backoff between attempts (500ms → 1s → 2s → 4s cap). Add Ollama as terminal fallback (always available locally). Log each fallback attempt with reason.
**Deps**: B3.1
**Acceptance**:
- First provider fails → 500ms wait → try second
- Backoff doubles per attempt, caps at 4s
- Ollama tried last if configured
- User sees "Trying backup provider..." in overlay

---

#### B3.4 🔴 [S] Rate limiters via governor crate
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B3.4 — "Rate limiters via governor crate per provider"
**Summary**: Per-provider rate limiter using the `governor` crate. Configurable limits: OpenAI (60 RPM), Anthropic (40 RPM), Groq (30 RPM). Requests that would exceed limit are queued, not rejected. Dashboard shows rate limit status.
**Deps**: B3.1
**Acceptance**:
- Rapid requests queued instead of 429'd
- Per-provider independent limits
- Dashboard shows "Rate limited — queued" status
- Limits configurable in settings

---

#### B3.5 🔴 [M] Streaming with 60Hz batching + CancellationToken
**Layer**: DAEMON + NATIVE
**Status**: 🟡 PARTIAL — SSE streaming exists, no batching or cancellation
**Source**: CUE-DESIGN-03 B3.5 — "Streaming response with 60Hz batching + CancellationToken"
**What's done**: `read_streaming_chat_response()` with word-by-word overlay updates.
**What's needed**: Batch tokens at 60Hz (collect tokens for 16ms, flush as single overlay update). Add CancellationToken that stops the stream on Escape key or new question. Emit stream events to dashboard too.
**Deps**: B3.1, D0.2
**Acceptance**:
- Overlay updates at 60fps (not per-token)
- Escape key cancels mid-stream within 100ms
- New question cancels previous stream
- Dashboard receives same stream events

---

#### B4.1 🔴 [M] Prompt composition system
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — hardcoded system prompt string
**Source**: CUE-DESIGN-03 B4.1 — "Prompt composition system (XML-tagged shared blocks)"
**What's done**: Single hardcoded system prompt in `provider_messages()`.
**What's needed**: Composable prompt system with blocks: CORE_IDENTITY (always), MODE_INSTRUCTIONS (per-mode), CONTEXT (transcript + docs + screen), CONSTRAINTS (anti-chatbot, length rules), LANGUAGE (user's language). Blocks assembled at request time based on current state.
**Deps**: None
**Code sketch**:
```rust
pub struct PromptComposer {
    core_identity: &'static str,
    modes: HashMap<AnswerMode, &'static str>,
    constraints: Vec<&'static str>,
}

impl PromptComposer {
    pub fn compose(&self, mode: AnswerMode, context: &ContextWindow, language: &str) -> String {
        let mut parts = vec![self.core_identity];
        parts.push(self.modes[&mode]);
        parts.push(&context.format_for_prompt());
        parts.extend(&self.constraints);
        if language != "en" {
            parts.push(&format!("<language>{language}</language>"));
        }
        parts.join("\n\n")
    }
}
```
**Acceptance**:
- System prompt is composed from blocks (not hardcoded)
- Each block independently testable
- Mode switch changes prompt without restart
- Context window respects per-kind character limits

---

#### B4.2 🟡 [M] Answer modes (Assist / Answer / WhatToAnswer)
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — 5 modes exist with different naming
**Source**: CUE-DESIGN-03 B4.2 — "3 modes (Assist / Answer / WhatToAnswer)"
**What's done**: General, Code, System Design, Meeting, Writing modes.
**What's needed**: Align with reference architecture: (1) Assist — help user formulate their answer, (2) Answer — provide the answer directly, (3) WhatToAnswer — suggest what to say next. Map existing modes as sub-modes of these three primary modes.
**Deps**: B4.1
**Acceptance**:
- 3 primary modes selectable from overlay mode picker
- Each mode has distinct prompt behavior
- Mode persisted per session
- Dashboard shows mode selector in session view

---

#### B4.6 🟡 [S] Anti-chatbot constraints
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — basic brevity instruction exists
**Source**: CUE-DESIGN-03 B4.6 — "Anti-chatbot constraints + HUMAN ANSWER LENGTH RULE"
**What's done**: "concise", "short sections" in system prompt.
**What's needed**: Formal negative constraints: no preambles ("Great question!"), no sign-offs, no bullet-point-everything, no over-explanation. HUMAN ANSWER LENGTH RULE: responses should be speakable in < 30 seconds unless code.
**Deps**: B4.1
**Acceptance**:
- AI never starts with "Great question!" or similar
- Responses are concise enough to speak aloud
- Code answers are an exception (can be longer)
- Unit tests verify constraint text is in composed prompt

---

#### B4.8 🟡 [S] Context prioritization matrix
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — per-kind character limits exist
**Source**: CUE-DESIGN-03 B4.8 — "Context prioritization matrix"
**What's done**: `provider_context_item_limit()` with transcript 10k, doc 6k, screenshot 3k, total 32k.
**What's needed**: Formalize as a priority matrix: (1) user's explicit question, (2) recent transcript (last 2 min), (3) screen context, (4) attached documents, (5) older transcript, (6) RAG results. When total exceeds limit, trim from lowest priority first.
**Deps**: B4.1
**Acceptance**:
- Priority order documented and enforced
- Trimming removes lowest-priority content first
- User's question never trimmed
- Total context stays within model's window

---

#### B3.9 🟡 [S] Key scrubbing via zeroize
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B3.9 — "scrubKeys via zeroize crate"
**Summary**: All API key strings wrapped in `Zeroizing<String>` from the `zeroize` crate. Keys zeroed from memory on drop. Prevents keys lingering in memory after use.
**Deps**: None
**Acceptance**:
- API keys use `Zeroizing<String>` type
- Keys zeroed on provider reconfiguration
- Keys zeroed on app quit
- No raw key strings in heap after drop

---

#### B3.10 🟢 [S] testConnection command
**Layer**: DAEMON + DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B3.10 — "testConnection with stable pingable model"
**Summary**: Dashboard settings page has "Test Connection" button per provider. Sends a minimal request to a stable cheap model (e.g., gpt-4o-mini for OpenAI) to validate the API key works. Shows ✓ or ✗ with error message.
**Deps**: B3.1
**Acceptance**:
- Test uses cheapest model (not user's selected model)
- Returns within 5s timeout
- Shows specific error (invalid key, quota exceeded, network)
- Works for all 5 providers

---

#### B3.12 🟡 [M] Vision fallback with parallel race
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — vision fallback exists (page text → screenshot → vision provider)
**Source**: CUE-DESIGN-03 B3.12 — "Parallel Gemini race + 3-tier vision fallback"
**What's done**: `analyze_screen_with_screenshot_fallback()` with sequential fallback.
**What's needed**: 3-tier: (1) page text extraction (fast, free), (2) screenshot + vision model, (3) OCR fallback. For tier 2, race multiple vision providers in parallel, use first response.
**Deps**: B3.1
**Acceptance**:
- Page text tried first (< 100ms)
- If no page text, screenshot + vision (parallel race)
- First vision response wins, others cancelled
- OCR fallback if all vision providers fail

---

## PHASE 5 — Memory / RAG (7–12 days)

### Entry Criteria
- C0.1 done (SQLite available)
- B2.5 done (STT produces transcripts to index)
- B3.1 done (LLM trait available for embedding providers)

### Exit Criteria
- sqlite-vec extension loaded, vector search working
- Transcripts chunked and embedded in real-time during sessions
- RAG results injected into LLM context
- Hybrid retrieval (vector + keyword) working
- Epoch summarization compresses old context

### PR Title
`feat(memory): add sqlite-vec RAG with semantic chunking and live indexing [Phase 5]`

### CHANGELOG Entry
```
## [Unreleased] - Phase 5: Memory
### Added
- sqlite-vec vector store for local RAG
- SemanticChunker with sliding-window overlap
- Multi-provider embedding (OpenAI, local)
- Live RAG indexer (indexes transcripts as they arrive)
- Epoch summarization for long sessions
- Hybrid retrieval (vector similarity + BM25 keyword)
```

### Verification
```bash
cargo build --release
cargo test --workspace
# Manual: start session, speak for 2 min, then ask "what did I say about X?"
# Manual: verify RAG retrieves relevant chunks
# Manual: verify keyword search still works for exact phrases
# Manual: run 30-min session, verify epoch summarization triggers
```

### Estimated Days: 10 (solo engineer)

### Review Checklist
- [ ] sqlite-vec loaded as extension (not compiled in)
- [ ] Embedding dimension matches provider (1536 for OpenAI, 384 for local)
- [ ] Chunks have session_id foreign key
- [ ] Live indexer doesn't block audio pipeline
- [ ] Summarization preserves key facts (not just truncation)
- [ ] Vector search uses spawn_blocking (sqlite-vec is sync)

---

#### B5.1 🔴 [M] sqlite-vec vector store
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B5.1 — "sqlite-vec vector store (Rust native)"
**Summary**: Load sqlite-vec extension into existing SQLite connection. Create `rag_chunks` virtual table with vector column. Provide insert/search functions. Search returns top-K chunks by cosine similarity.
**Deps**: C0.1 (SQLite)
**Code sketch**:
```sql
-- migrations/002_rag.sql
CREATE VIRTUAL TABLE rag_chunks USING vec0(
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding float[1536],
    created_at INTEGER NOT NULL
);

CREATE TABLE rag_metadata (
    chunk_id TEXT PRIMARY KEY REFERENCES rag_chunks(id),
    source TEXT NOT NULL, -- 'transcript', 'document', 'screen'
    start_ms INTEGER,
    end_ms INTEGER,
    speaker TEXT
);
```
**Acceptance**:
- sqlite-vec extension loads without error
- Insert 1000 chunks in < 1s
- Top-10 search in < 50ms
- Works on macOS + Windows + Linux

---

#### B5.2 🔴 [M] SemanticChunker with sliding-window overlap
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B5.2 — "SemanticChunker with sliding-window overlap"
**Summary**: Split transcript text into chunks of ~200 tokens with 50-token overlap. Chunk boundaries prefer sentence endings. Each chunk tagged with session_id, timestamp range, speaker.
**Deps**: None
**Acceptance**:
- Chunks are 150–250 tokens (flexible boundaries)
- 50-token overlap prevents context loss at boundaries
- Sentence boundaries preferred over mid-sentence splits
- Each chunk has metadata (session, time range, speaker)

---

#### B5.3 🔴 [M] Multi-provider embedding trait
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B5.3 — "Multi-provider embedding trait + resolver"
**Summary**: Async trait `EmbeddingProvider` with `embed(texts: &[String]) -> Vec<Vec<f32>>`. Implementations: OpenAI text-embedding-3-small (1536d), local ONNX model (384d). Resolver picks provider based on config + availability.
**Deps**: B3.1 (provider pattern)
**Acceptance**:
- Trait supports batch embedding (multiple texts per call)
- OpenAI provider batches up to 100 texts per request
- Local provider works offline
- Dimension mismatch detected at startup (not at query time)

---

#### B5.4 🔴 [M] Live RAG indexer
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B5.4 — "Live RAG indexer (JIT during meeting)"
**Summary**: As final transcripts arrive from STT, feed them to the chunker → embedder → sqlite-vec pipeline. Runs asynchronously (doesn't block audio). Chunks searchable immediately after insertion. Batches embeddings (every 5 chunks or 10s, whichever first).
**Deps**: B5.1, B5.2, B5.3, B2.5
**Acceptance**:
- Transcript from 30s ago is searchable
- Embedding batched (not one API call per chunk)
- Indexer doesn't block STT pipeline
- Backpressure: if embedding is slow, chunks queue (don't drop)

---

#### B5.6 🟡 [M] Epoch summarization
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B5.6 — "Epoch summarization (compress old context)"
**Summary**: When a session exceeds 100 chunks, summarize the oldest 50 into a single summary chunk. Summary generated by LLM (cheapest model). Original chunks marked as summarized (not deleted). Summary chunk used in context window instead of originals.
**Deps**: B5.1, B3.1
**Acceptance**:
- Summarization triggers at 100 chunks
- Summary preserves key facts, decisions, action items
- Original chunks still searchable via RAG
- Context window uses summary + recent chunks

---

#### B5.7 🟡 [S] Async vector search via spawn_blocking
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B5.7 — "Async vector search via spawn_blocking"
**Summary**: sqlite-vec queries are synchronous. Wrap in `tokio::task::spawn_blocking` to avoid blocking the async runtime. Return results via oneshot channel.
**Deps**: B5.1
**Acceptance**:
- Vector search doesn't block tokio runtime
- Search completes within 100ms for 10K chunks
- Concurrent searches don't deadlock

---

#### B5.8 🟡 [M] Hybrid retrieval (vector + BM25)
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — keyword substring search exists
**Source**: CUE-DESIGN-03 B5.8 — "Hybrid retrieval (vector + BM25 keyword)"
**What's done**: `search_memory()` with substring matching across meetings.
**What's needed**: Combine vector similarity search with SQLite FTS5 keyword search. Score fusion: `final_score = α * vector_score + (1-α) * bm25_score` with α=0.7 default. Return top-K by fused score.
**Deps**: B5.1, B5.7
**Acceptance**:
- Exact phrase queries find results even if embedding is poor
- Semantic queries find paraphrased content
- Score fusion configurable (α parameter)
- Results deduplicated (same chunk from both paths)

---

## PHASE 6 — Dashboard Polish (7–12 days)

### Entry Criteria
- Phase 1 complete (dashboard shell with routing)
- Phase 2 complete (sessions working)
- Phase 4 partial (LLM providers configured)

### Exit Criteria
- Chats history page with search
- System prompts library (create, edit, assign to sessions)
- Shortcuts rebinding page
- Full settings page (providers, audio, appearance, privacy)
- Dev page (custom providers, debug info)

### PR Title
`feat(dashboard): add chats, prompts, shortcuts, settings, and dev pages [Phase 6]`

### CHANGELOG Entry
```
## [Unreleased] - Phase 6: Dashboard Polish
### Added
- Chats history page with full-text search
- System prompts library with create/edit/delete
- Shortcuts rebinding page with conflict detection
- Settings page: providers, audio, appearance, privacy sections
- Dev page: custom provider config, debug logs, IPC inspector
- Command palette (Cmd+K) for quick actions
```

### Verification
```bash
cargo build --release
cd dashboard && pnpm test && pnpm build
# Manual: search chats for a keyword, verify results
# Manual: create custom prompt, assign to session, verify AI uses it
# Manual: rebind Cmd+Shift+B to Cmd+Shift+X, verify new binding works
# Manual: add custom provider endpoint in dev page, verify it connects
# Manual: Cmd+K opens command palette, can switch sessions
```

### Estimated Days: 10 (solo engineer)

### Review Checklist
- [ ] All pages have loading states and error states
- [ ] Settings changes take effect immediately (no restart)
- [ ] Shortcut conflicts detected and shown to user
- [ ] Custom prompts stored in SQLite
- [ ] Dev page gated behind a toggle (not visible by default)

---

#### B6.2 🟡 [M] Rebindable keybinds with settings UI
**Layer**: DASHBOARD + DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B6.2 — "Rebindable keybinds — settings UI with ShortcutRecorder"
**Summary**: Dashboard shortcuts page shows all registered hotkeys. User can click a binding and press new key combo to rebind. Validates for conflicts. Persists to SQLite. Daemon re-registers shortcuts on change.
**Deps**: B6.1, C0.1
**Acceptance**:
- All hotkeys listed with current binding
- Click-to-record new binding
- Conflict detection (shows which action conflicts)
- Changes take effect immediately (no restart)
- Reset-to-defaults button

---

#### B6.7 🟢 [M] Inertial scroll engine
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B6.7 — "Inertial scroll engine — physics-based scroll"
**Summary**: Custom scroll behavior for chat message list. Physics-based momentum with deceleration. Integrates with global shortcuts (Cmd+Up/Down for page scroll). Smooth scroll-to-bottom on new message.
**Deps**: B6.3.2
**Acceptance**:
- Scroll feels native (momentum + deceleration)
- Cmd+Down scrolls to bottom
- New message auto-scrolls only if already at bottom
- Manual scroll position preserved when new messages arrive above viewport

---

#### B6.9 🟡 [L] Command palette (Cmd+K)
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B6.9 — "Command palette (cmdk) — Cmd+K spotlight"
**Summary**: Spotlight-style command palette using cmdk library. Actions: switch session, change mode, change provider, toggle overlay, open settings section, search chats. Fuzzy matching on action names.
**Deps**: B6.3, B6.3.1
**Acceptance**:
- Cmd+K opens palette from any page
- Fuzzy search across all actions
- Session switching from palette
- Provider switching from palette
- Escape closes palette

---

#### B6.10 🟢 [S] Onboarding flow
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B6.10 — "Onboarding — FeatureSpotlight component"
**Summary**: First-launch onboarding: (1) welcome screen, (2) API key setup, (3) audio permission grant, (4) hotkey tutorial, (5) first session creation. Tracks completion in localStorage. Can be re-triggered from settings.
**Deps**: B6.3
**Acceptance**:
- Shows on first launch only
- Each step skippable
- API key validated before proceeding
- Audio permission requested with explanation
- Completion tracked (doesn't show again)

---

#### B4.5 🟡 [L] Skill library (prompt templates)
**Layer**: DASHBOARD + DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B4.5 — "Skill library (9 prompts with language injection)"
**Summary**: Dashboard page for managing system prompt templates ("skills"). Pre-built skills: General Interview, Coding Interview, System Design, Behavioral, Case Study, Sales Call, Meeting Notes, Writing Assistant, Custom. Each skill has a system prompt template with variable slots. User can create custom skills.
**Deps**: B4.1, C0.1
**Acceptance**:
- 9 pre-built skills available on first launch
- Custom skill creation with template editor
- Skills assignable to sessions
- Language injection works (skill prompt in user's language)
- Skills stored in SQLite

---

#### B4.3 🟢 [S] Per-provider prompt variants
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B4.3 — "Per-provider prompt variants (Claude XML, Groq terse)"
**Summary**: Prompt composition adapts to provider: Claude gets XML-tagged blocks, Groq gets terse instructions (smaller context window), OpenAI gets standard markdown. Variants defined per-provider in prompt composer.
**Deps**: B4.1, B3.1
**Acceptance**:
- Claude receives XML-structured prompts
- Groq receives shorter prompts (< 4K tokens)
- OpenAI receives standard format
- Unit tests verify variant generation per provider

---

#### B4.4 🟢 [S] TINY prompt set for fast mode
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B4.4 — "TINY prompt set for fast mode"
**Summary**: Minimal prompt (< 500 tokens) for latency-sensitive queries. Used when user selects "Fast" mode or when using rate-limited providers. Strips context blocks, keeps only core identity + question.
**Deps**: B4.1
**Acceptance**:
- Fast mode prompt < 500 tokens
- Latency measurably lower (< 200ms TTFT on fast providers)
- Quality acceptable for simple questions

---

#### B4.7 🟢 [S] System-prompt protection
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B4.7 — "System-prompt protection / jailbreak defense"
**Summary**: Add defensive instructions to system prompt that resist extraction attempts. If user's question contains "ignore previous instructions" or similar patterns, respond with a deflection. Log attempted jailbreaks.
**Deps**: B4.1
**Acceptance**:
- "Ignore previous instructions" doesn't leak system prompt
- "What are your instructions?" gets a deflection
- Jailbreak attempts logged for review
- Normal questions unaffected

---

#### B4.9 🟢 [S] First-person enforcement
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B4.9 — "First-person 'speak AS the user' enforcement"
**Summary**: In Answer mode, AI responds as if it IS the user speaking. Prompt enforces first-person perspective: "I think...", "In my experience...", not "You should say...". Critical for interview copilot use case.
**Deps**: B4.1, B4.2
**Acceptance**:
- Answer mode responses use first person
- No "You should say..." or "Here's what to answer..."
- Responses sound natural when spoken aloud
- Assist mode still uses second person (helping user)

---

#### B3.6 🟢 [M] Structured JSON generation
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B3.6 — "Structured JSON generation (6-provider chain)"
**Summary**: Some features need structured output (action items, decisions, recap). Use provider's native JSON mode where available (OpenAI json_object, Claude tool_use). Fallback: prompt-based JSON extraction with validation + retry.
**Deps**: B3.1
**Acceptance**:
- Structured output works with OpenAI (native JSON mode)
- Structured output works with Claude (tool_use)
- Fallback: prompt-based extraction for providers without native support
- Invalid JSON triggers one retry with error feedback

---

#### B3.7 🟢 [S] Custom cURL provider
**Layer**: DAEMON + DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B3.7 — "Custom cURL provider (parse + variable substitution)"
**Summary**: Dev page allows configuring a custom LLM endpoint via cURL-like template. Variables: `{{prompt}}`, `{{model}}`, `{{api_key}}`, `{{temperature}}`. Parses response JSON path for content extraction. Enables any OpenAI-compatible API.
**Deps**: B3.1
**Acceptance**:
- Custom endpoint configurable in dev page
- Variable substitution works
- Response JSON path configurable
- Streaming supported if endpoint supports SSE

---

#### B3.8 🟢 [S] Codex CLI subprocess integration
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B3.8 — "Codex CLI subprocess integration"
**Summary**: Spawn codex CLI as subprocess for code-generation tasks. Pipe question + context to stdin, read response from stdout. Used when user selects "Code" mode and has codex installed.
**Deps**: B3.1
**Acceptance**:
- Codex detected on PATH
- Question + context piped to codex
- Response streamed back to overlay
- Graceful fallback if codex not installed

---

## PHASE 7 — Ops & Security (5–10 days)

### Entry Criteria
- Phase 0 complete (Tauri binary, SQLite)
- Phase 4 partial (providers configured — needed for keychain)

### Exit Criteria
- API keys stored in OS keychain (not env vars)
- Structured JSON logs with rotation
- OpenTelemetry metrics exported
- Auto-updater working
- Single-instance enforcement

### PR Title
`feat(ops): add keychain, log rotation, telemetry, auto-updater [Phase 7]`

### CHANGELOG Entry
```
## [Unreleased] - Phase 7: Ops & Security
### Added
- OS keychain integration for API key storage
- Structured NDJSON logs with 10MB rotation
- OpenTelemetry metrics (TTFT, STT latency, provider usage)
- Auto-updater with release notes display
- Single-instance lock (focus existing on relaunch)
- Panic handler with crash log
### Security
- API keys migrated from env vars to OS keychain
- Keys scrubbed from memory on drop (zeroize)
- Log masking for sensitive values
```

### Verification
```bash
cargo build --release
cargo test --workspace
# Manual: set API key in settings, verify it's in Keychain Access (macOS)
# Manual: check ~/.local/share/bluey/logs/ for rotated NDJSON files
# Manual: launch second instance, verify first instance focused
# Manual: trigger update check, verify release notes shown
```

### Estimated Days: 7 (solo engineer)

### Review Checklist
- [ ] Keychain uses per-provider namespacing
- [ ] Old env var keys migrated on first launch
- [ ] Log rotation doesn't lose recent entries
- [ ] Metrics don't contain PII
- [ ] Auto-updater verifies signature
- [ ] Single-instance works across user sessions

---

#### B7.1 🔴 [S] Keychain integration
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B7.1 — "tauri-plugin-keychain for all API keys"
**Summary**: Store all API keys in OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service). Per-provider namespacing: `bluey/openai`, `bluey/anthropic`, etc. Migration: on first launch with keychain, read existing env vars and store in keychain, then stop reading env vars.
**Deps**: D0.1
**Acceptance**:
- Keys stored in OS keychain (visible in Keychain Access on macOS)
- Keys retrieved without user prompt (app-level access)
- Per-provider namespacing prevents collisions
- Env var migration on first launch
- Dashboard settings page reads/writes keychain

---

#### B7.2 🟡 [S] Key scrubbing on drop
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B7.2 — "zeroize crate on Drop, scrub on quit"
**Summary**: All API key strings use `Zeroizing<String>` wrapper. On drop, memory is zeroed. On app quit, explicit scrub of all provider configs.
**Deps**: B3.9 (zeroize already added in Phase 4)
**Acceptance**:
- `Zeroizing<String>` used for all key fields
- Memory zeroed on provider reconfiguration
- Explicit scrub on graceful shutdown

---

#### B7.3 🟡 [S] Log masking utility
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B7.3 — "mask_key() utility, grep audit"
**Summary**: `mask_key(key: &str) -> String` returns first 8 chars + "..." for logging. Audit all tracing calls to ensure no raw keys logged. Add clippy lint or grep check to CI.
**Deps**: None
**Acceptance**:
- `mask_key("sk-abc123xyz789")` → `"sk-abc12..."`
- grep for raw key patterns in logs returns zero results
- CI check prevents new raw key logging

---

#### B7.4 🔴 [S] Log rotation with NDJSON
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B7.4 — "10MB cap, tracing-appender" + B8.7 — "NDJSON structured logs"
**Summary**: Replace stderr-only logging with tracing-appender writing NDJSON to `~/.local/share/bluey/logs/`. Rotate at 10MB, keep 1 backup. Each line is valid JSON with timestamp, level, target, message, and structured fields.
**Deps**: None
**Acceptance**:
- Logs written to file (not just stderr)
- Each line is valid JSON (parseable by jq)
- Rotation at 10MB (old file renamed to .1)
- Structured fields include session_id, provider, latency_ms

---

#### B7.5 🔴 [M] SQLite schema migrations
**Layer**: DAEMON
**Status**: ❌ NOT STARTED (C0.1 creates initial tables, this adds full migration system)
**Source**: CUE-DESIGN-04 B7.5 — "SQLite schema — 4 migration files"
**Summary**: Proper migration system using embedded SQL files. Migrations tracked in `_migrations` table. Run on startup. Files: 001_sessions.sql, 002_rag.sql, 003_settings.sql, 004_shortcuts.sql.
**Deps**: C0.1
**Acceptance**:
- Migrations run automatically on startup
- Already-applied migrations skipped
- Failed migration rolls back and shows error
- Schema version queryable

---

#### B7.6 🟢 [S] Hot-reload config via notify crate
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B7.6 — "notify crate watcher with 150ms debounce"
**Summary**: Watch settings file for external changes (e.g., user edits JSON directly). On change, reload config and emit event to dashboard. 150ms debounce prevents rapid-fire reloads during save.
**Deps**: None
**Acceptance**:
- External settings file edit triggers reload
- Dashboard reflects changes within 200ms
- Debounce prevents multiple reloads per save
- Invalid JSON shows error (doesn't crash)

---

#### B7.7 🟡 [S] Single-instance lock
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — TCP ping check exists
**Source**: CUE-DESIGN-04 B7.7 — "tauri-plugin-single-instance"
**What's done**: Daemon checks if already running via TCP ping.
**What's needed**: Use tauri-plugin-single-instance for proper OS-level lock. On relaunch attempt, focus existing window instead of showing error.
**Deps**: D0.1
**Acceptance**:
- Second launch focuses existing window
- No error dialog on relaunch
- Works after crash (stale lock cleaned up)

---

#### B7.10 🟡 [S] Panic handler with crash log
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B7.10 — "Panic handler — custom hook logging to file"
**Summary**: Set custom panic hook that writes crash info to `~/.local/share/bluey/crash.log` before aborting. Include: panic message, backtrace, session state, last 10 log lines.
**Deps**: B7.4 (log file path)
**Acceptance**:
- Panic writes crash.log before exit
- Crash log includes backtrace
- Next launch detects crash.log and offers to send report
- Crash log < 1MB (truncate backtrace if needed)

---

#### B8.1 🟡 [M] OpenTelemetry initialization
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B8.1 — "opentelemetry + opentelemetry-otlp, OTLP HTTP exporter"
**Summary**: Initialize OpenTelemetry with OTLP HTTP exporter. Export metrics to configurable endpoint (default: disabled). Metrics: ai_ttft_ms, ai_total_ms, stt_decode_ms, provider_request_count, provider_error_count.
**Deps**: None
**Acceptance**:
- OTel initialized on startup (disabled by default)
- Metrics exported when endpoint configured
- No performance impact when disabled
- Histogram buckets appropriate for each metric

---

#### B8.2 🟡 [M] Metric definitions
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B8.2 — "ai_ttft_ms, stt_decode_ms, provider counters"
**Summary**: Define and instrument key metrics: time-to-first-token (TTFT), total response time, STT decode latency, provider request/error counters, audio pipeline health (buffer fullness, drops).
**Deps**: B8.1
**Acceptance**:
- TTFT measured from question submission to first token
- Provider counters increment on each request
- Error counters broken down by error type
- Metrics visible in dashboard dev page

---

#### B8.4 🟡 [S] AI pricing table
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — CostEstimate struct exists but always returns 0
**Source**: CUE-DESIGN-04 B8.4 — "per-model USD/1M tokens, compute_cost()"
**What's done**: `CostEstimate::usd(0.0)` placeholder.
**What's needed**: Pricing table with per-model input/output token costs. `compute_cost(model, input_tokens, output_tokens) -> f64`. Display running session cost in dashboard.
**Deps**: B3.1
**Acceptance**:
- Pricing for all supported models (updated quarterly)
- Cost computed per request and accumulated per session
- Dashboard shows session cost
- Cost stored in messages table

---

#### B9.1 🟡 [M] Auto-updater
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B9.1 — "tauri-plugin-updater, signing keys, endpoint config"
**Summary**: Use tauri-plugin-updater for automatic update checks. Check on startup + every 6 hours. Show release notes before update. Require user confirmation for major versions. Sign updates with Ed25519 key.
**Deps**: D0.1, B9.7 (build targets)
**Acceptance**:
- Update check on startup (non-blocking)
- Release notes shown in dashboard notification
- Update downloads in background
- Restart prompt after download
- Signature verification before install

---

#### B9.3 🟢 [S] Autostart on login
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B9.3 — "tauri-plugin-autostart with LaunchAgent"
**Summary**: Option in settings to start bluey on login. macOS: LaunchAgent plist. Windows: Registry Run key. Linux: XDG autostart desktop file.
**Deps**: D0.1
**Acceptance**:
- Toggle in settings enables/disables autostart
- macOS: LaunchAgent created/removed
- Windows: Registry key created/removed
- Starts minimized (overlay only, no dashboard)

---

#### B9.4 🟢 [S] PostHog analytics (opt-in)
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B9.4 — "disabled session recording, explicit events only"
**Summary**: Optional anonymous analytics via PostHog. Disabled by default. Events: app_launched, session_created, provider_used, error_occurred. No session recording, no PII, no transcript content.
**Deps**: None
**Acceptance**:
- Disabled by default (opt-in in settings)
- No PII in events
- No transcript/question content sent
- Events: launch, session, provider, error only
- Respects system-level tracking preferences

---

#### B9.7 🟡 [M] Build targets (.dmg, .msi, .AppImage)
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B9.7 — "Build targets — .dmg, .msi, .AppImage + .deb"
**Summary**: Configure Tauri bundler for all platforms. macOS: .dmg with drag-to-Applications. Windows: .msi installer. Linux: .AppImage (universal) + .deb (Debian/Ubuntu). CI builds all targets on tag push.
**Deps**: D0.1
**Acceptance**:
- `cargo tauri build` produces .dmg on macOS
- .dmg opens with drag-to-Applications layout
- .msi installs on Windows with Start Menu shortcut
- .AppImage runs on any Linux without install
- All bundles < 30MB

---

## PHASE 8 — Dev Discipline & Stealth Polish (4–7 days)

### Entry Criteria
- Phase 0 complete (repo structure finalized)
- Can run in parallel with any phase

### Exit Criteria
- All dev documentation files in place
- Stealth features that don't require Tauri webview completed in native overlay
- Process masquerading working
- Screen-share detection working (Windows)

### PR Title
`feat(dev): add dev docs, stealth polish, and codex agent configs [Phase 8]`

### CHANGELOG Entry
```
## [Unreleased] - Phase 8: Dev Discipline & Stealth
### Added
- CLAUDE.md, CHANGELOG.md, PR template, FIXES.md, AUDIT.md
- .codex/agents with 7 specialized configs
- Process masquerading (3 disguise presets)
- Screen-share detection (Windows)
- Click-through toggle for overlay
- Dock/taskbar hiding
```

### Verification
```bash
cargo build --release
cargo test --workspace
# Manual: verify all doc files exist and are well-formatted
# Manual: activate disguise mode, check Activity Monitor shows fake name
# Manual: start screen share, verify detection event fires
# Manual: toggle click-through, verify mouse passes through overlay
```

### Estimated Days: 5 (solo engineer)

### Review Checklist
- [ ] CLAUDE.md has correct project structure
- [ ] PR template matches team conventions
- [ ] Disguise presets have icon assets
- [ ] Screen-share detection doesn't false-positive
- [ ] Click-through has escape hatch (hotkey always works)

---

#### B10.1 🟡 [S] CLAUDE.md / development rules
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B10.1
**Summary**: Root-level CLAUDE.md with project structure, build commands, architecture overview, coding conventions, and PR workflow. Serves as context for AI coding assistants.
**Deps**: None
**Acceptance**: File exists, accurate, < 200 lines

---

#### B10.2 🟡 [S] CHANGELOG.md
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B10.2
**Summary**: Keep-a-Changelog format. Initial entries for all completed work. Updated with each phase PR.
**Deps**: None
**Acceptance**: Valid Keep-a-Changelog format, covers all shipped features

---

#### B10.3 🟡 [S] PR template
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B10.3
**Summary**: `.github/PULL_REQUEST_TEMPLATE.md` with sections: Summary, Changes, Testing, Screenshots, Checklist.
**Deps**: None
**Acceptance**: Template renders correctly on GitHub

---

#### B10.4 🟡 [S] FIXES.md
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B10.4
**Summary**: Template for documenting bug fixes with 6 sections: Problem, Root Cause, Fix, Testing, Regression Risk, Related Issues.
**Deps**: None
**Acceptance**: Template is clear and actionable

---

#### B10.5 🟡 [S] AUDIT.md
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B10.5
**Summary**: Self-security-audit checklist covering: key storage, network calls, content protection, data at rest, logging hygiene, dependency audit.
**Deps**: None
**Acceptance**: Checklist covers all security-relevant areas

---

#### B10.6 🟡 [M] .codex/agents configs
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B10.6 — "7 specialized agent configs"
**Summary**: Agent configs for: (1) audio-pipeline, (2) llm-routing, (3) dashboard-react, (4) native-overlay, (5) rag-memory, (6) ops-infra, (7) security-audit. Each has scope, tools, and constraints.
**Deps**: None
**Acceptance**: 7 agent config files, each scoped to relevant crate/directory

---

#### B10.7 🟢 [M] .codex/skills cards
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B10.7 — "10 reusable skill cards"
**Summary**: Reusable skill cards for common tasks: add-provider, add-stt-provider, add-dashboard-page, add-tauri-command, add-migration, add-test, fix-audio-bug, fix-streaming-bug, add-shortcut, security-review.
**Deps**: None
**Acceptance**: 10 skill cards, each with clear steps and examples

---

#### B1.5 🟡 [M] Process masquerading
**Layer**: NATIVE + DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-01 B1.5 — "3 disguise presets + icon assets + re-assertion timer"
**Summary**: 3 presets (Terminal, System Settings, Activity Monitor). On activation: set window titles, swap dock/taskbar icon, set process name (Linux). Re-assertion timer at 5s intervals. Icon assets bundled in app resources.
**Deps**: D0.1
**Acceptance**:
- Activity Monitor shows fake process name
- Dock shows fake icon
- Window title shows fake name
- Re-assertion prevents drift
- "None" preset restores real identity

---

#### B1.6 🟡 [S] Dock/taskbar visibility toggle
**Layer**: DAEMON + NATIVE
**Status**: 🟡 PARTIAL — Windows WS_EX_TOOLWINDOW done, macOS missing
**Source**: CUE-DESIGN-01 B1.6 — "ActivationPolicy::Accessory + set_skip_taskbar"
**What's done**: Windows overlay uses WS_EX_TOOLWINDOW (hidden from taskbar).
**What's needed**: macOS: set Tauri app activation policy to Accessory (hides from Dock). Toggle via settings.
**Deps**: D0.1
**Acceptance**:
- macOS: app hidden from Dock when stealth enabled
- Windows: already working (WS_EX_TOOLWINDOW)
- Toggle in settings page
- Hotkey to quickly toggle dock visibility

---

#### B1.7 🟢 [S] Click-through toggle
**Layer**: NATIVE
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-01 B1.7 — "set_ignore_cursor_events"
**Summary**: Toggle overlay between interactive and click-through (ghost) mode. In ghost mode, all mouse events pass through to the window below. Escape hatch: global hotkey always works regardless of mode.
**Deps**: B6.1
**Acceptance**:
- Hotkey toggles click-through
- Mouse passes through overlay in ghost mode
- Global hotkeys still work in ghost mode
- Visual indicator shows current mode (subtle border color)

---

#### B1.8 🟡 [S] Full-screen capture via xcap
**Layer**: DAEMON
**Status**: 🟡 PARTIAL — uses `screencapture` CLI tool
**Source**: CUE-DESIGN-01 B1.8 — "xcap in spawn_blocking"
**What's done**: `capture_screen_platform` shells out to `screencapture -x` (macOS) / PowerShell (Windows).
**What's needed**: Replace with xcap crate for cross-platform Rust-native capture. Content-protected windows automatically excluded. Use spawn_blocking since xcap is sync.
**Deps**: None
**Acceptance**:
- Screenshot captured without shelling out
- Content-protected overlay excluded from capture
- Works on macOS + Windows
- < 200ms capture time

---

#### B1.9 🟢 [L] Multi-monitor selective screenshot
**Layer**: DAEMON + DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-01 B1.9 — "overlay windows + canvas + crop"
**Summary**: User triggers selective capture → transparent overlay appears on each monitor → user draws rectangle → crop and return. Overlay windows cleaned up after capture.
**Deps**: B1.8, D0.1
**Acceptance**:
- Overlay appears on all monitors
- User can draw selection rectangle
- Cropped image returned as base64 PNG
- Overlay cleaned up on complete/cancel
- Works with multi-monitor setups

---

#### B1.12 🟢 [M] Window binding system
**Layer**: NATIVE + DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-01 B1.12 — "vertical column with coordinated movement"
**Summary**: Overlay and response panel move as a unit. Vertical column layout with configurable gap. Movement of one window moves the other. Bounds checking for both windows.
**Deps**: B1.10 (movement already done in native overlay)
**Acceptance**:
- Both windows move together
- Gap configurable (default 10px)
- Bounds checking prevents off-screen
- Binding toggleable via settings

---

#### B1.13 🟢 [M] Screen-share detection (Windows)
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-01 B1.13 — "EnumWindows heuristics"
**Summary**: Background thread polls EnumWindows every 1s. Checks window titles against 80+ indicator strings (Zoom sharing, Teams sharing, OBS, etc.). On detection, emit event to dashboard and overlay (auto-enable content protection, show notification).
**Deps**: None (Windows-only)
**Acceptance**:
- Detects Zoom, Teams, OBS, Chrome screen share
- Event emitted within 2s of share start
- No false positives on normal windows
- Detection stops when share ends

---

#### B1.14 🟢 [S] Cursor hiding for stealth
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-01 B1.14 — "CSS cursor: none"
**Summary**: When overlay is in stealth mode, hide cursor when hovering over the Tauri dashboard window. Prevents cursor shape from revealing an invisible window during screen share.
**Deps**: D0.1
**Acceptance**:
- Cursor hidden on dashboard when stealth active
- Cursor visible when stealth inactive
- No impact on overlay (native, not CSS)

---

#### B1.15 🟢 [S] Always-on-top re-assertion
**Layer**: NATIVE
**Status**: ✅ DONE — both platforms enforce always-on-top
**Source**: CUE-DESIGN-01 B1.15
**What's done**: macOS `panel.level = .screenSaver`, Windows `WS_EX_TOPMOST`.
**What's needed**: Add periodic re-assertion (every 3s) to handle edge cases where fullscreen apps or system dialogs push overlay below.
**Deps**: None
**Acceptance**:
- Overlay stays on top even after fullscreen app exits
- Re-assertion every 3s (low CPU cost)
- No flicker during re-assertion

---

## Remaining Tasks (assigned to phases above but detailed here)

#### B2.6 🟢 [M] Google gRPC STT + Soniox/ElevenLabs WebSocket
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-02 B2.6
**Phase**: 3 (Listening Upgrade)
**Summary**: Additional STT providers. Google: gRPC streaming with 305s limit handling. Soniox: WebSocket with word timestamps. ElevenLabs: WebSocket with low-latency mode.
**Deps**: B2.5 (trait)
**Acceptance**: All three providers pass integration test with real audio

---

#### B2.9 🟢 [L] LocalAgreement-2 streaming decoder
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-02 B2.9
**Phase**: 3 (Listening Upgrade) — stretch goal
**Summary**: Streaming decoder for local Whisper that emits partial results using LocalAgreement-2 algorithm. Compares consecutive decode passes, emits tokens that agree. Enables real-time local transcription.
**Deps**: B2.7 (whisper-rs)
**Acceptance**: Partial transcripts emitted within 500ms of speech, final within 2s

---

#### B3.11 🟢 [S] Triple-layer language injection
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-03 B3.11
**Phase**: 4 (Reasoning Upgrade)
**Summary**: Inject user's preferred language at 3 points in prompt: (1) system prompt header, (2) mode instructions, (3) response format directive. Ensures AI responds in user's language even when transcript is in English.
**Deps**: B4.1
**Acceptance**: Setting language to "Japanese" produces Japanese responses regardless of input language

---

#### B6.8 🟢 [S] Code expansion animation
**Layer**: DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B6.8
**Phase**: 6 (Dashboard Polish)
**Summary**: When AI response contains code blocks, animate expansion from collapsed (1 line preview) to full height. CSS transition with debounced visibility check.
**Deps**: B6.3.2
**Acceptance**: Code blocks expand smoothly, no layout jump

---

#### B8.3 🟢 [S] Host identity labels
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B8.3
**Phase**: 7 (Ops)
**Summary**: Generate stable machine UID (hash of hardware identifiers). Attach as label to all telemetry: machine_uid, os, arch, app_version.
**Deps**: B8.1
**Acceptance**: UID stable across restarts, different per machine

---

#### B8.5 🟢 [S] In-memory ring buffer for debug export
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B8.5
**Phase**: 7 (Ops)
**Summary**: Keep last 1000 log lines in memory ring buffer. Exportable via dashboard dev page "Copy Debug Logs" button. Useful for bug reports without requiring log file access.
**Deps**: B7.4
**Acceptance**: Ring buffer holds 1000 lines, export produces valid NDJSON

---

#### B8.6 🟢 [L] Grafana dashboard JSON
**Layer**: INFRA
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B8.6
**Phase**: 7 (Ops) — stretch goal
**Summary**: Pre-built Grafana dashboard JSON for visualizing bluey metrics. Panels: TTFT histogram, provider usage pie chart, error rate timeline, STT latency, session duration.
**Deps**: B8.1, B8.2
**Acceptance**: Dashboard importable into Grafana, shows real data

---

#### B9.2 🟢 [S] Release notes fetcher
**Layer**: DAEMON + DASHBOARD
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B9.2
**Phase**: 7 (Ops)
**Summary**: Fetch release notes from GitHub releases API. Parse markdown. Display in dashboard notification when update available.
**Deps**: B9.1
**Acceptance**: Release notes shown before update prompt

---

#### B9.5 🟢 [S] Anonymous install ping
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B9.5
**Phase**: 7 (Ops)
**Summary**: On first launch, send anonymous ping: UUID + OS + version + arch. Fire-and-forget POST (no retry, no error handling). Opt-out in settings.
**Deps**: B8.3 (machine UID)
**Acceptance**: Ping sent once on first launch, never again unless reinstall

---

#### B9.6 🟢 [S] Machine UID for license binding
**Layer**: DAEMON
**Status**: ❌ NOT STARTED
**Source**: CUE-DESIGN-04 B9.6
**Phase**: 7 (Ops)
**Summary**: Stable machine identifier for future license binding. Hash of: (macOS) IOPlatformSerialNumber, (Windows) MachineGuid registry, (Linux) /etc/machine-id.
**Deps**: None
**Acceptance**: UID stable across restarts, unique per machine, not reversible to hardware

---

## Deleted / Reframed Tasks (from old plan)

These tasks from the previous CUE-MASTER-PORT-PLAN.md were wrong under Option 4:

| Old Task | Disposition | Replacement |
|----------|-------------|-------------|
| "Add webview dashboard served by daemon HTTP" | ❌ DELETED | D0.1 + D0.2 (Tauri binary, no HTTP) |
| "Add localhost HTTP server in daemon" | ❌ DELETED | Tauri invoke replaces HTTP |
| "React SPA served by daemon on :3000" | ❌ DELETED | React bundled in Tauri binary |
| "Browser-based session switching" | REFRAMED | B6.3.3 (Tauri window session switching) |
| "WebSocket server for real-time updates" | ❌ DELETED | Tauri events replace WebSocket |
| "REST API for CLI ↔ dashboard" | ❌ DELETED | CLI uses TCP IPC, dashboard uses invoke |
| "Electron migration path" | ❌ DELETED | Never was Electron |
| "Service worker for offline" | ❌ DELETED | Native app, always offline-capable |
| "PWA manifest" | ❌ DELETED | Not a web app |
| "CORS configuration" | ❌ DELETED | No HTTP server |
| "Express.js middleware" | ❌ DELETED | No Node.js server |
| "Vite dev server proxy" | REFRAMED | Vite dev for dashboard only (Tauri dev mode) |
| "Docker deployment" | ❌ DELETED | Desktop app, not containerized |
| "nginx reverse proxy" | ❌ DELETED | No server infrastructure |
| "SSL certificate management" | ❌ DELETED | No HTTPS server |

---

## Full Task Index

| ID | Title | Layer | Phase | Sev | Size | Status | Deps | Notes |
|---|---|---|---|---|---|---|---|---|
| D0.1 | Add Tauri 2 to cue workspace | INFRA+DAEMON | 0 | 🔴 | M | ❌ | — | Foundation for all dashboard work |
| D0.2 | Tauri ↔ daemon shared-process IPC | DAEMON+DASH | 0 | 🔴 | M | ❌ | D0.1 | No HTTP, direct Rust calls |
| D0.3 | Single-binary architecture confirmation | INFRA | 0 | 🔴 | S | ❌ | D0.1 | Document startup sequence |
| C0.1 | Session-ID model in daemon DB | DAEMON+CROSS | 0 | 🔴 | M | ❌ | D0.1 | SQLite sessions + messages |
| D0.4 | Native overlay protocol upgrade | NATIVE+DAEMON | 0 | 🟡 | S | ❌ | C0.1 | SessionChanged command |
| N0.1 | Audit native overlay for Tauri-overlap | NATIVE | 0 | 🟢 | S | ❌ | D0.2 | Scope clarification doc |
| B6.3 | Dashboard window with sidebar nav | DASHBOARD | 1 | 🔴 | M | ❌ | D0.1,D0.2 | React Router + Radix |
| B6.1 | Hotkey system — Cmd+Shift+D | DAEMON | 1 | 🟡 | S | 🟡 | D0.1 | Extend existing hotkeys |
| B6.4 | Theme management — dark/light sync | DASH+CROSS | 1 | 🟡 | S | ✅→extend | D0.2 | Sync overlay↔dashboard |
| B6.5 | rAF streaming buffer hook | DASHBOARD | 1 | 🟡 | M | ❌ | D0.2 | 60fps render batching |
| B6.6 | React.memo MessageRow | DASHBOARD | 1 | 🟢 | S | ❌ | B6.3 | Prevent re-render storm |
| B7.8 | CSP configuration | INFRA | 1 | 🔴 | S | ❌ | D0.1 | Security headers |
| B7.9 | Error boundaries | DASHBOARD | 1 | 🟢 | S | ❌ | B6.3 | Crash isolation |
| B6.3.1 | Session list page | DASHBOARD | 2 | 🔴 | M | ❌ | C0.1,B6.3 | CRUD for sessions |
| B6.3.2 | Session detail page | DASHBOARD | 2 | 🔴 | M | ❌ | B6.3.1 | Chat history view |
| B6.3.3 | Session switching logic | DAEMON+CROSS | 2 | 🟡 | S | ❌ | C0.1,D0.4 | Overlay + dashboard sync |
| B5.5 | InterviewTranscriptBuffer | DAEMON | 2 | 🟡 | M | 🟡 | C0.1 | Add summarization |
| B10.8 | Graceful shutdown | DAEMON | 2 | 🟡 | S | 🟡 | C0.1 | Flush + in-flight tracking |
| B2.4 | Two-stage VAD (RMS + WebRTC) | DAEMON | 3 | 🔴 | M | ❌ | B2.3 | Saves STT costs |
| B2.1 | SystemAudioStream trait | DAEMON | 3 | 🔴 | L | 🟡 | — | Wrap native helpers |
| B2.2 | CPAL microphone capture | DAEMON | 3 | 🟡 | M | 🟡 | — | Stream recreation fix |
| B2.3 | Zero-copy DSP loop | DAEMON | 3 | 🟡 | S | 🟡 | B2.1,B2.2 | bytemuck + channels |
| B2.5 | SttProvider trait + 3 providers | DAEMON | 3 | 🔴 | L | 🟡 | B2.3 | Deepgram, OpenAI, Groq |
| B2.6 | Google gRPC + Soniox + ElevenLabs | DAEMON | 3 | 🟢 | M | ❌ | B2.5 | Additional providers |
| B2.7 | Local Whisper via whisper-rs | DAEMON | 3 | 🟡 | S | ❌ | B2.5 | Offline fallback |
| B2.8 | STT state machine + backoff | DAEMON | 3 | 🟡 | S | 🟡 | B2.5 | Error classification |
| B2.9 | LocalAgreement-2 decoder | DAEMON | 3 | 🟢 | L | ❌ | B2.7 | Streaming local STT |
| B2.10 | Speaker ID (ECAPA-TDNN ONNX) | DAEMON | 3 | 🟡 | L | ❌ | B2.5,B2.13 | User vs interviewer |
| B2.11 | Question extractor + noise filter | DAEMON | 3 | 🟡 | S | 🟡 | — | Coding question detect |
| B2.12 | Dual-channel hot-swap | DAEMON | 3 | 🟡 | M | ✅→extend | B2.5 | Per-channel provider |
| B2.13 | Sample rate + rubato resampler | DAEMON | 3 | 🟡 | S | 🟡 | B2.1,B2.2 | Proper resampling |
| B2.14 | Audio supervisor + recovery | DAEMON | 3 | 🟡 | M | ❌ | B2.1,B2.2 | Device watcher, sleep/wake |
| B3.1 | Multi-provider LLM trait | DAEMON | 4 | 🔴 | L | 🟡 | — | 5 providers |
| B3.2 | ModelVersionManager | DAEMON | 4 | 🟡 | M | ❌ | B3.1 | Background polling |
| B3.3 | Fallback chains + backoff | DAEMON | 4 | 🟡 | M | 🟡 | B3.1 | Exponential backoff |
| B3.4 | Rate limiters (governor) | DAEMON | 4 | 🔴 | S | ❌ | B3.1 | Per-provider limits |
| B3.5 | Streaming 60Hz + cancellation | DAEMON+NATIVE | 4 | 🔴 | M | 🟡 | B3.1,D0.2 | Batched overlay updates |
| B3.6 | Structured JSON generation | DAEMON | 6 | 🟢 | M | ❌ | B3.1 | JSON mode + fallback |
| B3.7 | Custom cURL provider | DAEMON+DASH | 6 | 🟢 | S | ❌ | B3.1 | Dev page config |
| B3.8 | Codex CLI integration | DAEMON | 6 | 🟢 | S | ❌ | B3.1 | Subprocess pipe |
| B3.9 | Key scrubbing (zeroize) | DAEMON | 4 | 🟡 | S | ❌ | — | Memory safety |
| B3.10 | testConnection command | DAEMON+DASH | 6 | 🟢 | S | ❌ | B3.1 | Validate API keys |
| B3.11 | Triple-layer language injection | DAEMON | 4 | 🟢 | S | ❌ | B4.1 | Multi-language support |
| B3.12 | Vision fallback + parallel race | DAEMON | 4 | 🟡 | M | 🟡 | B3.1 | 3-tier vision |
| B4.1 | Prompt composition system | DAEMON | 4 | 🔴 | M | 🟡 | — | XML-tagged blocks |
| B4.2 | Answer modes (3 primary) | DAEMON | 4 | 🟡 | M | 🟡 | B4.1 | Assist/Answer/WhatToAnswer |
| B4.3 | Per-provider prompt variants | DAEMON | 6 | 🟢 | S | ❌ | B4.1,B3.1 | Claude XML, Groq terse |
| B4.4 | TINY prompt for fast mode | DAEMON | 6 | 🟢 | S | ❌ | B4.1 | < 500 tokens |
| B4.5 | Skill library (9 prompts) | DASH+DAEMON | 6 | 🟡 | L | ❌ | B4.1,C0.1 | Template management |
| B4.6 | Anti-chatbot constraints | DAEMON | 4 | 🟡 | S | 🟡 | B4.1 | No preambles |
| B4.7 | System-prompt protection | DAEMON | 6 | 🟢 | S | ❌ | B4.1 | Jailbreak defense |
| B4.8 | Context prioritization matrix | DAEMON | 4 | 🟡 | S | 🟡 | B4.1 | Priority-based trimming |
| B4.9 | First-person enforcement | DAEMON | 6 | 🟢 | S | ❌ | B4.1,B4.2 | "I think..." not "You should..." |
| B5.1 | sqlite-vec vector store | DAEMON | 5 | 🔴 | M | ❌ | C0.1 | Local RAG foundation |
| B5.2 | SemanticChunker | DAEMON | 5 | 🔴 | M | ❌ | — | Sliding-window overlap |
| B5.3 | Embedding trait + providers | DAEMON | 5 | 🔴 | M | ❌ | B3.1 | OpenAI + local |
| B5.4 | Live RAG indexer | DAEMON | 5 | 🔴 | M | ❌ | B5.1,B5.2,B5.3 | JIT during session |
| B5.6 | Epoch summarization | DAEMON | 5 | 🟡 | M | ❌ | B5.1,B3.1 | Compress old context |
| B5.7 | Async vector search | DAEMON | 5 | 🟡 | S | ❌ | B5.1 | spawn_blocking wrapper |
| B5.8 | Hybrid retrieval (vec + BM25) | DAEMON | 5 | 🟡 | M | 🟡 | B5.1,B5.7 | Score fusion |
| B6.2 | Rebindable keybinds | DASH+DAEMON | 6 | 🟡 | M | ❌ | B6.1,C0.1 | Settings UI |
| B6.7 | Inertial scroll engine | DASHBOARD | 6 | 🟢 | M | ❌ | B6.3.2 | Physics-based |
| B6.8 | Code expansion animation | DASHBOARD | 6 | 🟢 | S | ❌ | B6.3.2 | CSS transition |
| B6.9 | Command palette (Cmd+K) | DASHBOARD | 6 | 🟡 | L | ❌ | B6.3 | cmdk spotlight |
| B6.10 | Onboarding flow | DASHBOARD | 6 | 🟢 | S | ❌ | B6.3 | First-launch wizard |
| B7.1 | Keychain integration | DAEMON | 7 | 🔴 | S | ❌ | D0.1 | OS keychain for keys |
| B7.2 | Key scrubbing on drop | DAEMON | 7 | 🟡 | S | ❌ | B3.9 | zeroize wrapper |
| B7.3 | Log masking utility | DAEMON | 7 | 🟡 | S | ❌ | — | mask_key() |
| B7.4 | Log rotation + NDJSON | DAEMON | 7 | 🔴 | S | ❌ | — | tracing-appender |
| B7.5 | SQLite migration system | DAEMON | 7 | 🔴 | M | ❌ | C0.1 | 4 migration files |
| B7.6 | Hot-reload config | DAEMON | 7 | 🟢 | S | ❌ | — | notify crate |
| B7.7 | Single-instance lock | DAEMON | 7 | 🟡 | S | 🟡 | D0.1 | tauri-plugin |
| B7.10 | Panic handler | DAEMON | 7 | 🟡 | S | ❌ | B7.4 | Crash log |
| B8.1 | OpenTelemetry init | DAEMON | 7 | 🟡 | M | ❌ | — | OTLP exporter |
| B8.2 | Metric definitions | DAEMON | 7 | 🟡 | M | ❌ | B8.1 | TTFT, latency, counters |
| B8.3 | Host identity labels | DAEMON | 7 | 🟢 | S | ❌ | B8.1 | machine_uid hash |
| B8.4 | AI pricing table | DAEMON | 7 | 🟡 | S | 🟡 | B3.1 | compute_cost() |
| B8.5 | In-memory ring buffer | DAEMON | 7 | 🟢 | S | ❌ | B7.4 | Debug export |
| B8.6 | Grafana dashboard JSON | INFRA | 7 | 🟢 | L | ❌ | B8.1,B8.2 | Stretch goal |
| B9.1 | Auto-updater | INFRA | 7 | 🟡 | M | ❌ | D0.1 | tauri-plugin-updater |
| B9.2 | Release notes fetcher | DAEMON+DASH | 7 | 🟢 | S | ❌ | B9.1 | GitHub API |
| B9.3 | Autostart on login | INFRA | 7 | 🟢 | S | ❌ | D0.1 | LaunchAgent/Registry |
| B9.4 | PostHog analytics | DAEMON | 7 | 🟢 | S | ❌ | — | Opt-in only |
| B9.5 | Anonymous install ping | DAEMON | 7 | 🟢 | S | ❌ | B8.3 | Fire-and-forget |
| B9.6 | Machine UID | DAEMON | 7 | 🟢 | S | ❌ | — | License binding |
| B9.7 | Build targets | INFRA | 7 | 🟡 | M | ❌ | D0.1 | .dmg, .msi, .AppImage |
| B10.1 | CLAUDE.md | INFRA | 8 | 🟡 | S | ❌ | — | Dev rules |
| B10.2 | CHANGELOG.md | INFRA | 8 | 🟡 | S | ❌ | — | Keep-a-Changelog |
| B10.3 | PR template | INFRA | 8 | 🟡 | S | ❌ | — | .github/ |
| B10.4 | FIXES.md | INFRA | 8 | 🟡 | S | ❌ | — | Bug fix template |
| B10.5 | AUDIT.md | INFRA | 8 | 🟡 | S | ❌ | — | Security checklist |
| B10.6 | .codex/agents | INFRA | 8 | 🟡 | M | ❌ | — | 7 agent configs |
| B10.7 | .codex/skills | INFRA | 8 | 🟢 | M | ❌ | — | 10 skill cards |
| B1.5 | Process masquerading | NATIVE+DAEMON | 8 | 🟡 | M | ❌ | D0.1 | 3 disguise presets |
| B1.6 | Dock/taskbar visibility | DAEMON+NATIVE | 8 | 🟡 | S | 🟡 | D0.1 | macOS Accessory policy |
| B1.7 | Click-through toggle | NATIVE | 8 | 🟢 | S | ❌ | B6.1 | Ghost mode |
| B1.8 | Full-screen capture (xcap) | DAEMON | 8 | 🟡 | S | 🟡 | — | Replace shell-out |
| B1.9 | Multi-monitor screenshot | DAEMON+DASH | 8 | 🟢 | L | ❌ | B1.8,D0.1 | Canvas selection |
| B1.12 | Window binding | NATIVE+DAEMON | 8 | 🟢 | M | ❌ | — | Coordinated movement |
| B1.13 | Screen-share detection | DAEMON | 8 | 🟢 | M | ❌ | — | Windows EnumWindows |
| B1.14 | Cursor hiding | DASHBOARD | 8 | 🟢 | S | ❌ | D0.1 | CSS cursor:none |
| B1.15 | Always-on-top re-assertion | NATIVE | 8 | 🟢 | S | ✅→extend | — | 3s periodic |

**Total: 97 tasks** (6 new glue tasks + 94 original − 3 merged)

---

## Critical-Path Analysis

### If 1 Engineer (Sequential)

```
Phase 0 (4d) → Phase 1 (5d) → Phase 2 (6d) → Phase 3 (12d) → Phase 4 (12d) → Phase 5 (10d) → Phase 6 (10d) → Phase 7 (7d) → Phase 8 (5d)
Total: ~71 working days (~14 weeks)
```

### If 2 Engineers (Parallel Tracks)

```
Engineer A (Backend/Daemon):          Engineer B (Frontend/Dashboard):
Phase 0 (4d) ─────────────────────── Phase 0 (shared, 4d)
Phase 3 — Listening (12d)             Phase 1 — Dashboard Shell (5d)
Phase 4 — Reasoning (12d)            Phase 2 — Session UX (6d)
Phase 5 — Memory (10d)               Phase 6 — Dashboard Polish (10d)
Phase 7 — Ops (7d)                   Phase 8 — Dev + Stealth (5d)
                                      
Total A: 45d                          Total B: 30d
Critical path: 45 working days (~9 weeks)
```

### Parallel-Safe Branches

These can run simultaneously without conflicts:
- **Phase 3 (Audio)** ∥ **Phase 2 (Session UX)** — different crates, no shared files
- **Phase 5 (Memory)** ∥ **Phase 6 (Dashboard Polish)** — daemon vs React
- **Phase 8 (Dev docs)** ∥ anything — documentation only
- **B8.1-B8.7 (Observability)** ∥ anything after Phase 0 — additive instrumentation
- **B10.1-B10.7 (Dev discipline)** ∥ anything — no code changes

### Blocking Dependencies (Cannot Parallelize)

- Phase 4 needs B2.5 (STT trait) to test LLM with real transcripts
- Phase 5 needs B3.1 (LLM trait) for embedding providers
- Phase 6 needs Phase 1 + Phase 2 (dashboard shell + sessions)
- Phase 7 needs D0.1 (Tauri for plugins)
- B9.7 (build targets) should be last (needs stable binary)

---

## Open Questions Needing User Decision

### Before Phase 0

1. **Tauri single-binary vs daemon-sidecar**
   - **Recommendation**: Single-binary (daemon IS the Tauri app)
   - **Tradeoff**: Sidecar allows daemon to run headless (no window), but adds IPC complexity
   - **Decision needed**: Can the daemon always have a Tauri runtime, even when user only wants CLI?
   - **Proposed resolution**: Single-binary for GUI mode. CLI (`cue-cli`) remains separate and connects via TCP IPC.

2. **React framework choice**
   - **Recommendation**: React 19 + Vite + React Router + Radix + Tailwind + shadcn/ui
   - **Alternative**: Solid.js (smaller bundle, faster) or Svelte (simpler)
   - **Decision needed**: Confirm React 19 or choose alternative

### Before Phase 3

3. **Speaker ID: Rust ONNX vs Python sidecar**
   - **Recommendation**: Rust ONNX (via `ort` crate) — no Python dependency
   - **Tradeoff**: Python sidecar faster to prototype (SpeechBrain has pre-trained models), but adds 200MB+ Python runtime
   - **Decision needed**: Accept longer dev time for zero-dependency solution?

4. **STT provider priority**
   - Which 3 providers to implement first? Recommendation: Deepgram (best streaming), OpenAI Whisper (most popular), whisper-rs (offline)
   - **Decision needed**: Confirm or reorder

### Before Phase 5

5. **Embedding model for local RAG**
   - **Option A**: OpenAI text-embedding-3-small (1536d, requires API key, best quality)
   - **Option B**: all-MiniLM-L6-v2 ONNX (384d, local, no API key, good quality)
   - **Option C**: Both (OpenAI primary, local fallback)
   - **Recommendation**: Option C
   - **Decision needed**: Confirm approach

### Before Phase 7

6. **Keychain plugin choice**
   - **Option A**: `tauri-plugin-keychain` (Tauri ecosystem, maintained)
   - **Option B**: `keyring` crate directly (no Tauri dependency, works in CLI too)
   - **Recommendation**: `keyring` crate (works for both Tauri app AND CLI)
   - **Decision needed**: Confirm

7. **Auto-updater distribution**
   - Where to host update artifacts? GitHub Releases? S3? Custom server?
   - **Recommendation**: GitHub Releases (free, integrated with tauri-plugin-updater)
   - **Decision needed**: Confirm hosting

### Architectural

8. **Session-switching context strategy**
   - **Option A**: Clear rolling context on switch (fast, clean)
   - **Option B**: Warm-start from saved state (load last N messages as context)
   - **Recommendation**: Option B with limit (load last 20 messages as context)
   - **Decision needed**: Confirm approach

9. **Dashboard content protection**
   - Should the dashboard window also be content-protected (invisible in screen recordings)?
   - **Recommendation**: Yes — it may show sensitive conversation history
   - **Tradeoff**: Makes it impossible to screenshot the dashboard for bug reports
   - **Decision needed**: Always protected, or toggle?

---

## Success Metrics

At end of Phase 8, bluey achieves:

### Feature Parity with Cluely
- [x] Always-on-top overlay with content protection (macOS + Windows)
- [ ] Multi-provider STT (3+ providers, hot-swappable)
- [ ] Multi-provider LLM (5+ providers, fallback chains)
- [ ] Local RAG with semantic search
- [ ] Session management (create, switch, archive, persist)
- [ ] Dashboard with settings, chats, prompts, shortcuts
- [ ] Process masquerading (3 disguise presets)
- [ ] Speaker identification (user vs interviewer)
- [ ] Auto-updater with release notes
- [ ] OS keychain for API key storage

### Performance Targets
- [ ] < 30MB installer (.dmg / .msi)
- [ ] < 500ms mic → first-AI-token latency (with fast provider)
- [ ] < 200ms TTFT on "fast mode" with Groq
- [ ] < 50ms VAD decision latency
- [ ] < 100ms vector search (10K chunks)
- [ ] 60fps overlay streaming (no dropped frames)
- [ ] < 2s cold start to listening state

### Reliability Targets
- [ ] Audio recovers from device disconnect within 2s
- [ ] STT reconnects after transient failure within 5s
- [ ] No message loss on crash (WAL mode)
- [ ] Graceful degradation: works offline with local Whisper + Ollama

### Security Targets
- [ ] API keys in OS keychain (not env vars or plaintext files)
- [ ] Keys zeroed from memory on drop
- [ ] No raw keys in logs
- [ ] Content protection on all windows
- [ ] CSP configured for dashboard webview

### Platform Support
- [ ] macOS 13+ (Apple Silicon + Intel)
- [ ] Windows 10+ (x64)
- [ ] Linux (AppImage, best-effort stealth)

---

## Appendix: Task Count Summary

| Phase | Tasks | Days (solo) | Layer Distribution |
|-------|-------|-------------|-------------------|
| 0 — Foundation | 6 | 4 | 2 INFRA, 2 DAEMON, 1 CROSS, 1 NATIVE |
| 1 — Dashboard Shell | 7 | 5 | 5 DASHBOARD, 1 DAEMON, 1 INFRA |
| 2 — Session UX | 5 | 6 | 2 DASHBOARD, 2 DAEMON, 1 CROSS |
| 3 — Listening | 14 | 12 | 14 DAEMON |
| 4 — Reasoning | 12 | 12 | 11 DAEMON, 1 CROSS |
| 5 — Memory | 8 | 10 | 8 DAEMON |
| 6 — Dashboard Polish | 14 | 10 | 8 DASHBOARD, 4 DAEMON, 2 CROSS |
| 7 — Ops | 18 | 7 | 12 DAEMON, 4 INFRA, 2 CROSS |
| 8 — Dev + Stealth | 13 | 5 | 7 INFRA, 3 NATIVE, 2 DAEMON, 1 DASHBOARD |
| **TOTAL** | **97** | **71** | |

---

*End of Master Port Plan V2*
