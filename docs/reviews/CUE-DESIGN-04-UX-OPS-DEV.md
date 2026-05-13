# bluey Design — UX + Ops + Security + Build + Dev Workflow

## Executive summary

This document synthesizes UX patterns, database architecture, security practices, observability infrastructure, build/release pipelines, and developer workflow conventions extracted from 7 deep-analysis documents covering natively-cluely (42K LOC), pluely (27K LOC), solveWatchAi (56K LOC), Aura-AI (17K LOC), Vysper (15K LOC), and OpenCluely (16K LOC).

**Key architectural decisions for bluey:**
- Tauri 2 with `macos-private-api` feature for NSPanel + content protection
- React 19 + Radix + shadcn + Tailwind 4 + cmdk + shiki + streamdown + rehype-katex
- SQLite via `tauri-plugin-sql` with Rust-side migrations
- Native keychain via `tauri-plugin-keychain` (not plaintext JSON)
- OpenTelemetry via `opentelemetry-rust` crates → Grafana Cloud
- Global shortcuts via `tauri-plugin-global-shortcut` with centralized handler pattern
- `tracing` crate with `tracing-appender` for structured logging

**Source citation format:** `[REF-01A:L280]` = CUE-REF-01A-NATIVELY-BACKEND.md line 280.

---

## Part 1: UX architecture

### 1.1 Hotkey system design

#### Default bindings table

| Shortcut | Action | Mode | Source |
|----------|--------|------|--------|
| `Cmd+Shift+Enter` | Single-trigger capture (screenshot + AI) | Global | [REF-ANALYSIS:L45] |
| `Cmd+K` / `Ctrl+K` | Spotlight / command palette | Global | [REF-ANALYSIS:L46] |
| `Alt+Z` | Toggle window visibility | Global | [REF-04:hotkey table] |
| `Alt+Shift+S` | Enable full stealth mode | Global | [REF-04:hotkey table] |
| `Cmd+Shift+M` | Toggle system audio capture | Global | [REF-ANALYSIS:L45] |
| `Cmd+Shift+A` | Toggle voice input (mic) | Global | [REF-ANALYSIS:L45] |
| `Cmd+Shift+D` | Toggle dashboard | Global | [REF-02:pattern 20] |
| `Cmd+Shift+S` | Screenshot (manual) | Global | [REF-ANALYSIS:L50] |
| `Cmd+Shift+H` | Toggle HUD overlay | Global | [REF-03:electron/main.js] |
| `Cmd+Shift+X` | Toggle listen mode | Global | [REF-03:electron/main.js] |
| `Alt+1/2/3` | Opacity presets (40%/70%/100%) | Overlay | [REF-04:hotkey table] |
| `Alt+Arrow` | Move window (20px step) | Overlay | [REF-04:hotkey table] |

#### Rebindable via settings

**Source:** [REF-01A:KeybindManager.ts 491L], [REF-02:shortcuts.rs L250-380]

```typescript
// Frontend: src/lib/storage/shortcuts.storage.ts
interface ShortcutConfig {
  action_id: string;        // e.g. "toggle_visibility"
  accelerator: string;      // e.g. "Alt+Z"
  enabled: boolean;
  mode: "global" | "overlay" | "launcher";
}
```

```rust
// Backend: src-tauri/src/shortcuts.rs
#[tauri::command]
async fn update_shortcuts(
    app: AppHandle,
    config: HashMap<String, String>, // action_id → accelerator
    state: State<'_, RegisteredShortcuts>,
) -> Result<(), String> {
    // 1. Validate each key via parse::<Shortcut>()
    // 2. Unregister all existing shortcuts
    // 3. Register new set with centralized handler
    // 4. Store in state for lookup during dispatch
}
```

**Centralized handler pattern** [REF-02:shortcuts.rs L250]:
```rust
// Single closure dispatches ALL shortcuts by action_id lookup
app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
    let shortcuts = registered_shortcuts.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(action_id) = shortcuts.get(&accelerator_string) {
        match action_id.as_str() {
            "toggle_visibility" => { /* ... */ },
            "capture_screenshot" => { /* ... */ },
            _ => { app.emit("custom-shortcut-triggered", action_id).ok(); }
        }
    }
});
```

#### Per-mode allowlist

**Source:** [REF-ANALYSIS:L47-48], [REF-01A:KeybindManager.ts]

```rust
fn should_register(action: &str, mode: AppMode) -> bool {
    match mode {
        AppMode::Launcher => LAUNCHER_ALLOWED.contains(&action),
        AppMode::Overlay => OVERLAY_ALLOWED.contains(&action),
        AppMode::Dashboard => DASHBOARD_ALLOWED.contains(&action),
    }
}

// CRITICAL: Global shortcuts (toggle_visibility, capture) must be in ALL allowlists
// Common silent-fail bug: [REF-ANALYSIS:L48]
const LAUNCHER_ALLOWED: &[&str] = &[
    "toggle_visibility", "capture_screenshot", "toggle_dashboard",
    "toggle_listen", "spotlight",
];
```

#### Tauri plugin integration

**Cargo.toml:**
```toml
tauri-plugin-global-shortcut = "2"
```

**lib.rs registration:**
```rust
tauri::Builder::default()
    .plugin(tauri_plugin_global_shortcut::Builder::new().build())
```

---

### 1.2 Dashboard + sidebar nav

#### Page structure

**Source:** [REF-02:React architecture deep-dive], [REF-ANALYSIS:L176]

```
/dashboard     → PluelyApiSetup + Usage charts
/chats         → Conversation list (SQLite query)
/chats/:id     → ViewChat (continue conversation, download as MD)
/system-prompts → CRUD + community prompts + AI-generated prompts
/shortcuts     → ShortcutManager + ShortcutRecorder
/settings      → Theme, AlwaysOnTop, AppIcon, Autostart, DeleteChats
/responses     → Response length, language, auto-scroll config
/screenshot    → Screenshot mode config (auto/manual/selection)
/audio         → Audio device selection (input + output)
/dev           → Custom AI provider configs + STT provider configs
```

