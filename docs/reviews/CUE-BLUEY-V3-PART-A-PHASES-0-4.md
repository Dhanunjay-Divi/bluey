# bluey V3 — Phases 0-4 Detailed Implementation Plan

**Scope**: Foundation → Dashboard Shell → Session UX → Listening Upgrade → Reasoning Upgrade
**Total duration**: ~13 weeks solo
**Date**: 2026-05-12

---

# Phase 0 — Foundation (~2 weeks solo)

## Entry criteria
- Repo cloned, `cargo build` passes on all 3 crates
- Node 20+ and npm available for Tauri frontend
- Rust 1.78+ with `cargo-tauri` CLI installed
- Native overlay binaries compile (Swift on macOS, C on Windows)

## Exit criteria
- `cargo tauri dev` launches Tauri window alongside existing daemon
- SQLite DB created at `~/.local/share/bluey/sessions.db` with sessions + messages tables
- Unix domain socket IPC proven: daemon sends JSON frame, overlay receives and renders
- Existing overlay still works via new socket path
- `cargo test` passes with ≥5 new integration tests

## Batch structure
Ships as 2 PRs:
- **PR1**: D0.1 + D0.2 + D0.3 + N0.1 (Tauri scaffold + IPC)
- **PR2**: C0.1 + D0.4 + L5 (Session model + overlay protocol upgrade + Unix socket)

## Tasks (in execution order)

### Task D0.1 🔴 [M] Tauri 2 Scaffold
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: —
**Summary**: Add Tauri 2 to the existing Cargo workspace. Create `crates/cue-dashboard/` with `tauri.conf.json`, a minimal React 19 + Vite frontend in `crates/cue-dashboard/ui/`, and wire it as a workspace member. The daemon remains a separate binary; Tauri manages only the dashboard webview.
**Design source**: CUE-DESIGN-01-STEALTH-WINDOWS.md §B1.1, CUE-BLUEY-V3-PLAN-SKELETON.md Part 2 Architecture Diagram
**Code sketch**:
```rust
// crates/cue-dashboard/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_daemon_status,
            commands::get_sessions,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}

mod commands {
    #[tauri::command]
    pub async fn get_daemon_status() -> Result<String, String> {
        // Connect to daemon TCP 57321, send Status request
        let resp = cue_core::ipc::send_request(
            cue_core::ipc::DaemonRequest::Status
        ).await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&resp).unwrap())
    }

    #[tauri::command]
    pub async fn get_sessions() -> Result<String, String> {
        let resp = cue_core::ipc::send_request(
            cue_core::ipc::DaemonRequest::SessionsList
        ).await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&resp).unwrap())
    }
}
```
```json
// crates/cue-dashboard/tauri.conf.json (key fields)
{
  "productName": "bluey",
  "identifier": "com.bluey.dashboard",
  "build": { "frontendDist": "../ui/dist" },
  "app": {
    "windows": [{
      "title": "bluey",
      "width": 420,
      "height": 700,
      "visible": false,
      "decorations": false
    }],
    "security": { "csp": "default-src 'self'; script-src 'self'" }
  }
}
```
**Acceptance criteria**:
- `cargo tauri build` produces a single binary containing the webview
- `cargo tauri dev` opens a window showing "bluey dashboard" placeholder
- Workspace `Cargo.toml` lists `cue-dashboard` as a member
- No regressions: `cargo build -p cue-daemon` and `cargo build -p cue-cli` still pass
**Verification commands**:
- `cargo tauri dev` — window appears
- `cargo build --workspace` — all crates compile
- `ls crates/cue-dashboard/tauri.conf.json` — exists
**Risks + mitigations**:
- Tauri 2 requires specific Rust toolchain features → pin `tauri = "2"` in Cargo.toml, test on CI

---

### Task D0.2 🔴 [M] Tauri↔Daemon IPC
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Implement bidirectional communication between the Tauri dashboard and the running cue-daemon. Uses existing TCP IPC on port 57321 for commands (invoke → daemon) and adds Tauri event emission for daemon→dashboard push (streaming tokens, status changes). Reuses `DaemonRequest`/`DaemonResponse` from cue-core.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 2 (Tauri invoke/events COLD PATH)
**Code sketch**:
```rust
// crates/cue-dashboard/src/ipc_bridge.rs
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct DaemonBridge {
    app: AppHandle,
}

impl DaemonBridge {
    pub async fn connect_event_stream(&self) -> anyhow::Result<()> {
        let stream = TcpStream::connect("127.0.0.1:57321").await?;
        let (reader, mut writer) = stream.into_split();
        // Subscribe to daemon events
        let subscribe = serde_json::to_string(
            &cue_core::ipc::DaemonRequest::SubscribeEvents
        )?;
        writer.write_all(format!("{}\n", subscribe).as_bytes()).await?;

        let app = self.app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                app.emit("daemon-event", &line).ok();
            }
        });
        Ok(())
    }
}
```
**Acceptance criteria**:
- `invoke('get_daemon_status')` from React returns valid JSON when daemon is running
- `invoke('get_daemon_status')` returns error string when daemon is not running
- Tauri event `daemon-event` fires when daemon pushes a status change
- Round-trip latency <30ms measured via timestamp in response
**Verification commands**:
- Start daemon: `cargo run -p cue-daemon &`
- Start dashboard: `cargo tauri dev`
- Browser console: `window.__TAURI__.invoke('get_daemon_status')` returns JSON
**Risks + mitigations**:
- Daemon not running when dashboard starts → show "daemon offline" banner, retry every 2s

---

### Task D0.3 🔴 [S] Single-Binary Architecture Confirmation
**Layer**: INFRA
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Write `ARCHITECTURE.md` documenting the hybrid architecture. Confirm that `cargo tauri build` produces a single distributable binary that bundles the webview. Verify the daemon remains a separate process launched by the CLI or auto-started.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 2
**Code sketch**:
```markdown
<!-- ARCHITECTURE.md (excerpt) -->
# bluey Architecture

## Process Model
- `cue-daemon`: Long-running background process. Owns audio, STT, LLM, sessions.
- `cue-dashboard`: Tauri 2 webview. Settings, sessions, prompts UI. Cold-path IPC.
- `cue-overlay`: Native Swift/C floating panel. Hot-path IPC via Unix socket.
- `cue-cli`: Thin client. Commands forwarded to daemon over TCP.

## IPC Summary
| Path | Transport | Latency | Use |
|------|-----------|---------|-----|
| Overlay↔Daemon | Unix domain socket | <10ms | Streaming tokens |
| Dashboard↔Daemon | Tauri invoke (TCP 57321) | 10-30ms | Commands, settings |
| CLI↔Daemon | TCP 57321 | 10-30ms | User commands |
```
**Acceptance criteria**:
- `ARCHITECTURE.md` exists at repo root with process model + IPC table
- `cargo tauri build` output is a single `.app` (macOS) or `.exe` (Windows)
- Dashboard binary size <30MB (Tauri webview + React bundle)
**Verification commands**:
- `cat ARCHITECTURE.md | grep "Unix domain socket"`
- `ls target/release/bundle/macos/*.app` — exists after `cargo tauri build`
**Risks + mitigations**:
- None significant — documentation task

---

### Task N0.1 🟢 [S] Audit Overlay for Tauri-Overlap
**Layer**: NATIVE
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.2
**Summary**: Audit the native overlay (2463 LOC Swift, 996 LOC C) to identify features that overlap with the new Tauri dashboard. Produce a scope document listing what stays in native overlay (real-time display, stealth, always-on-top) vs what moves to dashboard (settings, session management, prompt editing).
**Design source**: CUE-CURRENT-STATE.md §4 Native Modules
**Code sketch**:
```markdown
<!-- docs/OVERLAY-SCOPE-AUDIT.md -->
# Overlay Scope Audit

## STAYS in Native Overlay (hot path, stealth-critical)
- Card feed rendering (streaming tokens)
- Always-on-top + capture exclusion
- Collapsed pill mode
- Drag-to-move + resize
- Mode indicator badge (new: R5)
- Patch-mode diff rendering (new: F7)

## MOVES to Dashboard (cold path, non-stealth)
- Model picker dropdown → Dashboard settings page
- Instructions editor → Dashboard prompts page
- File attachment UI → Dashboard context page
- Theme toggle → Dashboard settings (sync to overlay via IPC)

## SHARED (both render, dashboard is source of truth)
- Session ID display
- Current mode indicator
```
**Acceptance criteria**:
- `docs/OVERLAY-SCOPE-AUDIT.md` exists with ≥10 items categorized
- No code changes to overlay in this task (audit only)
**Verification commands**:
- `test -f docs/OVERLAY-SCOPE-AUDIT.md && echo OK`
**Risks + mitigations**:
- None — read-only audit

---

### Task C0.1 🔴 [M] Session-ID Model in SQLite
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Replace JSON file persistence with SQLite (WAL mode) for sessions and messages. Each session has a UUID, title, created_at, updated_at, and state (active/archived). Messages store role, content, lane (snap/solve/think), provider, latency_ms, token counts. This is the foundation that CM1 (Phase 5) extends with turn model and lane metadata.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 6 (C0.1 → CM1 extension path)
**Code sketch**:
```rust
// crates/cue-daemon/src/storage/sqlite.rs
use rusqlite::{Connection, params};
use uuid::Uuid;

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn new(db_path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(include_str!("../../migrations/001_sessions.sql"))?;
        Ok(Self { conn })
    }

    pub fn create_session(&self, title: &str) -> anyhow::Result<String> {
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO sessions (id, title, state, created_at, updated_at)
             VALUES (?1, ?2, 'active', datetime('now'), datetime('now'))",
            params![id, title],
        )?;
        Ok(id)
    }

    pub fn add_message(&self, session_id: &str, msg: &Message) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO messages (id, session_id, role, content, lane, provider, latency_ms, input_tokens, output_tokens, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))",
            params![
                Uuid::new_v4().to_string(), session_id,
                msg.role, msg.content, msg.lane, msg.provider,
                msg.latency_ms, msg.input_tokens, msg.output_tokens
            ],
        )?;
        Ok(())
    }

    pub fn list_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, state, created_at, updated_at FROM sessions ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map([], |row| Ok(SessionSummary {
            id: row.get(0)?, title: row.get(1)?, state: row.get(2)?,
            created_at: row.get(3)?, updated_at: row.get(4)?,
        }))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

pub struct Message {
    pub role: String,
    pub content: String,
    pub lane: Option<String>,
    pub provider: Option<String>,
    pub latency_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}
```
```sql
-- crates/cue-daemon/migrations/001_sessions.sql
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','archived')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK(role IN ('user','assistant','system')),
    content TEXT NOT NULL,
    lane TEXT CHECK(lane IN ('snap','solve','think')),
    provider TEXT,
    latency_ms INTEGER,
    input_tokens INTEGER,
    output_tokens INTEGER,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, created_at);
```
**Acceptance criteria**:
- `SessionStore::new()` creates DB file with WAL mode enabled
- `create_session` + `list_sessions` round-trips correctly
- `add_message` stores lane metadata (snap/solve/think)
- Existing JSON persistence still works (migration is additive, not replacing yet)
- 3 unit tests: create, list, add_message
**Verification commands**:
- `cargo test -p cue-daemon storage::sqlite`
- `sqlite3 ~/.local/share/bluey/sessions.db ".tables"` — shows sessions, messages
**Risks + mitigations**:
- SQLite locking under concurrent access → WAL mode handles this; single-writer pattern in daemon

---

### Task D0.4 🟡 [S] Overlay Protocol Upgrade
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: C0.1
**Summary**: Extend the `OverlayCommand` enum in cue-core with `SessionChanged { id, title }` and `ModeIndicator { lane, model }` variants. The overlay renders a small badge showing current lane. This prepares for R5 (Phase 4) without requiring the full routing system yet.
**Design source**: CUE-CURRENT-STATE.md §2 cue-core overlay module, Skeleton R5 description
**Code sketch**:
```rust
// crates/cue-core/src/overlay.rs — additions to existing enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OverlayCommand {
    // ... existing variants ...
    SessionChanged { session_id: String, title: String },
    ModeIndicator { lane: Lane, model: String },
    PatchDiff { blocks: Vec<DiffBlock> }, // prep for F7
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Lane { Snap, Solve, Think }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffBlock {
    pub action: DiffAction,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DiffAction { Keep, Modify, Add, Remove }
```
**Acceptance criteria**:
- `OverlayCommand::SessionChanged` serializes to `{"type":"SessionChanged","session_id":"...","title":"..."}`
- `OverlayCommand::ModeIndicator` serializes with lane as lowercase string
- Existing overlay commands still deserialize correctly (backward compat)
- 2 unit tests for new variant serialization
**Verification commands**:
- `cargo test -p cue-core overlay`
**Risks + mitigations**:
- Native overlay ignores unknown commands (already does via serde `deny_unknown_fields` off) → safe

---

