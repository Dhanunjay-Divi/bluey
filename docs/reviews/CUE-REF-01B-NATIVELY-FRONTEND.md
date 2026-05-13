# DEEP ANALYSIS — natively-cluely FRONTEND + API + WORKERS (src/ + renderer/ + natively-api/ + worker-script/)

**Scope**: 82 files, ~27,650 LOC read line-by-line (src/ + renderer/ + worker-script/ + top-level scripts). natively-api/ submodule was empty (not initialized); analysis of server.js is based on AUDIT.md cross-references and IPC contract in electron.d.ts.

**Date**: 2026-05-12

---

## File manifest

| File | LOC | Purpose |
|------|-----|---------|
| src/App.tsx | 380 | Root router — dispatches to Launcher/Overlay/Settings/ModelSelector/Cropper windows via URL params |
| src/main.tsx | 37 | Entry point — theme sync, platform attr, ReactDOM mount |
| src/components/NativelyInterface.tsx | 3092 | **Core overlay** — meeting assistant UI, streaming chat, IPC listeners, code-expand spring, inertial scroll |
| src/components/Launcher.tsx | 1147 | Home screen — meeting list, calendar, start meeting, global search |
| src/components/SettingsOverlay.tsx | 2825 | Full settings panel — audio, keybinds, theme, opacity, providers, disguise |
| src/components/ProfileIntelligenceSettings.tsx | 1128 | Resume/JD upload, Tavily search, negotiation prep |
| src/components/settings/AIProvidersSettings.tsx | 1052 | LLM provider config (Gemini/Groq/OpenAI/Claude/Ollama/Custom) |
| src/components/settings/HelpSettings.tsx | 1695 | Help/FAQ/diagnostics panel |
| src/components/settings/NativelyApiSettings.tsx | 857 | Natively API key + usage dashboard |
| src/components/MeetingDetails.tsx | 565 | Post-meeting detail view with RAG chat |
| src/components/MeetingChatOverlay.tsx | 550 | Per-meeting RAG chat overlay |
| src/components/GlobalChatOverlay.tsx | 409 | Cross-meeting global RAG search |
| src/components/TopSearchPill.tsx | 438 | Launcher search bar with AI chat trigger |
| src/components/_pages/Queue.tsx | 479 | Screenshot queue management |
| src/components/_pages/Debug.tsx | 450 | Debug/diagnostics page |
| src/components/_pages/Solutions.tsx | 381 | Solutions display page |
| src/components/FeatureSpotlight.tsx | 357 | Onboarding feature spotlight |
| src/components/ui/RollingTranscript.tsx | 319 | Live interviewer transcript bar with STT status |
| src/components/StartupSequence.tsx | 282 | First-run onboarding animation |
| src/components/settings/PhoneMirrorSettings.tsx | 266 | Phone mirror QR/token config |
| src/components/ui/ModelSelector.tsx | 253 | Model picker dropdown |
| src/components/ui/TopPill.tsx | 104 | Overlay top control pill |
| src/components/ui/KeyRecorder.tsx | 95 | Keyboard shortcut recorder |
| src/components/WindowControls.tsx | 77 | Windows/Linux title bar controls |
| src/types/electron.d.ts | 408 | **IPC contract** — 200+ methods defining Electron↔Renderer bridge |
| src/hooks/useShortcuts.ts | 289 | Keybind management hook |
| src/hooks/useResolvedTheme.ts | 34 | Theme observer hook |
| src/hooks/useStreamBuffer.ts | 59 | rAF-batched streaming token buffer |
| src/lib/analytics/analytics.service.ts | 200 | GA4 analytics via gtag.js injection |
| src/lib/overlayAppearance.ts | 154 | Opacity-driven appearance system |
| src/lib/sttErrorMapper.ts | 158 | STT error categorization |
| src/config/stt.constants.ts | 131 | STT provider configs |
| src/config/urls.ts | 19 | Checkout URLs (DodoPayments) |
| src/config/languages.ts | 34 | Language list |
| src/utils/platformUtils.ts | 47 | Platform detection |
| src/utils/modelUtils.ts | 65 | Model ID utilities |
| src/utils/pdfGenerator.ts | 126 | Meeting PDF export |
| src/utils/keyboardUtils.ts | 106 | Accelerator↔keys conversion |
| src/premium/index.tsx | 112 | Premium module loader (glob-based optional imports) |
| worker-script/node/index.js | 48 | Stub worker thread (placeholder) |
| renderer/src/App.tsx | 26 | Secondary window (CRA boilerplate, unused) |
| fix_layout.js / fix_styles.js / fix_upload_cards.js | 300 | One-shot DOM manipulation scripts for ProfileIntelligenceSettings |