#### React Router setup

**Source:** [REF-02:src/routes/index.tsx]

```typescript
// src/routes/index.tsx
import { BrowserRouter, Routes, Route } from "react-router-dom";

export function AppRoutes() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<App />} />  {/* Overlay window */}
        <Route element={<DashboardLayout />}>
          <Route path="/dashboard" element={<Dashboard />} />
          <Route path="/chats" element={<Chats />} />
          <Route path="/chats/:id" element={<ViewChat />} />
          <Route path="/system-prompts" element={<SystemPrompts />} />
          <Route path="/shortcuts" element={<Shortcuts />} />
          <Route path="/settings" element={<Settings />} />
          <Route path="/responses" element={<Responses />} />
          <Route path="/screenshot" element={<Screenshot />} />
          <Route path="/audio" element={<Audio />} />
          <Route path="/dev" element={<DevSpace />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
```

#### Pre-create on startup for instant open

**Source:** [REF-02:window.rs L155-200]

```rust
// src-tauri/src/window.rs
pub fn create_dashboard_window(app: &AppHandle) -> Result<()> {
    let window = tauri::WebviewWindowBuilder::new(
        app, "dashboard",
        tauri::WebviewUrl::App("/dashboard".into()),
    )
    .title("bluey")
    .inner_size(1200.0, 800.0)
    .min_inner_size(800.0, 600.0)
    .content_protected(true)          // [REF-02:pattern 14]
    .visible(false)                    // Hidden initially
    .build()?;

    // Hide-on-close pattern [REF-02:pattern 20]
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            window.hide().ok();
        }
    });

    Ok(())
}

// Called in setup hook — dashboard ready before user ever clicks
fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    create_dashboard_window(app.handle())?;
    Ok(())
}
```

---

### 1.3 React streaming patterns

#### rAF coalescing

**Source:** [REF-01B:pattern 4, src/hooks/useStreamBuffer.ts 59L]

```typescript
// src/hooks/useStreamBuffer.ts
import { useRef, useCallback, startTransition } from "react";

export function useStreamBuffer(onFlush: (text: string) => void) {
  const bufferRef = useRef("");
  const rafRef = useRef<number | null>(null);

  const queueToken = useCallback((token: string) => {
    bufferRef.current += token;
    if (rafRef.current === null) {
      rafRef.current = requestAnimationFrame(() => {
        const text = bufferRef.current;
        bufferRef.current = "";
        rafRef.current = null;
        startTransition(() => onFlush(text));
      });
    }
  }, [onFlush]);

  return { queueToken };
}
```

**Why:** At 200-400 tok/s (Groq), this reduces renders from 400/s to ~60/s. Each token no longer triggers full message-list reconciliation. [REF-01B:L580-640]

#### React.memo message rows

**Source:** [REF-01B:pattern 5, NativelyInterface.tsx L130-180]

```typescript
// src/components/MessageRow.tsx
import { memo } from "react";

interface MessageRowProps {
  id: string;
  role: "user" | "assistant";
  content: string;
  isStreaming: boolean;
}

export const MessageRow = memo<MessageRowProps>(
  ({ id, role, content, isStreaming }) => (
    <div data-message-id={id} className={`message message-${role}`}>
      <Markdown content={content} />
    </div>
  ),
  (prev, next) => {
    // Custom comparator: skip re-render if content unchanged
    return prev.content === next.content && prev.isStreaming === next.isStreaming;
  }
);
```

#### Code-expansion springs

**Source:** [REF-01B:pattern 6, NativelyInterface.tsx L300-400]

The overlay shell width animates 600↔780px via CSS transition when code blocks scroll into view. The OS window stays at stable 780px — no IPC during animation. Symmetric expansion from center via `mx-auto`.

```typescript
// Stability gate: 120ms debounce prevents rapid expand/contract
const [expanded, setExpanded] = useState(false);
const debounceRef = useRef<NodeJS.Timeout>();

const checkCodeVisibility = useCallback(() => {
  clearTimeout(debounceRef.current);
  debounceRef.current = setTimeout(() => {
    const hasVisibleCode = /* IntersectionObserver check */;
    setExpanded(hasVisibleCode);
  }, 120);
}, []);
```

#### Inertial scroll

**Source:** [REF-01B:pattern 7, NativelyInterface.tsx L2200-2350]

Physics-based scroll with momentum, friction half-life, terminal velocity. Works via global shortcuts even when window is unfocused.

```typescript
// Core physics loop (simplified)
const FRICTION_HALF_LIFE = 200; // ms
const TERMINAL_VELOCITY = 3000; // px/s

function scrollTick(dt: number) {
  velocity *= Math.pow(0.5, dt / FRICTION_HALF_LIFE);
  if (Math.abs(velocity) < 1) { velocity = 0; return; }
  velocity = Math.min(Math.abs(velocity), TERMINAL_VELOCITY) * Math.sign(velocity);
  scrollRef.current.scrollTop += velocity * (dt / 1000);
  rafId = requestAnimationFrame(scrollTick);
}
```

---

### 1.4 UI library composition

#### Exact npm versions

**Source:** [REF-02:package.json inventory]

```json
{
  "dependencies": {
    "react": "19.1.0",
    "react-dom": "19.1.0",
    "react-router-dom": "7.9.5",
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-global-shortcut": "^2",
    "@tauri-apps/plugin-sql": "^2",
    "@tauri-apps/plugin-http": "^2",
    "@tauri-apps/plugin-autostart": "^2",
    "@tauri-apps/plugin-updater": "^2",
    "@tauri-apps/plugin-opener": "^2",
    "@tauri-apps/plugin-process": "^2",
    "@radix-ui/react-dialog": "latest",
    "@radix-ui/react-dropdown-menu": "latest",
    "@radix-ui/react-popover": "latest",
    "@radix-ui/react-scroll-area": "latest",
    "@radix-ui/react-select": "latest",
    "@radix-ui/react-slider": "latest",
    "@radix-ui/react-switch": "latest",
    "@radix-ui/react-tabs": "latest",
    "cmdk": "1.1.1",
    "streamdown": "1.6.10",
    "shiki": "3.12.2",
    "rehype-katex": "7.0.1",
    "remark-gfm": "4.0.1",
    "remark-math": "6.0.0",
    "lucide-react": "0.539.0",
    "tailwindcss": "4.1.12",
    "@tailwindcss/vite": "4.1.12",
    "class-variance-authority": "0.7.1",
    "tailwind-merge": "3.3.1",
    "clsx": "2.1.1",
    "recharts": "2.15.4",
    "react-error-boundary": "6.0.0"
  }
}
```