### Task L5 🔴 [M] Unix Domain Socket Daemon↔Overlay
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Replace stdin/stdout IPC between daemon and native overlay with a Unix domain socket (macOS/Linux) or named pipe (Windows). This enables <10ms streaming latency for the hot path. The daemon listens on `/tmp/bluey-overlay.sock`, overlay connects on launch. Framing: newline-delimited JSON (same as current stdin/stdout protocol).
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 2 (Unix Domain Socket HOT PATH <10ms), L5 task description
**Code sketch**:
```rust
// crates/cue-daemon/src/overlay_socket.rs
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const SOCKET_PATH: &str = "/tmp/bluey-overlay.sock";

pub struct OverlaySocket {
    writer: Option<tokio::io::WriteHalf<UnixStream>>,
}

impl OverlaySocket {
    pub async fn listen(tx: tokio::sync::mpsc::Sender<cue_core::overlay::OverlayEvent>) -> anyhow::Result<Self> {
        let _ = std::fs::remove_file(SOCKET_PATH);
        let listener = UnixListener::bind(SOCKET_PATH)?;
        let (stream, _) = listener.accept().await?;
        let (reader, writer) = tokio::io::split(stream);

        // Spawn reader for overlay→daemon events
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(event) = serde_json::from_str(&line) {
                    let _ = tx.send(event).await;
                }
            }
        });

        Ok(Self { writer: Some(writer) })
    }

    pub async fn send_command(&mut self, cmd: &cue_core::overlay::OverlayCommand) -> anyhow::Result<()> {
        if let Some(ref mut w) = self.writer {
            let json = serde_json::to_string(cmd)?;
            w.write_all(format!("{}\n", json).as_bytes()).await?;
        }
        Ok(())
    }
}
```
**Acceptance criteria**:
- Daemon creates `/tmp/bluey-overlay.sock` on startup
- Overlay connects and receives `OverlayCommand` JSON frames
- Measured latency: send→receive <5ms on localhost (use timestamp echo test)
- Fallback: if socket connection fails, daemon logs warning and overlay can still use stdin/stdout
- Windows: uses `\\.\pipe\bluey-overlay` named pipe (same framing)
**Verification commands**:
- `cargo test -p cue-daemon overlay_socket` — integration test with mock client
- `echo '{"type":"Status"}' | socat - UNIX-CONNECT:/tmp/bluey-overlay.sock` — gets response
**Risks + mitigations**:
- Socket file left behind on crash → `remove_file` on startup (already in code sketch)
- Windows lacks Unix sockets → use `tokio::net::windows::named_pipe` behind `#[cfg(windows)]`

---

## Phase deliverable
- A user can run `cargo tauri dev` and see a placeholder dashboard window
- The daemon stores sessions in SQLite with lane metadata
- The overlay communicates via Unix socket with <10ms latency
- All existing CLI + overlay functionality is preserved
- Codex review should check: no regressions in `cargo build --workspace`, SQLite WAL mode, socket cleanup on exit


---

# Phase 1 — Dashboard Shell (~1 week)

## Entry criteria
- Phase 0 complete: Tauri binary builds, IPC proven, SQLite sessions table exists
- React 19 + Vite scaffold in `crates/cue-dashboard/ui/` compiles

## Exit criteria
- Cmd+Shift+D toggles dashboard window visibility
- Sidebar navigation renders 4 placeholder pages (Sessions, Settings, Prompts, Dev)
- Daemon status displayed in dashboard header (online/offline badge)
- Theme syncs between dashboard and system preference
- CSP configured, error boundaries on all routes

## Batch structure
Ships as 1 PR:
- **PR3**: B6.3 + B6.1 + B6.4 + B6.5 + B6.6 + B7.8 + B7.9

## Tasks (in execution order)

### Task B6.3 🔴 [M] Dashboard Sidebar Navigation
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1, D0.2
**Summary**: Build the main dashboard layout with a collapsible sidebar using Radix UI primitives. Four nav items: Sessions, Settings, Prompts, Dev Tools. React Router for client-side routing. Header shows daemon connection status badge.
**Design source**: CUE-DESIGN-04-UX-OPS-DEV.md §B6.3
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/App.tsx
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { Sidebar } from './components/Sidebar';
import { DaemonStatus } from './components/DaemonStatus';
import { SessionsPage } from './pages/Sessions';
import { SettingsPage } from './pages/Settings';

export function App() {
  return (
    <BrowserRouter>
      <div className="flex h-screen bg-background text-foreground">
        <Sidebar />
        <main className="flex-1 flex flex-col overflow-hidden">
          <header className="h-12 flex items-center px-4 border-b">
            <DaemonStatus />
          </header>
          <Routes>
            <Route path="/" element={<SessionsPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="/prompts" element={<div>Prompts</div>} />
            <Route path="/dev" element={<div>Dev Tools</div>} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}
```
```typescript
// crates/cue-dashboard/ui/src/components/Sidebar.tsx
import { NavLink } from 'react-router-dom';
import { MessageSquare, Settings, Sparkles, Terminal } from 'lucide-react';

const NAV_ITEMS = [
  { to: '/', icon: MessageSquare, label: 'Sessions' },
  { to: '/settings', icon: Settings, label: 'Settings' },
  { to: '/prompts', icon: Sparkles, label: 'Prompts' },
  { to: '/dev', icon: Terminal, label: 'Dev Tools' },
] as const;

export function Sidebar() {
  return (
    <nav className="w-48 border-r flex flex-col py-2" role="navigation">
      {NAV_ITEMS.map(({ to, icon: Icon, label }) => (
        <NavLink key={to} to={to}
          className={({ isActive }) =>
            `flex items-center gap-2 px-3 py-2 text-sm rounded-md mx-2 ${
              isActive ? 'bg-accent text-accent-foreground' : 'hover:bg-muted'
            }`
          }>
          <Icon size={16} />
          {label}
        </NavLink>
      ))}
    </nav>
  );
}
```
**Acceptance criteria**:
- Sidebar renders 4 navigation items with icons
- Clicking nav item changes route and highlights active item
- Layout is responsive: sidebar collapses to icons at <600px width
- Keyboard navigation works (Tab through items, Enter to select)
**Verification commands**:
- `cd crates/cue-dashboard/ui && npm run build` — no errors
- `cargo tauri dev` — sidebar visible with 4 items
**Risks + mitigations**:
- React Router in Tauri webview → use `MemoryRouter` if `BrowserRouter` has issues with file:// protocol

---

### Task B6.1 🟡 [S] Hotkey System (Cmd+Shift+D)
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (macOS overlay has Cmd+Shift+B)
**Depends on**: D0.1
**Summary**: Register global hotkey Cmd+Shift+D (Ctrl+Shift+D on Windows/Linux) to toggle dashboard window visibility. Uses `tauri-plugin-global-shortcut`. Also registers Alt+D and Alt+F as prep for R3 (manual lane override) — these emit events but don't act yet.
**Design source**: CUE-CURRENT-STATE.md §B6.1 (existing hotkeys in overlay), Skeleton R3
**Code sketch**:
```rust
// crates/cue-dashboard/src/main.rs — in setup
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let toggle_shortcut = "CommandOrControl+Shift+D".parse::<Shortcut>()?;
            app.global_shortcut().on_shortcut(toggle_shortcut, |app, _, event| {
                if event.state == ShortcutState::Pressed {
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            window.hide().ok();
                        } else {
                            window.show().ok();
                            window.set_focus().ok();
                        }
                    }
                }
            })?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri");
}
```
**Acceptance criteria**:
- Cmd+Shift+D toggles dashboard window from any app
- Window appears focused when shown, releases focus when hidden
- Alt+D emits `lane-override-think` event (no handler yet)
- Alt+F emits `lane-override-snap` event (no handler yet)
- No conflict with existing Cmd+Shift+B (overlay toggle)
**Verification commands**:
- Press Cmd+Shift+D with dashboard running → window toggles
- `cargo test -p cue-dashboard` — shortcut registration doesn't panic
**Risks + mitigations**:
- Hotkey conflict with other apps → make configurable in Phase 6 (B6.2)

---

### Task B6.4 🟡 [S] Theme Management
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ✅ DONE (overlay has dark/light) → extend to dashboard
**Depends on**: D0.2
**Summary**: Sync theme between system preference, dashboard, and overlay. Dashboard uses CSS custom properties with `prefers-color-scheme` media query. On toggle, sends `ThemeChanged` command to overlay via daemon IPC.
**Design source**: CUE-CURRENT-STATE.md §B6.4 (overlay theme already works)
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/hooks/useTheme.ts
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

type Theme = 'light' | 'dark' | 'system';

export function useTheme() {
  const [theme, setTheme] = useState<Theme>('system');

  useEffect(() => {
    const resolved = theme === 'system'
      ? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
      : theme;
    document.documentElement.classList.toggle('dark', resolved === 'dark');
    invoke('set_theme', { theme: resolved }).catch(() => {});
  }, [theme]);

  return { theme, setTheme };
}
```
**Acceptance criteria**:
- Dashboard respects system dark/light preference on launch
- Manual toggle persists across restarts (stored in localStorage)
- Overlay receives `ThemeChanged` command when dashboard theme changes
**Verification commands**:
- Toggle macOS appearance → dashboard follows
- `cargo tauri dev` in dark mode → dark background renders
**Risks + mitigations**:
- None significant

---

### Task B6.5 🟡 [M] rAF Streaming Buffer
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.2
**Summary**: Implement `useStreamBuffer` React hook that coalesces incoming LLM tokens via `requestAnimationFrame` to prevent excessive re-renders. Tokens arrive at up to 60Hz from daemon events; the hook batches them into single state updates per animation frame.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §5 Streaming Response Handling (React side)
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/hooks/useStreamBuffer.ts
import { useRef, useState, useCallback } from 'react';

export function useStreamBuffer() {
  const bufferRef = useRef('');
  const rafRef = useRef<number | null>(null);
  const [displayText, setDisplayText] = useState('');

  const append = useCallback((chunk: string) => {
    bufferRef.current += chunk;
    if (rafRef.current === null) {
      rafRef.current = requestAnimationFrame(() => {
        setDisplayText(prev => prev + bufferRef.current);
        bufferRef.current = '';
        rafRef.current = null;
      });
    }
  }, []);

  const reset = useCallback(() => {
    setDisplayText('');
    bufferRef.current = '';
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
  }, []);

  return { displayText, append, reset };
}
```
**Acceptance criteria**:
- Hook batches 60 token events/sec into ~60 state updates/sec (1 per rAF)
- No dropped tokens: final `displayText` matches concatenation of all inputs
- `reset()` clears buffer and cancels pending rAF
- Unit test: feed 100 tokens synchronously, verify single rAF flush
**Verification commands**:
- `cd crates/cue-dashboard/ui && npm test -- --grep useStreamBuffer`
**Risks + mitigations**:
- React 19 concurrent mode may batch differently → useRef ensures no lost tokens

---

### Task B6.6 🟢 [S] React.memo MessageRow
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B6.3
**Summary**: Create a memoized `MessageRow` component with a custom comparator that only re-renders when content or lane changes. Prevents O(n) re-renders of the message list when new tokens stream in.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §5 (rAF coalescing pattern from NativelyInterface.tsx)
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/components/MessageRow.tsx
import { memo } from 'react';

interface MessageRowProps {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  lane?: 'snap' | 'solve' | 'think';
  isStreaming: boolean;
}

export const MessageRow = memo(function MessageRow({ role, content, lane, isStreaming }: MessageRowProps) {
  return (
    <div className={`px-4 py-3 ${role === 'assistant' ? 'bg-muted/50' : ''}`}>
      {lane && (
        <span className={`text-xs font-mono px-1.5 py-0.5 rounded ${
          lane === 'snap' ? 'bg-green-500/20 text-green-400' :
          lane === 'solve' ? 'bg-blue-500/20 text-blue-400' :
          'bg-purple-500/20 text-purple-400'
        }`}>{lane}</span>
      )}
      <div className="mt-1 text-sm whitespace-pre-wrap">{content}</div>
      {isStreaming && <span className="inline-block w-2 h-4 bg-foreground/60 animate-pulse" />}
    </div>
  );
}, (prev, next) => prev.content === next.content && prev.isStreaming === next.isStreaming);
```
**Acceptance criteria**:
- MessageRow does not re-render when sibling messages update (verified via React DevTools profiler)
- Lane badge renders with correct color per lane
- Streaming cursor animates during active generation
**Verification commands**:
- `npm test -- --grep MessageRow`
**Risks + mitigations**:
- None

---

### Task B7.8 🔴 [S] CSP Configuration
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Configure Content Security Policy in `tauri.conf.json` to restrict the webview. Allow only self-origin scripts, styles, and connections to localhost (daemon IPC). Block all external network from the webview — all cloud API calls go through the daemon.
**Design source**: CUE-DESIGN-04-UX-OPS-DEV.md §B7.8
**Code sketch**:
```json
// In tauri.conf.json → app.security
{
  "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' http://127.0.0.1:57321 ws://127.0.0.1:57321; img-src 'self' data:; font-src 'self'"
}
```
**Acceptance criteria**:
- Webview cannot load external scripts (test: inject `<script src="https://evil.com">` → blocked)
- Webview can connect to localhost:57321 (daemon IPC works)
- Inline styles allowed (needed for dynamic theming)
- No CSP violations in console during normal operation
**Verification commands**:
- `cargo tauri dev` → open devtools → Console shows no CSP errors
- Attempt `fetch('https://httpbin.org/get')` in console → blocked
**Risks + mitigations**:
- `unsafe-inline` for styles is acceptable for Tauri apps (no user-generated content)

---

