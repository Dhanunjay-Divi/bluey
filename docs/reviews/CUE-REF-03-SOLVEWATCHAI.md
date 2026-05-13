# DEEP ANALYSIS — solveWatchAi (3-service architecture)

**Scope**: 87 source files, ~56K LOC read line-by-line
**Why this matters**: Best open-source 3-service architecture for real-time interview assistance with speaker identification + MLX Whisper streaming + multi-provider AI fallback

## File manifest

```
solveWatchAi-main/
├── start.sh (22KB)                    # Orchestrator — installs deps, manages all 3 services
├── start.ps1 (17KB)                   # Windows equivalent
├── start.bat                          # Windows batch launcher
├── package.json                       # Node.js deps (Express 5, Socket.IO 4, Electron 33, OTel)
├── .env.example (8.7KB)               # Single source of truth for all config
├── architecture.md (821L)             # Mermaid diagrams — full system + sequence diagrams
├── CLAUDE.md (510L)                   # Developer workflow rules + architecture context
├── CHANGELOG.md                       # Keep-a-changelog format
├── CONTRIBUTING.md                    # PR checklist
├── CITATION.cff                       # Academic citation
├── .prettierrc                        # Code formatting
├── .github/                           # CI, PR template, issue templates, SECURITY.md
├── LICENSE                            # MIT
│
├── src/                               # Node.js backend (Express + Socket.IO)
│   ├── server.js                      # HTTP server bootstrap, OTel init, graceful shutdown
│   ├── app.js                         # Express factory: middleware, routes, static /settings
│   ├── config/constants.js            # PORT, intervals, getLocalIP()
│   ├── routes/
│   │   ├── image.routes.js            # POST /api/upload
│   │   ├── context.routes.js          # GET/POST /api/context-state
│   │   └── config.routes.js           # GET/POST /api/config/keys, /api/config/full
│   ├── controllers/
│   │   ├── image.controller.js        # Screenshot → OCR → AI pipeline
│   │   ├── context.controller.js      # Context state CRUD
│   │   └── config.controller.js       # Provider/model config CRUD
│   ├── services/
│   │   ├── ai.service.js (650L)       # Multi-provider fallback + streaming + pricing table
│   │   ├── image-processing.service.js # Sharp preprocessing + OCR + AI pipeline
│   │   ├── ocr.service.js            # Worker thread → Tesseract.js
│   │   └── screenshot-monitor.service.js # fs.watch + osx-mouse crop + Sharp
│   ├── sockets/
│   │   ├── dataHandler.js (550L)      # /data-updates namespace — all Socket.IO logic
│   │   └── InterviewTranscriptBuffer.js (200L) # Rolling memory with Ollama summarization
│   ├── middleware/
│   │   ├── error.middleware.js        # 404 + 500 handlers
│   │   ├── telemetry.middleware.js    # HTTP request duration histogram
│   │   └── upload.middleware.js       # Multer: 10MB, images only
│   ├── public/settings.html           # Browser settings UI
│   ├── utils/
│   │   ├── logger.js                  # Namespaced structured logger
│   │   └── telemetry.js (400L)        # OTel → Grafana Cloud (metrics + logs)
│   └── workers/ocr.worker.js          # Tesseract in worker thread
│
├── electron/                          # Electron HUD overlay
│   ├── main.js (200L)                 # BrowserWindow + hotkeys + IPC + window state persistence
│   ├── preload.js                     # Context bridge: hudAPI (drag, opacity, toggle-listen)
│   └── hud.html                       # Socket.IO client + markdown renderer + question cards
│
├── transcriber/                       # Python STT service (FastAPI)
│   ├── main.py (500L)                 # FastAPI app, lifespan, endpoints, pre-warm Whisper
│   ├── transcriber.py (200L)          # Whisper wrapper: MLX / local CPU / API backends
│   ├── streaming_stt.py (350L)        # ★ LocalAgreement-2 streaming decoder
│   ├── always_on_listener.py (350L)   # VAD state machine + speaker ID integration
│   ├── speaker_id.py (300L)           # ★ SpeechBrain ECAPA-TDNN speaker identification
│   ├── deepgram_listener.py (300L)    # Deepgram cloud STT alternative
│   ├── socket_client.py (200L)        # Socket.IO client → Node backend
│   ├── keyboard_handler.py            # Global hotkey (pynput)
│   ├── config.py (100L)               # All config from .env
│   ├── telemetry.py (400L)            # OTel → Grafana Cloud (Python side)
│   ├── vad/
│   │   ├── __init__.py                # Factory: create_vad(engine, config)
│   │   ├── base.py                    # Abstract BaseVAD
│   │   ├── silero_vad.py (100L)       # ONNX Silero DNN VAD
│   │   ├── webrtc_vad.py (80L)        # WebRTC GMM VAD
│   │   └── metrics.py                 # Rolling VAD metrics (5-min window)
│   ├── benchmark/                     # STT accuracy benchmarking
│   └── requirements.txt               # MLX, SpeechBrain, torch, OTel, Deepgram SDK
│
├── prompts/                           # Hot-reloaded prompt templates
│   ├── system-prompt.txt              # FAANG engineer persona (screenshot flow)
│   ├── transcription-prompt.txt       # Live interview processing
│   ├── interview-answer-prompt.txt    # Q: / A: format with memory context
│   ├── coding-prompt.txt              # Coding problem solver
│   ├── debug-prompt.txt               # Debugging specialist
│   ├── theory-prompt.txt              # Theory/concepts
│   └── context-prompt.txt             # {CONTEXT} substitution
│
├── docs/
│   ├── grafana-dashboard.json (9552L) # Pre-built Grafana dashboard
│   └── grafana-alert-rules.yaml       # Alert definitions
│
├── web/                               # Next.js marketing site (Vercel)
│   ├── app/                           # Pages: home, how-it-works, latency, observability
│   ├── components/                    # Hero, Features, FAQ, Comparison, etc.
│   └── package.json                   # Next.js + Tailwind
│
└── config/
    └── api-keys.json.example          # Legacy config (migrated to .env automatically)
```