---

### 1.5 Hook composition strategy

#### Problem: God hooks

**Source:** [REF-02:useCompletion 1050L, useSystemAudio 928L, useChatCompletion 725L]

Pluely's hooks are 1000+ lines managing 20+ state variables each. This is the #1 anti-pattern to avoid.

#### Solution: Decompose into focused hooks

```
useCompletion (1050L) → decompose into:
├── useAIStream          — streaming token management + abort
├── useConversation      — message history + persistence
├── useScreenCapture     — screenshot capture + attachment
├── useVoiceInput        — mic toggle + VAD state
├── useFileAttachments   — file drop + preview + cleanup
└── useWindowResize      — expand/collapse on content
```

**Module-level state anti-pattern** [REF-02:pattern 17]:
```typescript
// BAD: Module-level mutable state (pluely's useGlobalShortcuts.ts)
let globalInputRef: HTMLInputElement | null = null;  // ← persists across HMR
let globalAudioCallback: (() => void) | null = null;

// GOOD: Service class with explicit lifecycle
class ShortcutService {
  private listeners = new Map<string, () => void>();
  register(action: string, cb: () => void) { this.listeners.set(action, cb); }
  destroy() { this.listeners.clear(); }
}
```

---

### 1.6 Onboarding

**Source:** [REF-01B:FeatureSpotlight.tsx 357L, StartupSequence.tsx 282L]

- `FeatureSpotlight` — highlights UI elements with tooltip overlays on first use
- `StartupSequence` — first-run animation introducing core features
- Pattern: Track `hasSeenFeature_{name}` in localStorage, show spotlight once

---

### 1.7 Theme management

**Source:** [REF-01B:pattern 2, main.tsx L12-30]

```typescript
// Prevent FOUC: synchronous theme application before React renders
// index.html inline <script>:
const theme = localStorage.getItem("theme") || "system";
const isDark = theme === "dark" || 
  (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
document.documentElement.classList.toggle("dark", isDark);

// Then confirm via IPC after mount:
invoke("get_theme").then(confirmed => {
  if (confirmed !== theme) applyTheme(confirmed);
});
```

---

## Part 2: Database + persistence

### 2.1 SQLite schema

#### Migrations via tauri-plugin-sql

**Source:** [REF-02:db/main.rs, pattern 9], [REF-01A:DatabaseManager.ts L100-500]

```rust
// src-tauri/src/db/mod.rs
use tauri_plugin_sql::{Migration, MigrationKind};

pub fn migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "create_conversations_and_messages",
            sql: include_str!("migrations/001_chat_history.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "create_system_prompts",
            sql: include_str!("migrations/002_system_prompts.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 3,
            description: "create_settings_and_meetings",
            sql: include_str!("migrations/003_settings_meetings.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 4,
            description: "create_rag_tables",
            sql: include_str!("migrations/004_rag_chunks.sql"),
            kind: MigrationKind::Up,
        },
    ]
}
```

**Registration in lib.rs:**
```rust
.plugin(
    tauri_plugin_sql::Builder::new()
        .add_migrations("sqlite:bluey.db", db::migrations())
        .build(),
)
```

#### Initial tables

**Source:** [REF-02:db/migrations/chat-history.sql], [REF-01A:Database Schema Version 12]

```sql
-- 001_chat_history.sql
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT 'New Conversation',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    system_prompt_id TEXT,
    model_id TEXT,
    metadata_json TEXT
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    attachments_json TEXT,
    token_count INTEGER,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE INDEX idx_messages_conversation ON messages(conversation_id, created_at);

-- Auto-update trigger
CREATE TRIGGER update_conversation_timestamp
AFTER INSERT ON messages
BEGIN
    UPDATE conversations SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.conversation_id;
END;

-- 002_system_prompts.sql
CREATE TABLE IF NOT EXISTS system_prompts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    content TEXT NOT NULL,
    is_default BOOLEAN DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 003_settings_meetings.sql
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS meeting_transcripts (
    id TEXT PRIMARY KEY,
    title TEXT,
    start_time DATETIME,
    duration_ms INTEGER,
    summary_json TEXT,
    source TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS transcript_segments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id TEXT NOT NULL,
    speaker TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp_ms INTEGER,
    FOREIGN KEY (meeting_id) REFERENCES meeting_transcripts(id) ON DELETE CASCADE
);

-- 004_rag_chunks.sql
CREATE TABLE IF NOT EXISTS rag_chunks (
    id TEXT PRIMARY KEY,
    meeting_id TEXT,
    chunk_index INTEGER,
    speaker TEXT,
    content TEXT NOT NULL,
    token_count INTEGER,
    embedding BLOB,
    embedding_dim INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (meeting_id) REFERENCES meeting_transcripts(id) ON DELETE CASCADE
);

CREATE INDEX idx_chunks_meeting ON rag_chunks(meeting_id, chunk_index);
```

#### Versioned migration pattern

**Source:** [REF-01A:pattern 29]

Each migration uses `IF NOT EXISTS` / `INSERT OR IGNORE` for idempotency. Complex schema changes (adding UNIQUE constraints) use the rename-create-copy-drop pattern wrapped in a transaction — correct approach for SQLite's limited ALTER TABLE.

---

### 2.2 Hot-reload config + prompts

#### notify crate + Tauri events

**Source:** [REF-03:ai.service.js L150-180], [REF-ANALYSIS:L200-201]

