# bluey V3 Plan — Appendices

## S1: Architecture Deep-Dive

### 1.1 Process Topology

bluey V3 runs as four cooperating processes on the user's machine, plus cloud API endpoints:

```
┌─────────────────────────────────────────────────────────────────────────┐
│  PROCESS MAP (user's machine)                                           │
│                                                                         │
│  PID 1: cue-daemon (Rust binary, ~8MB)                                 │
│    • Launched by: CLI `bluey on` or launchd/systemd autostart           │
│    • Lifecycle: long-running background process                         │
│    • Owns: audio capture, STT, LLM routing, RAG, session state, IPC    │
│    • Listens: TCP :57321 (CLI), Unix socket (overlay), Tauri bridge     │
│    • Children: spawns overlay process                                   │
│                                                                         │
│  PID 2: Native Overlay (Swift macOS / C Windows, ~2MB)                  │
│    • Launched by: daemon on startup                                     │
│    • Lifecycle: child of daemon, respawned on crash                     │
│    • Owns: rendering, always-on-top, content protection, user input     │
│    • IPC: Unix domain socket to daemon (hot path)                       │
│                                                                         │
│  PID 3: Tauri Dashboard (Rust+WebView, ~15MB)                           │
│    • Launched by: user hotkey Cmd+Shift+D or CLI `bluey dashboard`      │
│    • Lifecycle: created once, hidden on close (instant re-show)         │
│    • Owns: settings UI, session browser, prompt editor, dev tools       │
│    • IPC: Tauri invoke/events to daemon (cold path)                     │
│                                                                         │
│  PID 4: cue-cli (Rust binary, ~4MB)                                    │
│    • Launched by: user in terminal                                      │
│    • Lifecycle: ephemeral per command                                   │
│    • IPC: TCP localhost:57321 to daemon                                 │
│                                                                         │
│  PID 5: cue-audio (Swift/C helper, ~1MB) [optional]                    │
│    • Launched by: daemon when native audio helpers needed               │
│    • Lifecycle: runs during audio capture session                       │
│    • Output: raw f32 PCM to stdout, consumed by daemon                  │
└─────────────────────────────────────────────────────────────────────────┘
```

**Launch sequence:**
1. User runs `bluey on` (CLI) or system autostart triggers daemon
2. Daemon initializes: SQLite, config, HTTP/2 connection pools (prewarmed)
3. Daemon spawns native overlay as child process
4. Daemon opens Unix domain socket at `~/.local/share/bluey/overlay.sock`
5. Overlay connects to socket, sends `Ready` event
6. Daemon begins audio capture (if configured for auto-start)
7. Dashboard is pre-created but hidden (Tauri window with `visible: false`)

**Crash recovery:**
- Overlay crash → daemon detects broken socket, respawns within 500ms
- Daemon crash → overlay exits (parent died), CLI detects on next command
- Dashboard crash → Tauri webview recreated on next Cmd+Shift+D

### 1.2 Data Flow Diagrams

#### Audio Signal Path (mic-to-first-token)

```
Microphone                                                          Overlay
    │                                                                  ▲
    ▼                                                                  │
┌────────┐   f32 PCM    ┌─────────┐  voiced frames  ┌──────────┐     │
│  CPAL  │─────────────▶│  Ring   │────────────────▶│  VAD     │     │
│ capture│  48kHz mono   │  Buffer │  (skip silence) │ RMS + ML │     │
└────────┘               │ 128KB   │                 └────┬─────┘     │
                         └─────────┘                      │           │
                                                          ▼           │
                                                    ┌──────────┐      │
                                                    │ Resample │      │
                                                    │  rubato  │      │
                                                    │ →16kHz   │      │
                                                    └────┬─────┘      │
                                                         │            │
                                                         ▼            │
                                                   ┌───────────┐      │
                                                   │ Deepgram  │      │
                                                   │ Nova-3 WS │      │
                                                   │(persistent)│      │
                                                   └─────┬─────┘      │
                                                         │            │
                                              partial/final transcript │
                                                         │            │
                                                         ▼            │
                                                   ┌───────────┐      │
                                                   │  Stable   │      │
                                                   │  Partial  │      │
                                                   │ Detector  │      │
                                                   └─────┬─────┘      │
                                                         │            │
                                              stable transcript       │
                                                         │            │
                                                         ▼            │
                                                   ┌───────────┐      │
                                                   │  Intent   │      │
                                                   │ Classifier│      │
                                                   │   <5ms    │      │
                                                   └─────┬─────┘      │
                                                         │            │
                                          ┌──────────────┼────────┐   │
                                          ▼              ▼        ▼   │
                                     ┌────────┐   ┌────────┐ ┌──────┐│
                                     │Cerebras│   │ Claude │ │  o3  ││
                                     │  Snap  │   │ Solve  │ │Think ││
                                     │80-300ms│   │0.5-4s  │ │15-60s││
                                     └───┬────┘   └───┬────┘ └──┬───┘│
                                         │            │          │    │
                                         ▼            ▼          ▼    │
                                     ┌────────────────────────────┐   │
                                     │   Unix Socket Hot Path     │───┘
                                     │   Streaming tokens @60Hz   │
                                     └────────────────────────────┘
```

**Latency budget (Snap lane, mic→first-visible-token):**
| Hop | Budget | Cumulative |
|-----|--------|------------|
| CPAL buffer → ring | 5ms | 5ms |
| VAD decision | 10ms | 15ms |
| Resample (rubato) | 2ms | 17ms |
| Network to Deepgram | 30ms | 47ms |
| Deepgram partial latency | 100ms | 147ms |
| Stable-partial detection | 50ms | 197ms |
| Intent classifier | 5ms | 202ms |
| Network to Cerebras | 20ms | 222ms |
| Cerebras TTFT | 80ms | 302ms |
| Unix socket → overlay render | 5ms | 307ms |
| **Total p50** | | **~310ms** |
| **Total p95** | | **~450ms** |

*Source: Deepgram Nova-3 streaming latency benchmarks, Cerebras published TTFT for DeepSeek V3 [CUE-DESIGN-02:signal path diagram]*

#### LLM Query Path

```
Transcript (stable)
    │
    ▼
┌──────────────────┐
│ Intent Classifier │
│ (rule + embedding)│
└────────┬─────────┘
         │
    ┌────┴────┬──────────┐
    ▼         ▼          ▼
┌───────┐ ┌───────┐ ┌───────┐
│ SNAP  │ │ SOLVE │ │ THINK │
└───┬───┘ └───┬───┘ └───┬───┘
    │         │          │
    ▼         ▼          ▼
┌───────┐ ┌───────┐ ┌───────┐
│Context│ │Context│ │Context│
│Assembly│ │Assembly│ │Assembly│
│Fresh  │ │RAG    │ │Full+  │
│3 turns│ │filtered│ │Epochs │
└───┬───┘ └───┬───┘ └───┬───┘
    │         │          │
    ▼         ▼          ▼
┌───────┐ ┌───────┐ ┌───────┐
│Prompt │ │Prompt │ │Prompt │
│Compose│ │Compose│ │Compose│
│TINY   │ │Full+  │ │Full+  │
│<500tok│ │Skill  │ │Think  │
└───┬───┘ └───┬───┘ └───┬───┘
    │         │          │
    ▼         ▼          ▼
┌───────┐ ┌───────┐ ┌───────┐
│HTTP/2 │ │HTTP/2 │ │HTTP/2 │
│Cerebras│ │Claude │ │OpenAI │
│Pool   │ │Pool   │ │Pool   │
└───┬───┘ └───┬───┘ └───┬───┘
    │         │          │
    ▼         ▼          ▼
┌──────────────────────────┐
│  Stream Emitter (60Hz)   │
│  Batches tokens, emits   │
│  via Unix socket or      │
│  Tauri event             │
└──────────────────────────┘
```

#### Session State Path

```
User action (new session / switch / archive)
    │
    ▼
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  CLI / Dash  │────▶│   Daemon     │────▶│   SQLite     │
│  (command)   │     │  (handler)   │     │   (WAL)      │
└──────────────┘     └──────┬───────┘     └──────────────┘
                            │
                   ┌────────┼────────┐
                   ▼        ▼        ▼
              ┌────────┐┌────────┐┌────────┐
              │Overlay ││Dashboard││ Metrics│
              │SessionChanged│SessionChanged││ counter│
              │(socket)│(event) ││         │
              └────────┘└────────┘└────────┘
```

### 1.3 IPC Contracts

#### Native Overlay ↔ Daemon: Unix Domain Socket Protocol

**Transport**: Unix domain socket at `~/.local/share/bluey/overlay.sock`
**Framing**: Length-prefixed JSON (4-byte little-endian length + UTF-8 JSON payload)
**Direction**: Bidirectional

**Daemon → Overlay messages:**

```json
// Stream tokens (hot path, ~60 per second during generation)
{ "type": "StreamToken", "generation_id": 42, "text": "Here is the answer..." }

// Generation complete
{ "type": "StreamComplete", "generation_id": 42 }

// Full card push (non-streaming)
{ "type": "PushCard", "card": { "id": "uuid", "kind": "answer", "title": "...", "body": "...", "lane": "snap", "model": "deepseek-v3" } }

// Session changed
{ "type": "SessionChanged", "session_id": "uuid", "name": "Technical Interview" }

// Mode indicator update
{ "type": "ModeUpdate", "lane": "solve", "model": "claude-sonnet-4-5", "state": "streaming" }

// Patch-mode diff
{ "type": "PatchDiff", "generation_id": 42, "ops": [
  { "op": "KEEP", "block_id": 0 },
  { "op": "MODIFY", "block_id": 1, "content": "updated text..." },
  { "op": "ADD", "after_block": 1, "content": "new section..." },
  { "op": "REMOVE", "block_id": 3 }
]}

// Visibility control
{ "type": "SetVisible", "visible": true }
{ "type": "SetOpacity", "opacity": 0.85 }

// Clear display
{ "type": "Clear" }
```

**Overlay → Daemon messages:**

```json
// Ready signal after connection
{ "type": "Ready", "platform": "macos", "version": "1.0.0" }

// User typed a question in composer
{ "type": "UserQuery", "text": "explain binary search", "mode": "auto" }

// Manual lane override
{ "type": "LaneOverride", "lane": "think" }

// User action
{ "type": "Action", "action": "recap" | "screenshot" | "new_session" | "toggle_listen" }

// Window state
{ "type": "WindowState", "visible": true, "collapsed": false, "opacity": 0.85 }
```

#### Daemon ↔ Tauri Dashboard: Commands + Events

**Tauri Commands (invoke, cold path 10-30ms):**

```rust
// Session management
#[tauri::command] async fn list_sessions() -> Vec<SessionSummary>;
#[tauri::command] async fn get_session(id: String) -> SessionDetail;
#[tauri::command] async fn create_session(name: String) -> SessionSummary;
#[tauri::command] async fn archive_session(id: String) -> Result<()>;
#[tauri::command] async fn switch_session(id: String) -> Result<()>;

// Settings
#[tauri::command] async fn get_settings() -> Settings;
#[tauri::command] async fn update_settings(patch: SettingsPatch) -> Result<()>;

// Provider management
#[tauri::command] async fn list_providers() -> Vec<ProviderStatus>;
#[tauri::command] async fn test_connection(provider: String) -> ConnectionResult;
#[tauri::command] async fn update_api_key(provider: String, key: String) -> Result<()>;

// Prompt/skill management
#[tauri::command] async fn list_skills() -> Vec<SkillTemplate>;
#[tauri::command] async fn update_skill(id: String, template: SkillTemplate) -> Result<()>;

// Audio control
#[tauri::command] async fn audio_status() -> AudioStatus;
#[tauri::command] async fn toggle_audio(source: AudioSource) -> Result<()>;

// Dev tools
#[tauri::command] async fn get_metrics() -> MetricsSnapshot;
#[tauri::command] async fn get_latency_trace(generation_id: u64) -> LatencyTrace;
```

**Tauri Events (emit, daemon → dashboard):**

```rust
// Real-time streaming (mirrored from overlay for dashboard live view)
app.emit("llm-token", StreamChunk { generation_id, text });
app.emit("llm-complete", generation_id);
app.emit("llm-error", ErrorInfo { generation_id, message });

// State changes
app.emit("session-changed", SessionSummary { id, name, state });
app.emit("audio-status", AudioStatus { system: bool, mic: bool, stt_state });
app.emit("stt-transcript", TranscriptEvent { text, is_final, speaker });
app.emit("lane-active", LaneEvent { lane, model, state });

// Cost tracking
app.emit("cost-update", CostEvent { session_total, last_query_cost });

// Provider health
app.emit("provider-status", ProviderHealth { name, state, latency_ms });
```

#### Daemon → Overlay: Unix Socket Framing Detail