### Task B7.9 🟢 [S] Error Boundaries
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B6.3
**Summary**: Wrap each route in a React error boundary that catches render errors and displays a recovery UI instead of a white screen. Log errors to daemon via IPC for crash reporting.
**Design source**: CUE-DESIGN-04-UX-OPS-DEV.md §B7.9
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/components/ErrorBoundary.tsx
import { Component, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Props { children: ReactNode; fallback?: ReactNode; }
interface State { error: Error | null; }

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error) { return { error }; }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    invoke('log_error', { message: error.message, stack: info.componentStack }).catch(() => {});
  }

  render() {
    if (this.state.error) {
      return this.props.fallback ?? (
        <div className="p-4 text-center">
          <p className="text-destructive font-medium">Something went wrong</p>
          <button onClick={() => this.setState({ error: null })}
            className="mt-2 text-sm underline">Try again</button>
        </div>
      );
    }
    return this.props.children;
  }
}
```
**Acceptance criteria**:
- Throwing component in /settings doesn't crash /sessions route
- Error boundary shows "Something went wrong" with retry button
- Error logged to daemon (visible in daemon logs)
**Verification commands**:
- Add `throw new Error('test')` in SettingsPage → boundary catches it
- `cargo tauri dev` → navigate to /settings → error UI shows, /sessions still works
**Risks + mitigations**:
- None

---

## Phase deliverable
- User presses Cmd+Shift+D → dashboard window appears with sidebar navigation
- Dashboard shows daemon online/offline status
- Theme follows system preference
- Streaming buffer hook ready for Phase 4 integration
- Codex review should check: CSP blocks external requests, error boundaries on all routes, no console errors


---

# Phase 2 — Session UX (~2 weeks)

## Entry criteria
- Phase 1 complete: dashboard opens via hotkey, sidebar nav works, IPC bridge functional
- SQLite session store (C0.1) operational

## Exit criteria
- User can create, switch, and archive sessions from dashboard
- Messages persist across daemon restarts (SQLite WAL)
- Overlay shows current session title and updates on switch
- Graceful shutdown saves in-flight state

## Batch structure
Ships as 1 PR:
- **PR4**: B6.3.1 + B6.3.2 + B6.3.3 + B5.5 + B10.8

## Tasks (in execution order)

### Task B6.3.1 🔴 [M] Session List Page
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: C0.1, B6.3
**Summary**: Dashboard page showing all sessions sorted by last activity. Each row shows title, lane distribution badge (how many snap/solve/think messages), relative timestamp, and archive button. "New Session" button at top. Fetches data via `invoke('get_sessions')`.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Phase 2 tasks
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/pages/Sessions.tsx
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useNavigate } from 'react-router-dom';

interface Session { id: string; title: string; state: string; updated_at: string; }

export function SessionsPage() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const navigate = useNavigate();

  useEffect(() => {
    invoke<string>('get_sessions').then(json => setSessions(JSON.parse(json)));
  }, []);

  const createSession = async () => {
    const id = await invoke<string>('create_session', { title: 'New Session' });
    navigate(`/session/${id}`);
  };

  return (
    <div className="flex-1 overflow-y-auto p-4">
      <div className="flex justify-between items-center mb-4">
        <h1 className="text-lg font-semibold">Sessions</h1>
        <button onClick={createSession}
          className="px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-md">
          New Session
        </button>
      </div>
      <ul className="space-y-1">
        {sessions.map(s => (
          <li key={s.id} onClick={() => navigate(`/session/${s.id}`)}
            className="flex items-center justify-between px-3 py-2 rounded-md hover:bg-muted cursor-pointer">
            <span className="text-sm font-medium">{s.title}</span>
            <span className="text-xs text-muted-foreground">
              {new Date(s.updated_at).toLocaleDateString()}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
```
**Acceptance criteria**:
- Page loads sessions from SQLite via daemon IPC
- "New Session" creates a session and navigates to detail page
- Sessions sorted by `updated_at` descending
- Empty state shows "No sessions yet" message
**Verification commands**:
- `cargo tauri dev` → Sessions page shows list after creating via CLI
- Create 3 sessions via CLI → all appear in dashboard
**Risks + mitigations**:
- Large session lists → paginate at 50 items (defer to Phase 6)

---

### Task B6.3.2 🔴 [M] Session Detail Page
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B6.3.1
**Summary**: Shows message history for a session with lane badges per message. Uses `MessageRow` component from B6.6. Subscribes to daemon events for live streaming when session is active. Shows lane distribution summary at top.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md (B6.3.2 "Now shows lane badge per message")
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/pages/SessionDetail.tsx
import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { MessageRow } from '../components/MessageRow';
import { useStreamBuffer } from '../hooks/useStreamBuffer';

interface Msg { id: string; role: 'user'|'assistant'; content: string; lane?: string; }

export function SessionDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [messages, setMessages] = useState<Msg[]>([]);
  const { displayText, append, reset } = useStreamBuffer();

  useEffect(() => {
    invoke<string>('get_session_messages', { sessionId: id })
      .then(json => setMessages(JSON.parse(json)));

    const unlisten = listen<string>('daemon-event', (event) => {
      const data = JSON.parse(event.payload);
      if (data.type === 'llm-token' && data.session_id === id) {
        append(data.text);
      } else if (data.type === 'llm-complete' && data.session_id === id) {
        setMessages(prev => [...prev, { id: data.msg_id, role: 'assistant', content: displayText, lane: data.lane }]);
        reset();
      }
    });
    return () => { unlisten.then(fn => fn()); };
  }, [id]);

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto">
        {messages.map(m => (
          <MessageRow key={m.id} id={m.id} role={m.role}
            content={m.content} lane={m.lane as any} isStreaming={false} />
        ))}
        {displayText && (
          <MessageRow id="streaming" role="assistant"
            content={displayText} isStreaming={true} />
        )}
      </div>
    </div>
  );
}
```
**Acceptance criteria**:
- Navigating to `/session/:id` loads message history
- Each message shows lane badge (snap=green, solve=blue, think=purple)
- Live streaming tokens appear in real-time via `useStreamBuffer`
- Scroll-to-bottom on new messages
**Verification commands**:
- Create session, send question via CLI, verify messages appear in dashboard
- `npm test -- --grep SessionDetail`
**Risks + mitigations**:
- Large message histories → virtualize list in Phase 6 (B6.7)

---

### Task B6.3.3 🟡 [S] Session Switching Logic
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: C0.1, D0.4
**Summary**: When user switches sessions in dashboard, daemon updates its active session pointer and sends `SessionChanged` command to overlay. Overlay updates its title display. CLI `sessions switch <id>` also triggers the same flow.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md (B6.3.3 "overlay + dashboard sync")
**Code sketch**:
```rust
// crates/cue-daemon/src/app.rs — new handler
async fn handle_switch_session(&self, session_id: &str) -> anyhow::Result<()> {
    // Validate session exists
    let session = self.session_store.get_session(session_id)?;

    // Update daemon state
    *self.active_session_id.lock().await = Some(session_id.to_string());

    // Notify overlay
    self.overlay_socket.send_command(
        &cue_core::overlay::OverlayCommand::SessionChanged {
            session_id: session_id.to_string(),
            title: session.title.clone(),
        }
    ).await?;

    // Emit event for dashboard
    tracing::info!(session_id, "Switched active session");
    Ok(())
}
```
**Acceptance criteria**:
- Clicking a session in dashboard list makes it active
- Overlay receives `SessionChanged` and updates title display
- CLI `bluey sessions switch <id>` triggers same flow
- Switching session does not lose in-flight streaming (current generation continues)
**Verification commands**:
- Create 2 sessions → switch between them → overlay title updates
- `cargo test -p cue-daemon handle_switch_session`
**Risks + mitigations**:
- Race condition if switching during active generation → generation tagged with session_id at start, not affected by switch

---

### Task B5.5 🟡 [M] InterviewTranscriptBuffer with Persistence
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (conversation history exists, capped at 80 turns, no summarization)
**Depends on**: C0.1
**Summary**: Upgrade the existing `MeetingRecord.conversation` (80-turn cap) to a proper `TranscriptBuffer` that persists to SQLite and supports rolling summarization. Recent 5 Q&A pairs kept verbatim; older pairs compressed to summaries. This is the precursor to CM3 (Phase 5 epoch summarization).
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §5 InterviewTranscriptBuffer
**Code sketch**:
```rust
// crates/cue-daemon/src/transcript_buffer.rs
use std::collections::VecDeque;

pub struct TranscriptBuffer {
    recent_pairs: VecDeque<QAPair>,
    summaries: Vec<String>,
    max_recent: usize,
    session_id: String,
}

#[derive(Clone, Debug)]
pub struct QAPair {
    pub question: String,
    pub answer: String,
    pub timestamp_ms: u64,
}

impl TranscriptBuffer {
    pub fn new(session_id: String) -> Self {
        Self { recent_pairs: VecDeque::new(), summaries: Vec::new(), max_recent: 5, session_id }
    }

    pub fn add_pair(&mut self, pair: QAPair) {
        self.recent_pairs.push_back(pair);
        // Summarization deferred to Phase 5 (CM3) — for now just drop oldest
        while self.recent_pairs.len() > self.max_recent * 2 {
            self.recent_pairs.pop_front();
        }
    }

    /// Build context string for LLM prompt (~850 tokens max)
    pub fn get_context(&self) -> String {
        let mut ctx = String::with_capacity(2048);
        if !self.summaries.is_empty() {
            ctx.push_str("Previous discussion:\n");
            for s in &self.summaries { ctx.push_str(&format!("- {}\n", s)); }
            ctx.push('\n');
        }
        ctx.push_str("Recent exchanges:\n");
        for pair in &self.recent_pairs {
            ctx.push_str(&format!("Q: {}\nA: {}\n\n", pair.question, pair.answer));
        }
        ctx
    }
}
```
**Acceptance criteria**:
- Buffer stores Q&A pairs with timestamps
- `get_context()` returns formatted string <2KB
- Pairs persist to SQLite messages table (via session_id)
- Buffer loads from SQLite on session resume
**Verification commands**:
- `cargo test -p cue-daemon transcript_buffer`
- Add 10 pairs → `get_context()` returns last 10 (summarization deferred)
**Risks + mitigations**:
- No summarization yet → buffer grows unbounded for long sessions → hard cap at 10 pairs until CM3

---

### Task B10.8 🟡 [S] Graceful Shutdown with Session Save
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (shutdown exists, no in-flight tracking)
**Depends on**: C0.1
**Summary**: On SIGTERM/SIGINT, daemon flushes all pending SQLite writes, closes the overlay socket cleanly, and saves the active session's transcript buffer. Track in-flight LLM generations via a counter; wait up to 2s for them to complete before force-exit.
**Design source**: CUE-CURRENT-STATE.md §B10.8 (existing shutdown_daemon)
**Code sketch**:
```rust
// crates/cue-daemon/src/shutdown.rs
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::signal;

static IN_FLIGHT: AtomicU32 = AtomicU32::new(0);

pub fn increment_in_flight() { IN_FLIGHT.fetch_add(1, Ordering::Relaxed); }
pub fn decrement_in_flight() { IN_FLIGHT.fetch_sub(1, Ordering::Relaxed); }

pub async fn graceful_shutdown(
    session_store: &crate::storage::sqlite::SessionStore,
    overlay: &mut crate::overlay_socket::OverlaySocket,
) {
    signal::ctrl_c().await.ok();
    tracing::info!("Shutdown signal received, draining in-flight requests...");

    // Wait up to 2s for in-flight generations
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while IN_FLIGHT.load(Ordering::Relaxed) > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Flush SQLite WAL
    session_store.checkpoint().ok();

    // Close overlay socket
    overlay.send_command(&cue_core::overlay::OverlayCommand::Shutdown).await.ok();

    // Remove socket file
    let _ = std::fs::remove_file("/tmp/bluey-overlay.sock");
    tracing::info!("Shutdown complete");
}
```
**Acceptance criteria**:
- `kill -TERM <daemon_pid>` → daemon waits for in-flight, then exits cleanly
- SQLite WAL is checkpointed (no data loss)
- Socket file removed on clean shutdown
- If in-flight takes >2s, force exit anyway
**Verification commands**:
- Start daemon, send long query, `kill -TERM` → query completes or times out, DB intact
- `cargo test -p cue-daemon shutdown`
**Risks + mitigations**:
- Crash before checkpoint → WAL mode ensures recovery on next open (SQLite handles this)

---

## Phase deliverable
- User can create sessions from dashboard, switch between them, see message history with lane badges
- Overlay reflects current session title
- Data persists across daemon restarts
- Codex review should check: SQLite WAL checkpoint on shutdown, no data loss on kill -9 (WAL recovery), session switch doesn't drop in-flight generation


---

# Phase 3 — Listening Upgrade (~3 weeks)

## Entry criteria
- Phase 0 complete (can run parallel with Phase 1-2)
- Native audio helpers compile (Swift SCK, C WASAPI)
- `cpal` and `webrtc-vad` crates resolve in workspace

## Exit criteria
- Two-stage VAD filters silence before STT billing
- Deepgram Nova-3 streaming WebSocket produces final transcripts
- Audio recovers from device disconnect within 2s
- Speaker ID deferred to Phase 5 if Deepgram diarization sufficient
- `cargo test` passes with mock audio + mock WebSocket tests

## Batch structure
Ships as 3 PRs:
- **PR5**: B2.1 + B2.2 + B2.3 + B2.13 (capture pipeline)
- **PR6**: B2.4 + B2.14 (VAD + supervisor)
- **PR7**: B2.5 + B2.8 + B2.11 (STT providers + state machine + question extractor)

## Tasks (in execution order)

### Task B2.1 🔴 [L] SystemAudioStream Trait
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (native helpers exist as standalone binaries, no Rust trait)
**Depends on**: —
**Summary**: Define a `SystemAudioStream` trait yielding mono f32 samples at device native rate. macOS impl wraps the existing Swift SCK helper via subprocess stdout pipe (interim) with a future pure-Rust cidre path. Windows impl wraps WASAPI helper similarly. The trait enables swapping backends without changing the DSP pipeline.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §1 (pluely SpeakerStream pattern)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/capture.rs
use tokio::sync::mpsc;

#[async_trait::async_trait]
pub trait SystemAudioStream: Send + 'static {
    fn sample_rate(&self) -> u32;
    async fn next_chunk(&mut self) -> Option<Vec<f32>>;
    fn stop(&mut self);
}

#[cfg(target_os = "macos")]
pub struct MacOsSystemAudio {
    rx: mpsc::Receiver<Vec<f32>>,
    rate: u32,
    child: Option<tokio::process::Child>,
}

#[cfg(target_os = "macos")]
impl MacOsSystemAudio {
    pub async fn new() -> anyhow::Result<Self> {
        let mut child = tokio::process::Command::new("cue-audio")
            .args(["--source", "system", "--duration-ms", "0"])
            .stdout(std::process::Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            Self::read_f32_frames(stdout, tx).await;
        });
        Ok(Self { rx, rate: 48_000, child: Some(child) })
    }

    async fn read_f32_frames(
        mut stdout: tokio::process::ChildStdout,
        tx: mpsc::Sender<Vec<f32>>,
    ) {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 3840]; // 960 f32 samples = 20ms at 48kHz
        loop {
            match stdout.read_exact(&mut buf).await {
                Ok(_) => {
                    let samples: Vec<f32> = buf.chunks_exact(4)
                        .map(|c| f32::from_le_bytes([c[0],c[1],c[2],c[3]]))
                        .collect();
                    if tx.send(samples).await.is_err() { break; }
                }
                Err(_) => break,
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[async_trait::async_trait]
impl SystemAudioStream for MacOsSystemAudio {
    fn sample_rate(&self) -> u32 { self.rate }
    async fn next_chunk(&mut self) -> Option<Vec<f32>> { self.rx.recv().await }
    fn stop(&mut self) {
        if let Some(ref mut c) = self.child { let _ = c.start_kill(); }
    }
}
```
**Acceptance criteria**:
- `MacOsSystemAudio::new()` spawns helper and yields f32 chunks
- Chunks arrive at ~50/sec (20ms intervals at 48kHz)
- `stop()` kills child process and channel closes
- Trait is object-safe (`Box<dyn SystemAudioStream>` compiles)
**Verification commands**:
- `cargo test -p cue-daemon audio::capture` (mock test with fake stdout)
- Manual: run with real audio, verify non-zero samples
**Risks + mitigations**:
- Helper binary not found → check PATH, bundle alongside daemon binary in build script