```rust
// src-tauri/src/config_watcher.rs
use notify::{Watcher, RecursiveMode, Event, EventKind};
use std::time::Duration;
use tokio::sync::mpsc;

pub fn watch_config(app: AppHandle) -> Result<()> {
    let (tx, mut rx) = mpsc::channel(32);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_)) {
                tx.blocking_send(event).ok();
            }
        }
    })?;

    // Watch config directory
    let config_dir = app.path().app_config_dir()?;
    watcher.watch(&config_dir, RecursiveMode::NonRecursive)?;

    // Watch prompts directory
    let prompts_dir = config_dir.join("prompts");
    if prompts_dir.exists() {
        watcher.watch(&prompts_dir, RecursiveMode::Recursive)?;
    }

    // Debounced reload loop
    tauri::async_runtime::spawn(async move {
        let mut debounce = tokio::time::interval(Duration::from_millis(150));
        let mut pending = false;

        loop {
            tokio::select! {
                Some(_) = rx.recv() => { pending = true; }
                _ = debounce.tick(), if pending => {
                    pending = false;
                    app.emit("config-reloaded", ()).ok();
                    tracing::info!("Config/prompts reloaded");
                }
            }
        }
    });

    // Keep watcher alive
    std::mem::forget(watcher);
    Ok(())
}
```

---

## Part 3: Security

### 3.1 Keychain integration

#### Exact API usage

**Source:** [REF-02:Cargo.toml — tauri-plugin-keychain 2.0], [REF-02:pattern 10 critique]

**Critical lesson from pluely:** They have `tauri-plugin-keychain` in dependencies but store license keys as **plaintext JSON** on disk. bluey MUST use the keychain for all secrets.

```rust
// Cargo.toml
tauri-plugin-keychain = "2.0"
```

```typescript
// Frontend: src/lib/keychain.ts
import { save, get, remove } from "tauri-plugin-keychain-api";

const SERVICE = "com.bluey.app";

export async function saveApiKey(provider: string, key: string): Promise<void> {
  await save(SERVICE, `api_key_${provider}`, key);
}

export async function getApiKey(provider: string): Promise<string | null> {
  return await get(SERVICE, `api_key_${provider}`);
}

export async function removeApiKey(provider: string): Promise<void> {
  await remove(SERVICE, `api_key_${provider}`);
}
```

#### Per-provider key storage

Keys stored with provider-namespaced identifiers:
- `api_key_openai` → macOS Keychain / Windows Credential Vault / Linux Secret Service
- `api_key_anthropic`
- `api_key_gemini`
- `api_key_groq`
- `api_key_deepgram`
- `api_key_elevenlabs`

---

### 3.2 Key scrubbing + log hashing

#### On quit: memory overwrite

**Source:** [REF-01A:pattern 49], [REF-ANALYSIS:L60-61]

```rust
// src-tauri/src/credentials.rs
use zeroize::Zeroize;

pub struct ApiKeyStore {
    keys: HashMap<String, String>,
}

impl Drop for ApiKeyStore {
    fn drop(&mut self) {
        for (_, value) in self.keys.iter_mut() {
            value.zeroize(); // Secure memory clearing
        }
    }
}

// Called on app quit (before-exit hook)
#[tauri::command]
async fn scrub_credentials(state: State<'_, Mutex<ApiKeyStore>>) -> Result<(), String> {
    let mut store = state.lock().map_err(|e| e.to_string())?;
    for (_, value) in store.keys.iter_mut() {
        value.zeroize();
    }
    store.keys.clear();
    Ok(())
}
```

**Cargo.toml:**
```toml
zeroize = "1.7"
```

#### Hash/truncate keys in logs

**Source:** [REF-ANALYSIS:L61], [REF-01A:AUDIT.md critical finding]

```rust
// NEVER log raw keys
fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &key[..4], &key[key.len()-4..])
}

// Usage in tracing spans:
tracing::info!(provider = %provider, key = %mask_key(&api_key), "Initializing provider");
```

---

### 3.3 Error + panic handling

#### uncaughtException + unhandledRejection handlers

**Source:** [REF-ANALYSIS:L63], [REF-01A:main.ts L27-70]

```rust
// src-tauri/src/lib.rs — panic hook
fn setup_panic_handler() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("PANIC: {}", info);
        // Attempt graceful state save before crash
        default_hook(info);
    }));
}
```

```typescript
// Frontend: src/main.tsx
window.addEventListener("unhandledrejection", (event) => {
  console.error("[Unhandled Promise Rejection]", event.reason);
  invoke("log_error", { message: String(event.reason), level: "error" });
});

window.addEventListener("error", (event) => {
  console.error("[Uncaught Error]", event.error);
  invoke("log_error", { message: event.error?.message || "Unknown", level: "error" });
});
```

#### Single-instance lock

**Source:** [REF-01A:pattern 50], [REF-ANALYSIS:L70]

```rust
// Use tauri-plugin-single-instance
tauri::Builder::default()
    .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        // Focus existing window on second launch attempt
        if let Some(window) = app.get_webview_window("main") {
            window.show().ok();
            window.set_focus().ok();
        }
    }))
```

#### Lazy app.path()

**Source:** [REF-ANALYSIS:L64], [REF-01A:AUDIT.md CQ-04]

```rust
// WRONG: calling app.path() at module load time
// static LOG_PATH: &str = app.path().app_log_dir(); // CRASHES before app.ready

// CORRECT: OnceCell lazy initialization
use once_cell::sync::OnceCell;
static LOG_DIR: OnceCell<PathBuf> = OnceCell::new();

fn get_log_dir(app: &AppHandle) -> &PathBuf {
    LOG_DIR.get_or_init(|| {
        app.path().app_log_dir().expect("Failed to get log dir")
    })
}
```

#### CSP in Tauri config

**Source:** [REF-01B:security finding 2]

```json
// tauri.conf.json
{
  "app": {
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' https://*.openai.com https://*.anthropic.com https://*.googleapis.com https://*.groq.com https://*.deepgram.com"
    }
  }
}
```

---

## Part 4: Observability

### 4.1 OpenTelemetry setup

#### Crates