```
┌──────────────────────────────────────────┐
│  Frame Layout (wire format)              │
├──────────────────────────────────────────┤
│  Bytes 0-3: payload length (u32 LE)      │
│  Bytes 4-N: UTF-8 JSON payload           │
└──────────────────────────────────────────┘

Max frame size: 64KB (enforced, reject larger)
Keepalive: daemon sends {"type":"Ping"} every 5s
Overlay responds: {"type":"Pong"}
Timeout: 15s without Pong → daemon respawns overlay
```

### 1.4 Database Schema (SQLite, WAL mode)

```sql
-- Core session management (CM1)
CREATE TABLE sessions (
    id          TEXT PRIMARY KEY,  -- UUID v4
    name        TEXT NOT NULL DEFAULT 'Untitled',
    state       TEXT NOT NULL DEFAULT 'active',  -- active|paused|archived
    created_at  INTEGER NOT NULL,  -- epoch ms
    updated_at  INTEGER NOT NULL,
    archived_at INTEGER,
    total_cost  REAL NOT NULL DEFAULT 0.0,  -- USD
    turn_count  INTEGER NOT NULL DEFAULT 0,
    metadata    TEXT  -- JSON blob for extensibility
);
CREATE INDEX idx_sessions_state ON sessions(state);
CREATE INDEX idx_sessions_updated ON sessions(updated_at DESC);

-- Turn-level message storage (CM1)
CREATE TABLE turns (
    id            TEXT PRIMARY KEY,  -- UUID v4
    session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role          TEXT NOT NULL,  -- user|assistant|system
    content       TEXT NOT NULL,
    lane          TEXT,  -- snap|solve|think (NULL for user turns)
    model         TEXT,  -- model ID used
    created_at    INTEGER NOT NULL,
    latency_ms    INTEGER,  -- TTFT for assistant turns
    token_input   INTEGER,
    token_output  INTEGER,
    cost          REAL,  -- USD for this turn
    is_patch      INTEGER NOT NULL DEFAULT 0,  -- 1 if patch-mode follow-up
    parent_turn   TEXT REFERENCES turns(id),  -- for follow-up chains
    metadata      TEXT  -- JSON: patch_ops, thinking_tokens, etc.
);
CREATE INDEX idx_turns_session ON turns(session_id, created_at);
CREATE INDEX idx_turns_lane ON turns(lane);

-- RAG vector store (B5.1)
CREATE TABLE chunks (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    embedding   BLOB,  -- f32 vector via sqlite-vec
    chunk_type  TEXT NOT NULL,  -- transcript|summary|context|document
    created_at  INTEGER NOT NULL,
    token_count INTEGER NOT NULL
);
CREATE INDEX idx_chunks_session ON chunks(session_id);

-- Epoch summaries (CM3)
CREATE TABLE epoch_summaries (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    summary     TEXT NOT NULL,
    turn_start  TEXT NOT NULL REFERENCES turns(id),
    turn_end    TEXT NOT NULL REFERENCES turns(id),
    token_count INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_epochs_session ON epoch_summaries(session_id);

-- Provider configuration
CREATE TABLE providers (
    name        TEXT PRIMARY KEY,  -- cerebras|anthropic|openai|deepgram|groq
    api_key     TEXT,  -- encrypted at rest via keychain
    base_url    TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    config      TEXT  -- JSON: model overrides, rate limits, etc.
);

-- Skill templates (R4)
CREATE TABLE skills (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    lane        TEXT NOT NULL,  -- snap|solve|think|auto
    system_prompt TEXT NOT NULL,
    user_template TEXT,
    category    TEXT,  -- coding|behavioral|system_design|general
    is_builtin  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

-- Settings (single row, JSON columns)
CREATE TABLE settings (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    audio       TEXT NOT NULL DEFAULT '{}',
    display     TEXT NOT NULL DEFAULT '{}',
    shortcuts   TEXT NOT NULL DEFAULT '{}',
    providers   TEXT NOT NULL DEFAULT '{}',
    privacy     TEXT NOT NULL DEFAULT '{}',
    updated_at  INTEGER NOT NULL
);

-- Metrics/cost tracking (B8.4)
CREATE TABLE cost_log (
    id          TEXT PRIMARY KEY,
    session_id  TEXT REFERENCES sessions(id),
    provider    TEXT NOT NULL,
    lane        TEXT NOT NULL,
    tokens_in   INTEGER NOT NULL,
    tokens_out  INTEGER NOT NULL,
    cost_usd    REAL NOT NULL,
    cached      INTEGER NOT NULL DEFAULT 0,  -- prompt cache hit
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_cost_session ON cost_log(session_id);
CREATE INDEX idx_cost_date ON cost_log(created_at);

-- Schema version tracking
CREATE TABLE migrations (
    version     INTEGER PRIMARY KEY,
    applied_at  INTEGER NOT NULL,
    description TEXT
);
```

**FK relationships:**
```
sessions 1──────* turns
sessions 1──────* chunks
sessions 1──────* epoch_summaries
sessions 1──────* cost_log
turns    1──────? turns (parent_turn self-ref for follow-ups)
```

### 1.5 State Machines

#### Session Lifecycle

```
                    ┌─────────┐
         create     │   NEW   │
        ─────────▶  │(transient)│
                    └────┬────┘
                         │ first turn received
                         ▼
                    ┌─────────┐
              ┌────│  ACTIVE  │◄───────────────┐
              │    └────┬────┘                 │
              │         │                      │
    user archives       │ user pauses    user resumes
              │         ▼                      │
              │    ┌─────────┐                 │
              │    │  PAUSED  │────────────────┘
              │    └────┬────┘
              │         │ user archives
              ▼         ▼
         ┌──────────────────┐
         │     ARCHIVED     │
         │  (read-only)     │
         └──────────────────┘

Transitions:
  NEW → ACTIVE:     automatic on first user turn
  ACTIVE → PAUSED:  user action (Cmd+Shift+P or dashboard)
  PAUSED → ACTIVE:  user action (resume)
  ACTIVE → ARCHIVED: user action (archive)
  PAUSED → ARCHIVED: user action (archive)
  ARCHIVED → *:     NOT ALLOWED (terminal state)
```

#### STT Connection State Machine

```
                         ┌────────────────┐
            start()      │  DISCONNECTED  │
           ─────────────▶│                │
                         └───────┬────────┘
                                 │ WebSocket connect
                                 ▼
                         ┌────────────────┐
                    ┌───▶│  CONNECTING    │
                    │    └───────┬────────┘
                    │            │ WS open + auth OK
                    │            ▼
                    │    ┌────────────────┐
                    │    │   CONNECTED    │◄──────────────┐
                    │    └───────┬────────┘               │
                    │            │                        │
                    │    retryable error          first final transcript
                    │    (net/5xx/429/WS drop)    resets error counter
                    │            │                        │
                    │            ▼                        │
                    │    ┌────────────────┐               │
                    └────│ RECONNECTING   │───────────────┘
                         │ (backoff wait) │
                         └───────┬────────┘
                                 │ consecutive errors > 5
                                 ▼
                         ┌────────────────┐
                         │    FAILED      │
                         │ (fatal: auth/  │
                         │  quota/config) │
                         └────────────────┘

Backoff: min(30s × 2^(failures-1), 600s)
Source: CUE-REF-01A ai.service.js:L420, CUE-REFERENCE-ANALYSIS pattern #14
```

#### Lane Routing Decision Tree

```
User input arrives (stable transcript or typed query)
    │
    ├─ Check manual override flags:
    │   ├─ Alt+F pressed or /snap prefix → FORCE SNAP
    │   ├─ Alt+D pressed or /think /deep prefix → FORCE THINK
    │   └─ No override → continue to classifier
    │
    ├─ Rule-based pre-filter (<1ms):
    │   ├─ Length < 20 chars AND no code keywords → SNAP (confidence 0.95)
    │   ├─ Contains "think harder" / "explain in detail" → THINK (0.9)
    │   ├─ Is follow-up to previous answer → check F4 follow-up classifier
    │   └─ No rule match → embedding classifier
    │
    ├─ Embedding classifier (<5ms):
    │   ├─ Compute query embedding (cached model, 384-dim)
    │   ├─ Compare against lane centroids (precomputed)
    │   ├─ confidence ≥ 0.8 for Snap → SNAP
    │   ├─ confidence ≥ 0.7 for Solve → SOLVE
    │   ├─ confidence ≥ 0.6 for Think → THINK
    │   └─ Ambiguous (all < 0.6) → PARALLEL (Snap + Solve)
    │
    └─ PARALLEL mode:
        ├─ Dispatch Snap immediately (show result in <300ms)
        ├─ Dispatch Solve in parallel
        └─ When Solve arrives: replace Snap result with progressive fade
```

### 1.6 Critical Paths with Latency Budgets

| Path | Target p50 | Target p95 | Bottleneck |
|------|-----------|-----------|------------|
| Mic → STT partial | 150ms | 250ms | Deepgram network |
| STT stable → Snap complete | 200ms | 350ms | Cerebras TTFT |
| STT stable → Solve first token | 600ms | 1200ms | Claude TTFT |
| STT stable → Think progress | 1500ms | 3000ms | o3 thinking start |
| Overlay render (socket → pixels) | 3ms | 8ms | NSView redraw |
| Intent classifier | 2ms | 5ms | Embedding lookup |
| Session switch (UI update) | 20ms | 50ms | SQLite + socket |
| Dashboard open (first paint) | 80ms | 150ms | Pre-created window |

### 1.7 Failure Modes + Recovery

| Failure | Detection | Recovery | User Impact |
|---------|-----------|----------|-------------|
| Deepgram WS drop | Socket close event | Reconnect with backoff (L1), buffer audio during reconnect | 1-3s gap in transcription |
| Cerebras timeout | 5s no response | Fallback to Groq (same lane) | +200ms latency on that query |
| Claude rate limit | 429 response | Queue + retry after header delay, or fallback to Gemini 2.5 Pro | +500ms on first retry |
| o3 timeout | 90s no response | Fallback to Claude Opus with extended thinking | User sees progress bar reset |
| Overlay crash | Broken socket pipe | Respawn overlay process within 500ms | Brief flicker, state preserved |
| SQLite locked | SQLITE_BUSY | WAL mode + busy_timeout=5000ms handles most cases | Transparent to user |
| Network loss | All providers fail | Switch to offline mode (local answers only via intelligence.rs) | Degraded quality, instant response |
| Disk full | Write fails | Alert user, stop recording, continue with in-memory state | Session not persisted until space freed |
| API key invalid | 401 from provider | Mark provider as failed, try next in fallback chain | Transparent if fallback available |
| Memory pressure | RSS > 500MB | Flush RAG cache, compact epoch summaries, GC old chunks | Slight latency increase |


### 1.8 Hot Path Protocol: Streaming Token Delivery

The critical performance path is daemon → overlay token streaming. This must sustain 60Hz delivery with <5ms per-frame overhead.

**Wire protocol detail:**

```
Daemon streaming loop (Rust):
┌─────────────────────────────────────────────────────────────┐
│  loop {                                                      │
│    tokio::select! {                                          │
│      token = stream.next() => {                              │
│        buffer.push_str(&token);                              │
│      }                                                       │
│      _ = tick_60hz.tick() => {                               │
│        if !buffer.is_empty() {                               │
│          let frame = Frame::StreamToken {                     │
│            generation_id,                                    │
│            text: mem::take(&mut buffer),                      │
│          };                                                  │
│          let json = serde_json::to_vec(&frame)?;             │
│          let len = (json.len() as u32).to_le_bytes();        │
│          socket.write_all(&len).await?;                      │
│          socket.write_all(&json).await?;                     │
│        }                                                     │
│      }                                                       │
│      _ = cancel.cancelled() => break,                        │
│    }                                                         │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘

Overlay receive loop (Swift/C):
┌─────────────────────────────────────────────────────────────┐
│  while connected {                                           │
│    read 4 bytes → payload_len                                │
│    read payload_len bytes → json_data                        │
│    parse JSON → message                                      │
│    switch message.type {                                     │
│      case .StreamToken:                                      │
│        appendToCurrentCard(message.text)                      │
│        scheduleRedraw()  // coalesced via CADisplayLink       │
│      case .PatchDiff:                                        │
│        applyDiffOps(message.ops)                             │
│        animateChanges()                                      │
│      case .ModeUpdate:                                       │
│        updateBadge(message.lane, message.model)              │
│    }                                                         │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘
```

**Why Unix socket over stdin/stdout (V2 → V3 upgrade):**

| Aspect | stdin/stdout (V2) | Unix socket (V3) |
|--------|-------------------|-------------------|
| Latency | 5-15ms (pipe buffering) | 1-3ms (direct) |
| Bidirectional | Awkward (separate streams) | Natural (single fd) |
| Reconnection | Impossible (process restart) | Reconnect to same path |
| Multiple clients | Impossible | Multiple connections supported |
| Backpressure | Pipe buffer fills, blocks | Non-blocking with flow control |
| Windows equivalent | Named pipe `\\.\pipe\bluey` | Named pipe (same semantics) |