---

### Task B2.2 🟡 [M] CPAL Microphone Capture
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (native helpers do mic, no CPAL in Rust)
**Depends on**: —
**Summary**: CPAL-based microphone capture with stream recreation on every start (fixes silent crash bug from natively-cluely). Atomic sample rate tracking. Error signaling via channel. Yields f32 chunks matching the SystemAudioStream interface.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §2 (CPAL stream recreation pattern)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/microphone.rs
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use tokio::sync::mpsc;

pub struct MicrophoneCapture {
    rx: Option<mpsc::Receiver<Vec<f32>>>,
    sample_rate: Arc<AtomicU32>,
    stream: Option<cpal::Stream>,
}

impl MicrophoneCapture {
    pub fn start(&mut self) -> anyhow::Result<()> {
        // Recreate stream every time (fixes silent crash)
        self.stream = None;
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device"))?;
        let config = device.default_input_config()?;
        self.sample_rate.store(config.sample_rate().0, Ordering::Relaxed);

        let (tx, rx) = mpsc::channel(128);
        self.rx = Some(rx);
        let chunk_size = (config.sample_rate().0 / 50) as usize; // 20ms
        let mut buf = Vec::with_capacity(chunk_size);

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                buf.extend_from_slice(data);
                while buf.len() >= chunk_size {
                    let chunk: Vec<f32> = buf.drain(..chunk_size).collect();
                    let _ = tx.try_send(chunk);
                }
            },
            |err| tracing::error!("CPAL error: {}", err),
            None,
        )?;
        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn sample_rate(&self) -> u32 { self.sample_rate.load(Ordering::Relaxed) }
    pub fn stop(&mut self) { self.stream = None; self.rx = None; }
}
```
**Acceptance criteria**:
- `start()` → `stop()` → `start()` works without panic (stream recreation)
- `sample_rate()` returns actual hardware rate (not hardcoded)
- Chunks arrive via `rx` at expected interval
- Error callback logs but doesn't panic
**Verification commands**:
- `cargo test -p cue-daemon audio::microphone`
- Manual: speak into mic, verify non-silent chunks
**Risks + mitigations**:
- No mic permission on macOS → detect TCC denial, surface as user-facing error

---

### Task B2.3 🟡 [S] Zero-Copy DSP Loop
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (f32→i16 exists but naive)
**Depends on**: B2.1, B2.2
**Summary**: DSP processing loop as a tokio task. Drains audio chunks from capture, converts f32→i16 via bytemuck-compatible path, processes in 20ms frames, feeds VAD, emits speech frames to STT channel. Zero-copy where possible using `bytemuck::cast_slice`.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §3 (DSP loop pattern)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/dsp.rs
use tokio::sync::mpsc;

pub struct AudioFrame {
    pub data: Vec<u8>,       // i16 LE bytes
    pub speech_ended: bool,
}

#[inline]
pub fn f32_to_i16(samples: &[f32], out: &mut Vec<i16>) {
    out.clear();
    out.extend(samples.iter().map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16));
}

pub async fn dsp_loop(
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    vad: &mut crate::audio::vad::TwoStageVad,
    stt_tx: mpsc::Sender<AudioFrame>,
) {
    let mut i16_buf = Vec::with_capacity(960);
    while let Some(chunk) = audio_rx.recv().await {
        f32_to_i16(&chunk, &mut i16_buf);
        let action = vad.process(&i16_buf);
        match action {
            crate::audio::vad::VadAction::Speech(ended) => {
                let bytes = bytemuck::cast_slice::<i16, u8>(&i16_buf).to_vec();
                let _ = stt_tx.send(AudioFrame { data: bytes, speech_ended: ended }).await;
            }
            _ => {}
        }
    }
}
```
**Acceptance criteria**:
- f32→i16 conversion matches expected values (±1 LSB)
- `bytemuck::cast_slice` produces correct LE byte layout
- DSP loop processes chunks without allocation per frame (reuses `i16_buf`)
- Only speech frames reach `stt_tx` (silence suppressed)
**Verification commands**:
- `cargo test -p cue-daemon audio::dsp` — unit test with known f32 input
**Risks + mitigations**:
- Endianness on non-LE platforms → bytemuck handles this; all targets are LE

---

### Task B2.13 🟡 [S] Sample Rate Detection + Rubato Resampler
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (hardcoded 48k→16k by skipping samples)
**Depends on**: B2.1, B2.2
**Summary**: Detect actual device sample rate from capture trait, resample to 16kHz (STT requirement) using rubato crate for high-quality sinc interpolation. Replaces the naive skip-every-3rd-sample approach.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §2 (sample rate detection bug fix from natively-cluely)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/resample.rs
use rubato::{SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};

pub struct Resampler {
    inner: SincFixedIn<f32>,
    input_rate: u32,
}

impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32, chunk_size: usize) -> anyhow::Result<Self> {
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        let ratio = output_rate as f64 / input_rate as f64;
        let resampler = SincFixedIn::new(ratio, 2.0, params, chunk_size, 1)?;
        Ok(Self { inner: resampler, input_rate })
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let input_frames = vec![input.to_vec()];
        match self.inner.process(&input_frames, None) {
            Ok(output) => output.into_iter().next().unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }
}
```
**Acceptance criteria**:
- 48kHz input → 16kHz output (3:1 ratio) with correct sample count
- 44.1kHz input → 16kHz output works (non-integer ratio)
- Output quality: no audible aliasing on speech (manual listen test)
- Resampler created dynamically based on detected rate (not hardcoded)
**Verification commands**:
- `cargo test -p cue-daemon audio::resample` — sine wave test, verify frequency preserved
**Risks + mitigations**:
- rubato adds ~2ms latency per chunk → acceptable for 20ms frame budget


---

### Task B2.4 🔴 [M] Two-Stage VAD
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B2.3
**Summary**: Two-stage voice activity detection: Stage 1 adaptive RMS threshold (fast reject of silence/noise), Stage 2 WebRTC ML VAD confirmation. Hangover FSM preserves trailing consonants. Ported directly from natively-cluely's `silence_suppression.rs` pattern.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §4 (full algorithm + state machine)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/vad.rs
use webrtc_vad::{Vad as WebRtcVad, VadMode};

pub enum VadAction { Speech(bool), Silence, Suppress }

#[derive(PartialEq, Clone, Copy)]
enum State { Active, Hangover, Suppressed }

pub struct TwoStageVad {
    noise_floor: f32,
    threshold_multiplier: f32,
    min_floor: f32,
    webrtc: WebRtcVad,
    state: State,
    hangover_remaining: u32,
    hangover_frames: u32,
    was_speaking: bool,
    silence_count: u32,
}

impl TwoStageVad {
    pub fn new(mode: VadMode) -> Self {
        let mut webrtc = WebRtcVad::new();
        webrtc.set_mode(mode);
        Self {
            noise_floor: 20.0, threshold_multiplier: 2.5, min_floor: 20.0,
            webrtc, state: State::Suppressed,
            hangover_remaining: 0, hangover_frames: 15,
            was_speaking: false, silence_count: 0,
        }
    }

    pub fn process(&mut self, samples: &[i16]) -> VadAction {
        let rms = (samples.iter().map(|&s| (s as f64).powi(2)).sum::<f64>()
            / samples.len() as f64).sqrt() as f32;
        let threshold = (self.noise_floor * self.threshold_multiplier).max(self.min_floor);

        if self.state == State::Suppressed {
            self.noise_floor = self.noise_floor * 0.98 + rms * 0.02;
        }

        let rms_pass = rms > threshold;
        let ml_pass = rms_pass && self.webrtc.is_voice_segment(samples).unwrap_or(false);
        let mut speech_ended = false;

        match self.state {
            State::Suppressed if ml_pass => { self.state = State::Active; self.was_speaking = true; }
            State::Active if !ml_pass => { self.state = State::Hangover; self.hangover_remaining = self.hangover_frames; }
            State::Hangover if ml_pass => { self.state = State::Active; }
            State::Hangover => {
                self.hangover_remaining = self.hangover_remaining.saturating_sub(1);
                if self.hangover_remaining == 0 {
                    self.state = State::Suppressed;
                    if self.was_speaking { speech_ended = true; self.was_speaking = false; }
                }
            }
            _ => {}
        }

        match self.state {
            State::Active | State::Hangover => VadAction::Speech(speech_ended),
            State::Suppressed => {
                self.silence_count += 1;
                if self.silence_count >= 5 { self.silence_count = 0; VadAction::Silence }
                else { VadAction::Suppress }
            }
        }
    }
}
```
**Acceptance criteria**:
- Silence input → `Suppress` (no STT billing)
- Speech input → `Speech(false)` during, `Speech(true)` on end
- Hangover preserves 300ms trailing audio after speech stops
- Noise floor adapts: loud environment raises threshold
- Unit test with synthetic speech/silence patterns
**Verification commands**:
- `cargo test -p cue-daemon audio::vad`
**Risks + mitigations**:
- WebRTC VAD requires 16kHz input → resample before VAD (B2.13 provides this)

---

### Task B2.5 🔴 [L] SttProvider Trait + Deepgram Nova-3 WS
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (only OpenAI Whisper REST exists)
**Depends on**: B2.3
**Summary**: Define `SttProvider` async trait with `write()`, `start()`, `stop()`, `state()`. Implement Deepgram Nova-3 as primary provider using persistent WebSocket (binary frames in, JSON transcripts out). Add OpenAI REST as fallback. The WebSocket stays open for the entire session (L1 lifecycle managed here).
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §5 (trait + Deepgram impl)
**Code sketch**:
```rust
// crates/cue-daemon/src/stt/mod.rs
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct Transcript { pub text: String, pub is_final: bool, pub timestamp_ms: u64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SttState { Connected, Reconnecting, Failed }

#[async_trait::async_trait]
pub trait SttProvider: Send + Sync {
    async fn write(&self, audio: &[u8]) -> anyhow::Result<()>;
    async fn start(&mut self, sample_rate: u32, language: &str) -> anyhow::Result<()>;
    async fn stop(&mut self) -> anyhow::Result<()>;
    fn state(&self) -> SttState;
    fn name(&self) -> &'static str;
}

// crates/cue-daemon/src/stt/deepgram.rs
pub struct DeepgramStt {
    tx: mpsc::Sender<Transcript>,
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,
    state: std::sync::Arc<std::sync::atomic::AtomicU8>,
}

#[async_trait::async_trait]
impl SttProvider for DeepgramStt {
    async fn write(&self, audio: &[u8]) -> anyhow::Result<()> {
        if let Some(ref tx) = self.audio_tx {
            tx.send(audio.to_vec()).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
        }
        Ok(())
    }

    async fn start(&mut self, sample_rate: u32, language: &str) -> anyhow::Result<()> {
        let url = format!(
            "wss://api.deepgram.com/v1/listen?model=nova-3&encoding=linear16&sample_rate={}&channels=1&language={}&punctuate=true&interim_results=true",
            sample_rate, language
        );
        // Connect WebSocket, spawn read/write tasks
        // (full impl follows design doc pattern)
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(64);
        self.audio_tx = Some(audio_tx);
        self.state.store(0, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        self.audio_tx = None;
        Ok(())
    }

    fn state(&self) -> SttState {
        match self.state.load(std::sync::atomic::Ordering::Relaxed) {
            0 => SttState::Connected, 1 => SttState::Reconnecting, _ => SttState::Failed,
        }
    }
    fn name(&self) -> &'static str { "deepgram" }
}
```
**Acceptance criteria**:
- Deepgram WS connects with valid API key and receives transcripts
- Partial (interim) transcripts arrive within 200ms of speech
- Final transcripts arrive within 500ms of speech end
- WebSocket stays open between utterances (persistent per session)
- Graceful reconnect on WS drop (state → Reconnecting → Connected)
**Verification commands**:
- `cargo test -p cue-daemon stt::deepgram` (mock WS server)
- Manual: speak → see transcripts in daemon logs
**Risks + mitigations**:
- Deepgram rate limits → L1 (Phase 9) handles lifecycle; here we just implement connect/write/read

---