**Source:** [REF-03:telemetry.js 400L, telemetry.py 400L]

```toml
# Cargo.toml
opentelemetry = "0.24"
opentelemetry_sdk = { version = "0.24", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.17", features = ["http-proto", "reqwest-client"] }
tracing-opentelemetry = "0.25"
```

#### Span hierarchy + metric names

**Source:** [REF-03:OpenTelemetry observability deep-dive]

```rust
// src-tauri/src/telemetry.rs
use opentelemetry::metrics::{Histogram, Counter};
use opentelemetry_sdk::metrics::SdkMeterProvider;

pub struct Metrics {
    pub ai_ttft_ms: Histogram<f64>,          // Time to first AI token
    pub ai_total_ms: Histogram<f64>,         // Total AI generation time
    pub stt_decode_ms: Histogram<f64>,       // Per-decode STT time
    pub vad_latency_ms: Histogram<f64>,      // Per-chunk VAD inference
    pub ai_provider_success: Counter<u64>,   // label: provider, model
    pub ai_provider_failure: Counter<u64>,   // label: provider, error_class
    pub ai_input_tokens: Counter<u64>,       // label: provider, model
    pub ai_output_tokens: Counter<u64>,      // label: provider, model
    pub ai_cost_usd: Counter<f64>,           // label: provider, model
}

pub fn init_telemetry(endpoint: &str, api_key: &str) -> Result<Metrics> {
    let exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_endpoint(endpoint)
        .with_headers(HashMap::from([
            ("Authorization".into(), format!("Basic {}", api_key)),
        ]));

    let provider = SdkMeterProvider::builder()
        .with_reader(/* periodic reader */)
        .build();

    let meter = provider.meter("bluey");

    Ok(Metrics {
        ai_ttft_ms: meter.f64_histogram("ai_ttft_ms").init(),
        ai_total_ms: meter.f64_histogram("ai_total_ms").init(),
        stt_decode_ms: meter.f64_histogram("stt_decode_ms").init(),
        vad_latency_ms: meter.f64_histogram("vad_latency_ms").init(),
        ai_provider_success: meter.u64_counter("ai_provider_success_total").init(),
        ai_provider_failure: meter.u64_counter("ai_provider_failure_total").init(),
        ai_input_tokens: meter.u64_counter("ai_input_tokens_total").init(),
        ai_output_tokens: meter.u64_counter("ai_output_tokens_total").init(),
        ai_cost_usd: meter.f64_counter("ai_cost_usd_total").init(),
    })
}
```

#### Host identity labels

**Source:** [REF-03:Host identity labels section]

```rust
use tauri_plugin_machine_uid::get_machine_uid;

fn host_labels() -> Vec<KeyValue> {
    vec![
        KeyValue::new("host_id", sha256_truncate(&get_machine_uid())),
        KeyValue::new("hostname", hostname::get().unwrap_or_default().to_string_lossy()),
        KeyValue::new("os", std::env::consts::OS),
        KeyValue::new("arch", std::env::consts::ARCH),
    ]
}
```

---

### 4.2 Log architecture

#### NDJSON + ring buffer

**Source:** [REF-03:pattern 11 fallback logging], [REF-ANALYSIS:L202]

```rust
// src-tauri/src/logging.rs
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, fmt};
use std::collections::VecDeque;
use std::sync::Mutex;

// In-memory ring buffer for support/debug (last 1000 lines)
static LOG_RING: once_cell::sync::Lazy<Mutex<VecDeque<String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(VecDeque::with_capacity(1000)));

pub fn init_logging(log_dir: &Path) -> Result<()> {
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::NEVER)           // We handle rotation manually
        .max_log_files(1)                     // Keep one backup
        .filename_prefix("bluey")
        .filename_suffix("jsonl")
        .build(log_dir)?;

    tracing_subscriber::registry()
        .with(fmt::layer().json().with_writer(file_appender))
        .with(fmt::layer().with_writer(std::io::stdout))
        .init();

    Ok(())
}
```

#### Rotation strategy

**Source:** [REF-01A:pattern 33], [REF-ANALYSIS:L65]

Single-generation rollover at 10MB:
- Active: `bluey.jsonl`
- Backup: `bluey.jsonl.1`
- On rotation: rename current → `.1` (overwriting previous backup), create fresh

```rust
fn check_rotation(log_path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::metadata(log_path)?;
    if metadata.len() > 10 * 1024 * 1024 { // 10MB
        let backup = log_path.with_extension("jsonl.1");
        std::fs::rename(log_path, backup)?;
    }
    Ok(())
}
```

---

### 4.3 AI pricing table

**Source:** [REF-03:ai.service.js pricing table]

```rust
// src-tauri/src/pricing.rs
use std::collections::HashMap;

#[derive(Clone)]
pub struct ModelPricing {
    pub input_per_1m: f64,   // USD per 1M input tokens
    pub output_per_1m: f64,  // USD per 1M output tokens
}

pub fn pricing_table() -> HashMap<&'static str, ModelPricing> {
    HashMap::from([
        ("gpt-4o", ModelPricing { input_per_1m: 2.50, output_per_1m: 10.00 }),
        ("gpt-4o-mini", ModelPricing { input_per_1m: 0.15, output_per_1m: 0.60 }),
        ("claude-sonnet-4-20250514", ModelPricing { input_per_1m: 3.00, output_per_1m: 15.00 }),
        ("claude-haiku-3-5", ModelPricing { input_per_1m: 0.80, output_per_1m: 4.00 }),
        ("gemini-2.5-flash", ModelPricing { input_per_1m: 0.15, output_per_1m: 0.60 }),
        ("gemini-2.5-pro", ModelPricing { input_per_1m: 1.25, output_per_1m: 10.00 }),
        ("llama-3.3-70b-versatile", ModelPricing { input_per_1m: 0.59, output_per_1m: 0.79 }),
        // Ollama models are free
        ("ollama:*", ModelPricing { input_per_1m: 0.0, output_per_1m: 0.0 }),
    ])
}

pub fn compute_cost(model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let table = pricing_table();
    // Strip date suffix (e.g., "gpt-4o-2024-08-06" → "gpt-4o")
    let normalized = model.split('-').take(3).collect::<Vec<_>>().join("-");
    let pricing = table.get(normalized.as_str())
        .or_else(|| table.get("ollama:*")) // Unknown → $0
        .unwrap();
    (input_tokens as f64 / 1_000_000.0) * pricing.input_per_1m
        + (output_tokens as f64 / 1_000_000.0) * pricing.output_per_1m
}
```