## 3-service architecture deep-dive (Electron ↔ Node ↔ Python) with exact IPC contracts

### Service 1: Python Transcriber (Port 8000)

**Role**: Real-time speech-to-text with speaker identification
**Framework**: FastAPI + uvicorn (single worker)
**Communication**: Socket.IO client → Node:4000/data-updates + HTTP API for control

**Outbound Socket.IO events (Python → Node)**:
| Event | Payload | Frequency |
|-------|---------|-----------|
| `stt_partial` | `{committed: str, tentative: str, timestamp: float}` | Every 300ms while speaking |
| `stt_final` | `{text: str, uid: str, silence_started_at: float, timestamp: float}` | On VAD silence threshold |
| `listen_state_update` | `{listening: bool}` | On keyboard toggle |
| `speaker_id_unavailable` | `{reason: str, timestamp: float}` | At startup if model fails |

**HTTP API (Node → Python)**:
| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/always-on-mode` | POST | Start/stop listener `{enabled: bool}` |
| `/set-stt-model` | POST | Hot-swap Whisper model `{model: str}` |
| `/set-vad-config` | POST | Update VAD params (engine, thresholds) |
| `/health` | GET | Liveness check |
| `/settings` | GET | Current STT model |
| `/vad-metrics` | GET | Rolling 5-min VAD stats |
| `/load-speaker-id` | POST | Load ECAPA model on-demand |
| `/enroll-voice` | POST | Record 30s → save embedding |
| `/enrollment-status` | GET | Model loaded + enrolled status |
| `/reload-telemetry` | POST | Re-read .env, reinit OTel |
| `/reload-stt-config` | POST | Hot-swap Deepgram ↔ local Whisper |

### Service 2: Node.js Backend (Port 4000)

**Role**: AI orchestration, OCR pipeline, settings, Socket.IO hub
**Framework**: Express 5 + Socket.IO 4 (websocket-only transport)
**Namespace**: `/data-updates` — single namespace for ALL real-time communication

**Socket.IO events (Node → HUD)**:
| Event | Payload | When |
|-------|---------|------|
| `connected` | `{socketId, timestamp}` | On client connect |
| `stt_partial` | `{committed, tentative}` | Relayed from Python |
| `interviewer_question` | `{questionId, questionText}` | Raw transcript as question |
| `question_text_updated` | `{questionId, questionText}` | AI-cleaned question text |
| `question_answer_started` | `{questionId}` | AI stream beginning |
| `question_answer_token` | `{token, questionId}` | Each AI token |
| `question_answer_complete` | `{questionId, response}` | Full answer done |
| `ai_token` | `{token, messageId}` | Screenshot/prompt flow tokens |
| `ai_processing_complete` | `{response, messageId}` | Screenshot flow done |
| `screenshot_captured` | `{message}` | New screenshot detected |
| `ocr_started` / `ocr_complete` | `{message}` | OCR lifecycle |
| `hud_opacity_updated` | `{value}` | Opacity change broadcast |
| `listen_state_changed` | `{listening}` | Listener toggle confirmed |
| `settings_state` | `{sttModel, answerMode, enabledProviders}` | Settings response |
| `enrollment_started` | `{seconds}` | Voice enrollment recording |
| `enrollment_complete` | `{success, ...}` | Enrollment result |

**Socket.IO events (HUD → Node)**:
| Event | Payload | Purpose |
|-------|---------|---------|
| `use_prompt` | `{promptType, messageId, screenshotRequired}` | Re-process with debug/theory/coding |
| `toggle_listen_mode` | `{enabled}` | Start/stop always-on listener |
| `set_stt_model` | `{model}` | Change Whisper model |
| `set_answer_mode` | `{mode}` | Route answers to specific provider |
| `get_settings` | `{}` | Request current settings |
| `set_hud_opacity` | `{value}` | Change overlay opacity |
| `set_vad_config` | `{engine, ...}` | Update VAD parameters |
| `enroll_voice` | `{}` | Trigger 30s enrollment |
| `load_speaker_id` | `{threshold}` | Load ECAPA model |

### Service 3: Electron HUD (No port — connects to Node:4000)

**Role**: Invisible overlay displaying AI answers
**Window**: 380×600px, frameless, `alwaysOnTop: 'screen-saver'`, `setContentProtection(true)`
**IPC (renderer ↔ main process)**:
| Channel | Direction | Purpose |
|---------|-----------|---------|
| `hud-drag-start` | renderer→main | Begin window drag `(screenX, screenY)` |
| `hud-drag-move` | renderer→main | Continue drag `(screenX, screenY)` |
| `hud-drag-end` | renderer→main | End drag |
| `hud-set-opacity` | renderer→main | Set window opacity (0-100) |
| `toggle-listen` | main→renderer | Cmd+Shift+X hotkey forwarded |

**Global hotkeys** (registered in main process):
- `Cmd+Shift+H` → toggle overlay visibility
- `Cmd+Shift+X` → toggle listen mode (forwarded to renderer → Socket.IO)

**Window state persistence**: Bounds saved to `~/Library/Application Support/<app>/hud-window-state.json`

## LocalAgreement-2 streaming Whisper deep-dive

**Source**: `transcriber/streaming_stt.py` (350 lines)
**Algorithm**: Produces stable "committed" words from a rolling audio buffer by comparing consecutive Whisper decodes

### Core concept

Whisper is a batch model — it transcribes a complete audio segment at once. To get streaming behavior, solveWatchAi re-decodes the entire rolling buffer every 300ms and uses a stability algorithm to determine which words are "committed" (won't change) vs "tentative" (may change on next decode).

### Algorithm step-by-step

```
Every 300ms (DECODE_INTERVAL_S):
  1. Snapshot the rolling audio buffer (deque of numpy chunks)
  2. RMS energy gate: skip if audio < -40 dBFS (RMS_GATE = 10^(-40/20))
  3. Run Whisper with word_timestamps=True on the FULL buffer
     - MLX backend: mlx_whisper.transcribe() under _mlx_lock
     - Local CPU: model.transcribe() under same lock
     - API fallback: no timestamps, uses text-only matching
  4. Apply LocalAgreement-2:
     - For each word at position i (starting after existing committed words):
       - Compare current[i] with previous_decode[i]
       - text_match = (word == prev_word)
       - ts_match = |start_time - prev_start_time| <= 0.30s (COMMIT_TS_TOL_S)
       - If BOTH match: commit the word
       - If EITHER fails: STOP (no further words committed this tick)
  5. Emit on_partial(committed_str, tentative_str)
  6. Prune buffer: drop audio before (last_committed_end - 0.5s)
  7. Check silence-final condition (adaptive threshold)