### Task B2.8 🟡 [S] STT State Machine + Backoff
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (minimal error handling exists)
**Depends on**: B2.5
**Summary**: Classified error handling for STT: auth errors (fatal), quota errors (fatal), transient errors (retry with exponential backoff). State machine broadcasts transitions. Max 5 retries before marking failed.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §6 (error classification + state machine)
**Code sketch**:
```rust
// crates/cue-daemon/src/stt/state_machine.rs
pub struct SttStateMachine {
    state: SttState,
    errors: u32,
    backoff_ms: u64,
}

impl SttStateMachine {
    pub fn new() -> Self { Self { state: SttState::Connected, errors: 0, backoff_ms: 1000 } }

    pub fn on_error(&mut self, status: Option<u16>, msg: &str) -> SttState {
        match status {
            Some(401) | Some(403) => { self.state = SttState::Failed; }
            Some(402) => { self.state = SttState::Failed; }
            _ => {
                self.errors += 1;
                if self.errors >= 5 { self.state = SttState::Failed; }
                else {
                    self.state = SttState::Reconnecting;
                    self.backoff_ms = (self.backoff_ms * 2).min(30_000);
                }
            }
        }
        self.state
    }

    pub fn on_success(&mut self) { self.state = SttState::Connected; self.errors = 0; self.backoff_ms = 1000; }
    pub fn backoff(&self) -> std::time::Duration { std::time::Duration::from_millis(self.backoff_ms) }
}
```
**Acceptance criteria**:
- Auth error → immediate Failed state (no retry)
- Transient error → Reconnecting with doubling backoff (1s, 2s, 4s, 8s, 16s)
- 5th transient error → Failed
- Successful transcript resets error counter
**Verification commands**:
- `cargo test -p cue-daemon stt::state_machine`
**Risks + mitigations**:
- None — pure state logic, well-tested

---

### Task B2.11 🟡 [S] Question Extractor + Noise Filter
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (basic `is_question()` exists in cue-core)
**Depends on**: —
**Summary**: Upgrade the basic question detector to filter noise (greetings, filler, too-short utterances) and detect coding questions. Output feeds the intent classifier (R1) in Phase 4. Heuristic-based, no ML.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §B2.11, CUE-REFERENCE-ANALYSIS.md pattern #36
**Code sketch**:
```rust
// crates/cue-core/src/intelligence.rs — upgrade existing
#[derive(Debug, PartialEq)]
pub enum QuestionType { General, Coding, Behavioral, Noise }

pub fn classify_utterance(text: &str) -> QuestionType {
    let trimmed = text.trim();
    if trimmed.len() < 10 { return QuestionType::Noise; }

    let lower = trimmed.to_lowercase();
    // Noise filter
    let noise_patterns = ["hello", "hi there", "thank you", "thanks", "okay", "um", "uh"];
    if noise_patterns.iter().any(|p| lower == *p || lower.starts_with(p) && lower.len() < 20) {
        return QuestionType::Noise;
    }

    // Coding detection
    let code_signals = ["implement", "algorithm", "time complexity", "data structure",
        "function", "leetcode", "binary tree", "linked list", "array"];
    if code_signals.iter().any(|s| lower.contains(s)) {
        return QuestionType::Coding;
    }

    // Behavioral detection
    let behavioral = ["tell me about a time", "describe a situation", "give an example",
        "what would you do", "how did you handle"];
    if behavioral.iter().any(|s| lower.contains(s)) {
        return QuestionType::Behavioral;
    }

    QuestionType::General
}
```
**Acceptance criteria**:
- "um" → Noise, "hello" → Noise
- "implement a binary tree" → Coding
- "tell me about a time you failed" → Behavioral
- "what's your approach to system design?" → General
- ≥10 unit tests covering edge cases
**Verification commands**:
- `cargo test -p cue-core intelligence::classify`
**Risks + mitigations**:
- Heuristic accuracy ~70% → R1 (Phase 4) adds embedding-based classification on top

---

### Task B2.14 🟡 [M] Audio Supervisor + Recovery
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B2.1, B2.2
**Summary**: Supervisor task that monitors audio capture health. Detects: device disconnect (no samples for 2s), error signals from CPAL, sleep/wake events. On failure: stops capture, waits 1s, restarts. Emits status events for overlay/dashboard.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §B2.14, CUE-REFERENCE-ANALYSIS.md pattern #27
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/supervisor.rs
use tokio::time::{interval, Duration, Instant};

pub struct AudioSupervisor {
    last_sample_time: Instant,
    restart_count: u32,
    max_restarts: u32,
}

impl AudioSupervisor {
    pub fn new() -> Self {
        Self { last_sample_time: Instant::now(), restart_count: 0, max_restarts: 10 }
    }

    pub fn on_samples_received(&mut self) { self.last_sample_time = Instant::now(); }

    pub fn check_health(&self) -> AudioHealth {
        if self.last_sample_time.elapsed() > Duration::from_secs(2) {
            AudioHealth::DeviceDisconnected
        } else {
            AudioHealth::Healthy
        }
    }

    pub fn on_restart(&mut self) -> bool {
        self.restart_count += 1;
        self.restart_count <= self.max_restarts
    }
}

pub enum AudioHealth { Healthy, DeviceDisconnected, ErrorSignaled(String) }
```
**Acceptance criteria**:
- No samples for 2s → triggers restart
- Restart succeeds and audio resumes within 3s total
- After 10 consecutive restarts, marks as permanently failed
- Sleep/wake: on wake, forces restart regardless of health
**Verification commands**:
- `cargo test -p cue-daemon audio::supervisor`
- Manual: unplug headphones → audio recovers when re-plugged
**Risks + mitigations**:
- Infinite restart loop → max_restarts cap prevents this

---

## Phase deliverable
- Audio pipeline captures system + mic, resamples to 16kHz, filters via two-stage VAD
- Deepgram Nova-3 produces streaming transcripts with <500ms latency
- Audio recovers from device disconnect automatically
- Question extractor classifies utterances for downstream routing
- Codex review should check: VAD actually reduces STT API calls (log speech vs suppress ratio), WebSocket stays open between utterances, no audio data leaks to logs


---

# Phase 4 — Reasoning Upgrade: Three-Lane Router + Patch-Mode (~5 weeks)

## Entry criteria
- Phase 0 complete (IPC, SQLite, Unix socket)
- B2.5 done (STT produces transcripts)
- B2.11 done (question extractor feeds intent classifier)

## Exit criteria
- Three-lane routing dispatches to Snap/Solve/Think based on intent
- Streaming with cancel-on-upgrade semantics works
- Patch-mode PATCH/KEEP/MODIFY/ADD/REMOVE parser produces structured diffs
- Overlay renders diff highlights for follow-up responses
- Progressive enhancement: Snap answer shows immediately, replaced by Solve if better
- Manual overrides (Alt+D, Alt+F, /think, /snap) work

## Batch structure
Ships as 3 PRs:
- **PR8** (Batch 4A, weeks 1-2): B3.1 + B3.2 + B3.3 + B3.4 + B3.5 + B3.9 + B3.12
- **PR9** (Batch 4B, weeks 2-3): B4.1 + B4.2 + B4.6 + B4.8 + R1 + R2 + R3 + R4 + R5
- **PR10** (Batch 4C, weeks 3-5): F1 + F2 + F3 + F4 + F5 + F6 + F7 + F8 + B3.11

## Tasks (in execution order)

---

## Batch 4A — LLM Foundation (weeks 1-2)

### Task B3.1 🔴 [L] Multi-Provider LLM Trait (Three-Lane Aware)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (routing exists as match+loop, no trait)
**Depends on**: —
**Summary**: Define `LlmProvider` async trait with `stream()` and `generate()` methods. Implement for Anthropic (Claude Sonnet 4.5 / Opus), OpenAI-compatible (Cerebras, Groq, OpenAI), and o3. Each provider tagged with supported lanes. The router dispatches based on lane assignment, not provider name.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §1 Provider Router Design
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/mod.rs
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane { Snap, Solve, Think }

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub messages: Vec<ChatMessage>,
    pub lane: Lane,
    pub max_tokens: u32,
    pub temperature: f32,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct ChatMessage { pub role: String, pub content: String }

#[derive(Debug, Clone)]
pub struct Token { pub text: String, pub done: bool }

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supported_lanes(&self) -> &[Lane];
    async fn stream(
        &self, request: &LlmRequest, cancel: CancellationToken,
    ) -> anyhow::Result<mpsc::Receiver<Token>>;
}

// crates/cue-daemon/src/llm/cerebras.rs
pub struct CerebrasProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

#[async_trait::async_trait]
impl LlmProvider for CerebrasProvider {
    fn name(&self) -> &str { "cerebras" }
    fn supported_lanes(&self) -> &[Lane] { &[Lane::Snap] }

    async fn stream(&self, request: &LlmRequest, cancel: CancellationToken,
    ) -> anyhow::Result<mpsc::Receiver<Token>> {
        let (tx, rx) = mpsc::channel(64);
        let client = self.client.clone();
        let url = "https://api.cerebras.ai/v1/chat/completions";
        let body = serde_json::json!({
            "model": &self.model,
            "messages": request.messages.iter().map(|m| serde_json::json!({"role": &m.role, "content": &m.content})).collect::<Vec<_>>(),
            "max_tokens": request.max_tokens,
            "stream": true,
        });
        let api_key = self.api_key.clone();

        tokio::spawn(async move {
            let resp = client.post(url)
                .bearer_auth(&api_key)
                .json(&body)
                .send().await;
            if let Ok(resp) = resp {
                let mut stream = resp.bytes_stream();
                use futures::StreamExt;
                while let Some(Ok(chunk)) = stream.next().await {
                    if cancel.is_cancelled() { break; }
                    // Parse SSE lines, extract content delta
                    let text = String::from_utf8_lossy(&chunk);
                    for line in text.lines().filter(|l| l.starts_with("data: ")) {
                        let data = &line[6..];
                        if data == "[DONE]" { let _ = tx.send(Token { text: String::new(), done: true }).await; return; }
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                                let _ = tx.send(Token { text: delta.to_string(), done: false }).await;
                            }
                        }
                    }
                }
            }
        });
        Ok(rx)
    }
}
```
**Acceptance criteria**:
- Trait is object-safe: `Box<dyn LlmProvider>` compiles
- CerebrasProvider streams tokens from Cerebras API (Snap lane)
- AnthropicProvider streams from Claude API (Solve/Think lanes)
- CancellationToken stops generation mid-stream
- 3 unit tests with mock HTTP responses
**Verification commands**:
- `cargo test -p cue-daemon llm` — mock provider tests
- Manual: set CEREBRAS_API_KEY, send query, see streaming tokens
**Risks + mitigations**:
- SSE parsing edge cases → use `eventsource-stream` crate for robust parsing

---

### Task B3.2 🟡 [M] ModelVersionManager
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B3.1
**Summary**: Background task that polls provider `/models` endpoints hourly to discover available models. Maintains a capability map (vision, streaming, JSON mode, thinking, max context). Used by router to select best model per lane.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §3 ModelVersionManager
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/model_versions.rs
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct ModelVersionManager {
    models: RwLock<HashMap<String, ModelInfo>>,
}

#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    pub max_context: u32,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub supports_thinking: bool,
}

impl ModelVersionManager {
    pub async fn refresh(&self, providers: &[Box<dyn super::LlmProvider>]) {
        // Poll each provider's model list endpoint
        // Update internal map
    }