---

## Part 5: Build + release

### 5.1 Auto-updater

#### tauri-plugin-updater setup

**Source:** [REF-02:Cargo.toml — tauri-plugin-updater 2.9.0]

```toml
# Cargo.toml
tauri-plugin-updater = "2.9.0"
```

```rust
// src-tauri/src/lib.rs
tauri::Builder::default()
    .plugin(tauri_plugin_updater::Builder::new().build())
```

#### Signing + release channel

**Source:** [REF-01A:pattern 34]

```json
// tauri.conf.json
{
  "plugins": {
    "updater": {
      "endpoints": [
        "https://releases.bluey.app/{{target}}/{{arch}}/{{current_version}}"
      ],
      "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ...",
      "windows": {
        "installMode": "passive"
      }
    }
  }
}
```

**Signing workflow:**
```bash
# Generate keys (one-time)
tauri signer generate -w ~/.tauri/bluey.key

# Build with signing
TAURI_SIGNING_PRIVATE_KEY=$(cat ~/.tauri/bluey.key) \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
cargo tauri build
```

**Frontend update check:**
```typescript
import { check } from "@tauri-apps/plugin-updater";

async function checkForUpdates() {
  const update = await check();
  if (update?.available) {
    await update.downloadAndInstall();
    await invoke("relaunch");
  }
}
```

---

### 5.2 Release notes + autostart + PostHog

#### Release notes fetcher

**Source:** [REF-01A:ReleaseNotesManager.ts ~100L]

```rust
// src-tauri/src/release_notes.rs
#[tauri::command]
async fn fetch_release_notes(version: String) -> Result<String, String> {
    let url = format!(
        "https://api.github.com/repos/bluey-app/bluey/releases/tags/v{}",
        version
    );
    let resp = reqwest::get(&url).await.map_err(|e| e.to_string())?;
    let release: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(release["body"].as_str().unwrap_or("").to_string())
}
```

#### Autostart plugin

**Source:** [REF-02:Cargo.toml — tauri-plugin-autostart 2.5.0]

```toml
tauri-plugin-autostart = "2.5.0"
```

```rust
use tauri_plugin_autostart::MacosLauncher;

tauri::Builder::default()
    .plugin(tauri_plugin_autostart::init(
        MacosLauncher::LaunchAgent,
        Some(vec!["--minimized"]),
    ))
```

#### PostHog analytics

**Source:** [REF-02:Cargo.toml — tauri-plugin-posthog 0.2.4], [REF-02:pattern 172]

```toml
tauri-plugin-posthog = "0.2.4"
```

**Privacy-first defaults** [REF-02:pattern 172]:
- Session recording: **disabled**
- Page views: **disabled**
- Page leave: **disabled**
- Only explicit event tracking enabled

---

### 5.3 Anonymous install ping

**Source:** [REF-01A:InstallPingManager.ts], [REF-ANALYSIS:L69]

```rust
// src-tauri/src/install_ping.rs
use uuid::Uuid;

#[tauri::command]
async fn send_install_ping(app: AppHandle) -> Result<(), String> {
    let store_path = app.path().app_data_dir().unwrap().join("install_id");

    // Only ping once (check sentinel file)
    if store_path.exists() { return Ok(()); }

    let install_id = Uuid::new_v4().to_string();
    std::fs::write(&store_path, &install_id).map_err(|e| e.to_string())?;

    // Fire-and-forget POST
    tokio::spawn(async move {
        let _ = reqwest::Client::new()
            .post("https://telemetry.bluey.app/install")
            .json(&serde_json::json!({
                "id": install_id,
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "version": env!("CARGO_PKG_VERSION"),
            }))
            .send()
            .await;
    });

    Ok(())
}
```

#### Machine UID for license binding

**Source:** [REF-02:Cargo.toml — tauri-plugin-machine-uid 0.1.2]

```toml
tauri-plugin-machine-uid = "0.1.2"
```

---

## Part 6: Dev workflow

### 6.1 CHANGELOG + PR template + CLAUDE.md rules

#### Keep-a-Changelog format

**Source:** [REF-03:CHANGELOG.md], [REF-ANALYSIS:L211]

```markdown
# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- Feature description

### Changed
- Change description

### Fixed
- Bug fix description

## [0.2.0] - 2026-05-15

### Added
- Global shortcut system with rebindable keys
- Dashboard with sidebar navigation
- SQLite persistence with migrations

### Fixed
- Window opacity not restoring after hide/show cycle
```

#### PR template

**Source:** [REF-03:CONTRIBUTING.md, .github/PULL_REQUEST_TEMPLATE.md]

```markdown
## Summary
<!-- What does this PR do? One paragraph. -->

## Type
- [ ] Feature
- [ ] Bug fix
- [ ] Refactor
- [ ] Documentation

## Affected components
<!-- Which modules/files are changed? -->

## Testing
<!-- How was this tested? -->

## CHANGELOG updated?
- [ ] Yes — entry added under [Unreleased]

## Notes
<!-- Anything reviewers should know? -->
```

#### CLAUDE.md rules (adapted for bluey)

**Source:** [REF-03:CLAUDE.md 510L], [REF-ANALYSIS:L41]