```

### Adaptive silence threshold (Fix #2)

```python
# Two thresholds:
SILENCE_FINAL_FAST = 0.30s  # when text ends in ?/.!/. AND committed stable ≥2 ticks
SILENCE_FINAL_SLOW = 1.00s  # default — guards against mid-sentence pauses

# Stability tracking:
# committed_words_only compared across ticks (text only, ignoring timestamp jitter)
# _stable_count increments when committed list unchanged, resets on change
# FAST fires only when: ends_in_punct AND _stable_count >= 2
```

### Pre-STT diarization hold/discard API

The streaming decoder integrates with speaker identification via a hold mechanism:
```
begin_utterance(uid)   — tag utterance at VAD speech-start
hold_final(uid)        — called before SpeakerIDWorker processes; stores result
release_held(uid)      — speaker ID says PASS; emit stored on_final
discard(uid)           — speaker ID says CANDIDATE; clear buffer, emit empty partial
```

This allows speaker ID to run IN PARALLEL with the silence wait, not sequentially after it.

### Generation counter

`_generation` increments on every `_reset()`. If a decode completes but generation changed mid-decode (because discard() was called), the result is silently dropped. This prevents stale transcriptions from leaking through.

### Buffer management

- Max buffer: 15 seconds (BUFFER_MAX_S) — oldest chunks dropped from deque head
- Min buffer: 1 second (MIN_BUFFER_S) — don't decode until enough audio
- Pruning: after each decode, drop audio before (last_committed_timestamp - 0.5s)

### Bluey port strategy

For Cue/Bluey (Tauri + Rust):
- Use `whisper-rs` (Rust bindings to whisper.cpp) with word timestamps
- Implement LocalAgreement-2 in Rust — it's just array comparison with tolerance
- The rolling buffer is a `VecDeque<Vec<f32>>` with the same pruning logic
- The decode loop is a `tokio::spawn` task with `tokio::time::interval(300ms)`
- Hold/discard API maps to `tokio::sync::watch` channels

---

## Speaker identification deep-dive

**Source**: `transcriber/speaker_id.py` (300 lines) + integration in `always_on_listener.py`

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Audio Callback Thread (100ms blocks)                    │
│  VAD detects speech → accumulates _speech_buffer         │
│  On first silence frame: submit to SpeakerIDWorker       │
└──────────────────────┬──────────────────────────────────┘
                       │ queue.put(_PendingSpeechSegment)
                       ▼
┌─────────────────────────────────────────────────────────┐
│  SpeakerIDWorker (daemon thread)                         │
│  Dequeues segments, runs ECAPA inference (~30-50ms)      │
│  Decision: CANDIDATE → on_discard(uid)                   │
│            PASS      → on_pass(uid)                      │
└──────────────────────┬──────────────────────────────────┘
                       │ callbacks
                       ▼
┌─────────────────────────────────────────────────────────┐
│  StreamingSTT                                            │
│  discard(uid) → clear buffer, emit empty partial         │
│  release_held(uid) → emit stored on_final                │
└─────────────────────────────────────────────────────────┘
```

### Embedding model

- **Model**: SpeechBrain ECAPA-TDNN (`speechbrain/spkrec-ecapa-voxceleb`)
- **Size**: 22 MB download, cached locally
- **Embedding dim**: 192 (L2-normalized)
- **No HF token required** (public model)
- **Device selection**: CUDA → CPU (MPS intentionally excluded — SpeechBrain 1.x bug)
- **Inference time**: < 50ms per utterance on CPU

### Enrollment flow

1. User clicks "Enroll Voice" in settings UI
2. Node emits `enrollment_started {seconds: 30}` to HUD (shows countdown)
3. Node POSTs to Python `/enroll-voice`
4. Python pauses AlwaysOnListener (releases mic)
5. Records 30 seconds via `sounddevice.rec()`
6. Computes 192-dim embedding via `encode_batch()`
7. L2-normalizes and saves to `transcriber/models/user_embedding.npy`
8. Resumes listener, emits `enrollment_complete`