    pub fn best_model_for_lane(&self, lane: super::Lane) -> Option<ModelInfo> {
        let models = self.models.blocking_read();
        match lane {
            super::Lane::Snap => models.values().find(|m| m.provider == "cerebras").cloned(),
            super::Lane::Solve => models.values().find(|m| m.id.contains("sonnet")).cloned(),
            super::Lane::Think => models.values().find(|m| m.supports_thinking).cloned(),
        }
    }
}
```
**Acceptance criteria**:
- Polls every hour (configurable)
- Returns correct model for each lane
- Handles API failures gracefully (keeps stale data)
**Verification commands**:
- `cargo test -p cue-daemon llm::model_versions`
**Risks + mitigations**:
- Provider API changes → hardcoded fallback model IDs as defaults

---

### Task B3.3 🟡 [M] Fallback Chains with Per-Lane Backoff
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (fallback iteration exists, no backoff)
**Depends on**: B3.1
**Summary**: Per-lane fallback chains. Snap: Cerebras → Groq → local. Solve: Claude → GPT-4o → Gemini. Think: o3 → Claude Opus. Exponential backoff per provider (30s base, 600s cap). Failed providers auto-recover when backoff expires.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §2 Fallback Chain Strategy
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/fallback.rs
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct FallbackChain {
    chains: HashMap<super::Lane, Vec<String>>,
    failures: HashMap<String, (u32, Instant)>,
}

impl FallbackChain {
    pub fn available_for_lane(&self, lane: super::Lane) -> Vec<&str> {
        let now = Instant::now();
        self.chains.get(&lane).map(|chain| {
            chain.iter().filter(|p| {
                match self.failures.get(*p) {
                    None => true,
                    Some((count, last)) => now.duration_since(*last) >= Self::backoff(*count),
                }
            }).map(|s| s.as_str()).collect()
        }).unwrap_or_default()
    }

    fn backoff(failures: u32) -> Duration {
        Duration::from_secs((30 * 2u64.pow(failures.saturating_sub(1))).min(600))
    }

    pub fn mark_failure(&mut self, provider: &str) {
        let entry = self.failures.entry(provider.to_string()).or_insert((0, Instant::now()));
        entry.0 += 1; entry.1 = Instant::now();
    }

    pub fn mark_success(&mut self, provider: &str) { self.failures.remove(provider); }
}
```
**Acceptance criteria**:
- First failure → 30s backoff, second → 60s, capped at 600s
- Provider auto-recovers after backoff expires
- Each lane has independent chain (Snap failure doesn't affect Solve)
**Verification commands**:
- `cargo test -p cue-daemon llm::fallback`
**Risks + mitigations**:
- All providers down → surface error to user with "check API keys" message

---

### Task B3.4 🔴 [S] Rate Limiters (Per-Lane)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B3.1
**Summary**: Token-bucket rate limiters via `governor` crate. Per-provider limits matching API quotas. Snap lane (Cerebras): 60 RPM. Solve lane (Claude): 50 RPM. Think lane (o3): 20 RPM. Limiter checked before dispatch; if exhausted, falls to next in chain.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §4 Rate Limiters
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/rate_limit.rs
use governor::{Quota, RateLimiter, clock::DefaultClock, state::InMemoryState, state::NotKeyed};
use std::num::NonZeroU32;

pub type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

pub fn create_limiters() -> std::collections::HashMap<String, Limiter> {
    let mut m = std::collections::HashMap::new();
    m.insert("cerebras".into(), RateLimiter::direct(Quota::per_minute(NonZeroU32::new(60).unwrap())));
    m.insert("anthropic".into(), RateLimiter::direct(Quota::per_minute(NonZeroU32::new(50).unwrap())));
    m.insert("openai".into(), RateLimiter::direct(Quota::per_minute(NonZeroU32::new(500).unwrap())));
    m.insert("groq".into(), RateLimiter::direct(Quota::per_minute(NonZeroU32::new(30).unwrap())));
    m
}
```
**Acceptance criteria**:
- Exceeding rate limit returns `Err` (caller falls to next provider)
- Limits are per-provider, not per-lane
- No blocking: `check()` is non-blocking, returns immediately
**Verification commands**:
- `cargo test -p cue-daemon llm::rate_limit`
**Risks + mitigations**:
- None — governor is well-tested

---

### Task B3.5 🔴 [M] Streaming 60Hz + Cancel-on-Upgrade
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (SSE streaming exists, no batching or cancellation)
**Depends on**: B3.1, D0.2
**Summary**: Stream emitter batches tokens at 60Hz before sending to overlay (Unix socket) and dashboard (Tauri event). CancellationToken per generation. Lane upgrade (Snap→Solve) cancels the Snap generation and drops its answer — the Solve answer replaces it entirely (no anchoring on wrong answer).
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §5 StreamEmitter, Skeleton "Lane upgrades drop previous"
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/stream_emitter.rs
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;

pub struct StreamEmitter {
    overlay_tx: mpsc::Sender<String>,
    generation_id: u64,
}

impl StreamEmitter {
    pub async fn run(
        &self,
        mut token_rx: mpsc::Receiver<super::Token>,
        cancel: CancellationToken,
    ) -> Option<String> {
        let mut buffer = String::new();
        let mut full_response = String::new();
        let mut tick = interval(Duration::from_millis(16)); // 60Hz

        loop {
            tokio::select! {
                _ = cancel.cancelled() => { return None; } // Dropped on upgrade
                _ = tick.tick() => {
                    if !buffer.is_empty() {
                        let chunk = std::mem::take(&mut buffer);
                        let _ = self.overlay_tx.send(chunk).await;
                    }
                }
                token = token_rx.recv() => {
                    match token {
                        Some(t) if t.done => {
                            if !buffer.is_empty() {
                                let _ = self.overlay_tx.send(std::mem::take(&mut buffer)).await;
                            }
                            return Some(full_response);
                        }
                        Some(t) => { buffer.push_str(&t.text); full_response.push_str(&t.text); }
                        None => return Some(full_response),
                    }
                }
            }
        }
    }
}
```
**Acceptance criteria**:
- Tokens batched: 60 individual tokens/sec → ~60 IPC messages/sec (not 1:1)
- Cancel mid-stream: `cancel.cancel()` → `run()` returns `None`
- Lane upgrade scenario: Snap starts, Solve dispatched, Snap cancelled, Solve replaces
- Full response returned on completion for persistence
**Verification commands**:
- `cargo test -p cue-daemon llm::stream_emitter`
**Risks + mitigations**:
- Token loss on cancel → acceptable (we're dropping the answer intentionally)

---

### Task B3.9 🟡 [S] Key Scrubbing (zeroize)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: —
**Summary**: Wrap all API keys in a `SecretString` type that uses `zeroize` crate to clear memory on drop. Prevents keys from lingering in memory after use or on crash.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §9 scrubKeys
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/credentials.rs
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, ZeroizeOnDrop)]
pub struct SecretString(#[zeroize] String);

impl SecretString {
    pub fn new(s: String) -> Self { Self(s) }
    pub fn expose(&self) -> &str { &self.0 }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretString(***)")
    }
}
```
**Acceptance criteria**:
- `SecretString` zeroes memory on drop (verified via `zeroize` guarantee)
- Debug output never shows key value
- All provider constructors accept `SecretString`, not raw `String`
**Verification commands**:
- `cargo test -p cue-daemon llm::credentials`
**Risks + mitigations**:
- Compiler optimizations may elide zeroing → `zeroize` uses volatile writes

---

### Task B3.12 🟡 [M] Vision Fallback + Parallel Race
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (vision fallback exists, no parallel race)
**Depends on**: B3.1
**Summary**: 3-tier vision fallback (current model → Gemini Flash → Groq Llama 4 Scout). Parallel race for Gemini: dispatch to both Flash and Pro simultaneously, use first response. Cancels loser.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §12 Parallel Race
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/vision.rs
impl super::LlmRouter {
    pub async fn generate_with_vision(&self, request: &super::LlmRequest, image_b64: &str) -> anyhow::Result<String> {
        let tiers = [
            vec!["claude-sonnet-4-5"],
            vec!["gemini-2.5-flash", "gemini-2.5-pro"], // parallel race
            vec!["groq-llama-4-scout"],
        ];
        for tier in &tiers {
            if tier.len() > 1 {
                // Parallel race: first success wins
                let futs: Vec<_> = tier.iter().map(|m| self.try_vision(request, image_b64, m)).collect();
                if let Ok((result, _)) = futures::future::select_ok(futs.into_iter().map(Box::pin)).await {
                    return Ok(result);
                }
            } else if let Ok(result) = self.try_vision(request, image_b64, tier[0]).await {
                return Ok(result);
            }
        }
        Err(anyhow::anyhow!("All vision providers exhausted"))
    }
}
```
**Acceptance criteria**:
- Vision request tries tiers in order
- Parallel tier races two models, uses first response
- Loser cancelled via CancellationToken
- Falls through all tiers before returning error
**Verification commands**:
- `cargo test -p cue-daemon llm::vision` (mock providers)
**Risks + mitigations**:
- Double billing on parallel race → acceptable (cost is not a concern per spec)


---

## Batch 4B — Prompt + Routing (weeks 2-3)

### Task B4.1 🔴 [M] Prompt Composition System (Per-Lane Templates)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (hardcoded system prompt string)
**Depends on**: —
**Summary**: Composable prompt system with XML-tagged shared blocks. Each lane gets a different composition: Snap gets TINY (<500 tokens), Solve gets full composition + skill template, Think gets full + extended-thinking preamble. Blocks: CORE_IDENTITY, EXECUTION_CONTRACT, CONTEXT_LAYER, CODING_RULES, ANTI_CHATBOT.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md Part 2 §1 Composition Pattern
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/composer.rs
use super::Lane;

pub struct PromptComposer {
    pub core_identity: &'static str,
    pub execution_contract: &'static str,
    pub context_layer: &'static str,
    pub coding_rules: &'static str,
    pub anti_chatbot: &'static str,
}

impl PromptComposer {
    pub fn compose(&self, lane: Lane, skill: Option<&str>, context: &str) -> String {
        match lane {
            Lane::Snap => format!(
                "Interview copilot. Output=user's exact words. 2-4 sentences. First person. No meta. STOP.\n\n{}",
                context
            ),
            Lane::Solve => format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n\n<context>\n{}\n</context>",
                self.core_identity, self.execution_contract, self.context_layer,
                self.coding_rules, self.anti_chatbot,
                skill.unwrap_or(""), context
            ),
            Lane::Think => format!(
                "<thinking-mode>\nYou have extended thinking enabled. Use it for complex reasoning.\nShow your work step by step before giving the final answer.\n</thinking-mode>\n\n{}\n{}\n{}\n\n<context>\n{}\n</context>",
                self.core_identity, self.execution_contract, self.context_layer, context
            ),
        }
    }
}
```
**Acceptance criteria**:
- Snap prompt <500 tokens (measured via len/4 estimate)
- Solve prompt includes all 5 blocks + skill template
- Think prompt includes thinking-mode preamble
- Context injected into all variants
**Verification commands**:
- `cargo test -p cue-daemon prompts::composer` — token count assertions
**Risks + mitigations**:
- Prompt too long for Snap models → hard cap at 500 tokens, truncate context

---

### Task B4.2 🟡 [M] Answer Modes → Lane Mapping
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (5 modes exist with different naming)
**Depends on**: B4.1
**Summary**: Map the three answer modes (Assist/Answer/WhatToAnswer) to lane-appropriate prompt variants. Assist mode uses Solve lane (screenshot analysis). Answer mode uses router decision. WhatToAnswer uses Solve with first-person enforcement. This replaces the old 5-mode system.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md Part 2 §2 Three Modes
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/modes.rs
#[derive(Debug, Clone, Copy)]
pub enum AnswerMode { Assist, Answer, WhatToAnswer }

impl AnswerMode {
    pub fn default_lane(&self) -> super::Lane {
        match self {
            Self::Assist => super::Lane::Solve,
            Self::Answer => super::Lane::Solve, // overridden by router
            Self::WhatToAnswer => super::Lane::Solve,
        }
    }

    pub fn mode_suffix(&self) -> &'static str {
        match self {
            Self::Assist => "<mode>PASSIVE OBSERVER analyzing screenshots. Solve completely or say unsure.</mode>",
            Self::Answer => "<mode>ACTIVE CO-PILOT. Priority: answer > define > advance.</mode>",
            Self::WhatToAnswer => "<mode>Generate EXACT TEXT user will speak. First person. No wrapper.</mode>",
        }
    }
}
```
**Acceptance criteria**:
- Each mode maps to a default lane
- Mode suffix appended to composed prompt
- Router can override default lane (Answer mode respects R1 decision)
**Verification commands**:
- `cargo test -p cue-daemon prompts::modes`
**Risks + mitigations**:
- None

---

### Task B4.6 🟡 [S] Anti-Chatbot Constraints
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (basic brevity instruction exists)
**Depends on**: B4.1
**Summary**: Explicit negative constraints preventing AI-like preambles, coaching language, sign-offs. Embedded in EXECUTION_CONTRACT block. Includes HUMAN ANSWER LENGTH RULE (2-4 sentences, speakable in 30s).
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md Part 2 §6-7 Anti-Chatbot + Length Rule
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/blocks.rs
pub const ANTI_CHATBOT: &str = r#"<forbidden-patterns>
NEVER output: "That's a great question!" / "I'd be happy to help" / "Would you like me to elaborate?" / "Here's what you could say:" / Any meta-commentary / Any coaching preamble / Any sign-off / Any AI acknowledgment
</forbidden-patterns>
<answer-length>
Maximum 2-4 sentences. Speakable in under 30 seconds. STOP after answer.
Code blocks don't count toward sentence limit. Verbal explanation: still 2-4 sentences max.
</answer-length>"#;
```
**Acceptance criteria**:
- Block is included in Solve and Think prompts (not Snap — too short)
- Forbidden patterns list has ≥8 entries
- Length rule specifies 30-second speakability constraint
**Verification commands**:
- `cargo test -p cue-daemon prompts::blocks` — assert ANTI_CHATBOT contains key phrases
**Risks + mitigations**:
- Models may still violate → post-processing strip in Phase 6

---

### Task B4.8 🟡 [S] Context Prioritization (Per-Lane Budgets)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (per-kind character limits exist)
**Depends on**: B4.1
**Summary**: Per-lane token budgets for context injection. Snap: 2K total (last 3 turns only). Solve: 16K input (relevant turns via future RAG). Think: 32K input (full history + epoch summaries). Truncation strategy: newest-first for Snap, relevance-ranked for Solve, chronological for Think.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 (Three-Lane Routing Spec token budgets)
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/context_budget.rs
use super::Lane;

pub struct ContextBudget {
    pub max_input_tokens: u32,
    pub max_output_tokens: u32,
    pub strategy: ContextStrategy,
}

pub enum ContextStrategy { LastNTurns(usize), RelevanceRanked, FullChronological }

pub fn budget_for_lane(lane: Lane) -> ContextBudget {
    match lane {
        Lane::Snap => ContextBudget { max_input_tokens: 2_000, max_output_tokens: 1_000, strategy: ContextStrategy::LastNTurns(3) },
        Lane::Solve => ContextBudget { max_input_tokens: 16_000, max_output_tokens: 4_000, strategy: ContextStrategy::RelevanceRanked },
        Lane::Think => ContextBudget { max_input_tokens: 32_000, max_output_tokens: 8_000, strategy: ContextStrategy::FullChronological },
    }
}
```
**Acceptance criteria**:
- Snap context never exceeds 2K tokens (hard truncation)
- Solve context uses relevance ranking (placeholder until RAG in Phase 5)
- Think context includes full history up to 32K
**Verification commands**:
- `cargo test -p cue-daemon prompts::context_budget`
**Risks + mitigations**:
- Relevance ranking not available until Phase 5 → fall back to LastNTurns(10) for Solve initially

---

### Task R1 🔴 [L] Intent Classifier (<5ms)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B2.11, B3.1
**Summary**: Rule-based intent classifier that routes to Snap/Solve/Think in <5ms. Uses question type from B2.11, utterance length, keyword signals, and explicit user overrides. No ML model (decision: rule-based only for zero latency). Confidence score determines routing.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 Router Decision Flow
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/classifier.rs
use crate::llm::Lane;

pub struct ClassifierResult { pub lane: Lane, pub confidence: f32 }