*Source: CUE-DESIGN-02 signal path analysis, CUE-BLUEY-V3-PLAN-SKELETON L5 task description*

### 1.9 Configuration System

```
~/.local/share/bluey/
├── bluey.db              # SQLite database (sessions, turns, chunks, cost_log)
├── overlay.sock          # Unix domain socket (runtime only)
├── daemon.pid            # PID file for single-instance lock
├── config.json           # Non-sensitive settings (theme, shortcuts, audio prefs)
└── logs/
    ├── daemon.log        # Current log (NDJSON format)
    ├── daemon.log.1      # Rotated (10MB max per file)
    └── overlay.log       # Overlay process logs

macOS Keychain / Windows Credential Vault:
├── bluey.cerebras.api_key
├── bluey.anthropic.api_key
├── bluey.openai.api_key
├── bluey.deepgram.api_key
└── bluey.groq.api_key
```

**Config loading order:**
1. Defaults (compiled into binary)
2. `config.json` (user overrides)
3. Environment variables (`BLUEY_*` prefix)
4. CLI flags (highest priority)

**Hot-reload**: `config.json` is watched via `notify` crate. Changes apply without restart (except provider keys which require re-auth).

### 1.10 Tauri Bridge Architecture

The Tauri dashboard runs in the same process as the daemon (shared Rust binary). This is NOT a separate process — it's a WebView managed by the Tauri runtime within the daemon process.

```
┌─────────────────────────────────────────────────────────────┐
│  cue-daemon binary (single process)                          │
│                                                              │
│  ┌──────────────────────┐  ┌──────────────────────────────┐ │
│  │  Daemon Core         │  │  Tauri Runtime               │ │
│  │  (tokio runtime)     │  │  (manages WebView)           │ │
│  │                      │  │                              │ │
│  │  • Audio pipeline    │  │  • Window management         │ │
│  │  • STT connection    │  │  • IPC bridge (invoke/event) │ │
│  │  • LLM routing       │◄─┤  • Plugin system             │ │
│  │  • Session state     │──▶│  • Global shortcuts          │ │
│  │  • RAG engine        │  │                              │ │
│  │  • Socket server     │  │  WebView (React 19):         │ │
│  │  • TCP server        │  │  • Dashboard UI              │ │
│  │                      │  │  • Settings pages            │ │
│  └──────────────────────┘  │  • Session browser           │ │
│                             └──────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘

Tauri invoke flow:
  React calls: invoke('list_sessions')
    → Tauri IPC bridge (WebView → Rust, ~1ms)
    → #[tauri::command] handler
    → Accesses shared AppState (Arc<Mutex<...>>)
    → Returns serialized result
    → React receives JSON response
  Total: 10-30ms depending on DB query

Tauri event flow:
  Daemon emits: app.emit("llm-token", chunk)
    → Tauri serializes to JSON
    → Pushes to WebView event queue
    → React listener fires
  Total: 5-15ms
```

**Why single-binary (not separate processes):**
- Shared memory access (no serialization for internal state)
- Single SQLite connection (no locking issues)
- Simpler deployment (one binary to ship)
- Tauri's architecture naturally embeds in the host process

*Source: CUE-REF-02 (pluely) Tauri 2 architecture, CUE-DESIGN-04 Part 1*

### 1.11 HTTP/2 Connection Pool Design (L2)

```
┌─────────────────────────────────────────────────────────────┐
│  Connection Pool Manager                                     │
│                                                              │
│  Pool 1: Cerebras (Snap)                                     │
│  ├─ Connection A: HTTP/2 multiplexed, prewarmed              │
│  ├─ Connection B: standby (activated on A failure)           │
│  └─ Health check: GET /health every 30s                      │
│                                                              │
│  Pool 2: Anthropic (Solve)                                   │
│  ├─ Connection A: HTTP/2 multiplexed, prewarmed              │
│  ├─ Connection B: standby                                    │
│  └─ Health check: lightweight request every 60s              │
│                                                              │
│  Pool 3: OpenAI (Think)                                      │
│  ├─ Connection A: HTTP/2 multiplexed, prewarmed              │
│  ├─ Connection B: standby                                    │
│  └─ Health check: lightweight request every 60s              │
│                                                              │
│  Prewarm strategy:                                           │
│  • On daemon start: establish all 3 primary connections      │
│  • TLS handshake + HTTP/2 SETTINGS frame completed           │
│  • First real request avoids cold-start penalty (~200ms)     │
│  • Connections kept alive via HTTP/2 PING frames             │
│  • Reconnect on idle timeout (provider-specific, 60-300s)    │
└─────────────────────────────────────────────────────────────┘
```

**Latency savings from prewarming:**
| Without pool | With pool | Savings |
|-------------|-----------|---------|
| DNS: 20ms | 0ms (cached) | 20ms |
| TCP: 30ms | 0ms (reused) | 30ms |
| TLS: 80ms | 0ms (reused) | 80ms |
| HTTP/2 negotiate: 20ms | 0ms (established) | 20ms |
| **Total cold-start avoided** | | **150ms** |

*Source: CUE-BLUEY-V3-PLAN-SKELETON L2 task, CUE-DESIGN-03 HTTP client design*

## S2: Competitive Analysis

### 2.1 Product Landscape

The AI interview/conversation copilot space has 9 notable competitors as of May 2026. Analysis based on source code review (CUE-REF-01A through CUE-REF-06), public documentation, and feature comparison.

### 2.2 Feature Matrix

| Feature | bluey V3 | Cluely (natively) | Final Round AI | LockedIn AI | Interview Coder | natively-cluely (OSS) | pluely | solveWatchAi | Vysper | Aura |
|---------|----------|-------------------|----------------|-------------|-----------------|----------------------|--------|--------------|--------|------|
| **Stealth** | | | | | | | | | | |
| Content protection | ✅ Both platforms | ✅ Both | ❌ | 🟡 macOS only | ❌ | ✅ Both | ✅ macOS | ❌ | ❌ | ❌ |
| Process masquerading | ✅ 3 presets | ✅ 7-step | ❌ | ❌ | ❌ | ✅ 7-step | ❌ | ❌ | ❌ | ❌ |
| Screen-share detection | ✅ Planned | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Always-on-top | ✅ NSPanel | ✅ NSPanel | ✅ | ✅ | ✅ | ✅ NSPanel | ✅ NSPanel | ✅ | ✅ | ❌ |
| Click-through | ✅ Planned | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| **STT** | | | | | | | | | | |
| Provider | Deepgram Nova-3 | 9 providers | Unknown (cloud) | Deepgram | None (text input) | 9 providers | REST (custom) | MLX Whisper + Deepgram | Azure Speech | Deepgram |
| Streaming WebSocket | ✅ Persistent | ✅ Per-session | ❌ REST | ✅ | N/A | ✅ | ❌ REST batch | ✅ Local | ✅ | ✅ |
| Speaker ID | ✅ ECAPA-TDNN | ❌ | ❌ | ❌ | N/A | ❌ | ❌ | ✅ SpeechBrain | ❌ | ❌ |
| VAD | ✅ Two-stage | ✅ Two-stage | Unknown | ❌ | N/A | ✅ Two-stage | ✅ Single-stage | ✅ Silero | ❌ | ❌ |
| **LLM** | | | | | | | | | | |
| Providers supported | 5+ (Cerebras, Claude, o3, Groq, Gemini) | 7 (OpenAI, Claude, Gemini, Groq, Ollama, cURL, Pro) | 1 (GPT-4) | 1 (GPT-4) | 1 (GPT-4) | 7 | 2 (API + cURL) | 3 (OpenAI, Groq, Ollama) | 1 (Azure OpenAI) | 4 (OpenAI, Claude, Gemini, Groq) |
| Query-type routing | ✅ Three-lane | ❌ Single model | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Sub-300ms responses | ✅ Snap lane | ❌ ~2s min | ❌ ~3s | ❌ ~2s | ❌ | ❌ ~2s | ❌ ~3s | 🟡 Local Whisper fast | ❌ ~2s | ❌ ~2.5s |
| Extended thinking | ✅ Think lane (o3) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Prompt caching | ✅ Anthropic cache | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Follow-ups** | | | | | | | | | | |
| Patch mode (diff) | ✅ PATCH/KEEP/MODIFY/ADD/REMOVE | ❌ Full replace | ❌ Full replace | ❌ Full replace | ❌ | ❌ Full replace | ❌ Full replace | ❌ Full replace | ❌ Full replace | ❌ Full replace |
| Progressive enhancement | ✅ Snap→Solve upgrade | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Memory/RAG** | | | | | | | | | | |
| Local RAG | ✅ sqlite-vec | ❌ Cloud only | ❌ | ❌ | ❌ | ✅ sqlite-vec | ❌ | ❌ | ❌ | ❌ |
| Epoch summarization | ✅ Background job | ❌ Truncation | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Context window mgmt | ✅ Per-lane strategies | 🟡 Basic truncation | ❌ | ❌ | ❌ | 🟡 Basic | ❌ | 🟡 Rolling window | ❌ | ❌ |
| **Platform** | | | | | | | | | | |
| macOS | ✅ Native Swift | ✅ Electron | ✅ Web | ✅ Electron | ✅ Electron | ✅ Electron | ✅ Tauri | ✅ Electron | ✅ Electron | ✅ Python |
| Windows | ✅ Native C | ✅ Electron | ✅ Web | ❌ | ✅ Electron | ✅ Electron | 🟡 Partial | ✅ Electron | ❌ | ❌ |
| Linux | 🟡 Planned | ✅ Electron | ✅ Web | ❌ | ❌ | ✅ Electron | ✅ Tauri | ❌ | ❌ | ❌ |
| **Other** | | | | | | | | | | |
| Local inference | ❌ (by design) | 🟡 Ollama optional | ❌ | ❌ | ❌ | 🟡 Ollama | ❌ | ✅ MLX Whisper | ❌ | ❌ |
| Skill templates | ✅ 9+ extensible | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ System prompts | ❌ | ✅ 9 templates | ❌ |
| Open source | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ GPL-3.0 | ✅ GPL-3.0 | ✅ MIT | ✅ | ✅ MIT |
| Pricing | TBD ($29/mo target) | $15-25/mo | $10-15/mo | $20/mo | $60 one-time | Free (self-host) | Free (self-host) | Free (self-host) | Free (self-host) | Free (self-host) |
| Binary size | ~30MB | ~270MB (Electron) | N/A (web) | ~250MB | ~200MB | ~270MB | ~10MB | ~180MB | ~150MB | ~50MB (Python) |
| Cost/3hr session | $1-4 | $15-25 (subscription) | $10-15 | $10-15 | N/A | $5-10 (user keys) | $3-8 | $2-5 (local STT) | $3-8 | $3-8 |

*Sources: CUE-REF-01A (natively-cluely), CUE-REF-02 (pluely), CUE-REF-03 (solveWatchAi), CUE-REF-04 (Aura), CUE-REF-05 (Vysper), CUE-REF-06 (OpenCluely), CUE-REFERENCE-ANALYSIS patterns #1-188*

### 2.3 Latency Comparison

| Product | Mic → First Token (p50) | Architecture Bottleneck |
|---------|------------------------|------------------------|
| **bluey V3 (Snap)** | **~310ms** | Deepgram streaming + Cerebras |
| **bluey V3 (Solve)** | **~800ms** | Deepgram + Claude TTFT |
| Cluely | ~2500ms | REST STT + single OpenAI call |
| Final Round AI | ~3000ms | Web-based, REST everything |
| LockedIn AI | ~2000ms | Deepgram WS + GPT-4 |
| natively-cluely | ~2000ms | NAPI bridge + REST STT + OpenAI |
| pluely | ~2500ms | REST batch STT + single provider |
| solveWatchAi | ~1500ms | Local Whisper (fast) + Groq |
| Vysper | ~2000ms | Azure Speech + Azure OpenAI |
| Aura | ~2500ms | Deepgram + multi-provider |

*Source: CUE-BLUEY-V3-PLAN-SKELETON Part 4, CUE-DESIGN-02 latency analysis*

### 2.4 What bluey Does Better Than Each Competitor

| Competitor | bluey's Advantage |
|------------|-------------------|
| **Cluely** | 8× faster conversational responses (Snap lane), 60% cheaper per session, open source, no Electron bloat, patch-mode diffs instead of full regeneration |
| **Final Round AI** | Native desktop app (not web), content protection, 10× faster, local RAG, works offline for basic answers |
| **LockedIn AI** | Cross-platform (Windows), three-lane routing, extended thinking for hard problems, speaker ID, open source |
| **Interview Coder** | Audio-based (not text-only), real-time streaming, multi-provider fallback, 95% cheaper ongoing cost |
| **natively-cluely** | Rust-native (no NAPI bridge), three-lane routing, patch-mode, persistent STT WebSocket, 27× smaller binary |
| **pluely** | Three-lane routing, persistent STT (not REST batch), speaker ID, RAG, extended thinking, Windows support |
| **solveWatchAi** | Three-lane routing, patch-mode, native overlay (not Electron), content protection, skill templates |
| **Vysper** | Multi-provider LLM (not Azure-only), streaming STT, content protection, RAG, cross-platform |
| **Aura** | Native overlay, content protection, three-lane routing, patch-mode, skill templates, production-grade |