### Identification logic

```python
def identify(audio, sample_rate=16000):
    emb = _compute_embedding(audio, sample_rate)  # 192-dim, L2-normalized
    sim = dot(emb, stored_embedding)               # cosine sim (both normalized)
    if sim >= threshold:  # default 0.70
        return CANDIDATE, sim   # it's the user → discard
    return PASS, sim            # interviewer → forward to AI
```

### Fail-safe design

- Any error in identify() → returns (PASS, 0.0) — never silently drops interviewer audio
- SpeakerIDWorker error → calls on_pass() — same fail-safe
- Stale UID handling: if speech resumes before speaker ID decides, `cancel_if_uid()` marks it stale

### Legacy migration

Old pyannote embeddings were 512-dim. On startup, shape mismatch is detected → file renamed to `.legacy-512` → user treated as not-enrolled until re-enrollment.

### Deepgram-mode fallback

When `DEEPGRAM_ENABLED=true`, speaker identification uses Deepgram's built-in diarization:
- Auto-enrollment: first N seconds of audio identify the user's speaker ID
- Saved to `config/deepgram_enrollment.pcm` (raw float32 PCM)
- On subsequent starts: replays saved audio → Deepgram assigns speaker ID
- Filters utterances where `dominant_speaker == user_speaker_id`

## start.sh orchestration walkthrough (line-by-line summary)

**Source**: `start.sh` (22KB, ~350 lines)

### Phase 1: Environment loading

```bash
1. Set strict mode (set -e)
2. Resolve SCRIPT_DIR (absolute path to repo root)
3. load_env() — custom .env parser:
   - Strips comments and blank lines
   - Handles quoted values (single/double)
   - Exports KEY=VALUE pairs
4. If .env missing → copy from .env.example
5. Migration: if config/api-keys.json exists AND config/.migrated doesn't:
   - Inline Python script reads JSON, maps keys to .env format
   - Backs up api-keys.json → api-keys.json.backup
   - Touches config/.migrated sentinel
   - Reloads .env
```

### Phase 2: Argument parsing

```bash
Flags: --setup, --setup-only, --newlogs, --debug, --telemetry-debug
NODE_PORT from PORT env var (default: 4000)
PIDS=() array for cleanup
OLLAMA_STARTED=false (tracks if this script launched Ollama)
```

### Phase 3: Cleanup trap

```bash
trap cleanup EXIT INT TERM
cleanup():
  1. Kill all PIDs in PIDS[] with SIGTERM
  2. If OLLAMA_STARTED: pkill -x ollama
  3. Sleep 1s
  4. Force-kill (SIGKILL) any survivors
```

### Phase 4: Setup (--setup flag)

```bash
Step 1/6: Homebrew — install if missing, eval shellenv
Step 2/6: Node.js — brew install node if missing
Step 3/6: Python 3 — brew install python3 if missing
Step 4/6: Ollama:
  - brew install ollama if missing
  - Start ollama serve in background
  - Pull OLLAMA_MODEL (default: llama3.2:1b)
  - Kill the temporary ollama serve
Step 5/6: npm install --silent
Step 6/6: Python venv:
  - Detect Apple Silicon → requirements.txt (MLX)
  - Otherwise → requirements-windows.txt (openai-whisper)
  - Create venv, pip install
```

### Phase 5: Runtime launch

```bash
1. Preflight checks: node, npm, python3 must exist
2. Check node_modules/.bin/electron exists (npm install if not)
3. Ollama lifecycle:
   - If ollama binary exists AND not already running (pgrep):
     - ollama serve &, add PID, set OLLAMA_STARTED=true
   - If already running: log "not managed by this script"
4. Read WHISPER_MODEL from STT_MODEL env (default: small)
5. Detect platform: Apple Silicon → mlx backend, else → local backend
6. Ensure Python venv exists (create + install if not)
7. Start Node.js:
   - TELEMETRY_DEBUG env var forwarded
   - node src/server.js >/dev/null 2>&1 &
   - wait_for_port(NODE_PORT, 30s timeout, nc -z polling)
8. Start Python transcriber:
   - WHISPER_MODEL, WHISPER_BACKEND, AUDIO_INPUT_DEVICE, LOG_LEVEL env vars
   - Output piped through sed for [transcriber] prefix coloring
   - venv/bin/python transcriber/main.py &
9. Start Electron HUD:
   - node_modules/.bin/electron electron/main.js >/dev/null 2>&1 &
10. Print status summary (PIDs, links, model info)
11. wait $NODE_PID $PYTHON_PID $ELECTRON_PID (blocks until any exits)
```

### Key design decisions

- **Single .env file**: All three services read the same file — no config drift
- **Port probing**: `nc -z 127.0.0.1 $port` with 30s timeout and 1s polling
- **Ollama lifecycle**: Only kills Ollama on exit if THIS script started it
- **Platform detection**: `uname -m == arm64` on Darwin → Apple Silicon → MLX
- **Graceful shutdown**: SIGTERM first, sleep 1s, then SIGKILL survivors

---

## ai.service.js fallback chain (pseudo-code walkthrough)

**Source**: `src/services/ai.service.js` (650 lines)

### Provider resolution

```javascript
getAvailableProviders():
  1. Read config.enabled[] (explicit list from PROVIDER_ENABLED env)
  2. If empty: use config.order[] filtered by "has non-empty API key"
  3. Filter out providers in exponential backoff:
     - backoff_ms = min(30000 * 2^(failures-1), 600000)
     - If (now - failedAt) >= backoff: remove from failed list, allow retry
  4. Always append 'ollama' as terminal fallback (unless ollama_enabled=false)
  5. Return ordered list of available providers
```

