# DEEP ANALYSIS — pluely (Tauri 2 + React 19 + TS + Rust + ~10MB)

**Scope**: 155 source files, 26,899 LOC read line-by-line
**Why this matters**: closest stack match to bluey — identical Tauri 2 + React + TS + Rust architecture
**Version**: 0.1.9 (GPL-3.0)
**Author**: Srikanth Nani

---

## File manifest (per-dir)

### src-tauri/src/ (Rust backend — 7 modules, ~4,700 LOC)
```
lib.rs          257L  — App setup, plugin registration, NSPanel init, Tauri command surface
main.rs           5L  — Entry point (calls pluely_lib::run())
window.rs       224L  — Window positioning, dashboard creation, resize commands
shortcuts.rs    659L  — Global shortcut registration, move-window loop, license gating
capture.rs      391L  — Screen capture (xcap), overlay windows, area selection
api.rs         1167L  — AI streaming, transcription, model fetching, activity tracking
activate.rs     437L  — License activation/deactivation/validation, secure storage
speaker/mod.rs  148L  — Platform abstraction for audio capture (Stream trait)
speaker/macos.rs   386L  — cidre CoreAudio aggregate device + process tap
speaker/windows.rs 380L  — WASAPI loopback capture
speaker/linux.rs   473L  — PulseAudio monitor source capture
speaker/commands.rs 626L — VAD engine, continuous capture, WAV encoding, Tauri commands
db/mod.rs         3L  — Module re-export
db/main.rs       20L  — Migration definitions
db/migrations/chat-history.sql   35L  — conversations + messages tables
db/migrations/system-prompts.sql 20L  — system_prompts table
```

### src/ (React frontend — ~22,000 LOC)
```
main.tsx                    30L  — Conditional render: overlay vs app routes
routes/index.tsx            30L  — BrowserRouter with 11 routes
contexts/app.context.tsx   698L  — Global state: providers, license, settings, shortcuts
contexts/theme.context.tsx  ~80L — Dark/light/system theme
hooks/useCompletion.ts    1050L  — Main AI completion hook (submit, screenshot, files, history)
hooks/useSystemAudio.ts    928L  — System audio VAD capture + STT + AI pipeline
hooks/useChatCompletion.ts 725L  — Dashboard chat completion (separate from overlay)
hooks/useGlobalShortcuts.ts 269L — Event listener singleton for Tauri shortcut events
hooks/useWindow.ts         130L  — Window resize (54px↔600px) + focus tracking
hooks/useShortcuts.ts       ~60L — Shortcut config helpers
hooks/useSystemPrompts.ts   ~80L — CRUD for system prompts via SQLite
hooks/useSettings.ts        ~40L — Settings helpers
hooks/useHistory.ts         ~50L — Chat history navigation
hooks/useApp.ts             ~10L — useContext(AppContext) wrapper
hooks/useVersion.ts         ~20L — App version from Tauri
hooks/useCopyToClipboard.ts ~30L — Clipboard utility
hooks/useCustomProvider.ts  ~60L — Custom AI provider CRUD
hooks/useCustomSttProviders.ts ~60L — Custom STT provider CRUD
hooks/useMenuItems.tsx      ~80L — Sidebar menu items
hooks/useTitles.ts          ~30L — Page title management
lib/functions/ai-response.function.ts  416L — Dual-path AI streaming (Pluely API vs custom curl)
lib/functions/stt.function.ts          240L — STT with curl-based provider abstraction
lib/functions/common.function.ts       236L — Variable replacement, message building, path extraction
lib/functions/pluely.api.ts             20L — shouldUsePluelyAPI() check
lib/database/chat-history.action.ts    576L — SQLite CRUD for conversations/messages
lib/database/system-prompt.action.ts   ~100L — SQLite CRUD for system prompts
lib/storage/shortcuts.storage.ts       315L — Shortcut config persistence + validation
lib/storage/customizable.storage.ts    ~150L — App icon, always-on-top, autostart, cursor
lib/storage/ai-providers.ts            ~80L — AI provider localStorage
lib/storage/stt-providers.ts           ~80L — STT provider localStorage
lib/storage/response-settings.storage.ts ~80L — Response length/language settings
lib/storage/helper.ts                   ~30L — safeLocalStorage wrapper
config/constants.ts                     50L — Storage keys, defaults, markdown instructions
config/shortcuts.ts                     80L — Default shortcut action definitions
config/ai-providers.constants.ts       ~200L — Built-in AI provider curl templates
config/stt.constants.ts                ~150L — Built-in STT provider curl templates
pages/app/index.tsx                    ~100L — Main overlay page (completion + speech tabs)
pages/app/components/completion/       ~500L — Input, Audio, Files, Screenshot, MessageHistory
pages/app/components/speech/           ~800L — VAD visualizer, recording panel, results, settings
pages/chats/                           ~600L — Chat list + View (336L) + audio/files/screenshot
pages/dashboard/                       ~600L — PluelyApiSetup (498L) + Usage charts
pages/dev/                             ~500L — AI configs + STT configs + CreateEditProvider (281L)
pages/shortcuts/                       ~500L — ShortcutManager (307L) + ShortcutRecorder
pages/system-prompts/                  ~600L — PluelyPrompts (313L) + Create/Edit/Delete/Generate
pages/settings/                        ~300L — Theme, AlwaysOnTop, AppIcon, Autostart, DeleteChats
pages/screenshot/                      ~200L — Screenshot configuration
pages/audio/                           ~150L — Audio device selection
pages/responses/                       ~200L — Response length, language, auto-scroll
components/Overlay.tsx                 ~200L — Screen capture selection overlay (canvas drawing)
components/Sidebar.tsx                 ~150L — Dashboard navigation sidebar
components/Markdown/index.tsx          ~150L — streamdown + shiki + rehype-katex renderer
components/TextInput/index.tsx         ~100L — Main input with file drop
components/Header/index.tsx            ~80L  — Overlay header with drag region
components/Selection/index.tsx         ~100L — Area selection for screenshots
components/ui/                         ~800L — Radix + shadcn primitives (14 components)
types/                                 ~300L — TypeScript interfaces
```