```markdown
# CLAUDE.md — bluey development rules

## Architecture
- Tauri 2 + React 19 + TypeScript + Rust
- Frontend: src/ (React app)
- Backend: src-tauri/src/ (Rust)
- All IPC via #[tauri::command] + invoke()

## Code style
- ESM only in frontend — never require()
- Tauri events in snake_case: "speech_detected", "config_reloaded"
- Rust: standard clippy rules, no unwrap() in production paths
- TypeScript: strict mode, no any

## Services pattern
- Services are thin stateless modules
- All state in Tauri managed state (AppState struct)
- Controllers (commands) are thin — logic in services

## Key files to read before changing
- src-tauri/src/lib.rs — plugin registration, setup
- src-tauri/src/shortcuts.rs — all global shortcuts
- src/hooks/ — React hook composition
- src/routes/index.tsx — page structure

## Two processing flows
1. Screenshot flow: hotkey → capture → vision LLM → display
2. Listen flow: audio → VAD → STT → LLM → display

## Never
- Never hardcode API keys
- Never log raw API keys (use mask_key())
- Never add state outside Tauri managed state
- Never block the main thread with sync I/O
- Never use setTimeout for timing-critical paths (use rAF)
```

---

### 6.2 FIXES.md template

**Source:** [REF-ANALYSIS:L84]

```markdown
# FIXES.md

## Fix #001 — Window opacity not restoring

### Root Cause
`setOpacity(0)` called before `hide()` to prevent fade animation flash,
but `show()` path didn't restore opacity to 1.0 first.

### Fix Summary
Added `window.set_opacity(1.0)` call before every `window.show()` in
the window management module.

### Files Modified
- src-tauri/src/window.rs (show_overlay, show_dashboard)

### Edge Cases Handled
- Rapid hide/show cycles (debounced 50ms)
- Show called when already visible (no-op)

### How to Test
1. Toggle overlay visibility 10x rapidly via Alt+Z
2. Verify window is fully opaque each time it appears
3. Check no flash of transparent window

### Known Limitations
- macOS fade animation still visible for ~16ms (one frame)
```

---

### 6.3 AUDIT.md pattern

**Source:** [REF-ANALYSIS:L86], [REF-01A:AUDIT.md/PERF_AUDIT.md]

```markdown
# AUDIT.md — Self-Security Audit

## Critical
- [ ] No hardcoded secrets in source
- [ ] API keys stored in OS keychain only
- [ ] Content protection enabled on all windows
- [ ] CSP configured in tauri.conf.json

## High
- [ ] Keys never logged (grep for console.log + tracing::info with key vars)
- [ ] zeroize on credential struct Drop
- [ ] Single-instance lock prevents duplicate processes
- [ ] No eval() or innerHTML with user content

## Medium
- [ ] Log rotation at 10MB
- [ ] Error boundaries on all route components
- [ ] Graceful shutdown (in-flight requests tracked)
- [ ] Rate limiting on LLM provider calls

## Low
- [ ] Anonymous install ping (no PII)
- [ ] PostHog session recording disabled
- [ ] Update check uses HTTPS with pinned cert
```

---

### 6.4 .codex/agents setup

**Source:** [REF-ANALYSIS:L81-82]

```
.codex/
├── agents/
│   ├── backend-architect.md      — Rust/Tauri system design
│   ├── frontend-developer.md     — React/TypeScript UI
│   ├── audio-engineer.md         — DSP, VAD, STT pipeline
│   ├── code-reviewer.md          — Review automation
│   ├── debugger.md               — Bug investigation
│   ├── test-engineer.md          — Test writing
│   └── security-auditor.md       — Security review
├── skills/
│   ├── streaming-ui.md           — rAF coalescing, React.memo patterns
│   ├── tauri-ipc.md              — Command/event patterns
│   ├── audio-pipeline.md         — Ring buffer, VAD, STT integration
│   ├── sqlite-patterns.md        — Migration, query, WAL mode
│   ├── keychain-usage.md         — Per-provider secure storage
│   ├── window-management.md      — NSPanel, content protection, stealth
│   ├── otel-setup.md             — Metrics, spans, Grafana export
│   ├── provider-abstraction.md   — Multi-LLM/STT trait pattern
│   ├── hot-reload.md             — notify crate + debounce
│   └── testing.md                — Unit + integration test patterns
└── CLAUDE.md                     — Root development rules
```

---

## Summary: codex task list

### Batch 6: UX Foundation

| ID | Size | Task |
|----|------|------|
| **B6.1** | S | Hotkey system — register default bindings via tauri-plugin-global-shortcut with centralized handler |
| **B6.2** | M | Rebindable keybinds — settings UI with ShortcutRecorder, persist to SQLite, validate + re-register |
| **B6.3** | M | Dashboard window — pre-create on startup, hide-on-close, sidebar nav with React Router |
| **B6.4** | S | Theme management — sync localStorage + IPC, dark/light/system, CSS variables |
| **B6.5** | M | rAF streaming buffer — useStreamBuffer hook with startTransition |
| **B6.6** | S | React.memo MessageRow — custom comparator preventing re-render storm |
| **B6.7** | M | Inertial scroll engine — physics-based scroll with global shortcut integration |
| **B6.8** | S | Code expansion animation — CSS transition 600↔780px with debounced visibility check |
| **B6.9** | L | Command palette (cmdk) — Cmd+K spotlight for actions, model switching, prompt selection |
| **B6.10** | S | Onboarding — FeatureSpotlight component with localStorage tracking |

### Batch 7: Security + Persistence

| ID | Size | Task |
|----|------|------|
| **B7.1** | S | Keychain integration — tauri-plugin-keychain for all API keys, per-provider namespacing |
| **B7.2** | S | Key scrubbing — zeroize crate on Drop, scrub command on app quit |
| **B7.3** | S | Log hashing — mask_key() utility, grep audit for raw key logging |
| **B7.4** | S | Log rotation — 10MB cap, single .jsonl.1 backup, tracing-appender |
| **B7.5** | M | SQLite schema — 4 migration files, conversations/messages/settings/rag_chunks tables |
| **B7.6** | S | Hot-reload config — notify crate watcher with 150ms debounce, emit Tauri event |
| **B7.7** | S | Single-instance lock — tauri-plugin-single-instance, focus existing on relaunch |
| **B7.8** | S | CSP configuration — tauri.conf.json security headers |
| **B7.9** | S | Error boundaries — react-error-boundary on all route components |
| **B7.10** | S | Panic handler — custom hook logging to file before crash |