### Streaming fallback (callAIWithFallbackStream)

```javascript
async *callAIWithFallbackStream(messages, options):
  providers = getAvailableProviders()  // never empty (ollama fallback)
  
  for each provider in providers:
    try:
      gen = stream{Provider}(messages, options)  // async generator
      first = await gen.next()                    // ← surfaces auth/connection errors
      
      markProviderAsSuccess(provider)
      
      // Re-yield first chunk + all remaining
      if first.value.text: yield {token, provider, model}
      for await chunk of gen:
        if chunk.text:  yield {token: chunk.text, provider, model}
        if chunk.usage: yield {usage: chunk.usage, provider, model}
      return  // success — stop trying
      
    catch err:
      markProviderAsFailed(provider)  // exponential backoff
      lastError = err
      continue  // try next provider
  
  throw "All providers failed"
```

### Exponential backoff

```
1 failure  → 30s cooldown
2 failures → 60s
3 failures → 120s
4+ failures → 600s (10 min cap)
```

### Answer mode routing (answerInterviewQuestion)

```javascript
async *answerInterviewQuestion(questionText, transcriptContext, memoryContext):
  answerMode = _answerMode || config.answer_mode || 'auto'
  
  if answerMode == 'ollama':
    try stream via Ollama; on fail → fall through to auto
  elif answerMode in ['openai','grok','gemini','claude']:
    try stream via that specific provider; on fail → fall through to auto
  
  // 'auto' or fallback from above
  yield* callAIWithFallbackStream(messages)
```

### Pricing table

The service includes a comprehensive pricing table (AI_PRICING) mapping model names to USD/1M tokens for input and output. Used to compute `ai_cost_usd_total` counter for Grafana dashboards. Handles:
- Date-versioned model IDs (strips `-YYYYMMDD` suffix)
- Family prefix matching (`ollama:*` → $0)
- Unknown models: logs one-time warning, records cost as 0

### Hot-reload mechanism

```javascript
// Config (.env file)
fs.watch(ENV_FILE_PATH, () => {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    dotenv.config({ path: ENV_FILE_PATH, override: true })
    this.config = { /* re-read all process.env vars */ }
  }, 150ms)
})

// Prompts directory
fs.watch(promptsDir, () => {
  clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    for each [type, filename] in PROMPT_FILE_MAP:
      content = fs.readFileSync(filename, 'utf8').trim()
      _promptCache.set(type, content)
  }, 150ms)
})
```

---

## Portable patterns (numbered 1..N)

### 1. LocalAgreement-2 Streaming Decoder
**Source**: `transcriber/streaming_stt.py:L1-L350`
**What**: Converts batch Whisper into a streaming decoder by re-decoding a rolling buffer every 300ms and committing words that appear identically in consecutive decodes
**Why good**: Elegant solution to Whisper's batch-only nature; produces stable partial results without model modification; adaptive silence threshold reduces latency for clear questions
**Why bad**: Re-decoding the full buffer is O(n) per tick — 15s buffer means ~400ms decode on M1; wastes GPU cycles on already-committed audio
**Bluey port strategy**: Implement in Rust with `whisper-rs`. The agreement algorithm is trivial (~30 lines). Use `tokio::time::interval` for the decode loop. Consider incremental decoding (only decode new audio + overlap) to reduce GPU waste.

### 2. Pre-STT Speaker Identification (Parallel Pipeline)
**Source**: `transcriber/always_on_listener.py:L180-L220`, `speaker_id.py`
**What**: Submits speech audio to SpeakerIDWorker on the FIRST silent frame (not after full silence threshold), so ECAPA inference runs in parallel with the silence wait
**Why good**: Eliminates ~50ms of sequential latency; speaker ID decision often arrives before silence threshold fires; fail-safe design (errors → PASS)
**Why bad**: Requires complex hold/discard/cancel state machine; race conditions possible if not carefully managed
**Bluey port strategy**: Use Rust `crossbeam::channel` for the segment queue. ECAPA via `ort` (ONNX Runtime Rust bindings). The hold/discard state maps to an `Arc<Mutex<HashMap<Uuid, HoldState>>>`.

### 3. Multi-Provider Fallback with Exponential Backoff
**Source**: `src/services/ai.service.js:L400-L480`
**What**: Tries providers in configured order; on failure marks provider with exponential backoff (30s→60s→120s→600s cap); always appends Ollama as terminal local fallback
**Why good**: Guarantees an answer even with zero paid API keys; first-chunk-await pattern surfaces auth errors before yielding; clean recovery when providers come back
**Why bad**: No circuit breaker pattern (half-open state); no health checks between requests; backoff is per-instance (lost on restart)
**Bluey port strategy**: Implement as a Rust trait `AiProvider` with `stream()` method. Use `tower::retry` or custom backoff. Persist failure state to SQLite for cross-restart memory.

### 4. Conversation Memory with Async Summarization
**Source**: `src/sockets/InterviewTranscriptBuffer.js`
**What**: Rolling window of Q&A pairs (max 5 recent); when over cap, oldest 3 are batch-merged via Ollama into a compressed summary; summaries capped at 3
**Why good**: Bounded token overhead (~850 tokens max); non-blocking (Ollama summarization is fire-and-forget); graceful degradation (if Ollama busy, sends raw pairs)
**Why bad**: No persistence (lost on restart); merge can fail silently; no semantic deduplication
**Bluey port strategy**: Same architecture but persist to SQLite. Use local Ollama or a small model for summarization. Consider embedding-based deduplication.