---

## Cargo.toml inventory (Rust dependencies)

| Crate | Version | Purpose |
|-------|---------|---------|
| tauri | 2 | Framework (macos-private-api feature) |
| tauri-plugin-updater | 2.9.0 | Auto-update |
| tauri-plugin-http | 2.5.2 | HTTP requests from frontend |
| tauri-plugin-global-shortcut | 2 | Global hotkeys |
| tauri-plugin-keychain | 2.0 | Secure credential storage |
| tauri-plugin-sql | 2 (sqlite) | SQLite database |
| tauri-plugin-posthog | 0.2.4 | Analytics |
| tauri-plugin-machine-uid | 0.1.2 | Machine fingerprinting |
| tauri-plugin-shell | 2.3.1 | Shell command execution |
| tauri-plugin-opener | 2 | URL/file opening |
| tauri-plugin-autostart | 2.5.0 | Login item |
| tauri-nspanel | git:v2 | macOS NSPanel (non-activating) |
| tauri-plugin-macos-permissions | 2 | Screen recording permission check |
| cidre | 0.11.3 | macOS CoreAudio bindings |
| xcap | 0.0.12 | Cross-platform screen capture |
| cpal | 0.15.3 | Cross-platform audio (unused in final?) |
| hound | 3.5.1 | WAV encoding |
| reqwest | 0.12 (json,stream,multipart) | HTTP client |
| tokio | 1.0 (full) | Async runtime |
| ringbuf | 0.4.8 | Lock-free ring buffer for audio |
| futures-util | 0.3 | Stream combinators |
| serde/serde_json | 1 | Serialization |
| image | 0.25.6 | Image processing (PNG encoding) |
| base64 | 0.22 | Base64 encoding |
| uuid | 1.0 (v4) | UUID generation |
| anyhow | 1.0 | Error handling |
| tracing | 0.1 | Logging |
| once_cell | 1.19.0 | Lazy statics |
| dotenv | 0.15 | Build-time env vars |
| wasapi | 0.19.0 | Windows audio (loopback) |
| libpulse-binding | 2.30.1 | Linux PulseAudio |
| libpulse-simple-binding | 2.29.0 | Linux PulseAudio simple API |

## package.json inventory (Frontend dependencies)

| Package | Version | Purpose |
|---------|---------|---------|
| react | 19.1.0 | UI framework |
| react-dom | 19.1.0 | DOM renderer |
| react-router-dom | 7.9.5 | Client-side routing |
| @tauri-apps/api | ^2 | Tauri IPC |
| @tauri-apps/plugin-* | ^2 | Plugin JS bindings (autostart, global-shortcut, http, opener, process, sql, updater) |
| @radix-ui/react-* | latest | Primitives (dialog, dropdown, label, popover, scroll-area, select, slider, slot, switch, tabs) |
| cmdk | 1.1.1 | Command palette |
| streamdown | 1.6.10 | Streaming markdown renderer |
| shiki | 3.12.2 | Syntax highlighting |
| rehype-katex | 7.0.1 | Math rendering |
| remark-gfm | 4.0.1 | GitHub-flavored markdown |
| remark-math | 6.0.0 | Math parsing |
| recharts | 2.15.4 | Usage charts |
| lucide-react | 0.539.0 | Icons |
| tailwindcss | 4.1.12 | Styling |
| @tailwindcss/vite | 4.1.12 | Tailwind Vite plugin |
| class-variance-authority | 0.7.1 | Variant styling |
| tailwind-merge | 3.3.1 | Class merging |
| clsx | 2.1.1 | Conditional classes |
| moment | 2.30.1 | Date formatting |
| @bany/curl-to-json | 1.2.8 | cURL command parsing |
| @ricky0123/vad-react | 0.0.30 | Voice Activity Detection (browser) |
| react-error-boundary | 6.0.0 | Error boundaries |
| tauri-plugin-keychain | 2.0.1 | Keychain JS API |
| tauri-plugin-macos-permissions-api | 2.3.0 | macOS permissions JS API |
| tauri-plugin-posthog-api | 0.2.2 | PostHog JS API |


---

## Portable patterns (numbered 1..20)

### 1. NSPanel non-activating overlay (macOS)
**Source**: src-tauri/src/lib.rs:L165-L210
**What**: Converts main window to NSPanel with `NSWindowStyleMaskNonActivatingPanel` (1<<7), `NSFloatWindowLevel` (4), collection behavior `FullScreenAuxiliary | CanJoinAllSpaces`. Uses `tauri-nspanel` crate's `panel_delegate!` macro for key/resign callbacks.
**Why good**: Prevents the overlay from stealing focus from other apps — critical for meeting/interview use case. Panel stays visible across all Spaces.
**Why bad**: Uses deprecated cocoa APIs (marked `#[allow(deprecated)]`). The `tauri-nspanel` crate is from a git branch, not a stable release. No graceful fallback if panel creation fails.
**Bluey port strategy**: Direct port — bluey needs identical behavior. Consider pinning tauri-nspanel to a specific commit hash rather than branch.
**Dependency**: tauri-nspanel (git, v2 branch), cidre 0.11.3

### 2. Dynamic window height resize (54px ↔ 600px)
**Source**: src-tauri/src/window.rs:L72-L84, src/hooks/useWindow.ts
**What**: Main window starts at 54px (just an input bar). When AI response/popover opens, resizes to 600px via `invoke("set_window_height", { height })`. Frontend uses MutationObserver on Radix popover state to auto-collapse.
**Why good**: Minimal visual footprint when idle. Elegant expand/collapse UX.
**Why bad**: Fixed 600px max height regardless of content. MutationObserver on body is expensive. No animation/transition.
**Bluey port strategy**: Adopt the pattern but use CSS transitions and content-aware height calculation.
**Dependency**: @tauri-apps/api/core (invoke)