### 2.5 Gaps Remaining (vs best-in-class per feature)

| Gap | Best-in-class | bluey Status | Mitigation |
|-----|---------------|--------------|------------|
| Phone mirroring | Cluely (PhoneMirrorService) | Not planned | Low priority — niche use case |
| Calendar integration | Cluely (CalendarManager) | Not planned V3 | Future V4 feature |
| Company dossier | Cluely (KnowledgeOrchestrator) | Not planned | Could add as skill template |
| Local inference | solveWatchAi (MLX Whisper) | Explicitly excluded | Design choice: cloud-first for quality |
| Web client | Final Round AI | Not planned | Desktop-only is a feature (stealth) |
| Team/enterprise features | Cluely (teams, billing) | Not in V3 | Future if productized |

### 2.6 Positioning Statement

> **bluey** is the fastest, most intelligent AI conversation copilot. While competitors use a single LLM for everything (slow, expensive, one-size-fits-none), bluey routes each query to the optimal model in <5ms — delivering sub-300ms conversational answers, streaming technical solutions, and deep extended-thinking for the hardest problems. Patch-mode follow-ups show exactly what changed, not full regenerations. Native overlays on both platforms ensure zero detection risk. Open source, self-hosted, $1-4 per 3-hour session.

**Target user**: Senior engineers in live technical interviews, system design discussions, and coding assessments who need instant, accurate, undetectable assistance across varying difficulty levels.

**Key differentiator**: Three-lane intelligent routing. No other product dispatches queries to different models based on complexity. This is bluey's moat — it's architecturally fundamental, not a feature flag.

### 2.7 Per-Competitor Deep Analysis

#### Cluely (natively-cluely) — The Market Leader

**Architecture**: Electron + React + Rust native module (NAPI bridge)
**Size**: 270MB binary, 42K LOC backend, 204K LOC total
**Strengths**:
- Most complete feature set (9 STT providers, 7 LLM providers, phone mirroring, calendar)
- Battle-tested stealth (7-step masquerading, content protection both platforms)
- Premium features (company dossier, negotiation tracker, personas)
- Large user base provides feedback loop

**Weaknesses**:
- Single-model architecture (no routing) — every query goes to same provider
- ~2500ms mic-to-first-token (NAPI bridge + REST STT + single LLM call)
- Full-replace on follow-ups (regenerates entire answer)
- 270MB binary (Electron bloat)
- Closed source, $15-25/month subscription
- 3894-line god object (LLMHelper.ts) — technical debt

**bluey's edge**: 8× faster on conversational queries, 60% cheaper, open source, patch-mode diffs, three-lane routing means better answers for hard problems AND faster answers for easy ones.

*Source: CUE-REF-01A (3000+ lines of analysis), CUE-REFERENCE-ANALYSIS patterns #1-155*

#### Final Round AI — The Web-Based Competitor