pub fn classify_intent(
    text: &str,
    question_type: cue_core::intelligence::QuestionType,
    explicit_override: Option<Lane>,
) -> ClassifierResult {
    // Explicit override always wins
    if let Some(lane) = explicit_override {
        return ClassifierResult { lane, confidence: 1.0 };
    }

    let lower = text.to_lowercase();
    let word_count = text.split_whitespace().count();

    // Think signals (high confidence)
    let think_signals = ["think harder", "explain in detail", "step by step", "prove", "analyze deeply"];
    if think_signals.iter().any(|s| lower.contains(s)) {
        return ClassifierResult { lane: Lane::Think, confidence: 0.9 };
    }

    // Snap signals: short, conversational, follow-ups
    if word_count < 8 && matches!(question_type, cue_core::intelligence::QuestionType::General | cue_core::intelligence::QuestionType::Noise) {
        return ClassifierResult { lane: Lane::Snap, confidence: 0.85 };
    }

    // Coding/behavioral → Solve
    if matches!(question_type, cue_core::intelligence::QuestionType::Coding | cue_core::intelligence::QuestionType::Behavioral) {
        return ClassifierResult { lane: Lane::Solve, confidence: 0.8 };
    }

    // Default: Solve for medium-length, Snap for short
    if word_count > 15 {
        ClassifierResult { lane: Lane::Solve, confidence: 0.7 }
    } else {
        ClassifierResult { lane: Lane::Snap, confidence: 0.75 }
    }
}
```
**Acceptance criteria**:
- Classification completes in <1ms (no I/O, pure logic)
- "what's 2+2" → Snap (short, simple)
- "implement a binary search tree with delete" → Solve (coding)
- "think harder about this" → Think (explicit signal)
- Explicit override (/think, /snap, Alt+D, Alt+F) → confidence 1.0
- ≥15 unit tests covering all paths
**Verification commands**:
- `cargo test -p cue-daemon routing::classifier`
- Benchmark: `cargo bench` shows <1ms p99
**Risks + mitigations**:
- Rule-based accuracy ~75% → acceptable for V3; embedding classifier deferred to future

---

### Task R2 🔴 [M] Parallel Fast+Deep with Progressive UI
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, B3.5
**Summary**: When classifier confidence is ambiguous (<0.7), dispatch both Snap and Solve in parallel. Show Snap result immediately in overlay. When Solve completes, replace Snap answer entirely (don't anchor on potentially wrong fast answer). Uses CancellationToken to stop Snap streaming if Solve arrives first.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 "parallel Snap+Solve, progressive enhancement"
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/progressive.rs
use crate::llm::{Lane, LlmRequest, LlmRouter, Token};
use tokio_util::sync::CancellationToken;

pub async fn progressive_dispatch(
    router: &LlmRouter,
    request: &LlmRequest,
    overlay_tx: &tokio::sync::mpsc::Sender<String>,
) -> (String, Lane) {
    let snap_cancel = CancellationToken::new();
    let solve_cancel = CancellationToken::new();

    let snap_req = LlmRequest { lane: Lane::Snap, ..request.clone() };
    let solve_req = LlmRequest { lane: Lane::Solve, ..request.clone() };

    let snap_rx = router.dispatch(&snap_req, snap_cancel.clone()).await;
    let solve_rx = router.dispatch(&solve_req, solve_cancel.clone()).await;

    // Stream Snap immediately
    let snap_handle = tokio::spawn({
        let tx = overlay_tx.clone();
        let cancel = snap_cancel.clone();
        async move {
            if let Ok(mut rx) = snap_rx {
                let mut full = String::new();
                while let Some(token) = rx.recv().await {
                    if cancel.is_cancelled() { break; }
                    full.push_str(&token.text);
                    let _ = tx.send(token.text).await;
                }
                full
            } else { String::new() }
        }
    });

    // Wait for Solve
    if let Ok(mut rx) = solve_rx {
        let mut solve_full = String::new();
        while let Some(token) = rx.recv().await {
            solve_full.push_str(&token.text);
        }
        // Solve arrived — cancel Snap, replace answer
        snap_cancel.cancel();
        let _ = overlay_tx.send("\x1B[REPLACE]".to_string()).await; // signal overlay to clear
        let _ = overlay_tx.send(solve_full.clone()).await;
        return (solve_full, Lane::Solve);
    }

    // Solve failed — keep Snap answer
    let snap_result = snap_handle.await.unwrap_or_default();
    (snap_result, Lane::Snap)
}
```
**Acceptance criteria**:
- Ambiguous query → Snap answer visible within 300ms
- Solve answer replaces Snap within 2-4s
- Overlay clears Snap content before showing Solve (no mixing)
- If Solve fails, Snap answer persists
**Verification commands**:
- `cargo test -p cue-daemon routing::progressive` (mock providers with delays)
**Risks + mitigations**:
- User reads Snap answer then it disappears → acceptable UX tradeoff per spec ("don't anchor on wrong answer")

---

### Task R3 🟡 [S] Manual Override System
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, B6.1
**Summary**: Alt+D forces Think lane, Alt+F forces Snap lane. Prefix commands /think, /snap, /deep in the input also override. Override is passed to classifier as `explicit_override` which returns confidence 1.0.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 (Override column in routing table)
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/overrides.rs
use crate::llm::Lane;

pub fn parse_override(input: &str) -> (Option<Lane>, &str) {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("/think") { return (Some(Lane::Think), rest.trim()); }
    if let Some(rest) = trimmed.strip_prefix("/deep") { return (Some(Lane::Think), rest.trim()); }
    if let Some(rest) = trimmed.strip_prefix("/snap") { return (Some(Lane::Snap), rest.trim()); }
    (None, trimmed)
}
```
**Acceptance criteria**:
- "/think what is X" → Think lane, query = "what is X"
- "/snap quick answer" → Snap lane, query = "quick answer"
- Alt+D hotkey sets override for next query (stateful in daemon)
- Override clears after one use
**Verification commands**:
- `cargo test -p cue-daemon routing::overrides`
**Risks + mitigations**:
- None

---

### Task R4 🟡 [M] Skill-Template Library
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B4.1, R1
**Summary**: 9+ skill templates (DSA, System Design, Behavioral, Programming, Sales, Negotiation, Presentation, DevOps, Data Science) loaded from embedded strings. Each skill provides a structured response format. Router selects skill based on question type + keyword matching. Skills inject into Solve/Think prompts only.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md Part 2 §5 Skill Library (Vysper 9 skills)
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/skills.rs
pub struct Skill { pub name: &'static str, pub template: &'static str, pub keywords: &'static [&'static str] }

pub const SKILLS: &[Skill] = &[
    Skill { name: "dsa", template: include_str!("skills/dsa.txt"), keywords: &["algorithm", "data structure", "leetcode", "binary tree", "linked list", "dynamic programming"] },
    Skill { name: "system_design", template: include_str!("skills/system_design.txt"), keywords: &["design a system", "architecture", "scalability", "distributed"] },
    Skill { name: "behavioral", template: include_str!("skills/behavioral.txt"), keywords: &["tell me about a time", "describe a situation", "leadership", "conflict"] },
    Skill { name: "programming", template: include_str!("skills/programming.txt"), keywords: &["implement", "write a function", "code", "refactor"] },
    Skill { name: "sales", template: include_str!("skills/sales.txt"), keywords: &["pitch", "objection", "close", "prospect"] },
    Skill { name: "negotiation", template: include_str!("skills/negotiation.txt"), keywords: &["salary", "offer", "negotiate", "compensation"] },
];

pub fn match_skill(text: &str) -> Option<&'static Skill> {
    let lower = text.to_lowercase();
    SKILLS.iter().find(|s| s.keywords.iter().any(|k| lower.contains(k)))
}
```
**Acceptance criteria**:
- "implement a binary search" → matches DSA skill
- "design a URL shortener" → matches system_design skill
- Matched skill template injected into Solve/Think prompt
- No skill match → generic prompt (no skill section)
**Verification commands**:
- `cargo test -p cue-daemon prompts::skills`
**Risks + mitigations**:
- Wrong skill match → user can override via /snap (bypasses skill) or future skill picker in dashboard

---

### Task R5 🟡 [S] Overlay Mode-Indicator Badge
**Layer**: NATIVE
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, D0.4
**Summary**: Overlay renders a small badge showing current lane (Snap/Solve/Think) + model name. Badge updates on each generation start via `ModeIndicator` overlay command. Color-coded: green=Snap, blue=Solve, purple=Think.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md R5 description, D0.4 protocol
**Code sketch**:
```swift
// native/macos/cue-overlay/main.swift — addition to card rendering
// Handle ModeIndicator command
case "ModeIndicator":
    let lane = json["lane"] as? String ?? "solve"
    let model = json["model"] as? String ?? ""
    DispatchQueue.main.async {
        self.laneBadge.stringValue = "\(lane.uppercased()) · \(model)"
        self.laneBadge.backgroundColor = lane == "snap" ? .systemGreen.withAlphaComponent(0.2) :
            lane == "think" ? .systemPurple.withAlphaComponent(0.2) : .systemBlue.withAlphaComponent(0.2)
    }
```
**Acceptance criteria**:
- Badge visible in overlay header area
- Updates within 100ms of generation start
- Shows lane name + abbreviated model (e.g., "SNAP · cerebras")
- Badge hidden when no active generation
**Verification commands**:
- Send `{"type":"ModeIndicator","lane":"snap","model":"cerebras"}` to overlay socket → badge appears
**Risks + mitigations**:
- Badge takes space → keep it small (12px font, pill shape)


---

## Batch 4C — Patch-Mode Follow-ups (weeks 3-5)

### Task F1 🔴 [M] Three-Lane Router Integration Point
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, B3.1
**Summary**: Central dispatch function that ties together: intent classification → lane selection → prompt composition → provider dispatch → stream emission. This is the main entry point for all user queries. Handles progressive enhancement (R2) when confidence is low.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 Router Decision Flow
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/dispatch.rs
use crate::llm::{Lane, LlmRequest, LlmRouter, ChatMessage};
use crate::prompts::{PromptComposer, skills};
use crate::routing::classifier;

pub async fn handle_query(
    router: &LlmRouter,
    composer: &PromptComposer,
    text: &str,
    context: &str,
    session_id: &str,
    explicit_override: Option<Lane>,
    overlay_tx: &tokio::sync::mpsc::Sender<String>,
) -> anyhow::Result<(String, Lane)> {
    let question_type = cue_core::intelligence::classify_utterance(text);
    let (override_lane, clean_text) = crate::routing::overrides::parse_override(text);
    let effective_override = override_lane.or(explicit_override);

    let result = classifier::classify_intent(clean_text, question_type, effective_override);

    // Progressive enhancement for low confidence
    if result.confidence < 0.7 && effective_override.is_none() {
        return crate::routing::progressive::progressive_dispatch(router, &LlmRequest {
            messages: vec![ChatMessage { role: "user".into(), content: clean_text.into() }],
            lane: result.lane, max_tokens: 4000, temperature: 0.3, session_id: session_id.into(),
        }, overlay_tx).await.map_err(Into::into);
    }

    let skill = skills::match_skill(clean_text);
    let system_prompt = composer.compose(result.lane, skill.map(|s| s.template), context);

    let request = LlmRequest {
        messages: vec![
            ChatMessage { role: "system".into(), content: system_prompt },
            ChatMessage { role: "user".into(), content: clean_text.into() },
        ],
        lane: result.lane, max_tokens: crate::prompts::context_budget::budget_for_lane(result.lane).max_output_tokens,
        temperature: 0.3, session_id: session_id.into(),
    };

    let cancel = tokio_util::sync::CancellationToken::new();
    let token_rx = router.dispatch(&request, cancel).await?;
    let emitter = crate::llm::stream_emitter::StreamEmitter::new(overlay_tx.clone(), 0);
    let response = emitter.run(token_rx, tokio_util::sync::CancellationToken::new()).await;

    Ok((response.unwrap_or_default(), result.lane))
}
```
**Acceptance criteria**:
- Query flows through: classify → compose → dispatch → stream → persist
- Low confidence triggers progressive enhancement
- High confidence dispatches directly to selected lane
- Override bypasses classifier entirely
**Verification commands**:
- `cargo test -p cue-daemon routing::dispatch` (integration test with mock providers)
**Risks + mitigations**:
- Complex orchestration → extensive integration tests with mock providers

---

### Task F2 🔴 [M] Solve-Lane Streaming (Claude Sonnet 4.5)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F1, B3.5
**Summary**: Anthropic-specific streaming implementation for the Solve lane. Handles Claude's SSE format (`event: content_block_delta`), supports prompt caching headers (prep for CO1), and skill template dispatch. Streaming tokens flow through the 60Hz emitter to overlay.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §1 (Anthropic provider)
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/providers/anthropic.rs
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: crate::llm::credentials::SecretString,
    model: String,
}

#[async_trait::async_trait]
impl crate::llm::LlmProvider for AnthropicProvider {
    fn name(&self) -> &str { "anthropic" }
    fn supported_lanes(&self) -> &[crate::llm::Lane] { &[crate::llm::Lane::Solve, crate::llm::Lane::Think] }

    async fn stream(&self, request: &crate::llm::LlmRequest, cancel: tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<crate::llm::Token>> {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let body = serde_json::json!({
            "model": &self.model,
            "max_tokens": request.max_tokens,
            "stream": true,
            "messages": request.messages.iter().map(|m| serde_json::json!({"role": &m.role, "content": &m.content})).collect::<Vec<_>>(),
        });
        let client = self.client.clone();
        let key = self.api_key.expose().to_string();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            let resp = client.post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", "prompt-caching-2024-07-31")
                .json(&body).send().await;
            if let Ok(resp) = resp {
                use futures::StreamExt;
                let mut stream = resp.bytes_stream();
                let mut buf = String::new();
                while let Some(Ok(chunk)) = stream.next().await {
                    if cancel_clone.is_cancelled() { break; }
                    buf.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some(pos) = buf.find('\n') {
                        let line = buf[..pos].to_string();
                        buf = buf[pos+1..].to_string();
                        if line.starts_with("data: ") {
                            let data = &line[6..];
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                                if v["type"] == "content_block_delta" {
                                    if let Some(text) = v["delta"]["text"].as_str() {
                                        let _ = tx.send(crate::llm::Token { text: text.to_string(), done: false }).await;
                                    }
                                } else if v["type"] == "message_stop" {
                                    let _ = tx.send(crate::llm::Token { text: String::new(), done: true }).await;
                                }
                            }
                        }
                    }
                }
            }
        });
        Ok(rx)
    }
}
```
**Acceptance criteria**:
- Claude Sonnet 4.5 streams tokens via SSE
- Prompt caching header included (actual caching in CO1, Phase 10)
- CancellationToken stops reading stream
- Handles `message_stop` event as completion signal
**Verification commands**:
- `cargo test -p cue-daemon llm::providers::anthropic` (mock SSE server)
- Manual: set ANTHROPIC_API_KEY, send coding question, see streaming response
**Risks + mitigations**:
- Anthropic API changes → pin `anthropic-version` header