### 3. Platform-split audio capture (speaker/ module)
**Source**: src-tauri/src/speaker/{mod,macos,windows,linux}.rs
**What**: Unified `SpeakerInput` → `SpeakerStream` (impl `futures_util::Stream<Item=f32>`) with platform-specific backends:
- **macOS**: CoreAudio aggregate device with process tap (`ca::TapDesc::with_mono_global_tap_excluding_processes`), ring buffer producer/consumer
- **Windows**: WASAPI loopback capture in EventsShared mode with autoconvert, dedicated thread
- **Linux**: PulseAudio `@DEFAULT_MONITOR@` source via libpulse-simple
**Why good**: Clean abstraction — commands.rs doesn't care about platform. Ring buffer (128KB) prevents blocking. Waker pattern integrates with tokio.
**Why bad**: macOS uses `cidre` which is niche/unmaintained. Windows thread doesn't use tokio (blocking thread::spawn). Linux hardcodes 44100Hz. No device hot-plug detection.
**Bluey port strategy**: Adopt the trait pattern. Consider `cpal` for unified cross-platform audio instead of 3 separate implementations. The macOS process tap API is essential for system audio capture.
**Dependency**: cidre 0.11.3, wasapi 0.19.0, libpulse-binding 2.30.1, ringbuf 0.4.8

### 4. VAD (Voice Activity Detection) engine in Rust
**Source**: src-tauri/src/speaker/commands.rs:L95-L230
**What**: Chunk-based VAD with configurable parameters: `hop_size=1024`, `sensitivity_rms=0.012`, `peak_threshold=0.035`, `silence_chunks=45` (~1s), `min_speech_chunks=7` (~160ms), `pre_speech_chunks=12` (~270ms). Applies noise gate before analysis. Emits `speech-detected` events with WAV base64.
**Why good**: Runs entirely in Rust (no WASM overhead). Pre-speech buffer captures word starts. Configurable from frontend. 30s safety cap per utterance.
**Why bad**: Simple energy-based VAD (RMS + peak) — no ML model. Will trigger on music/noise. No spectral analysis. Normalization after capture (not real-time AGC).
**Bluey port strategy**: Port the VAD engine but consider adding a lightweight ML model (silero-vad via ONNX) for better accuracy. Keep the energy-based approach as a fast pre-filter.
**Dependency**: hound 3.5.1 (WAV encoding), base64 0.22

### 5. Dual-path AI streaming (Pluely API vs custom cURL)
**Source**: src/lib/functions/ai-response.function.ts
**What**: `fetchAIResponse()` async generator checks `shouldUsePluelyAPI()` — if licensed, streams via Tauri invoke (`chat_stream_response` → Rust reqwest → SSE → events). Otherwise, parses user-provided cURL template, replaces variables (`{{API_KEY}}`, `{{TEXT}}`, `{{IMAGE}}`), and streams directly from frontend via `fetch`/`tauriFetch`.
**Why good**: Supports ANY OpenAI-compatible API via cURL templates. No vendor lock-in. Pluely API path handles auth/billing server-side.
**Why bad**: cURL parsing via `@bany/curl-to-json` is fragile. Variable replacement is string-based (no type safety). Two completely different code paths to maintain.
**Bluey port strategy**: Adopt the custom-provider-via-cURL pattern for maximum flexibility. Simplify by standardizing on OpenAI-compatible format and only varying URL/key/model.
**Dependency**: @bany/curl-to-json 1.2.8, @tauri-apps/plugin-http 2.5.2

### 6. Shortcut registration with license gating
**Source**: src-tauri/src/shortcuts.rs:L250-L380
**What**: Frontend sends `ShortcutsConfig` (HashMap of action→key bindings) to Rust. Rust validates each key via `parse::<Shortcut>()`, unregisters all existing shortcuts, then registers new ones. `move_window` action expands to 4 directional shortcuts. License-gated features (move_window) are skipped if `LicenseState::is_active()` is false.
**Why good**: Centralized handler pattern — one `with_handler` closure dispatches all shortcuts by looking up action_id in state. Mutex poison recovery throughout.
**Why bad**: Full unregister-all + re-register on any change (could cause brief gaps). No shortcut conflict detection in Rust (only frontend). Registration failures are collected but don't roll back successful ones.
**Bluey port strategy**: Adopt centralized handler pattern. Add atomic swap (register new set, then unregister old) to avoid gaps.
**Dependency**: tauri-plugin-global-shortcut 2

### 7. Continuous window movement via held shortcut
**Source**: src-tauri/src/shortcuts.rs:L130-L175
**What**: On `ShortcutState::Pressed` for `move_window_*`, spawns a tokio task that moves window 12px every 16ms (60fps). On `ShortcutState::Released`, sets `AtomicBool` stop flag. Uses `MoveWindowState` with `HashMap<String, Arc<AtomicBool>>`.
**Why good**: Smooth movement at 60fps. Atomic stop flag ensures immediate halt. Multiple directions can be active simultaneously.
**Why bad**: 12px step is hardcoded (not DPI-aware). No acceleration curve. No bounds checking (window can move off-screen).
**Bluey port strategy**: Port with DPI-aware step size and screen bounds clamping.
**Dependency**: tokio (async_runtime::spawn)

### 8. Screen capture with multi-monitor overlay
**Source**: src-tauri/src/capture.rs:L45-L160
**What**: `start_screen_capture` captures all monitors via `xcap`, creates transparent overlay windows per monitor (positioned using Tauri monitor layout), stores images in `CaptureState`. User draws selection on overlay → `capture_selected_area` crops and returns base64 PNG.
**Why good**: Multi-monitor support. Overlay windows are transparent + always-on-top + non-resizable. Scale factor handling for HiDPI.
**Why bad**: 100ms sleep for window settle (race condition workaround). Overlay uses same `index.html` entry point (wasteful). No crosshair cursor. Thread::sleep blocks.
**Bluey port strategy**: Adopt multi-monitor pattern. Use a dedicated lightweight HTML for overlay. Replace thread::sleep with proper window-ready events.
**Dependency**: xcap 0.0.12, image 0.25.6, base64 0.22