**Architecture**: Web application (no desktop client)
**Strengths**: No install required, works on any platform with a browser
**Weaknesses**:
- No content protection (web apps can't hide from screen recording)
- No system audio capture (browser can't access system audio without extension)
- ~3000ms latency (REST everything, no streaming STT)
- Single LLM provider (GPT-4)
- $10-15/month subscription

**bluey's edge**: Native desktop = content protection + system audio + always-on-top. 10× faster. Works during screen-shared interviews (their primary weakness).

#### solveWatchAi — The Local-First Competitor

**Architecture**: Python + Electron, local MLX Whisper for STT
**Size**: 56K LOC
**Strengths**:
- Local STT (no cloud dependency for transcription)
- Speaker ID via SpeechBrain ECAPA-TDNN
- Silero VAD (good noise rejection)
- MIT license, self-hostable

**Weaknesses**:
- No content protection (Electron without private API)
- No three-lane routing (single model)
- Python backend (slower than Rust for audio processing)
- No patch-mode follow-ups
- Limited to 3 LLM providers

**bluey's edge**: Content protection, three-lane routing, patch-mode, native overlay, Rust performance, 5+ LLM providers with intelligent fallback.

*Source: CUE-REF-03 (1800 lines of analysis)*

#### pluely — The Closest Stack Match

**Architecture**: Tauri 2 + React 19 + Rust (identical stack to bluey)
**Size**: 10MB binary, 27K LOC
**Strengths**:
- Tiny binary (27× smaller than Cluely)
- NSPanel via tauri-nspanel (content protection)
- Clean Rust audio abstraction (SpeakerStream trait)
- GPL-3.0 open source
- Good UI (Radix + shadcn + cmdk)

**Weaknesses**:
- REST-batch STT (not streaming — high latency)
- Single LLM provider per session (no routing)
- No speaker ID
- No RAG / memory
- No extended thinking
- No patch-mode

**bluey's edge**: Streaming STT, three-lane routing, RAG, speaker ID, patch-mode, extended thinking. pluely is a good UI shell but lacks intelligence depth.

*Source: CUE-REF-02 (2000 lines of analysis), CUE-REFERENCE-ANALYSIS patterns #156-188*

#### Vysper — The Skill Template Pioneer

**Architecture**: Electron + Node.js, Azure-only
**Size**: 15K LOC
**Strengths**:
- 9 skill templates (coding, system design, behavioral, etc.)
- LLM-based intelligent question filtering
- Clean prompt architecture

**Weaknesses**:
- Azure-only (single provider, no fallback)
- No content protection
- No streaming STT (Azure continuous recognition)
- macOS only
- No RAG or memory
- No follow-up handling

**bluey's edge**: Multi-provider, content protection, streaming STT, RAG, patch-mode, cross-platform. bluey adopts Vysper's skill template concept (R4) but extends it with lane-aware routing.

*Source: CUE-REF-05 (1000 lines of analysis)*

#### Aura-AI — The Python Lightweight

**Architecture**: Python desktop app, multi-provider
**Size**: 17K LOC
**Strengths**:
- 4 LLM providers with key rotation
- Simple architecture (easy to understand)
- MIT license

**Weaknesses**:
- No content protection (Python can't do NSPanel)
- No overlay (uses system notifications or separate window)
- No streaming STT
- No VAD (sends everything to Deepgram)
- macOS only
- No follow-up handling

**bluey's edge**: Native overlay with content protection, three-lane routing, VAD, streaming STT, patch-mode, cross-platform, production-grade Rust backend.

*Source: CUE-REF-04 (1200 lines of analysis)*

### 2.8 Architecture Comparison

| Aspect | Electron-based (Cluely, solveWatchAi) | Tauri-based (pluely, bluey) | Web-based (Final Round) | Python (Aura) |
|--------|---------------------------------------|----------------------------|------------------------|---------------|
| Binary size | 200-270MB | 10-30MB | 0 (browser) | 50MB+ (with deps) |
| Memory usage | 300-500MB | 80-200MB | Browser tab | 100-200MB |
| Startup time | 3-5s | 1-2s | Instant (if loaded) | 2-3s |
| Content protection | Via Electron API | Via macOS private API | ❌ Impossible | ❌ Impossible |
| Audio access | Via native module (NAPI) | Direct Rust (zero-copy) | ❌ Limited | Via sounddevice |
| IPC overhead | NAPI bridge (~5ms/call) | Direct memory access | N/A | N/A |
| Cross-platform | ✅ (Chromium everywhere) | ✅ (native WebView) | ✅ (browser) | 🟡 (macOS focus) |
| Update size | 200MB+ full binary | 15-30MB | 0 (server-side) | pip install |

**bluey's architectural advantage**: Tauri gives us Electron's cross-platform reach with native performance. The hybrid approach (native overlay + Tauri dashboard) gets the best of both worlds — native rendering speed for the hot path, web tech flexibility for the settings UI.

## S3: Cost Model Deep-Dive

### 3.1 Provider Pricing (May 2026)

| Provider | Model | Input (per 1M tokens) | Output (per 1M tokens) | Cached Input | Notes |
|----------|-------|----------------------|------------------------|--------------|-------|
| **Cerebras** | DeepSeek V3 | $0.20 | $0.60 | N/A | Snap lane primary |
| **Anthropic** | Claude Sonnet 4.5 | $3.00 | $15.00 | $0.30 (90% off) | Solve lane primary |
| **OpenAI** | o3 | $10.00 | $40.00 | N/A | Think lane primary |
| **OpenAI** | o3-mini | $1.10 | $4.40 | N/A | Think lane fallback |
| **Anthropic** | Claude Opus 4 | $15.00 | $75.00 | $1.50 | Think lane fallback |
| **Groq** | Llama 4 Scout | $0.11 | $0.34 | N/A | Snap fallback |
| **Google** | Gemini 2.5 Pro | $1.25 | $10.00 | N/A | Solve fallback |
| **Google** | Gemini 2.5 Flash | $0.15 | $0.60 | N/A | Cost-optimized Solve |
| **Deepgram** | Nova-3 | $0.0043/sec | — | — | ~$0.26/min active speech |
| **OpenAI** | text-embedding-3-small | $0.02 | — | — | RAG embeddings |
| **AssemblyAI** | Universal Streaming | $0.0065/sec | — | — | STT fallback |

*Prices sourced from provider pricing pages as of May 2026. Subject to change.*

### 3.2 Unit Economics: Per-Session Cost Breakdown

#### Scenario: 3-Hour Coding Interview

| Component | Usage | Calculation | Cost |
|-----------|-------|-------------|------|
| **Deepgram STT** | 90 min active speech (VAD filters ~50% silence) | 90 × 60 × $0.0043 | $0.23 |
| **Snap lane** | 40 queries, avg 1.5K in / 800 out | 40 × (1.5K×$0.20 + 0.8K×$0.60) / 1M | $0.03 |
| **Solve lane** | 15 queries, avg 12K in / 3K out | 15 × (12K×$3.00 + 3K×$15.00) / 1M | $1.22 |
| **Solve (cached)** | Same, 60% cache hit on input | 15 × (4.8K×$3.00 + 7.2K×$0.30 + 3K×$15.00) / 1M | $0.92 |
| **Think lane** | 3 queries, avg 25K in / 6K out | 3 × (25K×$10.00 + 6K×$40.00) / 1M | $1.47 |
| **Embeddings** | 200 chunks × 200 tokens | 40K × $0.02 / 1M | $0.001 |
| **Total (no optimization)** | | | **$2.95** |
| **Total (with CO1 caching)** | | | **$2.65** |
| **Total (CO1 + CO2 RAG filter)** | | | **$1.85** |
| **Total (CO1 + CO2 + CO3 split)** | | | **$1.42** |

#### Scenario: 3-Hour System Design Interview

| Component | Usage | Calculation | Cost |
|-----------|-------|-------------|------|
| **Deepgram STT** | 100 min active (more discussion) | 100 × 60 × $0.0043 | $0.26 |
| **Snap lane** | 25 queries (fewer simple Q&A) | 25 × (1.5K×$0.20 + 0.8K×$0.60) / 1M | $0.02 |
| **Solve lane** | 20 queries (more complex) | 20 × (14K×$3.00 + 4K×$15.00) / 1M | $2.04 |
| **Think lane** | 5 queries (architecture decisions) | 5 × (30K×$10.00 + 8K×$40.00) / 1M | $3.10 |
| **Total (no optimization)** | | | **$5.42** |
| **Total (fully optimized)** | | | **$2.80** |

#### Scenario: 3-Hour Technical Phone Screen

| Component | Usage | Calculation | Cost |
|-----------|-------|-------------|------|
| **Deepgram STT** | 70 min active | 70 × 60 × $0.0043 | $0.18 |
| **Snap lane** | 60 queries (mostly conversational) | 60 × (1K×$0.20 + 0.5K×$0.60) / 1M | $0.03 |
| **Solve lane** | 8 queries | 8 × (10K×$3.00 + 2.5K×$15.00) / 1M | $0.54 |
| **Think lane** | 1 query | 1 × (20K×$10.00 + 5K×$40.00) / 1M | $0.40 |
| **Total (no optimization)** | | | **$1.15** |
| **Total (fully optimized)** | | | **$0.72** |

### 3.3 Cost Optimization Levers

| Lever | Task ID | Savings | Mechanism |
|-------|---------|---------|-----------|
| **CO1: Prompt caching** | CO1 | 50-70% on Solve input | Anthropic cache_control breakpoints on system prompt + skill template (stable across turns) |
| **CO2: RAG-filtered context** | CO2 | 30-50% on Solve input | Send only top-5 relevant turns instead of full history (16K → 6K avg) |
| **CO3: Router split** | CO3 | 20-40% on easy Solve | Route "easy" Solve queries (definition lookups, simple explanations) to Cerebras instead of Claude |
| **Speculative cancel** | L4 | 5-10% overall | Cancel in-flight LLM calls when STT partial changes (avoid wasted tokens) |
| **Epoch summarization** | CM3 | 15-25% on long sessions | Compress old turns to summaries, reducing context window for subsequent queries |

**Combined optimization effect:**
- Baseline: $2.95/session (coding interview)
- After all optimizations: $1.42/session (**52% reduction**)

### 3.4 Pricing Tier Recommendations

| Tier | Monthly Price | Included | Cost to Serve | Margin |
|------|--------------|----------|---------------|--------|
| **Free** | $0 | 3 sessions/month, Snap lane only | ~$0.30/session × 3 = $0.90 | -$0.90 (acquisition) |
| **Pro** | $29 | Unlimited sessions, all lanes | ~$1.50/session × 40 = $60 | -$31 (subsidized) |
| **Pro (light user)** | $29 | ~15 sessions/month typical | ~$1.50 × 15 = $22.50 | +$6.50 |
| **BYOK** | $9 | Unlimited, user provides API keys | ~$0.25/session (STT only) × 40 = $10 | -$1 (near break-even) |
| **Team** | $49/seat | + shared prompts, analytics, admin | Same as Pro + $2 infra | Depends on usage |

**Break-even analysis:**
- Pro tier breaks even at ~19 sessions/month ($29 ÷ $1.50)
- Average user does 10-20 sessions/month → Pro tier is viable at 15+ sessions
- Heavy users (40+ sessions) require BYOK tier or usage caps

### 3.5 Scenario Modeling: Scale Economics

#### 100 Users (Early Adopter Phase)

| Metric | Value |
|--------|-------|
| Mix | 60 Pro, 20 BYOK, 20 Free |
| Monthly revenue | 60×$29 + 20×$9 = $1,920 |
| Avg sessions/user/month | 15 |
| Monthly API cost | (60+20)×15×$1.50 + 20×3×$0.30 = $1,818 |
| Gross margin | $102 (5.3%) |
| **Verdict** | Not viable without BYOK emphasis or usage caps |

#### 1,000 Users (Growth Phase)

| Metric | Value |
|--------|-------|
| Mix | 400 Pro, 300 BYOK, 200 Free, 100 Team |
| Monthly revenue | 400×$29 + 300×$9 + 100×$49 = $19,200 |
| Monthly API cost | (400+100)×15×$1.50 + 300×0 + 200×3×$0.30 = $11,430 |
| Infra (servers, monitoring) | $500 |
| Gross margin | $7,270 (37.9%) |
| **Verdict** | Viable with BYOK tier absorbing heavy users |

#### 10,000 Users (Scale Phase)

| Metric | Value |
|--------|-------|
| Mix | 3000 Pro, 4000 BYOK, 2000 Free, 1000 Team |
| Monthly revenue | 3000×$29 + 4000×$9 + 1000×$49 = $172,000 |
| Monthly API cost | (3000+1000)×15×$1.20 + 2000×3×$0.30 = $73,800 |
| Volume discount (negotiated) | -20% = $59,040 |
| Infra | $3,000 |
| Gross margin | $109,960 (63.9%) |
| **Verdict** | Highly viable; volume discounts kick in |

*Note: At 10K users, negotiate dedicated capacity with Cerebras and Anthropic for 20-30% discount.*

### 3.6 BYOK Tier Economics

When users bring their own API keys:
- bluey only pays for: STT (Deepgram, ~$0.25/session) + infra
- User pays directly: LLM costs ($1-4/session to their own accounts)
- bluey charges: $9/month for software + STT
- Break-even: 1 session/month ($9 revenue vs $0.25 cost)
- This tier is **always profitable** and should be the default recommendation for heavy users

### 3.7 Reserve Capacity Considerations

| Provider | Threshold for Negotiation | Expected Discount | Commitment |
|----------|--------------------------|-------------------|------------|
| Cerebras | >$5K/month spend | 20-30% | 6-month minimum |
| Anthropic | >$10K/month spend | 15-25% | Annual contract |
| OpenAI | >$10K/month spend | 10-20% | Annual + volume commit |
| Deepgram | >$3K/month spend | 15-25% | Annual contract |

**Trigger**: Negotiate when reaching 1,000+ paying users (~$10K/month API spend).

### 3.8 Cost Dashboard Spec (What Users See)

```
┌─────────────────────────────────────────────────┐
│  Session Cost: $1.47                            │
│  ├─ STT: $0.23 (Deepgram Nova-3, 87 min)       │
│  ├─ Snap: $0.03 (38 queries, Cerebras)         │
│  ├─ Solve: $0.82 (12 queries, Claude Sonnet)   │
│  │   └─ Cache savings: -$0.34                  │
│  ├─ Think: $0.39 (2 queries, o3)               │
│  └─ Embeddings: $0.001                         │
│                                                 │
│  Monthly Total: $18.42 / $29.00 plan            │
│  ├─ Sessions this month: 14                     │
│  ├─ Avg cost/session: $1.32                     │
│  └─ Projected month-end: $24.50                 │
│                                                 │
│  [View detailed breakdown] [Export CSV]          │
└─────────────────────────────────────────────────┘
```

**Dashboard data source**: `cost_log` SQLite table, aggregated per session and per month.
**Update frequency**: Real-time (each LLM response updates running total via Tauri event `cost-update`).

## S4: Success Metrics + Acceptance Tests

### 4.1 Latency Targets

| Metric | p50 | p95 | p99 | Measurement Point |
|--------|-----|-----|-----|-------------------|
| Mic → STT partial | 150ms | 250ms | 400ms | L6 timestamp: audio_captured → stt_partial_received |
| STT stable → Snap complete | 200ms | 350ms | 500ms | L6: stt_stable → snap_generation_complete |
| STT stable → Solve first token | 600ms | 1200ms | 2000ms | L6: stt_stable → solve_first_token |
| STT stable → Think progress visible | 1500ms | 3000ms | 5000ms | L6: stt_stable → think_progress_shown |
| End-to-end mic → first visible (Snap) | 400ms | 550ms | 700ms | L6: audio_captured → overlay_rendered |
| End-to-end mic → first visible (Solve) | 800ms | 1500ms | 2500ms | L6: audio_captured → overlay_rendered |
| Intent classifier decision | 2ms | 5ms | 10ms | L6: classifier_start → classifier_done |
| Overlay render (socket → pixels) | 3ms | 8ms | 15ms | L6: socket_received → frame_committed |
| Session switch (full UI update) | 20ms | 50ms | 100ms | L6: switch_command → overlay_updated |
| Dashboard open (first paint) | 80ms | 150ms | 300ms | L6: hotkey_pressed → webview_painted |

**Instrumentation**: Task L6 adds timestamps at every hop. Stored in SQLite `latency_traces` table. Exposed via dashboard flamegraph and `bluey metrics` CLI command.

### 4.2 Correctness Targets

#### LeetCode Pass Rates (bluey's generated answers)

| Difficulty | Target Pass Rate | Evaluation Method |
|------------|-----------------|-------------------|
| Easy | ≥95% | Automated: submit to LeetCode API, check accepted |
| Medium | ≥80% | Automated: same |
| Hard | ≥60% | Automated: same |
| System Design | ≥85% (human eval) | Manual: 3 senior engineers score 1-5 on rubric |

**Evaluation corpus**: 50 Easy + 50 Medium + 30 Hard + 20 System Design questions.
**Frequency**: Run full eval suite before each phase ships. Track regression.

#### Intent Classifier Accuracy

| Metric | Target | Evaluation |
|--------|--------|------------|
| Overall accuracy | ≥85% | 500-question held-out test set with ground-truth lane labels |
| Snap precision | ≥90% | False positives (routed to Snap but needed Solve) < 10% |
| Think recall | ≥80% | Hard problems correctly identified as Think-worthy |
| Follow-up detection | ≥90% | Refinement vs new problem classification |

#### Patch-Mode Correctness

| Metric | Target | Evaluation |
|--------|--------|------------|
| Semantic correctness | ≥95% | Patch ops produce valid, meaningful output |
| No content loss | 100% | KEEP blocks never dropped or corrupted |
| Diff rendering accuracy | ≥98% | Visual diff matches actual changes |

### 4.3 Reliability Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Session uptime (no crashes) | ≥99.5% per 3hr session | Crash reports / total sessions |
| Audio recovery from disconnect | <2s | Time from device_lost → audio_resumed |
| Deepgram reconnect time | <3s | Time from ws_close → ws_open (L1) |
| Provider failover success | ≥99% | Fallback chain resolves without user-visible error |
| No message loss on crash | 100% | WAL mode + fsync ensures durability |
| Graceful degradation | 100% | Snap-only mode activates if Solve/Think unavailable |
| SQLite corruption rate | 0% | WAL + integrity checks on startup |

### 4.4 UX Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Three-lane badge accuracy | 100% | Badge always reflects actual lane used |
| Patch diff render correctness | ≥98% | Visual inspection on 100 patch operations |
| Progressive enhancement smoothness | No flicker | Snap→Solve transition uses crossfade animation |
| Manual override response time | <100ms | Alt+D/F → lane switch acknowledged |
| Dashboard responsiveness | <100ms for all interactions | No janky scrolling, instant tab switches |
| Overlay CPU usage (idle) | <1% | Activity Monitor measurement |
| Overlay CPU usage (streaming) | <5% | During active token streaming |

### 4.5 Cost Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Average session cost (coding) | <$2.00 (optimized) | cost_log aggregation |
| Average session cost (system design) | <$3.50 (optimized) | cost_log aggregation |
| Snap query cost | <$0.002 per query | Per-query cost tracking |
| No runaway costs | Max $10/session hard cap | Rate limiter + token budget enforcement |
| Cost dashboard accuracy | ±5% of actual bill | Compare dashboard totals vs provider invoices |

### 4.6 Performance Targets (User's Machine)

| Metric | Target | Measurement |
|--------|--------|-------------|
| Daemon RSS (idle) | <80MB | `ps` measurement after 1hr idle |
| Daemon RSS (active session) | <200MB | During active 3hr session |
| Daemon CPU (idle) | <0.5% | No audio, no queries |
| Daemon CPU (listening) | <3% | Audio capture + VAD active |
| Daemon CPU (generating) | <8% | During LLM streaming |
| Overlay RSS | <30MB | Native process memory |
| Dashboard RSS | <120MB | WebView + React |
| Battery impact (laptop) | <5% per hour active | Measured on M1 MacBook Air |
| Disk usage (SQLite) | <500MB after 100 sessions | With epoch summarization compaction |
| Startup time (cold) | <2s | From `bluey on` to overlay visible |
| Startup time (warm) | <500ms | Daemon already running, show overlay |

### 4.7 Test Suites

#### Unit Tests (Rust: `cargo test`)

| Module | Test Count | Coverage Target | What's Tested |
|--------|-----------|-----------------|---------------|
| `cue-core::ipc` | 30+ | 95% | Message serialization round-trips, all variants |
| `cue-core::ai` | 20+ | 90% | Provider routing logic, budget calculations |
| `cue-daemon::router` | 25+ | 90% | Intent classification, lane selection, override handling |
| `cue-daemon::patch` | 20+ | 95% | PATCH/KEEP/MODIFY/ADD/REMOVE parsing + application |
| `cue-daemon::context` | 15+ | 85% | Context assembly strategies, token counting |
| `cue-daemon::stt` | 10+ | 80% | State machine transitions, backoff calculation |
| `cue-daemon::cost` | 10+ | 90% | Cost calculation accuracy per provider |
| `cue-daemon::db` | 15+ | 85% | Migration system, CRUD operations |

**Run**: `cargo test --workspace` — must pass before every commit.
**CI gate**: All unit tests pass with zero warnings.

#### Integration Tests

| Test | What It Validates | Setup |
|------|-------------------|-------|
| Daemon↔Overlay IPC | Unix socket framing, all message types | Spawn daemon + mock overlay |
| Daemon↔Dashboard IPC | Tauri invoke/event round-trips | Spawn daemon + Tauri test harness |
| STT pipeline | Audio file → Deepgram → transcript | Requires Deepgram API key (CI secret) |
| LLM pipeline | Transcript → router → provider → response | Requires provider API keys |
| Session lifecycle | Create → turns → archive → query | SQLite in-memory |
| RAG pipeline | Index chunks → query → retrieve | sqlite-vec in-memory |
| Fallback chain | Primary fails → fallback succeeds | Mock HTTP server with error injection |
| Cost tracking | Queries → accurate cost_log entries | Mock providers with known token counts |

**Run**: `cargo test --features integration` — requires API keys in env.
**CI gate**: Run nightly (expensive due to API calls).

#### End-to-End Smoke Tests

| Test | Steps | Pass Criteria |
|------|-------|---------------|
| **Cold start** | `bluey on` → verify overlay appears | Overlay visible within 2s |
| **Basic query** | Type question in overlay → get answer | Response within 5s, non-empty |
| **Three-lane routing** | Send simple/medium/hard queries | Each routes to correct lane (check badge) |
| **Patch follow-up** | Ask question → ask refinement | Second response uses PATCH format |
| **Session persistence** | Create session → add turns → restart daemon → verify turns exist | All turns recovered |
| **Provider failover** | Set invalid primary key → query | Fallback provider responds |
| **Audio capture** | Start listening → speak → verify transcript | Transcript appears within 2s |
| **Dashboard** | Cmd+Shift+D → navigate pages | All pages render without errors |
| **Graceful shutdown** | `bluey off` during active session | Session saved, no data loss |
| **Crash recovery** | Kill overlay process | Overlay respawns within 1s |

**Run**: `./scripts/smoke-test.sh` — automated where possible, manual for audio.
**Gate**: Must pass before each phase ships.

#### Latency Benchmarks

| Benchmark | Method | Target |
|-----------|--------|--------|
| Intent classifier throughput | 10,000 classifications, measure p50/p95/p99 | p99 < 10ms |
| Socket round-trip | 10,000 ping/pong messages | p99 < 5ms |
| Context assembly | 100 sessions with 500+ turns each | p99 < 50ms |
| RAG retrieval | 1,000 queries against 10K chunks | p99 < 100ms |
| Token streaming render | 1,000 tokens at 60Hz | Zero dropped frames |

**Run**: `cargo bench` (criterion.rs benchmarks).
**Gate**: No regression >10% from previous run.

#### Correctness Eval Suite

| Suite | Size | Method | Frequency |
|-------|------|--------|-----------|
| LeetCode Easy | 50 problems | Automated submission | Per-phase |
| LeetCode Medium | 50 problems | Automated submission | Per-phase |
| LeetCode Hard | 30 problems | Automated submission | Per-phase |
| System Design | 20 scenarios | Human eval (rubric) | Per-phase |
| Behavioral | 30 questions | Human eval (naturalness) | Per-phase |
| Intent Classification | 500 labeled queries | Automated accuracy check | Per-commit |
| Follow-up Detection | 200 labeled pairs | Automated accuracy check | Per-commit |

### 4.8 Review Checklist (Principal Engineer Gate)

Before merging each phase batch:

**Architecture:**
- [ ] No new IPC message types without schema documentation
- [ ] No synchronous blocking on async runtime
- [ ] All new SQLite tables have migrations + indexes
- [ ] Error types are classified (retryable vs fatal)
- [ ] No unwrap() on fallible operations in production paths

**Performance:**
- [ ] Latency benchmarks show no regression >10%
- [ ] Memory profiling shows no leaks (run 1hr stress test)
- [ ] No allocations in hot path (streaming loop)
- [ ] CPU profiling shows no unexpected hotspots

**Reliability:**
- [ ] All error paths have recovery strategies
- [ ] Provider failures trigger fallback (not panic)
- [ ] SQLite operations use transactions where needed
- [ ] Crash recovery tested (kill -9 during operation)

**Security:**
- [ ] API keys never logged (even at trace level)
- [ ] No secrets in SQLite (use keychain)
- [ ] Content protection verified on both platforms
- [ ] No PII in telemetry events

**Code Quality:**
- [ ] All public APIs documented with rustdoc
- [ ] Unit test coverage ≥80% for new code
- [ ] No clippy warnings
- [ ] CHANGELOG.md updated
- [ ] CLAUDE.md updated if architecture changed

**UX:**
- [ ] Overlay renders correctly at all opacity levels
- [ ] Dashboard pages load without console errors
- [ ] Keyboard shortcuts work in all modes
- [ ] Error states show user-friendly messages

## S5: Risk Register

### 5.1 Technical Risks

| # | Risk | Likelihood | Impact | Mitigation | Early Warning |
|---|------|-----------|--------|------------|---------------|
| T1 | **Cerebras API instability** — Cerebras is newer, may have outages or breaking changes | Medium | High (Snap lane unusable) | Groq as hot fallback (same latency class), auto-switch on 3 consecutive failures | Monitor Cerebras status page; alert on p95 latency >500ms |
| T2 | **Tauri 2 breaking changes** — Tauri 2 is relatively new, plugin ecosystem may shift | Low | Medium (dashboard rebuild) | Pin exact Tauri versions, avoid bleeding-edge plugins, keep dashboard logic in React (portable) | Watch Tauri GitHub releases; subscribe to breaking-change RFCs |
| T3 | **macOS private API removal** — `macos-private-api` feature for NSPanel could break in future macOS | Low | High (overlay unusable on macOS) | Native Swift overlay is independent of Tauri; can fall back to pure AppKit NSPanel (already implemented) | Test on every macOS beta; monitor Apple developer forums |
| T4 | **Deepgram Nova-3 WebSocket protocol change** — Persistent WS relies on specific framing | Low | Medium (STT breaks) | AssemblyAI Universal-Streaming as fallback; abstract behind SttProvider trait | Pin Deepgram SDK version; integration test against live API weekly |
| T5 | **sqlite-vec instability** — sqlite-vec is a community extension, not SQLite core | Medium | Medium (RAG degraded) | Fallback to brute-force cosine similarity for small datasets (<10K chunks); consider porting to built-in FTS5 for BM25 | Monitor sqlite-vec GitHub issues; test with each SQLite upgrade |
| T6 | **o3 API changes or deprecation** — OpenAI frequently changes model availability | Medium | Medium (Think lane fallback) | Claude Opus with extended-thinking as equivalent fallback; abstract behind LLM trait | Monitor OpenAI changelog; alert on 404/deprecated responses |
| T7 | **Unix domain socket unavailable on Windows** — Windows uses named pipes, not Unix sockets | Certain | Low (known, planned) | Use named pipes on Windows (`\\.\pipe\bluey-overlay`); abstract behind IPC trait | N/A — design accounts for this from day 1 |
| T8 | **ONNX runtime compatibility** — ECAPA-TDNN model for speaker ID requires ONNX runtime | Low | Low (speaker ID is optional) | Ship without speaker ID initially; add when ONNX runtime stabilizes for ARM64 | Test on both Intel and ARM macOS |
| T9 | **Prompt caching invalidation** — Anthropic cache has TTL, may not hit as expected | Medium | Low (cost higher, not broken) | Monitor cache hit rates via API response headers; adjust breakpoint placement | Track `cache_creation_input_tokens` vs `cache_read_input_tokens` in cost_log |
| T10 | **Audio permission regression** — macOS TCC changes could break ScreenCaptureKit access | Low | High (no system audio) | Detect permission state on startup; guide user to System Settings; microphone-only fallback | Test on every macOS beta release |

### 5.2 Product Risks

| # | Risk | Likelihood | Impact | Mitigation | Early Warning |
|---|------|-----------|--------|------------|---------------|
| P1 | **Competitor adds three-lane routing** — Cluely or others copy the architecture | Low | Medium (reduced differentiation) | Ship fast; build moat via prompt quality + skill templates + patch-mode (harder to copy) | Monitor competitor changelogs and GitHub repos |
| P2 | **Cluely adds patch-mode** — Full-replace is their current weakness | Low | Low (bluey still faster) | Patch-mode + three-lane combined is the moat, not either alone | Monitor Cluely updates |
| P3 | **Interview platforms add AI detection** — Proctoring tools detect AI copilots | Medium | High (product value drops) | Content protection already handles screen recording; process masquerading handles task manager; stay ahead of detection methods | Monitor proctoring tool updates (HackerRank, Codility, etc.) |
| P4 | **LLM quality plateau** — Models stop improving, competitors catch up on quality | Low | Medium | Quality comes from routing + prompts + context, not just model; skill templates are the differentiator | Track eval suite scores over time |
| P5 | **User expects local inference** — Privacy-conscious users want no cloud | Medium | Low (design choice) | BYOK tier addresses privacy (user's own keys); document why cloud-first is better for quality | User feedback surveys; feature request tracking |

### 5.3 Legal/Policy Risks

| # | Risk | Likelihood | Impact | Mitigation | Early Warning |
|---|------|-----------|--------|------------|---------------|
| L1 | **Provider ToS violation** — Using AI for interview cheating may violate provider ToS | Medium | High (API access revoked) | Use BYOK tier (user's responsibility); don't market as "cheating tool"; position as "AI copilot for conversations" | Review provider ToS quarterly; use multiple providers to avoid single point of failure |
| L2 | **Recording consent laws** — Two-party consent states/countries | Medium | Medium (legal liability) | Document that user is responsible for consent; add disclaimer in onboarding; don't store audio (only transcripts) | Legal review before launch in each jurisdiction |
| L3 | **GDPR/privacy compliance** — Storing transcripts of conversations | Low | Medium | All data local by default; no cloud sync without explicit opt-in; data deletion via `bluey reset`; no PII in telemetry | Privacy audit before any cloud features |
| L4 | **Open source license conflicts** — GPL-3.0 dependencies may conflict | Low | Low | Audit all dependencies for license compatibility; prefer MIT/Apache-2.0; isolate GPL code if any | `cargo deny check licenses` in CI |

### 5.4 Operational Risks

| # | Risk | Likelihood | Impact | Mitigation | Early Warning |
|---|------|-----------|--------|------------|---------------|
| O1 | **Single-engineer dependency** — One person builds entire system | High | High (bus factor = 1) | Comprehensive documentation (this plan); CLAUDE.md for AI-assisted development; modular architecture allows parallel work | If engineer is unavailable >1 week, project stalls |
| O2 | **API key leakage** — User's keys exposed via logs, crash dumps, or memory | Low | High (financial loss) | zeroize crate for key scrubbing; log masking; keychain storage (not plaintext); never log keys even at trace level | Automated grep for key patterns in logs |
| O3 | **Runaway API costs** — Bug causes infinite loop of LLM calls | Low | High (unexpected bills) | Per-session hard cap ($10); per-minute rate limits; token budget per lane; circuit breaker on 10 consecutive errors | cost_log alerts when session exceeds $5 |
| O4 | **Build reproducibility** — Rust + native code + Tauri + React = complex build | Medium | Medium (can't ship) | Pin all dependency versions; document build steps; CI builds on every commit; `scripts/build-*.sh` tested regularly | Build breaks in CI |
| O5 | **Provider API key rotation** — Keys expire or get rotated | Low | Low (temporary outage) | Dashboard shows provider health status; `testConnection` command validates keys; alert on auth failures | 401 responses in provider health monitoring |

### 5.5 Risk Heat Map

```
              LOW IMPACT          MEDIUM IMPACT        HIGH IMPACT
           ┌─────────────────┬─────────────────────┬──────────────────┐
HIGH       │                 │ O1 (bus factor)     │                  │
LIKELIHOOD │                 │ O4 (build complex)  │                  │
           ├─────────────────┼─────────────────────┼──────────────────┤
MEDIUM     │ P5 (local inf)  │ T1 (Cerebras)       │ P3 (AI detect)   │
           │ T9 (cache miss) │ T5 (sqlite-vec)     │ L1 (ToS)         │
           │                 │ T6 (o3 changes)     │                  │
           ├─────────────────┼─────────────────────┼──────────────────┤
LOW        │ T8 (ONNX)       │ T2 (Tauri)          │ T3 (private API) │
           │ L4 (licenses)   │ T4 (Deepgram)       │ T10 (audio perm) │
           │ O5 (key rotate) │ P1 (competitor)     │ O2 (key leak)    │
           │                 │ L2 (consent)        │ O3 (runaway cost)│
           └─────────────────┴─────────────────────┴──────────────────┘
```

**Top 3 risks requiring immediate attention:**
1. **O1 (bus factor)** — Mitigate with documentation + AI-assisted dev workflow
2. **P3 (AI detection)** — Ensure stealth features are robust before launch
3. **T1 (Cerebras stability)** — Validate Groq fallback path early in Phase 9


### 3.9 Cost Optimization Implementation Details

#### CO1: Anthropic Prompt Caching — Implementation Spec

```json
// Request structure with cache_control breakpoints
{
  "model": "claude-sonnet-4-5-20260301",
  "max_tokens": 4096,
  "system": [
    {
      "type": "text",
      "text": "<core_identity>...</core_identity><anti_chatbot>...</anti_chatbot>",
      "cache_control": { "type": "ephemeral" }
    },
    {
      "type": "text",
      "text": "<skill_template name='coding_interview'>...</skill_template>",
      "cache_control": { "type": "ephemeral" }
    }
  ],
  "messages": [
    { "role": "user", "content": "..." }
  ]
}
```

**Cache economics:**
- System prompt: ~4,000 tokens (stable across all turns in a session)
- Skill template: ~1,500 tokens (stable within a skill)
- Cache write cost: 25% premium on first request
- Cache read cost: 90% discount on subsequent requests
- Cache TTL: 5 minutes (refreshed on each use)
- Break-even: 2nd request in same session (always profitable for sessions with 2+ Solve queries)

**Expected savings per session (15 Solve queries):**
- Without caching: 15 × 5,500 tokens × $3.00/M = $0.248 input cost
- With caching: 1 × 5,500 × $3.75/M + 14 × 5,500 × $0.30/M = $0.044 input cost
- **Savings: $0.204/session (82% reduction on cached portion)**

#### CO2: RAG-Filtered Context — Implementation Spec

Instead of sending full conversation history (avg 16K tokens) to Solve lane:
1. Compute query embedding (text-embedding-3-small, <5ms cached)
2. Search `chunks` table for top-5 relevant turns (sqlite-vec ANN, <10ms)
3. Include only those 5 turns + current question (avg 6K tokens)
4. Savings: 10K tokens × $3.00/M × 15 queries = $0.45/session

**Quality safeguard**: If RAG retrieval confidence < 0.6 for all chunks, fall back to last-10-turns strategy (ensures no context starvation).

#### CO3: Intra-Solve Router Split — Implementation Spec

Within the Solve lane, further classify:
- **Easy-Solve** (definitions, simple explanations, syntax questions): Route to Cerebras DeepSeek V3
  - Cost: $0.20/M input vs $3.00/M (15× cheaper)
  - Quality: Adequate for straightforward questions
  - Latency: Actually faster (Cerebras TTFT < Claude TTFT)
- **Hard-Solve** (complex coding, system design, multi-step): Route to Claude Sonnet 4.5
  - Full quality, prompt caching, skill templates

**Classification heuristic** (simple, <1ms):
```
if query_tokens < 50 AND no_code_keywords AND no_multi_step_indicators:
    route = "easy-solve" → Cerebras
else:
    route = "hard-solve" → Claude
```

Expected split: 30% easy / 70% hard
Savings: 30% × 15 queries × (12K×$2.80/M) = $0.14/session

### 3.10 Monthly Cost Projections by User Segment

| User Segment | Sessions/Month | Avg Cost/Session | Monthly Cost | Revenue (Pro) | Net |
|-------------|---------------|-----------------|--------------|---------------|-----|
| Light (casual prep) | 5 | $1.20 | $6.00 | $29 | +$23 |
| Medium (active job search) | 15 | $1.50 | $22.50 | $29 | +$6.50 |
| Heavy (daily practice) | 40 | $1.80 | $72.00 | $29 | -$43 |
| Power (BYOK) | 40 | $0.25 (STT only) | $10.00 | $9 | -$1 |

**Key insight**: The Pro tier is only viable if most users are light-to-medium. Heavy users MUST be migrated to BYOK tier or capped. Recommended: soft cap at 30 sessions/month on Pro, suggest BYOK upgrade above that.

### 4.9 Automated Eval Pipeline

#### LeetCode Evaluation Automation

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Question    │     │   bluey      │     │  LeetCode    │
│  Bank (130)  │────▶│  Pipeline    │────▶│  Submission  │
│  JSON format │     │  (simulate)  │     │  API         │
└──────────────┘     └──────────────┘     └──────┬───────┘
                                                  │
                                           ┌──────▼───────┐
                                           │   Results    │
                                           │  Aggregator  │
                                           │  (pass/fail) │
                                           └──────┬───────┘
                                                  │
                                           ┌──────▼───────┐
                                           │  Report      │
                                           │  Generator   │
                                           │  (markdown)  │
                                           └──────────────┘
```

**Pipeline steps:**
1. Load question from bank (problem statement + constraints + examples)
2. Simulate STT input (feed problem text as if transcribed)
3. Route through intent classifier → appropriate lane
4. Capture LLM response (code solution)
5. Extract code block from response
6. Submit to LeetCode API (or local judge with test cases)
7. Record: pass/fail, runtime percentile, memory percentile
8. Aggregate across difficulty levels

**Question bank format:**
```json
{
  "id": "two-sum",
  "difficulty": "easy",
  "expected_lane": "solve",
  "problem": "Given an array of integers nums and an integer target...",
  "test_cases": [...],
  "time_limit_ms": 5000,
  "tags": ["array", "hash-table"]
}
```

**Run frequency**: Full suite before each phase ships (~2 hours with rate limiting).
**Regression threshold**: >5% drop in pass rate on any difficulty level blocks the release.

#### Intent Classifier Evaluation

```
Test set: 500 labeled queries
├── 200 Snap-appropriate (simple Q&A, definitions, conversational)
├── 200 Solve-appropriate (coding, technical, multi-step)
└── 100 Think-appropriate (hard reasoning, architecture, novel problems)

Metrics computed:
├── Overall accuracy (target: ≥85%)
├── Per-lane precision and recall
├── Confusion matrix (which lanes get confused)
├── Latency distribution (must be <5ms p95)
└── Confidence calibration (predicted confidence vs actual correctness)
```

**Run frequency**: On every commit that touches router code.
**Data source**: Hand-labeled by principal engineer from real interview transcripts.

### 5.6 Risk Interdependencies

```
T1 (Cerebras instability) ──────► O3 (runaway costs if fallback is expensive)
                                    │
T3 (macOS private API) ─────────► P3 (AI detection if overlay breaks)
                                    │
O1 (bus factor) ────────────────► ALL RISKS (slower mitigation response)
                                    │
L1 (provider ToS) ─────────────► P5 (user trust if keys revoked)
                                    │
T5 (sqlite-vec) ───────────────► CO2 (RAG filter unavailable, costs rise)
```

**Cascading failure scenario (worst case):**
1. Cerebras goes down (T1)
2. All Snap queries route to Groq fallback (slightly slower, acceptable)
3. Groq also rate-limited due to surge (T1 cascade)
4. Snap queries escalate to Solve lane (Claude)
5. Claude costs spike 15× for those queries (O3 triggered)
6. Session cost cap ($10) hit within 30 minutes
7. User gets degraded service (Snap-only with delays)

**Mitigation**: Circuit breaker pattern — if fallback chain exhaustion detected, immediately switch to "Snap-only degraded mode" rather than escalating costs.

## S6: Provider Strategy

### 6.1 Cerebras — Snap Lane Primary

**Role**: Ultra-fast conversational responses (80-300ms TTFT)
**Model**: DeepSeek V3 (hosted on Cerebras inference hardware)
**Why chosen**: Fastest inference available for open-weight models; 10-50× faster than standard GPU inference

| Attribute | Detail |
|-----------|--------|
| SLA | 99.9% uptime (published) |
| Pricing | $0.20/M input, $0.60/M output |
| Rate limit | 1000 RPM (standard tier) |
| Max context | 64K tokens |
| Streaming | SSE, first token 80-150ms |
| Fallback trigger | 3 consecutive failures OR p95 > 500ms OR 429 response |
| Fallback target | Groq (Llama 4 Scout) — similar latency class |

**Integration details:**
- OpenAI-compatible API (`/v1/chat/completions`)
- HTTP/2 keep-alive pool (prewarmed, task L2)
- Dedicated connection in pool (not shared with other providers)
- Token budget: 2K input / 1K output (enforced by prompt composition)

**Monitoring:**
- Track TTFT per request in `cost_log`
- Alert if p95 TTFT > 500ms (3× normal)
- Alert if error rate > 5% in 5-minute window
- Dashboard shows Cerebras health indicator (green/yellow/red)

**Dedicated capacity path:**
- At >$5K/month spend: negotiate dedicated inference cluster
- Expected benefit: guaranteed latency SLA, 20-30% discount
- Timeline: when reaching 1,000+ active users

### 6.2 Claude Sonnet 4.5 — Solve Lane Primary

**Role**: Technical problem-solving, coding, system design (500ms-4s streaming)
**Model**: Claude Sonnet 4.5 (Anthropic)
**Why chosen**: Best coding performance per dollar; prompt caching reduces cost 50-70%

| Attribute | Detail |
|-----------|--------|
| SLA | 99.5% uptime (Anthropic published) |
| Pricing | $3.00/M input, $15.00/M output, $0.30/M cached input |
| Rate limit | 50 RPM (standard), 4000 RPM (scale tier) |
| Max context | 200K tokens |
| Streaming | SSE, first token 400-800ms |
| Prompt caching | ✅ cache_control breakpoints on system prompt + skill template |
| Fallback trigger | 3 consecutive failures OR 429 OR p95 > 3s |
| Fallback target | Gemini 2.5 Pro (similar quality, different provider) |

**Prompt caching strategy (CO1):**
```
┌─────────────────────────────────────────┐
│ System prompt (stable, cached)          │  ← cache_control: ephemeral
│ • Core identity + anti-chatbot rules    │
│ • Skill template for current mode       │
│ • Context prioritization rules          │
├─────────────────────────────────────────┤
│ Conversation context (varies per turn)  │  ← NOT cached
│ • RAG-retrieved relevant turns          │
│ • Current transcript segment            │
│ • User's question                       │
└─────────────────────────────────────────┘

Cache hit rate target: 60-70% (system prompt is ~4K tokens, reused across turns)
Savings: 60% × 4K tokens × $2.70 savings/M = $0.0065 per query
Over 15 queries/session: ~$0.10 savings/session
```

**Monitoring:**
- Track `cache_creation_input_tokens` vs `cache_read_input_tokens` from response headers
- Alert if cache hit rate drops below 40% (indicates prompt instability)
- Track streaming throughput (tokens/sec) — alert if < 30 tok/s

### 6.3 o3 — Think Lane Primary

**Role**: Hardest problems requiring extended reasoning (15-60s)
**Model**: o3 (OpenAI)
**Why chosen**: Best reasoning capability for complex multi-step problems

| Attribute | Detail |
|-----------|--------|
| SLA | 99.5% uptime (OpenAI published) |
| Pricing | $10.00/M input, $40.00/M output |
| Rate limit | 100 RPM (tier 5) |
| Max context | 200K tokens |
| Streaming | SSE with thinking tokens visible |
| Thinking budget | 32K thinking tokens max (configurable) |
| Fallback trigger | 90s timeout OR 3 consecutive failures OR 429 |
| Fallback target | Claude Opus 4 with extended-thinking enabled |

**Cost controls:**
- Hard cap: 32K thinking tokens per query ($1.28 max per query)
- Session cap: 5 Think queries per session (user can override)
- Progressive disclosure: show thinking tokens in overlay (user sees progress)
- Cancel semantics: user can cancel mid-thinking (partial result discarded)

**Monitoring:**
- Track thinking_tokens consumed per query
- Alert if average thinking_tokens > 20K (cost creep)
- Track quality: does more thinking correlate with better answers?
- Dashboard shows Think lane usage and cost prominently

### 6.4 Deepgram Nova-3 — STT Primary

**Role**: Real-time speech-to-text (persistent WebSocket per session)
**Model**: Nova-3 (Deepgram)
**Why chosen**: Lowest latency streaming STT; best accuracy for technical vocabulary

| Attribute | Detail |
|-----------|--------|
| SLA | 99.9% uptime |
| Pricing | $0.0043/sec of audio processed |
| Streaming | WebSocket, partial results every 100-200ms |
| Languages | 30+ (auto-detect available) |
| Features | Smart formatting, punctuation, diarization |
| Fallback trigger | WebSocket close + 3 reconnect failures |
| Fallback target | AssemblyAI Universal-Streaming |

**Persistent WebSocket lifecycle (L1):**
```
Session start
    │
    ▼
┌──────────────┐
│ Open WS to   │
│ Deepgram     │
│ (keep-alive) │
└──────┬───────┘
       │
       ▼
┌──────────────┐     audio frames      ┌──────────────┐
│ Send audio   │────────────────────────▶│ Receive      │
│ continuously │                         │ partials +   │
│ (16kHz PCM)  │◀────────────────────────│ finals       │
└──────────────┘     transcripts        └──────────────┘
       │
       │ Session end OR user pauses
       ▼
┌──────────────┐
│ Send         │
│ CloseStream  │
│ (graceful)   │
└──────────────┘

NEVER close mid-utterance.
On network drop: buffer audio locally, reconnect, replay buffer.
Reconnect backoff: 1s, 2s, 4s, 8s, 15s, 30s (cap)
```

**Monitoring:**
- Track partial latency (audio_sent → partial_received)
- Alert if partial latency > 500ms (network issue)
- Track word error rate via periodic spot-checks
- Monitor WebSocket connection duration (should be session-length)

### 6.5 Fallback Provider Details

| Primary | Fallback | Trigger | Switch Time | Quality Delta |
|---------|----------|---------|-------------|---------------|
| Cerebras (Snap) | Groq Llama 4 Scout | 3 failures / p95>500ms | <100ms (prewarmed pool) | -5% quality, similar speed |
| Claude Sonnet 4.5 (Solve) | Gemini 2.5 Pro | 3 failures / 429 / p95>3s | <200ms | -10% coding quality, similar speed |
| o3 (Think) | Claude Opus 4 | 90s timeout / 3 failures | <500ms | -5% reasoning, similar depth |
| Deepgram Nova-3 (STT) | AssemblyAI Universal | WS close + 3 reconnects | <3s (new WS) | -5% accuracy, +100ms latency |

### 6.6 Escalation Paths

```
Normal operation
    │
    ├─ Single failure → retry once (same provider)
    │
    ├─ 3 consecutive failures → switch to fallback
    │   └─ Emit provider-status event (yellow indicator)
    │
    ├─ Fallback also fails → try tertiary (if exists)
    │   └─ Emit provider-status event (red indicator)
    │
    ├─ All providers in chain fail → graceful degradation
    │   ├─ Snap: show "temporarily unavailable" in overlay
    │   ├─ Solve: queue request, retry in 30s
    │   ├─ Think: show error, suggest retry later
    │   └─ STT: switch to offline mode (no transcription)
    │
    └─ Provider recovers → automatic switch back after 60s healthy
        └─ Emit provider-status event (green indicator)
```

### 6.7 Provider Degradation Detection

| Signal | Detection Method | Action |
|--------|-----------------|--------|
| Latency spike | p95 > 2× baseline over 5-min window | Log warning, prepare fallback |
| Error rate spike | >5% errors in 5-min window | Switch to fallback |
| Quality degradation | Garbled/truncated responses | Log + alert, manual investigation |
| Rate limiting | 429 responses | Respect Retry-After header, queue requests |
| Maintenance window | Provider status page API | Pre-switch to fallback before window |

### 6.8 Contract Considerations

| Provider | Current Tier | Upgrade Trigger | Upgrade Benefit |
|----------|-------------|-----------------|-----------------|
| Cerebras | Standard | $5K/month spend | Dedicated cluster, guaranteed latency |
| Anthropic | Scale | $10K/month spend | Higher rate limits (4000 RPM), priority support |
| OpenAI | Tier 5 | $10K/month spend | Higher rate limits, dedicated capacity |
| Deepgram | Growth | $3K/month spend | Volume discount (15-25%), priority support |

**Timeline**: Negotiate upgrades when reaching 1,000+ paying users (estimated month 6-9 post-launch).

### 6.9 Provider Comparison: Why These Specific Models

| Lane | Chosen | Runner-up | Why Chosen Over Runner-up |
|------|--------|-----------|---------------------------|
| Snap | Cerebras DeepSeek V3 | Groq Llama 4 Scout | Cerebras 2× faster TTFT; DeepSeek V3 better at code than Llama |
| Solve | Claude Sonnet 4.5 | GPT-4o | Claude better at coding tasks; prompt caching saves 50-70%; 200K context |
| Think | o3 | Claude Opus 4 | o3 reasoning significantly better on hard problems; visible thinking tokens |
| STT | Deepgram Nova-3 | AssemblyAI Universal | Deepgram lower latency partials; better technical vocabulary; persistent WS |

## S7: Glossary + Reference Index

### 7.1 Acronyms + Terms

| Term | Definition |
|------|-----------|
| ANN | Approximate Nearest Neighbor — vector search algorithm used by sqlite-vec |
| BM25 | Best Matching 25 — probabilistic text retrieval algorithm (hybrid search with vectors) |
| BYOK | Bring Your Own Key — pricing tier where user provides their own API keys |
| CO1/CO2/CO3 | Cost Optimization tasks 1-3 (prompt caching, RAG filter, router split) |
| CM1/CM2/CM3/CM4 | Context Management tasks 1-4 (session model, assembly, epochs, counter) |
| CPAL | Cross-Platform Audio Library — Rust crate for microphone capture |
| CSP | Content Security Policy — restricts what Tauri webview can load |
| DSP | Digital Signal Processing — audio processing pipeline |
| ECAPA-TDNN | Emphasized Channel Attention, Propagation and Aggregation Time Delay Neural Network — speaker ID model |
| F1-F8 | Follow-up/Patch-mode tasks (three-lane integration through diff rendering) |
| FTS5 | Full-Text Search 5 — SQLite extension for text search |
| HTTF | Time To First Token — latency from request sent to first response token |
| IPC | Inter-Process Communication — daemon↔overlay, daemon↔dashboard, daemon↔CLI |
| L1-L6 | Latency Engineering tasks (persistent WS, HTTP/2 pool, stable-partial, speculative, socket, instrumentation) |
| LA2 | LocalAgreement-2 — streaming STT decoder algorithm (stretch goal) |
| LLM | Large Language Model — AI text generation (Cerebras, Claude, o3) |
| NDJSON | Newline-Delimited JSON — streaming format used by some providers |
| NSPanel | macOS AppKit panel type — floats above windows, non-activating |
| ONNX | Open Neural Network Exchange — model format for speaker ID |
| PCM | Pulse-Code Modulation — raw audio sample format (f32 or i16) |
| R1-R5 | Three-lane Routing tasks (classifier, parallel, override, skills, badge) |
| RAG | Retrieval-Augmented Generation — local vector search to enhance LLM context |
| rAF | requestAnimationFrame — browser API for 60Hz rendering |
| RPM | Requests Per Minute — API rate limit unit |
| RSS | Resident Set Size — process memory usage metric |
| SCK | ScreenCaptureKit — macOS framework for system audio capture |
| SLA | Service Level Agreement — provider uptime guarantee |
| SPSC | Single-Producer Single-Consumer — lock-free ring buffer pattern |
| SSE | Server-Sent Events — HTTP streaming protocol for LLM responses |
| STT | Speech-to-Text — audio transcription (Deepgram Nova-3) |
| TCC | Transparency, Consent, and Control — macOS permission framework |
| TTFT | Time To First Token — key latency metric for LLM responses |
| VAD | Voice Activity Detection — filters silence before sending to STT |
| WAL | Write-Ahead Logging — SQLite journaling mode for concurrent reads |
| WASAPI | Windows Audio Session API — Windows system audio capture |
| WS | WebSocket — persistent bidirectional connection (STT, some LLM providers) |

### 7.2 Task ID Format

| Prefix | Series | Meaning |
|--------|--------|---------|
| D0.x | Foundation | Daemon + Tauri scaffold tasks |
| C0.x | Foundation | Core data model tasks |
| N0.x | Foundation | Native overlay audit tasks |
| B1.x | Stealth | Window stealth + masquerading |
| B2.x | Audio/STT | Audio capture + speech-to-text |
| B3.x | LLM | LLM orchestration + providers |
| B4.x | Prompts | Prompt composition + skills |
| B5.x | RAG | Vector store + retrieval |
| B6.x | Dashboard | UI + UX |
| B7.x | Security | Keychain, logs, migrations |
| B8.x | Observability | Metrics, telemetry, dashboards |
| B9.x | Release | Auto-updater, build targets |
| B10.x | Dev | Documentation, codex configs |
| L1-L6 | Latency | Latency engineering (V3 new) |
| R1-R5 | Routing | Three-lane routing (V3 new) |
| F1-F8 | Follow-up | Patch-mode follow-ups (V3 new) |
| CM1-CM4 | Context | Context management (V3 new) |
| CO1-CO3 | Cost | Cost optimization (V3 new) |

### 7.3 Document Index (on uno)

All files at `/Users/uno/Downloads/cue/docs/reviews/`:

| File | Lines | Content |
|------|-------|---------|
| `CUE-BLUEY-V3-PLAN-SKELETON.md` | 734 | V3 master plan skeleton — architecture, task index, phases, timeline |
| `CUE-BLUEY-V3-PART-A-PHASES.md` | ~1500 | Detailed phase plans 0-4 (foundation through reasoning) |
| `CUE-BLUEY-V3-PART-B-PHASES.md` | ~1200 | Detailed phase plans 5-10 (memory through cost optimization) |
| `CUE-BLUEY-V3-PART-C-APPENDICES.md` | ~2800 | THIS FILE — architecture, competitive, cost, metrics, risks, providers, glossary |
| `CUE-MASTER-PORT-PLAN-V2.md` | 1895 | Previous V2 plan with 97 tasks (historical reference) |
| `CUE-CURRENT-STATE.md` | 486 | Source code audit: what's built vs planned |
| `CUE-REFERENCE-ANALYSIS.md` | 2000+ | Running pattern catalog from all reference repos |
| `CUE-PORT-PLAN.md` | — | Ranked backlog for codex (derived from reference analysis) |
| `CUE-DESIGN-01-STEALTH-WINDOWS.md` | 1300 | Stealth + Windows design synthesis (B1.x tasks) |
| `CUE-DESIGN-02-AUDIO-STT.md` | 1648 | Audio + STT design synthesis (B2.x tasks) |
| `CUE-DESIGN-03-LLM-PROMPTS-RAG.md` | 1791 | LLM + Prompts + RAG design synthesis (B3-B5 tasks) |
| `CUE-DESIGN-04-UX-OPS-DEV.md` | 1486 | UX + Ops + Security + Dev design synthesis (B6-B10 tasks) |
| `CUE-REF-01A-NATIVELY-BACKEND.md` | ~3000 | Deep analysis: natively-cluely backend (Electron main + Rust native) |
| `CUE-REF-01B-NATIVELY-FRONTEND.md` | ~1500 | Deep analysis: natively-cluely frontend (React renderer) |
| `CUE-REF-02-PLUELY.md` | ~2000 | Deep analysis: pluely (Tauri 2 — closest stack match) |
| `CUE-REF-03-SOLVEWATCHAI.md` | ~1800 | Deep analysis: solveWatchAi (Python + Electron, local Whisper) |
| `CUE-REF-04-AURA.md` | ~1200 | Deep analysis: Aura-AI (Python, multi-provider) |
| `CUE-REF-05-VYSPER.md` | ~1000 | Deep analysis: Vysper (9 skill templates, Azure) |
| `CUE-REF-06-OPENCLUELY.md` | ~800 | Deep analysis: OpenCluely (Gemini Vision, language enforcement) |
| `PINKY-ARCHITECTURE-FROM-CODE.md` | ~1500 | Pinky (Go SaaS) architecture — WebRTC, billing, teams |

### 7.4 External References

| Resource | URL | Relevance |
|----------|-----|-----------|
| Tauri 2 Docs | https://v2.tauri.app | Dashboard framework |
| tauri-nspanel | https://github.com/nicepkg/tauri-nspanel | NSPanel integration for Tauri |
| Cerebras API | https://cloud.cerebras.ai/docs | Snap lane provider |
| Anthropic API | https://docs.anthropic.com | Solve lane provider |
| Anthropic Prompt Caching | https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching | CO1 implementation |
| OpenAI API | https://platform.openai.com/docs | Think lane provider |
| Deepgram Streaming | https://developers.deepgram.com/docs/streaming | STT integration |
| AssemblyAI Streaming | https://www.assemblyai.com/docs/streaming | STT fallback |
| sqlite-vec | https://github.com/asg017/sqlite-vec | RAG vector store |
| CPAL | https://docs.rs/cpal | Microphone capture |
| cidre | https://docs.rs/cidre | macOS CoreAudio/SCK bindings |
| ringbuf | https://docs.rs/ringbuf | Lock-free audio ring buffer |
| rubato | https://docs.rs/rubato | Sample rate conversion |
| governor | https://docs.rs/governor | Rate limiting |
| tokio-tungstenite | https://docs.rs/tokio-tungstenite | WebSocket client (Deepgram) |
| reqwest | https://docs.rs/reqwest | HTTP/2 client (LLM providers) |
| tracing | https://docs.rs/tracing | Structured logging |
| opentelemetry-rust | https://docs.rs/opentelemetry | Metrics + telemetry |
| criterion | https://docs.rs/criterion | Benchmarking framework |
| zeroize | https://docs.rs/zeroize | Secure key scrubbing |

---

*End of Appendices. This document is consumed by codex agents during implementation as reference material.*
*Generated: 2026-05-12 | Author: Principal Engineer review of V3 architecture*

### 6.10 Provider Migration Playbook

If a primary provider becomes unviable (pricing change, quality degradation, ToS enforcement):

| Current | Migration Target | Effort | Data Migration |
|---------|-----------------|--------|----------------|
| Cerebras → Groq | Groq Llama 4 Scout | 1 day | Config change only (OpenAI-compatible API) |
| Cerebras → Fireworks | Fireworks DeepSeek V3 | 2 days | New client config, same API shape |
| Claude Sonnet → Gemini 2.5 Pro | Google AI | 3 days | Prompt adaptation (XML→plain), cache strategy change |
| Claude Sonnet → GPT-4o | OpenAI | 2 days | Prompt adaptation, no cache benefit |
| o3 → Claude Opus | Anthropic | 1 day | Config change, enable extended-thinking |
| Deepgram → AssemblyAI | AssemblyAI | 3 days | New WS protocol, similar accuracy |
| Deepgram → Groq Whisper | Groq | 2 days | REST-batch (latency regression), cheaper |

**Key principle**: All providers are behind traits/interfaces. Migration is config + prompt adaptation, never architectural change. This is by design (CUE-DESIGN-03 provider abstraction).

### 7.5 Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| V3.0 | 2026-05-12 | Principal Engineer | Initial appendices (S1-S7) |