### 5. Content Protection (Invisible Overlay)
**Source**: `electron/main.js:L100-L110`
**What**: `setContentProtection(true)` + `alwaysOnTop: 'screen-saver'` + `visibleOnAllWorkspaces: true`
**Why good**: OS-level exclusion from ALL capture streams (not just window capture); works with Zoom, Meet, Teams, OBS, Loom
**Why bad**: Electron-specific API; no equivalent in web browsers
**Bluey port strategy**: Tauri supports `set_content_protected(true)` on macOS (maps to same `NSWindow.sharingType = .none`). On Windows: `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`. Both available in Tauri's window API.

### 6. Adaptive Silence Threshold
**Source**: `transcriber/streaming_stt.py:L35-L45`
**What**: Two silence thresholds — FAST (300ms) when text ends in sentence punctuation AND committed words stable for ≥2 decode ticks; SLOW (1000ms) otherwise
**Why good**: Reduces end-to-end latency by 700ms for clear questions ending in "?"; guards against mid-sentence pauses
**Why bad**: Punctuation detection is fragile (Whisper may not always produce punctuation); stability tracking adds complexity
**Bluey port strategy**: Direct port — same logic in Rust. Consider also using prosodic features (pitch drop) as an additional signal.

### 7. Hot-Reload via fs.watch with Debounce
**Source**: `src/services/ai.service.js:L150-L180`
**What**: Watches .env file and prompts/ directory; on change, debounces 150ms then reloads config/prompts into memory
**Why good**: Zero-downtime config changes; edit prompts in real-time during interview; no restart needed for API key rotation
**Why bad**: fs.watch is unreliable on some platforms (Linux inotify limits); no validation on reload (bad config silently applied)
**Bluey port strategy**: Use `notify` crate (Rust file watcher). Debounce with `tokio::time::sleep`. Validate config on reload, reject invalid.

### 8. Single .env as Config Source of Truth
**Source**: `start.sh` load_env(), `transcriber/config.py`, `src/server.js`
**What**: All three services read the same `.env` file at project root; legacy `api-keys.json` auto-migrated on first run
**Why good**: Single place to configure everything; no config drift between services; settings UI writes to .env and all services pick it up
**Why bad**: No schema validation; no type safety; easy to introduce typos
**Bluey port strategy**: Use a single `config.toml` with a Rust struct + serde for type-safe deserialization. Tauri can expose it to the frontend via commands.

### 9. Ollama as Classification + Summarization Engine
**Source**: `src/services/ai.service.js` (summarizeQAPair, summarizeMerge)
**What**: Uses local Ollama (llama3.2:1b) exclusively for: (1) Q&A summarization for memory compression, (2) conversation merge. Never for primary answers.
**Why good**: Free, fast, always available; keeps expensive API calls for actual answers; non-blocking (fire-and-forget)
**Why bad**: 1b model quality is marginal for summarization; no fallback if Ollama is down
**Bluey port strategy**: Use `ollama-rs` crate or direct HTTP to localhost:11434. Consider `candle` for a truly embedded small model.

### 10. Dual-Mode STT (Local Whisper vs Deepgram Cloud)
**Source**: `transcriber/main.py`, `transcriber/deepgram_listener.py`
**What**: Toggle between fully-offline local Whisper (MLX/CPU) and Deepgram cloud STT (nova-2, ~300ms latency) via config flag; same downstream interface (stt_partial/stt_final)
**Why good**: User choice between privacy (offline) and speed (cloud); same HUD experience regardless of backend; Deepgram's built-in diarization replaces local speaker ID
**Why bad**: Two completely different code paths to maintain; Deepgram costs money ($0.0059/min)
**Bluey port strategy**: Abstract behind a Rust trait `SttBackend { fn start(); fn stop(); fn on_partial(); fn on_final(); }`. Implement for whisper-rs and Deepgram SDK.

### 11. OpenTelemetry Dual-Surface Observability
**Source**: `src/utils/telemetry.js`, `transcriber/telemetry.py`
**What**: Both Node and Python export metrics (histograms, counters, gauges) and structured logs to Grafana Cloud via OTLP HTTP; fallback to local JSONL when disabled
**Why good**: Production-grade observability; same metric names across both services; pre-built Grafana dashboard (9552-line JSON); system metrics sampler (CPU, memory, GPU)
**Why bad**: Heavy dependency tree (OTel SDK); adds ~50ms startup time; Grafana Cloud costs money for high-volume metrics
**Bluey port strategy**: Use `opentelemetry-rust` crate. Same OTLP HTTP export. Metrics: `ai_ttft_ms`, `ai_total_ms`, `end_to_end_question_ms`, `whisper_decode_ms`, `vad_latency_ms`, `speaker_id_latency_ms`.

### 12. Question Answerability Pre-Filter
**Source**: `transcriber/always_on_listener.py:L50-L100`
**What**: Before sending to AI, checks: (1) not a greeting, (2) not a goodbye, (3) not too short (<5 words), (4) not a hallucination, (5) not gibberish (>60% word repetition or repeated bigrams ≥3)
**Why good**: Prevents wasted API calls on small talk; reduces false positives; conservative default (borderline → pass through)
**Why bad**: Rule-based heuristics miss nuanced cases; no ML-based intent classification
**Bluey port strategy**: Same heuristics in Rust. Consider adding a tiny classifier (e.g., distilled BERT) for better accuracy.

## OpenTelemetry observability deep-dive

### Architecture

Both services export to the same Grafana Cloud instance via OTLP HTTP:
- **Node.js** (`src/utils/telemetry.js`): service name `solvewatch.server`
- **Python** (`transcriber/telemetry.py`): service name `solvewatch.transcriber`