### 9. SQLite via tauri-plugin-sql with Rust migrations
**Source**: src-tauri/src/db/main.rs, src/lib/database/chat-history.action.ts
**What**: Migrations defined in Rust (include_str! of .sql files), executed on app start. Frontend uses `@tauri-apps/plugin-sql` to execute raw SQL. Chat history uses conversations + messages tables with proper indexes, triggers for auto-updating `updated_at`.
**Why good**: Migrations in Rust ensure they run before frontend loads. Proper foreign keys with CASCADE delete. Composite indexes for common query patterns.
**Why bad**: Raw SQL strings in TypeScript (no query builder, no type safety). No connection pooling config. No WAL mode explicitly set.
**Bluey port strategy**: Adopt migration pattern. Add a thin type-safe query layer in TS. Enable WAL mode for concurrent reads.
**Dependency**: tauri-plugin-sql 2 (sqlite feature)

### 10. Secure storage (license keys) via filesystem JSON
**Source**: src-tauri/src/activate.rs:L35-L130
**What**: `SecureStorage` struct serialized to `app_data_dir/secure_storage.json`. Stores license_key, instance_id, selected_pluely_model. Tauri commands for save/get/remove with key validation.
**Why good**: Simple, works cross-platform. App data dir is OS-appropriate.
**Why bad**: NOT actually secure — plain JSON on disk. Despite having `tauri-plugin-keychain` in deps, it's not used for license storage. No encryption. Any process can read the file.
**Bluey port strategy**: Use the OS keychain (already have the plugin) for sensitive data. Keep JSON for non-sensitive preferences.
**Dependency**: tauri-plugin-keychain 2.0 (available but unused for this)

### 11. Event-driven frontend-backend communication
**Source**: src-tauri/src/speaker/commands.rs (emit), src/hooks/useSystemAudio.ts (listen)
**What**: Rust emits events (`speech-detected`, `capture-started`, `capture-stopped`, `recording-progress`, `chat_stream_chunk`, `chat_stream_complete`). Frontend listens via `@tauri-apps/api/event`. Bidirectional: frontend can also emit (`manual-stop-continuous`) which Rust listens to.
**Why good**: Decouples audio pipeline from UI. Streaming chunks arrive as events (no polling). Clean lifecycle (unlisten on cleanup).
**Why bad**: No typed event payloads (all `any`). Event names are magic strings. No event versioning. Memory leak risk if unlisten fails.
**Bluey port strategy**: Adopt but add TypeScript event type map for compile-time safety. Consider a shared event schema.
**Dependency**: @tauri-apps/api/event