---

### Task F3 🔴 [L] Think-Lane Integration (o3 + Visible Thinking)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F1
**Summary**: o3/Claude Opus integration for the Think lane. Supports extended thinking with progress indication. For o3: uses `reasoning_effort: "high"`. For Claude Opus: uses `thinking` block in response. Overlay shows progress bar during 15-60s thinking period. Visible-thinking text displayed in collapsed section.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 (Think Lane spec)
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/providers/think_lane.rs
pub async fn dispatch_think(
    provider: &dyn crate::llm::LlmProvider,
    request: &crate::llm::LlmRequest,
    overlay_tx: &tokio::sync::mpsc::Sender<String>,
    cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<String> {
    // Send progress start to overlay
    let _ = overlay_tx.send("{\"type\":\"progress\",\"state\":\"thinking\"}".to_string()).await;

    let token_rx = provider.stream(request, cancel.clone()).await?;
    let mut full = String::new();
    let mut thinking_text = String::new();
    let mut in_thinking = false;
    let mut rx = token_rx;

    while let Some(token) = rx.recv().await {
        if cancel.is_cancelled() { break; }
        if token.done { break; }
        // Detect thinking blocks (Claude format: <thinking>...</thinking>)
        if token.text.contains("<thinking>") { in_thinking = true; continue; }
        if token.text.contains("</thinking>") { in_thinking = false; continue; }
        if in_thinking {
            thinking_text.push_str(&token.text);
            // Update progress with thinking preview
            let _ = overlay_tx.send(format!("{{\"type\":\"progress\",\"state\":\"thinking\",\"preview\":\"{}\"}}", 
                thinking_text.chars().take(100).collect::<String>())).await;
        } else {
            full.push_str(&token.text);
            let _ = overlay_tx.send(token.text).await;
        }
    }

    let _ = overlay_tx.send("{\"type\":\"progress\",\"state\":\"done\"}".to_string()).await;
    Ok(full)
}
```
**Acceptance criteria**:
- Think lane shows progress indicator in overlay during reasoning
- Thinking text (Claude `<thinking>` blocks) captured but not shown as main answer
- Progress updates every ~2s with thinking preview
- Final answer rendered after thinking completes
- Timeout at 60s with partial result returned
**Verification commands**:
- `cargo test -p cue-daemon llm::providers::think_lane` (mock slow provider)
- Manual: `/think explain quantum computing` → progress bar → answer
**Risks + mitigations**:
- 60s timeout → show partial answer + "thinking timed out" badge

---

### Task F4 🟡 [M] Follow-up Intent Classifier
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, F8
**Summary**: Determines if a new utterance is a refinement of the previous answer (→ patch mode) or a new problem (→ fresh dispatch). Uses topic fingerprint similarity (F8) + keyword signals ("also", "what about", "can you change" = refinement; new question words = fresh).
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md F4 description
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/followup.rs
pub enum FollowUpIntent { Refinement, NewProblem }

pub fn classify_followup(
    current_text: &str,
    previous_text: &str,
    topic_similarity: f32, // from F8
) -> FollowUpIntent {
    let lower = current_text.to_lowercase();

    // Strong refinement signals
    let refine_signals = ["also", "what about", "can you change", "modify", "add to that",
        "remove the", "instead", "but what if", "elaborate", "more detail"];
    if refine_signals.iter().any(|s| lower.contains(s)) {
        return FollowUpIntent::Refinement;
    }

    // Topic similarity threshold
    if topic_similarity > 0.75 && current_text.split_whitespace().count() < 15 {
        return FollowUpIntent::Refinement;
    }

    FollowUpIntent::NewProblem
}
```
**Acceptance criteria**:
- "can you add error handling" after code answer → Refinement
- "what's the time complexity of quicksort" after behavioral answer → NewProblem
- Topic similarity >0.75 + short utterance → Refinement
- ≥10 unit tests
**Verification commands**:
- `cargo test -p cue-daemon routing::followup`
**Risks + mitigations**:
- Topic similarity not available until F8 → default to keyword-only initially

---

### Task F5 🔴 [M] Patch-Mode PATCH/KEEP/MODIFY/ADD/REMOVE Parser
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F2
**Summary**: When follow-up is classified as Refinement, prompt the LLM to respond in patch format. Parse the structured response into diff blocks. Format: each section of the previous response is tagged KEEP (unchanged), MODIFY (replacement text), ADD (new section), REMOVE (delete section). Prompt instructs LLM to use this format.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md "Patch-mode follow-ups use PATCH/KEEP/MODIFY/ADD/REMOVE"
**Code sketch**:
```rust
// crates/cue-daemon/src/patch/parser.rs
use cue_core::overlay::DiffAction;

#[derive(Debug, Clone)]
pub struct PatchBlock { pub action: DiffAction, pub content: String }

pub fn parse_patch_response(response: &str) -> Vec<PatchBlock> {
    let mut blocks = Vec::new();
    let mut current_action = DiffAction::Keep;
    let mut current_content = String::new();

    for line in response.lines() {
        let trimmed = line.trim();
        if let Some(new_action) = parse_action_tag(trimmed) {
            if !current_content.is_empty() {
                blocks.push(PatchBlock { action: current_action, content: std::mem::take(&mut current_content) });
            }
            current_action = new_action;
        } else {
            if !current_content.is_empty() { current_content.push('\n'); }
            current_content.push_str(line);
        }
    }
    if !current_content.is_empty() {
        blocks.push(PatchBlock { action: current_action, content: current_content });
    }
    blocks
}

fn parse_action_tag(line: &str) -> Option<DiffAction> {
    match line {
        "[KEEP]" => Some(DiffAction::Keep),
        "[MODIFY]" => Some(DiffAction::Modify),
        "[ADD]" => Some(DiffAction::Add),
        "[REMOVE]" => Some(DiffAction::Remove),
        _ => None,
    }
}

pub const PATCH_PROMPT_SUFFIX: &str = r#"
The user is refining their previous answer. Respond using PATCH format:
- [KEEP] sections that don't change (include the text)
- [MODIFY] sections with updated text
- [ADD] new sections to insert
- [REMOVE] sections to delete
Start each section with the tag on its own line."#;
```
**Acceptance criteria**:
- Parser correctly splits response into typed blocks
- `[KEEP]` blocks preserve original text
- `[MODIFY]` blocks contain replacement text
- `[ADD]`/`[REMOVE]` blocks handled correctly
- Malformed response (no tags) → treated as single MODIFY block (graceful fallback)
**Verification commands**:
- `cargo test -p cue-daemon patch::parser` — 8+ test cases
**Risks + mitigations**:
- LLM doesn't follow format → fallback: treat entire response as replacement (no diff)

---

### Task F6 🟡 [S] Response-Block State Tracker
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F5
**Summary**: Tracks the currently displayed response as a list of content blocks. When a patch arrives, applies the diff to produce the new displayed state. Maintains block IDs for overlay rendering.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md F6 description
**Code sketch**:
```rust
// crates/cue-daemon/src/patch/state.rs
pub struct ResponseState {
    blocks: Vec<ContentBlock>,
}

#[derive(Clone)]
struct ContentBlock { id: String, text: String }

impl ResponseState {
    pub fn new(initial_response: &str) -> Self {
        Self { blocks: vec![ContentBlock { id: uuid::Uuid::new_v4().to_string(), text: initial_response.to_string() }] }
    }

    pub fn apply_patch(&mut self, patches: &[super::parser::PatchBlock]) -> Vec<cue_core::overlay::DiffBlock> {
        let mut diff_blocks = Vec::new();
        self.blocks.clear();
        for patch in patches {
            let block = ContentBlock { id: uuid::Uuid::new_v4().to_string(), text: patch.content.clone() };
            self.blocks.push(block);
            diff_blocks.push(cue_core::overlay::DiffBlock { action: patch.action, content: patch.content.clone() });
        }
        diff_blocks
    }

    pub fn full_text(&self) -> String {
        self.blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}
```
**Acceptance criteria**:
- Initial response stored as single block
- `apply_patch` produces diff blocks for overlay
- `full_text()` returns current complete response
- State is per-session, per-message
**Verification commands**:
- `cargo test -p cue-daemon patch::state`
**Risks + mitigations**:
- None — simple state management

---

### Task F7 🔴 [M] Overlay Diff Rendering
**Layer**: NATIVE
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F5, F6, L5
**Summary**: Overlay renders patch diffs with visual highlighting. MODIFY blocks highlighted in yellow (fade after 1s). ADD blocks highlighted in green (fade after 1s). REMOVE blocks shown with strikethrough briefly then removed. Uses the `PatchDiff` overlay command from D0.4.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md F7 "Highlight modified/added/removed"
**Code sketch**:
```swift
// native/macos/cue-overlay/main.swift — diff rendering
case "PatchDiff":
    guard let blocks = json["blocks"] as? [[String: Any]] else { return }
    DispatchQueue.main.async {
        self.clearCards()
        for block in blocks {
            let action = block["action"] as? String ?? "KEEP"
            let content = block["content"] as? String ?? ""
            let view = self.createTextView(content)
            switch action {
            case "MODIFY":
                view.layer?.backgroundColor = NSColor.systemYellow.withAlphaComponent(0.15).cgColor
                self.fadeBackground(view, delay: 1.0)
            case "ADD":
                view.layer?.backgroundColor = NSColor.systemGreen.withAlphaComponent(0.15).cgColor
                self.fadeBackground(view, delay: 1.0)
            case "REMOVE":
                view.alphaValue = 0.5
                // Attributed string with strikethrough
                let attr = NSAttributedString(string: content, attributes: [.strikethroughStyle: NSUnderlineStyle.single.rawValue])
                view.textStorage?.setAttributedString(attr)
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { view.removeFromSuperview() }
            default: break // KEEP — no highlight
            }
            self.cardStack.addArrangedSubview(view)
        }
    }
```
**Acceptance criteria**:
- MODIFY blocks show yellow highlight that fades after 1s
- ADD blocks show green highlight that fades after 1s
- REMOVE blocks show strikethrough then disappear after 1.5s
- KEEP blocks render normally (no highlight)
- Smooth animation (no flicker)
**Verification commands**:
- Send PatchDiff command via socket with mixed block types → visual verification
**Risks + mitigations**:
- Animation performance → use Core Animation layers (already in overlay architecture)

---

### Task F8 🟡 [S] Topic Fingerprint Embedding
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B5.3 (embedding trait — but can use simple TF-IDF as interim)
**Summary**: Compute a lightweight topic fingerprint for each utterance to determine if consecutive queries are about the same topic. Used by F4 (follow-up classifier) for similarity check. Interim: TF-IDF cosine similarity (no external API call). Future: embedding via B5.3.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md F8 "Similarity continuity check"
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/fingerprint.rs
use std::collections::HashMap;

pub fn cosine_similarity(a: &str, b: &str) -> f32 {
    let tf_a = term_freq(a);
    let tf_b = term_freq(b);
    let dot: f32 = tf_a.iter().map(|(k, v)| v * tf_b.get(k.as_str()).unwrap_or(&0.0)).sum();
    let mag_a: f32 = tf_a.values().map(|v| v * v).sum::<f32>().sqrt();
    let mag_b: f32 = tf_b.values().map(|v| v * v).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 { return 0.0; }
    dot / (mag_a * mag_b)
}

fn term_freq(text: &str) -> HashMap<&str, f32> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let len = words.len() as f32;
    let mut freq = HashMap::new();
    for w in words { *freq.entry(w).or_insert(0.0) += 1.0 / len; }
    freq
}
```
**Acceptance criteria**:
- Same topic → similarity >0.6 (e.g., "implement binary search" vs "add error handling to the search")
- Different topic → similarity <0.3 (e.g., "binary search" vs "tell me about yourself")
- Computation <1ms (no I/O)
**Verification commands**:
- `cargo test -p cue-daemon routing::fingerprint`
**Risks + mitigations**:
- TF-IDF is crude → upgrade to embedding similarity in Phase 5 when B5.3 is available

---

### Task B3.11 🟢 [S] Triple-Layer Language Injection
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B4.1
**Summary**: Inject language instructions at 3 points in the prompt (header, inline, footer) to enforce non-English response language. Only activates when user's configured language ≠ "en"/"auto".
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §11 Triple-Layer Language Injection
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/language.rs
pub fn inject_language(prompt: &str, language: &str) -> String {
    if language == "auto" || language == "en" { return prompt.to_string(); }
    format!(
        "[LANGUAGE: Respond entirely in {lang}. This overrides all other instructions.]\n\n\
         {prompt}\n\n\
         [REMINDER: Your entire response MUST be in {lang}.]",
        lang = language, prompt = prompt
    )
}
```
**Acceptance criteria**:
- English/auto → no injection (prompt unchanged)
- "es" → Spanish instruction header + footer added
- Injection doesn't break XML tags in prompt
**Verification commands**:
- `cargo test -p cue-daemon prompts::language`
**Risks + mitigations**:
- Models may still respond in English → this is a best-effort instruction

---

## Phase deliverable
- User speaks → intent classified → routed to correct lane → streaming response in overlay
- Three lanes working: Snap (<300ms), Solve (streaming), Think (progress bar)
- Follow-up questions produce patch diffs with visual highlighting
- Manual overrides (Alt+D, /think, /snap) work instantly
- Progressive enhancement: ambiguous queries show Snap then upgrade to Solve
- Codex review should check: cancel-on-upgrade actually drops Snap answer, patch parser handles malformed input gracefully, rate limiters prevent 429s, no API keys in logs