### Metric names (histograms)

| Metric | Service | Unit | Description |
|--------|---------|------|-------------|
| `ai_ttft_ms` | Node | ms | Time to first AI token |
| `ai_total_ms` | Node | ms | Total AI generation time |
| `end_to_end_question_ms` | Node | ms | Silence detected → first AI token |
| `stt_socket_rtt_ms` | Node | ms | Python→Node socket transit time |
| `question_extraction_ms` | Node | ms | Time to extract clean question from AI stream |
| `ocr_duration_ms` | Node | ms | Tesseract OCR time |
| `screenshot_pipeline_total_ms` | Node | ms | Full screenshot→answer pipeline |
| `http_request_duration_ms` | Node | ms | Express request duration |
| `vad_latency_ms` | Python | ms | Per-chunk VAD inference time |
| `whisper_decode_ms` | Python | ms | Per-decode Whisper inference time |
| `speaker_id_latency_ms` | Python | ms | ECAPA embedding + comparison |
| `silence_wait_actual_ms` | Python | ms | Actual silence duration before firing |

### Metric names (counters)

| Metric | Service | Labels | Description |
|--------|---------|--------|-------------|
| `ai_provider_success_total` | Node | provider, flow | Successful AI calls |
| `ai_provider_failure_total` | Node | provider, flow, error_class | Failed AI calls |
| `ai_input_tokens_total` | Node | provider, model, flow | Input tokens consumed |
| `ai_output_tokens_total` | Node | provider, model, flow | Output tokens generated |
| `ai_cost_usd_total` | Node | provider, model, flow | Estimated cost in USD |
| `ai_cache_read_tokens_total` | Node | provider, model | Anthropic cache hits |
| `ai_cache_creation_tokens_total` | Node | provider, model | Anthropic cache writes |
| `screenshot_captured_total` | Node | — | Screenshots processed |
| `utterances_detected` | Python | — | VAD speech-start events |
| `utterances_passed` | Python | — | Utterances sent to AI |
| `utterances_discarded` | Python | reason | Filtered utterances |
| `deepgram_events` | Python | event_type | Deepgram event counts |
| `deepgram_audio_seconds` | Python | — | Audio streamed to Deepgram |
| `deepgram_cost_usd` | Python | — | Estimated Deepgram cost |

### Metric names (gauges)

| Metric | Service | Labels | Description |
|--------|---------|--------|-------------|
| `listener_active` | Python | — | 1 = listening, 0 = stopped |
| `whisper_model_loaded` | Python | model_name, backend | Currently loaded model |
| `speaker_id_model_status` | Python | status | ok/disabled/load_failed/not_enrolled |
| `deepgram_connected` | Python | — | WebSocket connection state |
| `host_cpu_percent` | Both | host_id, hostname | System CPU usage |
| `host_memory_percent` | Both | host_id, hostname | System memory usage |
| `host_memory_used_bytes` | Both | host_id, hostname | Memory in bytes |
| `process_cpu_percent` | Both | host_id, hostname | Process CPU |
| `process_memory_rss_bytes` | Both | host_id, hostname | Process RSS |
| `gpu_utilization_percent` | Both | host_id, hostname | GPU usage (Apple/NVIDIA) |
| `gpu_memory_used_bytes` | Both | host_id, hostname | GPU memory |

### Structured log events

Both services emit structured log records via OTel LoggerProvider:
- `server_start`, `server_stop` (Node)
- `transcriber_start`, `transcriber_stop` (Python)
- `question_answered` (Node — per-question with latency, cost, provider)
- `stt_final_emitted`, `stt_final_discarded` (Python)
- `vad_chunk` (Python — sampled at 10%)
- `voice_enrolled`, `speaker_id_loaded_dynamic` (Python)

### Fallback logging

When OTel is disabled, both services write to local JSONL files:
- `logs/telemetry_node.jsonl` (truncated on every server start)
- `logs/telemetry_python.jsonl` (truncated on every server start)

### Host identity labels

Both services compute stable host identity for multi-machine dashboards:
- `host_id`: IOPlatformUUID (macOS) or /etc/machine-id (Linux)
- `hostname`: os.hostname()
- `host_owner`: from HOST_OWNER env var
- `hardware_model`: sysctl hw.model (macOS)
- `cpu_brand`: sysctl machdep.cpu.brand_string

### Grafana dashboard

`docs/grafana-dashboard.json` (9552 lines) — pre-built panels including:
- AI latency (TTFT, total, end-to-end)
- Token spend and cost tracking
- Provider success/failure rates
- VAD performance (latency distribution)
- Whisper decode times
- Speaker ID latency
- System resources (CPU, memory, GPU)
- Deepgram metrics (when enabled)

---

## prompts/*.txt inventory + full content of each

### system-prompt.txt
FAANG engineer persona for screenshot flow. Extracts core question from UI noise, provides concise solutions. Format: Problem → Code → Complexity → Constraints.

### transcription-prompt.txt
Live interview processing. Identifies question type (coding vs theoretical). For coding: extracts problem, provides solution. For theory: extracts question, provides structured answer with key points.

### interview-answer-prompt.txt
**The most important prompt** — used for always-on listener answers. Format:
```
Q: <one-sentence restatement>
A:
- bullet 1 (max 5 bullets)
- bullet 2
```
Injects `{TRANSCRIPT_CONTEXT}` and `{MEMORY_CONTEXT}` placeholders. Rules: no filler, no "great question", max 5 bullets, optional code (max 10 lines).