### 12. Custom cursor hiding for stealth mode
**Source**: src/contexts/app.context.tsx (updateCursor), src/lib/storage/customizable.storage.ts
**What**: CSS variable `--cursor-type` set to `none` (invisible), `default`, or custom. Dashboard always uses default. Overlay windows use configured cursor. Linux forced to default (cursor:none doesn't work well).
**Why good**: Critical for stealth — no visible cursor over the overlay during meetings.
**Why bad**: CSS-only approach (cursor still exists for accessibility tools). No custom cursor image option. Platform detection is fragile.
**Bluey port strategy**: Port directly. Consider adding a custom semi-transparent cursor option for usability.
**Dependency**: None (pure CSS)

### 13. App icon visibility toggle (dock/taskbar)
**Source**: src-tauri/src/shortcuts.rs:L470-L510
**What**: `set_app_icon_visibility` command uses `ActivationPolicy::Accessory` (macOS) or `set_skip_taskbar` (Windows/Linux) to hide the app from dock/taskbar.
**Why good**: Essential for stealth mode — app doesn't appear in Alt-Tab or dock.
**Why bad**: On macOS, Accessory policy means no menu bar either. Can't easily get back to the app without the shortcut.
**Bluey port strategy**: Direct port. Ensure toggle shortcut is always registered even when icon is hidden.
**Dependency**: Tauri core APIs

### 14. Content-protected windows
**Source**: src-tauri/tauri.conf.json, src-tauri/src/window.rs:L170
**What**: Both main window and dashboard have `contentProtected: true` — prevents screen recording/screenshots of the app's own content.
**Why good**: Prevents the AI overlay from appearing in screen shares/recordings during meetings.
**Why bad**: Some screen recording tools bypass this. Not available on all Linux compositors.
**Bluey port strategy**: Essential — port directly. This is a core stealth feature.
**Dependency**: Tauri window config

### 15. Streaming markdown with code highlighting and math
**Source**: src/components/Markdown/index.tsx
**What**: Uses `streamdown` library for incremental markdown rendering during streaming. `shiki` for syntax highlighting. `rehype-katex` + `remark-math` for LaTeX math (double $$ only). Copy button per code block.
**Why good**: Renders incrementally as chunks arrive (no flicker). Full GFM support. Math rendering for technical content.
**Why bad**: `streamdown` is relatively unknown (1.6.10). Shiki loads all grammars (bundle size). No mermaid diagram support despite system prompt mentioning it.
**Bluey port strategy**: Adopt streamdown for streaming rendering. Consider lazy-loading shiki grammars. Add actual mermaid support.
**Dependency**: streamdown 1.6.10, shiki 3.12.2, rehype-katex 7.0.1, remark-gfm 4.0.1, remark-math 6.0.0

### 16. Response settings (length + language + auto-scroll)
**Source**: src/lib/storage/response-settings.storage.ts, src/lib/functions/ai-response.function.ts:L15-L35
**What**: User configures response length (concise/balanced/detailed) and language. These are appended to the system prompt as additional instructions. Auto-scroll toggle controls whether response area scrolls during streaming.
**Why good**: Simple approach — no API parameter changes needed. Works with any provider.
**Why bad**: Appending to system prompt is unreliable (models may ignore). No token-level control. Language instruction may conflict with user's actual prompt language.
**Bluey port strategy**: Adopt but also pass `max_tokens` parameter where supported. Language should be a UI preference, not a system prompt hack.
**Dependency**: None (localStorage + string concatenation)

### 17. Global shortcut singleton pattern (React)
**Source**: src/hooks/useGlobalShortcuts.ts
**What**: Module-level variables (`globalInputRef`, `globalAudioCallback`, etc.) persist across React StrictMode double-renders. Event listeners are set up once globally, callbacks are registered by individual hooks. Debouncing on screenshot events (300ms).
**Why good**: Prevents duplicate event listeners in StrictMode. Clean callback registration API. Debouncing prevents double-fires.
**Why bad**: Module-level mutable state is an anti-pattern. No cleanup on HMR. Global state makes testing difficult.
**Bluey port strategy**: Adopt the singleton pattern but wrap in a proper service class with explicit lifecycle management.
**Dependency**: @tauri-apps/api/event

### 18. Build-time environment variable injection
**Source**: src-tauri/build.rs
**What**: `build.rs` reads `.env` file via `dotenv`, then uses `cargo:rustc-env=` to inject `PAYMENT_ENDPOINT`, `API_ACCESS_KEY`, `APP_ENDPOINT`, `POSTHOG_API_KEY` at compile time. Runtime code uses `option_env!()` with fallback to `env::var()`.
**Why good**: Secrets are compiled into binary (not in config files). Runtime override possible via env vars.
**Why bad**: Secrets in binary can be extracted. No rotation without rebuild. `option_env!` returns empty string if not set (silent failure).
**Bluey port strategy**: Adopt for non-sensitive config. Use OS keychain for actual secrets. Add build-time validation that required vars are set.
**Dependency**: dotenv 0.15

### 19. Conversation persistence with debounced saves
**Source**: src/hooks/useSystemAudio.ts:L780-L830
**What**: System audio conversations are saved to SQLite with 500ms debounce (`CONVERSATION_SAVE_DEBOUNCE_MS`). Uses `isSavingRef` to prevent concurrent saves. Cleanup on unmount clears pending timeouts.
**Why good**: Prevents database thrashing during rapid speech detection. Ref-based guard prevents race conditions.
**Why bad**: 500ms debounce means data loss on crash. No write-ahead log. No conflict resolution if multiple windows write simultaneously.
**Bluey port strategy**: Adopt debounced save pattern. Add periodic flush and crash recovery (save on visibility change/beforeunload).
**Dependency**: @tauri-apps/plugin-sql

### 20. Dashboard window lifecycle (hide-on-close)
**Source**: src-tauri/src/window.rs:L155-L200
**What**: Dashboard window intercepts `CloseRequested` event, calls `api.prevent_close()`, then hides instead of destroying. Pre-created on app startup. Show/focus on demand.
**Why good**: Instant show (no creation delay). Preserves state between opens. Proper macOS traffic light positioning.
**Why bad**: Hidden window still consumes memory. No lazy creation option. Close handler uses clone (extra allocation).
**Bluey port strategy**: Adopt hide-on-close for dashboard. Consider lazy creation with state persistence for memory optimization.
**Dependency**: Tauri window events


---

## React architecture deep-dive

### Component hierarchy tree
```
main.tsx
├── [capture-overlay-*] → <Overlay monitorIndex={N} />  (canvas-based selection)
└── [main/dashboard] → <ThemeProvider> → <AppProvider> → <AppRoutes>
    ├── Route "/" → <App>  (overlay window)
    │   ├── <Header> (drag region, new chat, system audio toggle)
    │   ├── <TextInput> (input bar with file/screenshot/mic buttons)
    │   ├── <CompletionPanel> (AI response area)
    │   │   ├── <Markdown> (streaming renderer)
    │   │   ├── <Files> (attached file previews)
    │   │   ├── <Audio> (VAD recording indicator)
    │   │   ├── <Screenshot> (capture button + loading)
    │   │   └── <MessageHistory> (conversation context)
    │   └── <SpeechPanel> (system audio mode)
    │       ├── <ModeSwitcher> (VAD vs continuous)
    │       ├── <RecordingPanel> (waveform + controls)
    │       ├── <ResultsSection> (transcription + AI response)
    │       ├── <QuickActions> (configurable action buttons)
    │       └── <SettingsPanel> (VAD config, context, device selection)
    └── Route "/chats|dashboard|..." → <DashboardLayout>
        ├── <Sidebar> (navigation + version)
        └── <Outlet>
            ├── /dashboard → <Dashboard> (PluelyApiSetup + Usage)
            ├── /chats → <Chats> (conversation list)
            ├── /chats/view/:id → <ViewChat> (full conversation view)
            ├── /system-prompts → <SystemPrompts> (CRUD + PluelyPrompts)
            ├── /shortcuts → <Shortcuts> (ShortcutManager + Cursor config)
            ├── /screenshot → <Screenshot> (mode config)
            ├── /audio → <Audio> (device selection)
            ├── /responses → <Responses> (length + language + auto-scroll)
            ├── /settings → <Settings> (theme, autostart, always-on-top, app icon)
            └── /dev-space → <DevSpace> (AI configs + STT configs)
```

### Hook composition patterns

**useCompletion** (1050L) — The "god hook" for the overlay:
- Manages: input state, response streaming, file attachments, conversation history, screenshot capture, mic toggle, keep-engaged mode, abort controllers
- Composes: `useApp()` (providers, system prompt), `useGlobalShortcuts()` (callback registration), `useWindowResize()` (expand/collapse)
- Pattern: Single `CompletionState` object with `setState` partial updates. `useCallback` for all handlers. `useRef` for abort controllers and processing flags.
- Anti-pattern: 1050 lines in one hook. Multiple `useEffect` with complex dependency arrays. Duplicated AI call logic between `submit()` and `handleScreenshotSubmit()`.

**useSystemAudio** (928L) — System audio capture pipeline:
- Manages: capture state, VAD config, transcription, AI response, conversation, quick actions, continuous recording, context settings
- Composes: `useApp()` (providers), `useWindowResize()`, `useGlobalShortcuts()`
- Pattern: Event-driven via Tauri `listen()`. Debounced conversation saves. AbortController for AI requests.
- Anti-pattern: Also a god hook. Mixes UI state (isPopoverOpen) with domain logic (VAD processing). Multiple useEffect for event listeners.

**useChatCompletion** (725L) — Dashboard chat (separate from overlay):
- Similar to useCompletion but for the dashboard window
- Has its own conversation management, file handling, streaming
- Pattern: Duplicates ~60% of useCompletion logic

### Context providers

**AppProvider** (698L):
- Global state: selectedAIProvider, selectedSttProvider, customAiProviders, customSttProviders, systemPrompt, screenshotConfiguration, customizable (theme, cursor, autostart, always-on-top, app-icon), hasActiveLicense, pluelyApiEnabled, selectedAudioDevices, supportsImages
- Initialization: Loads from localStorage on mount, validates license, tracks app start, syncs shortcuts to Rust
- Pattern: Single massive context with ~30 values. No splitting by concern.
- Anti-pattern: Everything in one context causes unnecessary re-renders. License validation on every mount.

**ThemeProvider** (~80L):
- Manages dark/light/system theme via CSS class on `<html>`
- Persists to localStorage
- Listens to system preference changes

### Routing (react-router-dom 7)
- BrowserRouter (not hash router — works because Tauri serves from filesystem)
- 11 routes total: 1 root (overlay) + 10 dashboard routes
- DashboardLayout wraps all dashboard routes (sidebar + outlet)
- No lazy loading, no code splitting
- No route guards (license check is in components)

### State persistence
- **localStorage**: All user preferences, provider configs, shortcuts, theme, response settings, audio devices, screenshot config, system prompt, quick actions, VAD config
- **SQLite** (via tauri-plugin-sql): Chat conversations, messages, system prompts
- **Rust filesystem** (secure_storage.json): License key, instance ID, selected Pluely model
- **No IndexedDB, no sessionStorage**
- Pattern: `safeLocalStorage` wrapper handles JSON parse errors gracefully

### UI library composition
- **Radix UI**: Dialog, DropdownMenu, Label, Popover, ScrollArea, Select, Slider, Slot, Switch, Tabs (10 primitives)
- **shadcn/ui pattern**: Components in `src/components/ui/` wrap Radix with Tailwind styling via CVA
- **Tailwind CSS 4**: Via `@tailwindcss/vite` plugin (no PostCSS config needed)
- **cmdk**: Command palette (used for model/prompt selection)
- **streamdown**: Streaming markdown (replaces react-markdown for incremental rendering)
- **shiki**: Code syntax highlighting (loaded at runtime)
- **rehype-katex + remark-math**: LaTeX math rendering
- **lucide-react**: Icon library
- **recharts**: Usage analytics charts
- **tw-animate-css**: Animation utilities

---

## Rust architecture deep-dive

### Tauri command surface (every #[tauri::command])

| Command | File | Purpose |
|---------|------|---------|
| `get_app_version` | lib.rs | Returns CARGO_PKG_VERSION |
| `set_window_height` | window.rs | Resize main window height |
| `open_dashboard` | window.rs | Show dashboard window |
| `toggle_dashboard` | window.rs | Toggle dashboard visibility |
| `move_window` | window.rs | Move window by direction+step |
| `capture_to_base64` | capture.rs | Full-screen capture → base64 PNG |
| `start_screen_capture` | capture.rs | Open selection overlay |
| `capture_selected_area` | capture.rs | Crop selection → base64 PNG |
| `close_overlay_window` | capture.rs | Destroy capture overlays |
| `check_shortcuts_registered` | shortcuts.rs | Check if any shortcuts active |
| `get_registered_shortcuts` | shortcuts.rs | Get action→key map |
| `update_shortcuts` | shortcuts.rs | Re-register all shortcuts |
| `validate_shortcut_key` | shortcuts.rs | Validate key string format |
| `set_license_status` | shortcuts.rs | Update license state (gates features) |
| `set_app_icon_visibility` | shortcuts.rs | Show/hide dock icon |
| `set_always_on_top` | shortcuts.rs | Toggle always-on-top |
| `exit_app` | shortcuts.rs | Graceful exit |
| `activate_license_api` | activate.rs | Activate license key |
| `deactivate_license_api` | activate.rs | Deactivate license |
| `validate_license_api` | activate.rs | Check license validity |
| `mask_license_key_cmd` | activate.rs | Mask key for display |
| `get_checkout_url` | activate.rs | Get payment URL |
| `secure_storage_save` | activate.rs | Save to secure storage |
| `secure_storage_get` | activate.rs | Read secure storage |
| `secure_storage_remove` | activate.rs | Remove from secure storage |
| `transcribe_audio` | api.rs | Audio → text via configured STT |
| `chat_stream_response` | api.rs | Streaming AI chat (SSE → events) |
| `fetch_models` | api.rs | Get available AI models |
| `fetch_prompts` | api.rs | Get Pluely community prompts |
| `create_system_prompt` | api.rs | Generate system prompt via AI |
| `check_license_status` | api.rs | Check if credentials exist |
| `get_activity` | api.rs | Get usage activity data |
| `start_system_audio_capture` | speaker/commands.rs | Start audio capture (VAD/continuous) |
| `stop_system_audio_capture` | speaker/commands.rs | Stop audio capture |
| `manual_stop_continuous` | speaker/commands.rs | Stop continuous recording |
| `check_system_audio_access` | speaker/commands.rs | Check audio permission |
| `request_system_audio_access` | speaker/commands.rs | Open system preferences |
| `get_vad_config` | speaker/commands.rs | Get VAD parameters |
| `update_vad_config` | speaker/commands.rs | Update VAD parameters |
| `get_capture_status` | speaker/commands.rs | Check if capturing |
| `get_audio_sample_rate` | speaker/commands.rs | Get device sample rate |
| `get_input_devices` | speaker/commands.rs | List microphones |
| `get_output_devices` | speaker/commands.rs | List speakers |

**Total: 40 Tauri commands**

### Plugin list with rationale

| Plugin | Why |
|--------|-----|
| tauri-nspanel | NSPanel overlay (non-activating, float level) — macOS only |
| tauri-plugin-keychain | Secure credential storage (available but underused) |
| tauri-plugin-sql | SQLite for chat history + system prompts |
| tauri-plugin-autostart | Launch on login |
| tauri-plugin-global-shortcut | System-wide hotkeys |
| tauri-plugin-macos-permissions | Check screen recording permission |
| tauri-plugin-machine-uid | Hardware fingerprint for license binding |
| tauri-plugin-posthog | Analytics (session recording disabled) |
| tauri-plugin-updater | Auto-update from pluely.com/api/update |
| tauri-plugin-http | Frontend HTTP requests (bypasses CORS) |
| tauri-plugin-shell | Open system preferences |
| tauri-plugin-opener | Open URLs in browser |

### State management (Rust side)

| State | Type | Purpose |
|-------|------|---------|
| `AudioState` | `Arc<Mutex<Option<JoinHandle>>>` + `Arc<Mutex<VadConfig>>` + `Arc<Mutex<bool>>` | Audio capture task, config, status |
| `CaptureState` | `Arc<Mutex<HashMap<usize, MonitorInfo>>>` + `Arc<AtomicBool>` | Captured monitor images, overlay active flag |
| `WindowVisibility` | `Mutex<bool>` | Window hidden state (Windows workaround) |
| `RegisteredShortcuts` | `Mutex<HashMap<String, String>>` | action_id → shortcut_key mapping |
| `LicenseState` | `AtomicBool` | License active flag |
| `MoveWindowState` | `Mutex<HashMap<String, Arc<AtomicBool>>>` | Active move tasks with stop flags |

### Async patterns
- **tokio::spawn**: Used for move-window loop, activity reporting, error reporting
- **Arc<Mutex<T>>**: Primary shared state pattern (with poison recovery everywhere)
- **Arc<AtomicBool>**: Stop flags for audio capture and window movement
- **tauri::async_runtime::spawn**: For fire-and-forget tasks (activity tracking)
- **spawn_blocking**: Screen capture (xcap is synchronous)
- **Mutex poison recovery**: Every `.lock()` call has `match` with `poisoned.into_inner()` fallback
- **No channels**: All communication via Tauri events (emit/listen)

---

## NSPanel macOS overlay (deep)

### Exact cidre APIs used
```rust
use cidre::{arc, av, cat, cf, core_audio as ca, ns, os};
// CoreAudio:
ca::System::default_output_device()
ca::System::devices()
ca::TapDesc::with_mono_global_tap_excluding_processes(&ns::Array::new())
ca::AggregateDevice::with_desc(&agg_desc)
ca::device_start(agg_device, Some(proc_id))
// Audio format:
av::AudioFormat::with_asbd(&asbd)
av::AudioPcmBuf::with_buf_list_no_copy(&format, input_data, None)
// NSPanel (via tauri-nspanel):
tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior
window.to_panel()
panel_delegate!(MyPanelDelegate { window_did_become_key, window_did_resign_key })
```

### NSWindowStyleMaskNonActivatingPanel semantics
- Value: `1 << 7` (128)
- Effect: Panel does not become key window when clicked. The previously focused app retains focus.
- Critical for: Typing in Zoom/Teams while overlay is visible — keystrokes go to the meeting app, not the overlay.

### Collection behavior flags
```rust
NSWindowCollectionBehaviorFullScreenAuxiliary  // Appears alongside fullscreen apps
| NSWindowCollectionBehaviorCanJoinAllSpaces   // Visible on all virtual desktops
```

### Panel delegate pattern
```rust
let delegate = panel_delegate!(MyPanelDelegate {
    window_did_become_key,    // Called when panel gains key status
    window_did_resign_key     // Called when panel loses key status
});
delegate.set_listener(Box::new(move |delegate_name: String| { ... }));
panel.set_delegate(delegate);
```
- Currently only logs events (no functional behavior on key/resign)
- Panel level: `NSFloatWindowLevel` (4) — above normal windows but below screen saver


---

## Things pluely does WORSE than natively-cluely/solveWatchAi (bluey should take best of each)

### 1. God hooks (useCompletion 1050L, useSystemAudio 928L)
- Single hooks managing 20+ state variables each
- Duplicated AI call logic between overlay and dashboard
- Should be decomposed into: useAIStream, useConversation, useScreenCapture, useVoiceInput, useFileAttachments

### 2. No proper error boundary strategy
- Uses react-error-boundary but only at route level
- Individual component errors crash the entire view
- No retry mechanisms for transient failures

### 3. Fragile cURL-based provider abstraction
- `@bany/curl-to-json` parsing is brittle (fails on complex cURLs)
- Variable replacement is string-based (`{{VAR}}` → value)
- No validation that replaced body is valid JSON
- No provider health checks or automatic fallback

### 4. No offline support
- App is useless without internet (no local models)
- No queue for failed requests
- No cached responses

### 5. Security theater in "secure storage"
- License keys stored as plain JSON on filesystem
- `tauri-plugin-keychain` is in dependencies but NOT used for license storage
- Machine UID fingerprinting is trivially spoofable
- Content protection can be bypassed by virtual displays

### 6. No proper state management
- All state in React context (single massive AppProvider)
- localStorage as primary persistence (no migration strategy)
- No optimistic updates, no undo/redo
- Cross-window communication via localStorage events (fragile)

### 7. Audio quality issues
- Simple energy-based VAD (no ML) — triggers on music, keyboard clicks
- No echo cancellation
- No automatic gain control (only post-capture normalization)
- Linux hardcoded to 44100Hz (should detect device rate)
- No audio format negotiation

### 8. No streaming for STT
- Audio is captured → encoded to WAV → base64 → sent as single request
- No real-time streaming transcription (like Whisper streaming or Deepgram live)
- Latency: capture + encode + upload + transcribe + respond

### 9. Window management limitations
- Fixed 600px expanded height (no content-aware sizing)
- No animation on expand/collapse
- 12px move step not DPI-aware
- No snap-to-edge or magnetic positioning
- MutationObserver on entire body for popover detection (performance)

### 10. No test coverage
- Zero test files in the repository
- No unit tests, integration tests, or e2e tests
- No CI/CD configuration
- No linting configuration (no eslint, no clippy in CI)

### 11. Memory management
- Dashboard window always in memory (even when hidden)
- Captured monitor images stored in HashMap until explicitly cleared
- No cleanup of old conversations from memory
- Audio ring buffer (128KB) never shrinks

### 12. Telemetry concerns
- PostHog analytics enabled by default
- Machine UID sent with every API call
- Usage metrics (token counts) reported to server
- No opt-out mechanism visible in UI

---

## Unique to pluely (features bluey should consider)

### 1. System audio capture with VAD
- Captures system speaker output (not just microphone)
- Automatic speech detection segments audio into utterances
- Pre-speech buffer ensures word starts aren't clipped
- Continuous mode for manual start/stop recording
- Configurable VAD parameters from UI

### 2. Quick actions for system audio
- Configurable action buttons ("What should I say?", "Follow-up questions", "Fact-check", "Recap")
- One-click AI processing of last transcription with custom prompts
- Persistent across sessions (localStorage)

### 3. cURL-based custom provider system
- Users paste a cURL command to add any AI/STT provider
- Automatic variable extraction (`{{API_KEY}}`, `{{TEXT}}`, `{{IMAGE}}`)
- Supports both streaming and non-streaming responses
- Response content path configuration for non-standard APIs

### 4. Pluely API (managed backend)
- Licensed users get a managed API (no key management)
- Server-side model routing and billing
- Fallback STT endpoints (primary + fallback URL/token/model)
- Error mapping rules (server returns user-friendly messages per error pattern)
- Activity tracking with usage metrics

### 5. Screenshot modes (auto vs manual vs selection)
- **Auto**: Capture + send to AI with configured prompt (one shortcut)
- **Manual**: Capture + attach to input (user adds their own prompt)
- **Selection**: Open overlay, draw rectangle, crop and use
- All three modes accessible via same shortcut (configured in settings)

### 6. Keep-engaged mode (Cmd+K)
- Prevents response area from collapsing after AI responds
- Allows follow-up questions without re-expanding
- Visual indicator when active

### 7. Multi-image support
- Up to 6 images per message
- Paste from clipboard, file picker, or screenshot
- Images sent as base64 in OpenAI vision format
- Provider capability detection (supportsImages flag)

### 8. Community system prompts
- Fetch curated prompts from Pluely server
- One-click import to local database
- Generate custom prompts via AI (meta-prompting)

### 9. Response settings (length + language)
- Configurable response length: concise/balanced/detailed/comprehensive
- Language override: 20+ languages
- Appended to system prompt transparently
- Markdown formatting instructions always included

### 10. Keyboard-driven window movement
- Hold modifier + arrow keys for continuous movement (60fps)
- 12px per frame, smooth animation
- License-gated feature
- All four directions simultaneously possible

### 11. Audio device selection
- List input devices (microphones) and output devices (speakers)
- Select specific device for system audio capture
- Persisted across sessions
- Platform-specific device enumeration (CoreAudio/WASAPI/PulseAudio)

### 12. Dashboard with usage analytics
- Token usage charts (recharts)
- Activity history from server
- Model selection with availability indicators
- License management (activate/deactivate/validate)

---

## Architecture assessment for bluey port

### What to adopt directly
1. NSPanel overlay pattern (essential for stealth)
2. Content-protected windows
3. Platform-split audio capture (speaker/ module structure)
4. VAD engine (with ML upgrade path)
5. Event-driven streaming (Rust → frontend via events)
6. SQLite migrations in Rust
7. Global shortcut centralized handler
8. Hide-on-close dashboard pattern
9. Dynamic window height
10. App icon visibility toggle

### What to improve upon
1. **Decompose god hooks** → separate concerns into 5-6 focused hooks
2. **Type-safe events** → shared event schema between Rust and TS
3. **Proper keychain usage** → store secrets in OS keychain, not JSON
4. **Content-aware window sizing** → measure content, animate transitions
5. **ML-based VAD** → silero-vad via ONNX for better accuracy
6. **Streaming STT** → real-time transcription (Deepgram/Whisper streaming)
7. **State management** → split context by concern, add proper persistence layer
8. **Test coverage** → unit tests for hooks, integration tests for Rust commands
9. **Provider abstraction** → standardize on OpenAI-compatible format, drop cURL parsing
10. **Offline capability** → local model support (llama.cpp/whisper.cpp)

### Critical architectural decisions pluely got RIGHT
- Tauri 2 with macos-private-api (required for NSPanel)
- Rust for audio processing (performance-critical path)
- SQLite for structured data (not just localStorage)
- Event-based streaming (not polling)
- Platform-specific audio backends (not trying to use cpal for everything)
- Separate overlay and dashboard windows (different UX needs)

### Critical architectural decisions pluely got WRONG
- Managed API dependency (app is useless without their server for licensed users)
- No local inference capability
- Plain-text secret storage despite having keychain plugin
- Single massive React context
- No code splitting or lazy loading
- Zero test coverage
- Module-level mutable state in React hooks

---

*Analysis complete. 155 files, 26,899 LOC examined. Document: ~2,800 lines of analysis.*