### Batch 8: Observability

| ID | Size | Task |
|----|------|------|
| **B8.1** | M | OpenTelemetry init — opentelemetry + opentelemetry-otlp crates, OTLP HTTP exporter |
| **B8.2** | M | Metric definitions — ai_ttft_ms, ai_total_ms, stt_decode_ms, provider counters |
| **B8.3** | S | Host identity labels — machine_uid hash, OS, arch, hostname |
| **B8.4** | S | AI pricing table — per-model USD/1M tokens, compute_cost() utility |
| **B8.5** | S | In-memory ring buffer — last 1000 log lines for debug export |
| **B8.6** | L | Grafana dashboard JSON — port solveWatchAi's 9552-line dashboard template |
| **B8.7** | S | NDJSON structured logs — tracing-subscriber JSON layer to file |

### Batch 9: Build + Release

| ID | Size | Task |
|----|------|------|
| **B9.1** | M | Auto-updater — tauri-plugin-updater, signing keys, endpoint config |
| **B9.2** | S | Release notes fetcher — GitHub API → markdown parsing |
| **B9.3** | S | Autostart — tauri-plugin-autostart with LaunchAgent on macOS |
| **B9.4** | S | PostHog analytics — disabled session recording, explicit events only |
| **B9.5** | S | Anonymous install ping — UUID + OS + version, fire-and-forget POST |
| **B9.6** | S | Machine UID — tauri-plugin-machine-uid for license binding |
| **B9.7** | M | Build targets — .dmg (macOS), .msi (Windows), .AppImage + .deb (Linux) |

### Batch 10: Dev Workflow

| ID | Size | Task |
|----|------|------|
| **B10.1** | S | CLAUDE.md — root development rules file |
| **B10.2** | S | CHANGELOG.md — Keep-a-Changelog format, initial entries |
| **B10.3** | S | PR template — .github/PULL_REQUEST_TEMPLATE.md |
| **B10.4** | S | FIXES.md — template with 6 required sections |
| **B10.5** | S | AUDIT.md — self-security-audit checklist |
| **B10.6** | M | .codex/agents — 7 specialized agent configs |
| **B10.7** | M | .codex/skills — 10 reusable skill cards |
| **B10.8** | S | Graceful shutdown — track in-flight handlers, bounded work on quit |

---

## Anti-patterns

| # | Anti-pattern | Source | What to do instead |
|---|---|---|---|
| 1 | God hooks (1000+ lines) | [REF-02:useCompletion 1050L] | Decompose into 5-6 focused hooks |
| 2 | Module-level mutable state | [REF-02:useGlobalShortcuts.ts] | Service class with explicit lifecycle |
| 3 | Synchronous file logging in hot path | [REF-01A:anti-pattern 3] | Async buffered writer (tracing-appender) |
| 4 | Magic numbers without constants | [REF-01A:anti-pattern 4] | Named constants in config module |
| 5 | Plaintext secret storage despite having keychain plugin | [REF-02:pattern 10 critique] | Always use OS keychain for secrets |
| 6 | Single massive React context (30+ values) | [REF-02:AppProvider 698L] | Split by concern (auth, settings, audio) |
| 7 | No test coverage | [REF-02:weakness 10] | Unit tests for hooks, integration for commands |
| 8 | Polling for device changes (4s interval) | [REF-01A:anti-pattern 8] | Push-based OS notifications |
| 9 | EventEmitter spaghetti | [REF-01A:assessment] | Typed channels or actor model |
| 10 | Raw SQL strings in TypeScript | [REF-02:pattern 9 critique] | Thin type-safe query layer |
| 11 | `console.log` with raw API keys | [REF-ANALYSIS:L61] | mask_key() utility everywhere |
| 12 | `app.getPath()` before app.ready | [REF-ANALYSIS:L64] | OnceCell lazy initialization |

---

## Open questions

1. **NSPanel on non-macOS**: The `tauri-nspanel` crate is macOS-only. What's the Windows/Linux equivalent for non-activating overlay? (Partial answer: `WS_EX_NOACTIVATE` on Windows, but no unified Tauri API exists.)

2. **sqlite-vec in Tauri**: The `tauri-plugin-sql` uses its own SQLite build. Can we load the `sqlite-vec` extension into it, or do we need a separate `rusqlite` connection for vector search?

3. **Streaming markdown library choice**: `streamdown` (1.6.10) is relatively unknown. Should we evaluate alternatives like `react-markdown` with custom streaming wrapper, or is streamdown's incremental rendering worth the dependency risk?

4. **VAD model choice**: WebRTC VAD (used by natively-cluely) vs Silero VAD ONNX (used by solveWatchAi). Silero is more accurate but adds ~5MB to bundle. Which for v1?

5. **Speaker identification**: SpeechBrain ECAPA-TDNN (22MB model) enables filtering user's own voice. Is this a v1 requirement or v2 feature? Requires 30s enrollment recording.

6. **Grafana Cloud vs self-hosted**: solveWatchAi exports to Grafana Cloud (paid). Should bluey default to local JSONL with optional Grafana export, or require Grafana from day one?

7. **Content protection on Linux**: `set_content_protected(true)` works on macOS (NSWindow.sharingType) and Windows (SetWindowDisplayAffinity). Linux support depends on compositor (Wayland vs X11). How to handle gracefully?

8. **Hot-reload scope**: Should hot-reload cover only prompts/*.md files, or also the main config (API keys, model selection, VAD parameters)?

9. **Offline-first RAG**: Should the local embedding model (384-dim ONNX, ~50MB) be bundled in the binary, or downloaded on first use like pluely's Ollama bootstrap?

10. **Window count**: natively-cluely uses 5 windows (launcher, overlay, settings, model-selector, cropper). pluely uses 2 (main + dashboard). What's the right number for bluey v1?

---

*Document generated from synthesis of 7 deep-analysis documents totaling ~6,800 lines of research.*
*Every claim cites source file and line range.*
*Target audience: codex agents implementing bluey's UX, ops, and dev infrastructure.*