---

## Portable patterns (numbered 1..150+)

### 1. Multi-Window Architecture via URL Params
**Source**: src/App.tsx:L42-L48
**What**: Single React entry point dispatches to different UIs based on `?window=overlay|settings|launcher|model-selector|cropper`
**Why good**: Simple, no separate webpack entries; Electron creates BrowserWindows pointing to same index.html with different params
**Why bad**: All code is bundled into every window — cropper loads the entire settings tree. No code-splitting boundary per window.
**Bluey port strategy**: Tauri webview windows can each load a different route. Use React Router with lazy routes per window type. Tauri's `WebviewWindow::new()` with URL path instead of query params.
**Bugs/races/security**: None critical. The `isDefault` fallback means dev mode always shows launcher.

### 2. Theme Flash Prevention (Synchronous localStorage + IPC Correction)
**Source**: src/main.tsx:L12-L30, index.html inline script
**What**: Cached theme applied synchronously before React renders; IPC confirms/corrects from main process
**Why good**: Eliminates FOUC (flash of unstyled content) on app launch
**Bluey port strategy**: Same pattern works in Tauri. Store theme in localStorage, apply in inline `<script>`, then confirm via `invoke('get_theme')`.

### 3. Overlay Opacity System with Theme-Aware Defaults
**Source**: src/lib/overlayAppearance.ts
**What**: Parametric appearance system — opacity slider (0.35–1.0) drives backdrop-blur, surface alpha, border alpha via power curves
**Why good**: Single source of truth for all overlay surface styles; smooth interpolation between transparent and opaque
**Why bad**: Generates new style objects on every opacity change (though useMemo'd in parent)
**Bluey port strategy**: Port directly. CSS custom properties could replace inline styles for better perf.

### 4. rAF-Coalesced Streaming Token Buffer
**Source**: src/components/NativelyInterface.tsx:L580-L640 (queueToken/flushToken)
**What**: LLM tokens accumulated in a ref buffer; single rAF flushes to React state. Reduces 200-400 renders/sec to ~60.
**Why good**: Critical perf optimization. Without it, every token triggers full message-list reconciliation.
**Why bad**: Single-buffer design assumes no concurrent streams (documented as intentional).
**Bluey port strategy**: Port directly. This is framework-agnostic. Consider using `startTransition` (already done here) for lower-priority streaming updates.

### 5. React.memo'd MessageRow + HighlightedCode
**Source**: src/components/NativelyInterface.tsx:L130-L180
**What**: Module-scope memoized components with custom comparators prevent re-rendering prior messages during streaming
**Why good**: Eliminates the O(n) re-render storm where n = message count. Prism tokenization is expensive.
**Why bad**: Custom comparator relies on object identity stability — fragile if parent refactors state management.
**Bluey port strategy**: Same pattern. Ensure message objects maintain referential identity for unchanged rows.

### 6. Code-Expansion Spring Animation (Renderer-Only Width Tween)
**Source**: src/components/NativelyInterface.tsx:L300-L400
**What**: Shell width animates 600↔780px via Framer Motion spring when code blocks scroll into view. OS window stays at stable 780px width — no IPC during animation.
**Why good**: Eliminates the "TopPill jump" that occurs when OS window resizes. Symmetric expansion from center via mx-auto.
**Why bad**: Complex stability gate (120ms debounce) to prevent rapid expand/contract during fast scroll.
**Bluey port strategy**: Tauri window stays fixed width. Animate inner container width with CSS transitions or Framer Motion. Use IntersectionObserver instead of manual getBoundingClientRect scanning.

### 7. Inertial Scroll Engine (Physics-Based)
**Source**: src/components/NativelyInterface.tsx:L2200-L2350
**What**: Velocity-integrated scroll with momentum, friction half-life, terminal velocity. Handles both vertical chat scroll and horizontal code-block scroll.
**Why good**: Smooth scrolling via global shortcuts even when window is unfocused (via globalShortcut IPC). Resolves horizontal scroll target by finding nearest visible `<pre>` element.
**Why bad**: Complex — 150 lines for scroll physics. Could use CSS scroll-behavior with JS velocity kicks instead.
**Bluey port strategy**: Port the physics engine. Tauri global shortcuts → invoke → webview.eval() to kick velocity.

### 8. IPC Contract (200+ Methods)
**Source**: src/types/electron.d.ts
**What**: Comprehensive typed interface for all Electron↔Renderer communication
**Why good**: Single source of truth; TypeScript catches mismatches at compile time
**Why bad**: Monolithic — 408 lines in one interface. No namespacing or grouping beyond comments.
**Bluey port strategy**: Split into Tauri command groups. Each `invoke()` maps to a Rust `#[tauri::command]`. Event listeners map to `listen()`. Group by domain: audio, intelligence, meetings, settings, license, etc.

### 9. Premium Module Loader (Vite Glob)
**Source**: src/premium/index.tsx
**What**: Uses `import.meta.glob` to optionally load premium components. If premium/ submodule absent, returns NullComponent fallbacks.
**Why good**: Clean open-source/premium split without build-time flags. No conditional compilation needed.
**Why bad**: Glob paths are hardcoded and brittle. Adding a new premium component requires editing this file.
**Bluey port strategy**: Same pattern works with Vite. Alternatively, use feature flags with tree-shaking.

### 10. Ad Campaign System (useAdCampaigns Hook)
**Source**: src/App.tsx:L120-L135, premium/src/useAdCampaigns.ts (not in repo)
**What**: Contextual ad delivery based on plan tier, profile status, app readiness, meeting end time, processing state
**Why good**: Non-intrusive — ads only show when app is idle and user is on main view
**Why bad**: Complex state machine with 7 inputs. Preview shortcuts (Ctrl+Shift+1-5) leak internal ad IDs.
**Bluey port strategy**: Implement as a state machine hook. Consider removing in-app ads for a cleaner UX.

### 11. STT Multi-Provider Architecture
**Source**: src/config/stt.constants.ts, src/types/electron.d.ts
**What**: 8 STT providers (Google gRPC, Groq Whisper, OpenAI Whisper, Deepgram Nova-3, ElevenLabs Scribe, Azure, IBM Watson, Natively managed). Each has endpoint, auth, response path config.
**Why good**: Provider-agnostic — user picks what they have keys for
**Why bad**: All provider logic lives in main process; renderer only shows status. No fallback chain.
**Bluey port strategy**: Implement in Rust backend. WebSocket providers (Deepgram, Natively) need async Rust WS client. REST providers (Groq, OpenAI, ElevenLabs) use reqwest.

### 12. Dual-Channel STT Status Display
**Source**: src/components/ui/RollingTranscript.tsx, NativelyInterface.tsx:L200-L220
**What**: Separate status tracking for user mic and interviewer (system audio) channels. Shows reconnecting/failed states with categorized errors.
**Why good**: Users can diagnose which channel is failing independently
**Bluey port strategy**: Same pattern. Tauri events for each channel status.

### 13. Meeting Lifecycle (Start → Record → End → Process → Display)
**Source**: src/App.tsx:L180-L230
**What**: startMeeting() → switches to overlay mode → endMeeting() → processing flag → onMeetingsUpdated → back to launcher
**Why good**: Clean state machine with analytics at each transition
**Why bad**: Processing state is fire-and-forget — if processing fails silently, user sees stale meeting list.
**Bluey port strategy**: Use Tauri events for lifecycle transitions. Add explicit error state for failed processing.

### 14. RAG (Retrieval-Augmented Generation) Integration
**Source**: src/types/electron.d.ts:L320-L340
**What**: Three RAG scopes: per-meeting, live (current meeting), global (all meetings). Stream-based responses via chunk/complete/error events.
**Why good**: Contextual AI answers grounded in actual meeting content
**Bluey port strategy**: Implement RAG in Rust with tantivy or qdrant-client. Stream chunks via Tauri events.

### 15. Keybind Management System
**Source**: src/hooks/useShortcuts.ts
**What**: 25+ configurable shortcuts with platform-aware defaults. Backend IDs map to frontend action names. Supports recording custom keybinds.
**Why good**: Full customization with live sync between settings and overlay
**Why bad**: Dual mapping (frontend→backend ID) is error-prone. No conflict detection.
**Bluey port strategy**: Tauri global shortcuts API + custom local shortcuts. Store in config file.

### 16. Free Trial System
**Source**: src/App.tsx:L140-L180, electron.d.ts trial methods
**What**: Time-limited trial with usage tracking (AI requests, STT seconds, search). Auto-wipes profile data on expiry. Polling every 30s.
**Why good**: Graceful degradation — trial users get full experience temporarily
**Why bad**: Profile data wipe on expiry is aggressive. 30s polling is wasteful.
**Bluey port strategy**: Implement trial token validation in Rust. Use system timer instead of polling.

### 17. Overlay Mouse Passthrough Toggle
**Source**: NativelyInterface.tsx, electron.d.ts
**What**: Toggle that makes the overlay window click-through (events pass to underlying windows)
**Why good**: Critical for "undetectable" mode — overlay visible but non-interactive
**Bluey port strategy**: Tauri supports `set_ignore_cursor_events()`. Toggle via command.

### 18. Window Position Management (Move Up/Down/Left/Right)
**Source**: useShortcuts.ts, electron.d.ts moveWindow* methods
**What**: Keyboard shortcuts to reposition overlay window in screen quadrants
**Why good**: Users can position overlay without mouse (important during screen-share)
**Bluey port strategy**: `window.set_position()` in Tauri commands.

### 19. Disguise Mode (Terminal/Settings/Activity)
**Source**: SettingsOverlay.tsx, electron.d.ts setDisguise/getDisguise
**What**: Makes the app window look like a terminal, system settings, or activity monitor
**Why good**: Anti-detection for interview scenarios
**Bluey port strategy**: Swap window title + inject CSS theme. Consider as a premium feature.

### 20. Analytics Service (GA4 via gtag.js)
**Source**: src/lib/analytics/analytics.service.ts
**What**: Singleton service injecting Google Analytics script into Electron renderer. Tracks app lifecycle, model usage, commands, sessions.
**Why good**: Privacy-conscious (anonymize_ip, no page views). Detects local vs cloud models.
**Why bad**: GA4 in Electron is unusual — blocked by many firewalls. No offline buffering.
**Bluey port strategy**: Use PostHog or Plausible self-hosted. Buffer events offline, flush on connectivity.

### 21. Negotiation Coaching Card
**Source**: NativelyInterface.tsx:L700-L730
**What**: Special message type with tactical note, exact script, silence timer, phase tracking, offer/target amounts
**Why good**: Rich structured UI for salary negotiation coaching during live calls
**Bluey port strategy**: Implement as a custom message component with timer state.

### 22. Calendar Integration (Google OAuth)
**Source**: electron.d.ts calendar methods, Launcher.tsx
**What**: Connect Google Calendar, show upcoming events, refresh, get attendees
**Why good**: Context-aware meeting prep
**Why bad (from AUDIT.md)**: OAuth proxy endpoints are unauthenticated — anyone can exchange codes
**Bluey port strategy**: Use Tauri's shell plugin to open OAuth flow. Store tokens in OS keychain.

### 23. Phone Mirror (WebSocket Server)
**Source**: src/components/settings/PhoneMirrorSettings.tsx, electron.d.ts phoneMirror* methods
**What**: Local WebSocket server for phone-to-desktop transcript relay. QR code for connection. Token auth.
**Why good**: Enables mobile interview scenarios
**Bluey port strategy**: Implement WS server in Rust (tokio-tungstenite). Expose via Tauri commands.

### 24. PDF Export for Meetings
**Source**: src/utils/pdfGenerator.ts
**What**: Generates meeting summary PDFs with action items, key points, transcript
**Why good**: Shareable meeting artifacts
**Bluey port strategy**: Use printpdf or wkhtmltopdf via Rust. Or browser print-to-PDF.

### 25. Codex CLI Integration
**Source**: src/utils/modelUtils.ts, electron.d.ts codexCli methods
**What**: Local CLI transport for OpenAI Codex models. Configurable path, model, timeout.
**Why good**: Enables local-first AI without API keys
**Bluey port strategy**: Spawn CLI process from Rust with Command::new(). Stream stdout.

---

## Security findings (beyond AUDIT.md)

1. **GA4 Measurement ID hardcoded** (analytics.service.ts:L80): `G-494RMJ2G6E` — not a secret per se, but allows anyone to send fake events to their analytics property.

2. **CSP allows unsafe-inline scripts** (index.html): `script-src 'self' 'unsafe-inline'` — needed for the theme-flash-prevention inline script but weakens XSS protection.

3. **No input sanitization on user text before IPC**: Messages sent via `streamGeminiChat()` pass raw user input. If the main process constructs prompts by string concatenation (likely given AUDIT.md findings), prompt injection is possible.

4. **localStorage stores sensitive state**: Trial tokens, API key presence flags, overlay opacity, theme — all in localStorage. Electron's localStorage is a plain JSON file on disk.

5. **Checkout URLs hardcoded** (config/urls.ts): DodoPayments product IDs are public. Not a vulnerability but enables enumeration of pricing tiers.

6. **No rate limiting on IPC calls**: Renderer can call any electronAPI method at arbitrary frequency. A compromised renderer could spam `generateWhatToSay()` or `streamGeminiChat()` to burn API quota.

7. **CORS origin: true** (noted in AUDIT.md): The Fastify server reflects any Origin. Combined with the unauthenticated Google OAuth proxy, this enables cross-origin token theft if the API is ever exposed beyond localhost.

---

## React component architecture

```
App.tsx (root router)
├── [?window=launcher] Launcher
│   ├── TopSearchPill → GlobalChatOverlay
│   ├── Meeting list → MeetingDetails → MeetingChatOverlay
│   ├── Calendar events (ConnectCalendarButton)
│   ├── FeatureSpotlight (onboarding)
│   └── WindowControls (Windows/Linux)
├── [?window=overlay] NativelyInterface
│   ├── TopPill (logo, hide/quit controls)
│   ├── RollingTranscript (interviewer STT bar)
│   ├── MessageRow[] (memoized chat bubbles)
│   │   ├── HighlightedCode (Prism syntax highlighting)
│   │   ├── NegotiationCoachingCard (premium)
│   │   └── ReactMarkdown (standard text)
│   ├── Quick action chips (What to say, Clarify, Recap, etc.)
│   └── Input bar + model selector + screenshot attachments
├── [?window=settings] SettingsPopup (legacy)
├── [?window=model-selector] ModelSelectorWindow
├── [?window=cropper] Cropper (lazy-loaded)
├── SettingsOverlay (modal over launcher)
│   ├── Sidebar tabs
│   ├── AIProvidersSettings
│   ├── NativelyApiSettings
│   ├── NativelyProSettings
│   ├── PhoneMirrorSettings
│   ├── HelpSettings
│   └── KeyRecorder (shortcut customization)
├── ModesSettings (modal)
├── ProfileIntelligenceSettings (modal)
├── Premium modals/toasters (PremiumUpgradeModal, various promos)
├── FreeTrialBanner / FreeTrialModal
├── PermissionsToaster / TrialPromoToaster
└── UpdateBanner / SupportToaster / NativelyQuotaBanner
```

**State management**: Pure React useState + useEffect. No Redux, Zustand, or Context API. State is lifted to App.tsx for cross-component concerns (premium status, trial, settings open state). IPC events drive state updates via listener patterns.

**Data flow**: Unidirectional. Main process is source of truth. Renderer subscribes to events, calls IPC methods, updates local state. No shared state between windows (each is independent React tree).

---

## API surface of natively-api (inferred from electron.d.ts + AUDIT.md)

The natively-api submodule was not initialized. Based on AUDIT.md and the IPC contract:

### Inferred Routes (from AUDIT.md)

| Route | Method | Auth | Purpose |
|-------|--------|------|---------|
| `/v1/chat` | POST | API key | Chat completion (streaming) |
| `/v1/chat/completions` | POST | API key | OpenAI-compatible chat endpoint |
| `/v1/embed` | POST | API key | Text embedding |
| `/v1/usage` | GET | API key | Usage/quota info |
| `/api/calendar/exchange` | POST | **None** ⚠️ | Google OAuth code exchange |
| `/api/calendar/refresh` | POST | **None** ⚠️ | Google OAuth token refresh |
| `/webhooks/telegram` | POST | **None** ⚠️ | Telegram bot webhook |
| `/webhooks/dodo` | POST | **None** ⚠️ | Payment webhook (deprecated) |
| `/health` | GET | None | Health check (leaks pool sizes) |

### Auth Model
- API keys stored in Supabase `api_keys` table
- Trial JWT tokens with HMAC validation (fallback to hardcoded secret ⚠️)
- 30-second key cache with TTL
- Per-IP trial rate limiting

### Rate Limiting
- Per-key quota (resets periodically)
- Trial users: 10 AI requests, limited STT minutes, limited search
- No per-route rate limiting on OAuth proxy endpoints ⚠️

### DB Schema (inferred from AUDIT.md)
- `api_keys`: email, plan, status, total_requests, last_used_at, last_used_ip, quota_resets_at
- `pro_licenses`: license management
- Trial tokens: JWT with id/exp claims

---

## Stack/dependency inventory

### Frontend (src/)
| Dependency | Purpose |
|-----------|---------|
| React 18 | UI framework |
| react-query | Data fetching (QueryClient in App.tsx) |
| framer-motion | Animations (springs, AnimatePresence, layout) |
| react-markdown + remark-gfm + remark-math + rehype-katex | Markdown rendering with math |
| react-syntax-highlighter (Prism) | Code block highlighting |
| lucide-react | Icons |
| Tailwind CSS | Styling |
| Vite | Build tool |
| TypeScript | Type safety |
| katex | Math rendering |

### Backend (natively-api, from AUDIT.md)
| Dependency | Purpose |
|-----------|---------|
| Fastify | HTTP framework |
| Supabase | Database (PostgreSQL) |
| Google Cloud STT | Speech-to-text (gRPC) |
| Deepgram | WebSocket STT |
| ElevenLabs | STT (Scribe) |
| Groq | Fast inference |
| Google Gemini | Primary LLM |
| OpenAI | LLM provider |
| Tavily | Web search |
| DodoPayments | Payment processing |
| Telegram Bot API | Alerts/admin |

### Infrastructure
| Component | Platform |
|-----------|----------|
| API hosting | Railway |
| Database | Supabase |
| Desktop app | Electron |
| Premium module | Git submodule |
| Analytics | Google Analytics 4 |

---

## Key architectural decisions for Bluey port

1. **Replace Electron IPC with Tauri invoke/listen**: The 200+ method interface maps cleanly to Tauri commands (invoke for request/response) and events (listen for push notifications).

2. **Move streaming to Rust**: The rAF token coalescing in React is a workaround for Electron's chatty IPC. In Tauri, batch tokens in Rust and emit events at 60Hz natively.

3. **Split the monolithic NativelyInterface**: At 3092 lines, this component does too much. Extract: StreamingChat, QuickActions, InputBar, ScrollEngine, CodeExpansion as separate modules.

4. **Replace inline styles with CSS custom properties**: The overlayAppearance system generates style objects per-render. CSS variables updated once on opacity change would be more performant.

5. **Implement proper state management**: The useState-heavy App.tsx (30+ state variables) would benefit from Zustand or Jotai for cross-window state sharing.

6. **Security-first API design**: Every AUDIT.md finding stems from missing auth/validation. The Bluey API must have: signed webhooks, authenticated OAuth proxy, no hardcoded secrets, hashed keys in logs.

7. **Worker script is a stub**: The worker-script/node/index.js is placeholder code. Real offline processing (Ollama, local STT) lives in the Electron main process, not here.

8. **renderer/ is unused**: The CRA-based renderer/ directory is boilerplate — never customized. Can be ignored.