### coding-prompt.txt
Coding problem specialist. Extracts problem, analyzes constraints, provides complete solution (default: JavaScript). Format: Problem → Solution → Complexity → Constraints.

### debug-prompt.txt
Debugging specialist. Identifies bug, analyzes root cause, provides step-by-step debugging approach, solution, and prevention strategies.

### theory-prompt.txt
Theoretical CS/SE concepts. Covers: databases, system design, DS&A, networking, security, OS, distributed systems. Provides comprehensive answer with examples and trade-offs.

### context-prompt.txt
Context-aware processing. Injects `{CONTEXT}` (previous AI response) so follow-up screenshots build on prior answers. Maintains conversation flow.

---

## Dependencies (Node package.json + Python requirements.txt)

### Node.js (package.json)

| Package | Version | Purpose |
|---------|---------|---------|
| express | ^5.1.0 | HTTP framework |
| socket.io | ^4.7.2 | Real-time WebSocket |
| electron | ^33.0.0 | Desktop overlay |
| openai | ^6.10.0 | OpenAI SDK (GPT + Whisper API) |
| groq-sdk | ^0.36.0 | Groq SDK |
| @google/generative-ai | ^0.24.1 | Gemini SDK |
| @anthropic-ai/sdk | ^0.88.0 | Claude SDK |
| sharp | ^0.34.5 | Image preprocessing |
| tesseract.js | ^6.0.0 | OCR engine |
| dotenv | ^16.4.7 | .env loading |
| multer | ^2.0.2 | File upload middleware |
| osx-mouse | ^2.0.0 | Mouse click detection (screenshot crop) |
| systeminformation | ^5.21.20 | Screen dimensions |
| @opentelemetry/* | ^1.25.0 | Metrics + logs export |

### Python (requirements.txt)

| Package | Purpose |
|---------|---------|
| fastapi + uvicorn | HTTP API server |
| python-socketio | Socket.IO client |
| sounddevice | Microphone capture |
| mlx + mlx-whisper | Apple Silicon Whisper (GPU) |
| openai | Whisper API fallback |
| webrtcvad | WebRTC VAD engine |
| onnxruntime | Silero VAD inference |
| speechbrain + torch + torchaudio | ECAPA-TDNN speaker ID |
| deepgram-sdk | Cloud STT alternative |
| opentelemetry-* | Metrics + logs export |
| psutil | System metrics |
| pynput | Keyboard shortcuts |
| python-dotenv | .env loading |

---

## Unique to solveWatchAi

1. **LocalAgreement-2 streaming decoder** — Novel algorithm for streaming Whisper without model modification. Re-decodes rolling buffer, commits words stable across consecutive decodes. No other open-source project implements this.

2. **Pre-STT speaker identification with parallel pipeline** — ECAPA-TDNN runs in parallel with silence wait (not sequentially). Hold/discard/cancel state machine prevents race conditions. Unique architecture.

3. **Adaptive silence threshold** — Two-tier silence detection (300ms fast / 1000ms slow) based on punctuation + committed-word stability. Reduces end-to-end latency by 700ms for clear questions.

4. **Dual-surface OpenTelemetry** — Both Node.js and Python export to the same Grafana Cloud instance with matching metric names. Pre-built 9552-line dashboard JSON. System metrics sampler (CPU/memory/GPU) every 10s.

5. **AI pricing table with cost tracking** — Comprehensive per-model pricing (USD/1M tokens) for all supported models. Computes real-time cost per question. Handles date-versioned model IDs and family prefixes.

6. **Conversation memory with async Ollama summarization** — Rolling Q&A window with automatic compression. Oldest entries batch-merged into summaries via local Ollama. Non-blocking, bounded token overhead (~850 tokens max).

7. **Content protection + screen-saver level always-on-top** — OS-level capture exclusion confirmed working on Zoom, Meet, Teams, Loom, OBS. Combined with `visibleOnAllWorkspaces` for multi-monitor support.

8. **Single .env config with auto-migration** — Legacy JSON config automatically migrated to .env on first run. All three services read the same file. Hot-reloaded by both Node (fs.watch) and Python (dotenv reload endpoint).

9. **Deepgram cloud STT as drop-in alternative** — Same stt_partial/stt_final interface regardless of backend. Deepgram's built-in diarization replaces local speaker ID. Auto-enrollment via saved PCM audio.

10. **Question answerability pre-filter** — Rule-based filter prevents AI calls on greetings, goodbyes, hallucinations, gibberish. Conservative default (borderline → pass through). Reduces false positives without ML overhead.

---

## CLAUDE.md rules to adopt for Bluey

From the 510-line CLAUDE.md, these developer-workflow rules should be adopted:

1. **ESM only** — never `require()`, always `import/export`
2. **Socket events in snake_case** — consistent naming across all services
3. **Services are thin stateless singletons** — export instance or pure functions
4. **Controllers are thin** — all logic in services
5. **All Socket.IO logic in one handler file** — single source of truth for events
6. **No business logic in server.js or app.js** — bootstrap only
7. **Always async/await** — wrap Socket.IO handlers in try/catch
8. **Ollama calls must be fire-and-forget** — never block answer streaming
9. **Never hardcode API keys** — always from config
10. **Never add state outside the designated buffer** — single memory location
11. **PR checklist**: update CHANGELOG + fill PR template before creating
12. **Graph tools over grep** — use code-review-graph for exploration
13. **Key files to read before changing** — dependency map for safe modifications
14. **Two processing flows documented** — screenshot flow + always-on listen flow
15. **Troubleshooting table** — symptom → likely cause → fix mapping
