# Cue/Bluey Reference-Repo Analysis (IN PROGRESS)

**Status**: Phase 1, Repo 1 of 6 (natively-cluely, 204K LOC). Writing as I read.
**User directive**: Read every zip line-by-line, largest first. Output feeds a `CUE-PORT-PLAN.md` for codex.

---

## Patterns / features to port to cue/bluey (running list from natively-cluely)

### Stealth + Masquerading (CRITICAL)

1. **Content protection on every window** — `setContentProtection(state)` on launcher, overlay, settings, modelSelector, cropper (main.ts:3237-3241)
2. **7-step process masquerading** (main.ts _applyDisguise:3390-3540):
   - `process.title = "Terminal "` — Activity Monitor / Task Manager display
   - `app.setName("Terminal ")` — macOS Menu + Dock text (SKIP if undetectable, causes re-registration)
   - `process.env.CFBundleName = "Terminal"` — mac CFBundleName identity
   - `app.setAppUserModelId("com.natively.assistant.terminal")` — Windows Taskbar grouping (unique per disguise!)
   - `nativeImage.createFromPath(iconPath)` + `app.dock.setIcon(image)` — fake icons
   - `window.setIcon(image)` — Windows/Linux per-window icon
   - `window.setTitle("Terminal")` — window title text
3. **Pre-built fake icon set** in `assets/fakeicon/{mac,win}/{terminal,settings,activity}.png` — 6 PNG files to ship
4. **Re-assertion ticker**: `setTimeout` at 200ms + 1s + 5s after disguise applied to re-set `process.title` (drifts on some systems). Track timers in `_disguiseTimers[]`, cancel on next disguise change.
5. **NEVER repeat `app.setName()` in re-assertion** — repeated calls on macOS show "second dock tile" during re-registration
6. **Dock toggle debounced 150ms** — user rapid-toggles race with dock.show()+NSApp.activate() (main.ts:3266-3320)
7. **Focus preservation**: capture `nativelyWasFocused` BEFORE `dock.hide()` (dock.hide triggers macOS app-deactivation which leaks focus to next app e.g. Chrome). If was focused, restore with `window.focus()` NOT `app.focus()` (latter has `[NSApp activateIgnoringOtherApps:YES]` side-effect)
8. **`setIgnoreBlur(true)` on modal windows during stealth transitions** — Settings + ModelSelector — prevents self-hide, restore after 500ms
9. **Cancel `_disguiseTimers` when entering undetectable** — prevents stale `app.setName()` calls from re-registering dock icon (main.ts:3244-3251)

### Multi-provider STT (CRITICAL)

10. **9 providers**: natively, deepgram, soniox, elevenlabs, openai, groq, azure, ibmwatson, google (default); plus `'none'` to disable. User-selectable per session.
11. **Per-speaker instance**: separate STT objects for `'interviewer'` (system audio) vs `'user'` (mic). NativelyProSTT uses channel suffix `${key}:system` vs `${key}:mic` on session key to avoid `concurrent_session_blocked` when both streams active.
12. **Always-fallback to GoogleSTT** if API key missing for selected provider (graceful degrade, log warning)
13. **3-state machine**: connected / reconnecting / failed. `stt-status` IPC channel broadcasts transitions. Renderer banner reflects state.
14. **Classified error handling**: auth (401/invalid_key/auth_timeout) → fatal; quota → fatal; retryable (net/5xx/400/429/WS drop) → reconnect with consecutive-error counter (threshold 5).
15. **Axios error enrichment**: pull `response.data.error` message, concatenate with HTTP status for log clarity
16. **gRPC code 11 suppression** — Google's 10s silence timeout downgrade to one-line warn, reconnect on next chunk
17. **Reset counter on first successful final transcript** → emit `connected` state transition
18. **Persistent-reconnect signal** — NativelyProSTT retries forever with 30s backoff cap, emits `persistent-reconnect` event after 5 attempts so UI can show "check network" banner
19. **`languageDetected` auto-emit** — STT provider detects language from first audio batch, renderer shows detected BCP47 in Settings
20. **OpenAI Realtime WS + REST fallback** — Whisper-1 REST when WS path unavailable (custom endpoint etc.)
21. **REST STT via shared class** — RestSTT class takes provider name + key + model + region, serves groq/azure/ibmwatson

### Audio pipeline (native Rust module)

22. **Zero-copy via `napi::Buffer` (Uint8Array)** — bypass V8 GC on continuous audio capture (CHANGELOG v2.0.4)
23. **Two-stage VAD**: adaptive RMS + WebRTC ML VAD before billing STT. Rejects typing, fan noise, non-speech (CHANGELOG v2.0.4)
24. **CPAL audio backend** (Rust)
25. **Hardware sample-rate detection**: fix was hardcoding 16kHz while streaming 48kHz. Track `_sysSttRateApplied` / `_micSttRateApplied` in AppState, re-apply on reconfigureAudio
26. **CPAL stream recreation on mic restart** fixes "Input missing" silent crash
27. **Consolidated wireSystemCapture / wireMicCapture helper** — was duplicated 3x with different chunk counters and log prefixes. `label` param only affects logging. `setupAudioRecoveryHandler` called inside helper → every path gets recovery for free.

### LLM orchestration

28. **7-provider LLM router**: OpenAI / Anthropic / Gemini / Groq / Ollama / custom-cURL / Natively Pro
29. **Provider priority chain** for structured generation: OpenAI → Claude → Gemini Pro → Gemini Flash → Groq → Ollama
30. **Smart vision fallback**: Groq Llama 4 Scout when default vision models rate-limited
31. **Per-LLM capability map** (`modelCapabilities.ts`) — knows which models do vision, streaming, structured output
32. **Model dropdown dynamic** — provider `/models` endpoint sync, shows user's available models
33. **API keys auto-save** after 5s keystroke idle in Settings
34. **`currentModelId` honored in every provider call** — do NOT hardcode `OPENAI_MODEL` / `CLAUDE_MODEL` constants in generate/stream functions (FIXES.md #96)
35. **Connection-test with stable pingable model** — `gpt-4o-mini` not user's selected model (just validates key)
36. **Anti-chatbot prompt constraints** — negative prompts against "AI-like lectures", "robot preambles", over-explanation (prompts.ts, CHANGELOG v1.1.4)
37. **Triple-layer strict language injection** — native languages prioritized over default English via 3 injection points (CHANGELOG v2.0.7)

### RAG + Memory

38. **Local RAG** via SQLite + sqlite-vec extension (offline-capable)
39. **Sliding-window chunking** in `SemanticChunker.ts` — 50-token overlap prevents conversational context loss across chunk boundaries
40. **Live RAG indexing (JIT)** — final transcripts fed to `ragManager.feedLiveTranscript()` as they arrive, searchable immediately
41. **4 embedding providers**: OpenAI, Gemini, Ollama, local model — via `IEmbeddingProvider` interface + resolver
42. **Epoch summarization** for long transcripts instead of hard truncation — preserves early-meeting context
43. **Vector search worker thread** — `vectorSearchWorker.ts` keeps main thread responsive
44. **Rolling context memory window** (README) — conversation context for smarter answers across turns

### UX / shortcuts

45. **`Cmd+Shift+Enter` single-trigger capture** — screenshot + AI analysis in one hotkey (FIXES.md #90)
46. **`Cmd+K / Ctrl+K` global spotlight** for chat overlay (CHANGELOG v1.1.5)
47. **User-rebindable keybinds** via Settings → Shortcuts; validated via `shouldRegister()` per app mode (launcher/overlay)
48. **`shouldRegister()` allowlist must include global shortcuts in launcher mode** — common silent-fail (FIXES.md #133)
49. **Mouse passthrough toggle** on overlay window — click-through mode for unobtrusive floating
50. **Screenshot → cropper window** — `CropperWindowHelper` for selective screenshots (`Cmd+Shift+H`)
51. **`setOpacity(0)` before `hide()`** on macOS/Linux to eliminate fade-animation flash (FIXES.md #89)
52. **`setOpacity(1)` restore before every `show()`** — windows were coming back invisible (FIXES.md #134)
53. **`requestAnimationFrame + useRef`** (not `setTimeout(0)`) for React 18 concurrent-mode timing-critical paths (FIXES.md #135)
54. **IPC receive-only binding pattern** for Settings/Overlay state sync (CHANGELOG v2.0.5)

### Premium features (conditional modules)

55. **Try/catch require of premium modules** — premium/electron/knowledge/KnowledgeOrchestrator + KnowledgeDatabaseManager. Open-source ships without, degrades to core features. Pattern: `let X = null; try { X = require('../premium/...').X; } catch {}`
56. **Profile Intelligence**: JD + Resume context-aware AI (CHANGELOG v2.0.1)
57. **Company Dossier UI** — interview difficulty badges, 5-star work culture grid w/ sub-dimensions, employee reviews w/ sentiment analysis, critics/complaints tracking, core benefits pills (CHANGELOG v2.0.7)
58. **Negotiation Tracker** — `knowledgeOrchestrator.feedInterviewerUtterance(text)` on final interviewer-speaker transcripts
59. **Personas** — switch AI role (Tech / Sales / HR) with tailored prompt sets + reference PDFs

### Security + ops

60. **API key scrubbing on app quit** — overwrite key memory before disposal (CHANGELOG v1.1.7, `CredentialsManager` 582 LOC)
61. **Hash before logging** — never log raw API keys, use truncated prefix (`sk.slice(0,16) + '...'`)
62. **`process.stdout/.on('error', ()=>{})`** — prevents EIO crash when Electron terminal is detached (main.ts:10-11)
63. **uncaughtException + unhandledRejection** both logged to file (some products miss the second)
64. **Lazy `app.getPath()`** — do NOT call at module-load time; returns null before `app.whenReady()`, resolve on first use
65. **Log rotation at 10MB** — `logToFile` checks size, renames to `.log.1` on overflow, single-generation rollover
66. **Mac mic permission helper** wraps `systemPreferences.askForMediaAccess('microphone')` with current-status check first
67. **Mac screen-capture permission has no askForMediaAccess API** — only prompts on first protected call (SCK/CoreAudio tap). If 'denied', must re-enable in System Settings manually.
68. **Dev-mode mac screen permission bypass** — TCC falsely reports 'denied' for unpackaged electron binary
69. **Fire-and-forget install ping** — `sendAnonymousInstallPing()` on app ready (anonymous telemetry)
70. **`requestSingleInstanceLock()` + `second-instance` handler** — prevent double-launch, focus existing window on relaunch attempt
71. **`app.commandLine.appendSwitch("disable-background-timer-throttling")`** — keep audio pipeline timers accurate when window blurred
72. **Token bucket `RateLimiter` service** — configurable burst + refill rates for free-tier APIs (CHANGELOG v1.1.7)

### Config + persistence

73. **`SettingsManager` singleton** — `get('isUndetectable') ?? false` pattern with defaults
74. **`CredentialsManager` singleton** — 582 LOC, wraps all API-key storage, isolates keystore impl (keychain, encrypted file, etc.)
75. **Boot-critical settings loaded before windows** — isUndetectable + disguiseMode + verboseLogging read first, so windows init with correct state

### Build / release

76. **Auto-updater wrapped in `setupAutoUpdater()` + manual `checkForUpdatesManual()`** — user can force check in addition to auto
77. **Version comparison** — custom `isVersionNewer()` vs semver parsing (avoids semver dep)
78. **Release notes fetched/cached** via `ReleaseNotesManager`
79. **Ad-hoc signing entitlements fix** — V8/Electron crash on Intel Macs without `entitlements.mac.plist` during ad-hoc signing (CHANGELOG v1.1.7)
80. **Helper process renaming** for Activity Monitor stealth — part of build script, not runtime

### Codex/agent meta

81. **`.codex/agents/` directory** with 7 specialized codex agent configs: code-reviewer, backend-architect, test-engineer, frontend-developer, debugger, ui-ux-designer, fullstack-developer. Pattern we should copy for bluey's codex setup.
82. **`.agents/skills/mobile-design/*.md`** — 10 reusable skill cards (design-thinking, typography, performance, touch-psychology, decision-trees, debugging, platform-android/ios, backend, testing). Pattern: encode design knowledge as MD cards for AI agent retrieval.
83. **`.agent/rules/claude-mem-context.md`** — Claude memory-persistence rule

---

## Documentation / meta patterns

84. **Structured bug-doc template (FIXES.md)** — every issue gets 6 sections: Root Cause / Fix Summary / Files Modified / Edge Cases Handled / How to Test / Known Limitations. Use this for bluey.
85. **CHANGELOG format** — per-version: What's New / Improvements / Fixes / Technical. Very scannable.
86. **AUDIT.md self-security-audit** — owner runs their own security review: Critical / High / Medium / Low / Dead Code / Notes. Mirrors the Pinky review style I used.

---

## natively-cluely full file coverage (so far)

- [x] Top-level: README.md, AUDIT.md, CHANGELOG.md, FIXES.md (full read)
- [ ] changes.md (76KB), diff_settings.txt (268KB) — probably dev-notes / git artifacts, low value
- [x] main.ts partial (stealth + masquerade + STT provider creation + init flow; function signature index of all 100+ methods)
- [ ] main.ts remaining (wireSystemCapture 1136, wireMicCapture 1289, setupSystemAudioPipeline 1394, startMeeting 2313, endMeeting 2442, setupIntelligenceEvents 2571, IPC handlers 3624)
- [ ] LLMHelper.ts (3894 lines) — LLM routing core
- [ ] ipcHandlers.ts (3411 lines)
- [ ] electron/llm/prompts.ts (2140 lines) — system prompts
- [ ] electron/llm/* — 20 specialized LLMs (AnswerLLM, AssistLLM, BrainstormLLM, ClarifyLLM, CodeHintLLM, FollowUpLLM, RecapLLM, WhatToAnswerLLM, etc.)
- [ ] electron/audio/* — 10 STT providers + capture
- [ ] electron/rag/* — 15 files including SemanticChunker, VectorStore, EmbeddingPipeline, vectorSearchWorker
- [ ] electron/db/DatabaseManager.ts (1470)
- [ ] electron/services/* — 15 managers (Credentials, Keybind, RateLimiter, ModelVersion, Modes, Calendar, PhoneMirror, CodexCli, Ollama, InstallPing, Settings)
- [ ] electron/WindowHelper.ts + SettingsWindowHelper + ModelSelectorWindowHelper + CropperWindowHelper
- [ ] electron/ScreenshotHelper.ts (811)
- [ ] native-module/src/*.rs — Rust audio
- [ ] src/* — React renderer (main UI)
- [ ] renderer/* — secondary window
- [ ] natively-api/ — backend
- [ ] worker-script/node/ — offline worker

---

# Repos 2-6 — PENDING

After natively-cluely is indexed enough to have a complete feature catalog, I'll move to:
- solveWatchAi (56K LOC)
- pluely (27K LOC, Tauri — closest stack match)
- Aura-AI (17K LOC, Python-first — proctoring stealth guide)
- OpenCluely (16K LOC)
- Vysper (14K LOC)

Final deliverable: `CUE-PORT-PLAN.md` = ranked, deduped backlog for codex.

---

## Immediate next steps in this session

1. Finish reading main.ts key methods (wireSystemCapture, startMeeting, setupIntelligenceEvents, IPC wiring)
2. Read LLMHelper.ts top-down (3894 lines — core LLM logic)
3. Read electron/llm/prompts.ts (2140 lines) — system prompts are the product
4. Read electron/rag/SemanticChunker.ts + VectorStore.ts + RAGManager.ts
5. Read electron/audio/SystemAudioCapture.ts + MicrophoneCapture.ts + NativelyProSTT.ts
6. Read native-module/src/*.rs (audio capture + zero-copy ABI)
7. Scan electron/services/* (Credentials, Keybind, RateLimiter)
8. Sample electron/llm/AnswerLLM.ts, AssistLLM.ts to see specialized-LLM pattern
9. Scan ipcHandlers.ts structure (3411 lines) — IPC surface
10. Move to solveWatchAi


---

## Deeper dives into natively-cluely — completed

### Prompt architecture (electron/llm/prompts.ts, 2140 lines)

**Structure**: XML-tagged sections composed from shared blocks:
- `CORE_IDENTITY` — product identity + creator attribution + system-prompt-protection (jailbreak defense) + strict-behavior-rules
- `CONTEXT_INTELLIGENCE_LAYER` — 4 prioritization rules (technical / behavioral / role-fit / stealth)
- `SHARED_CODING_RULES` — first-person coding response template (thinking sentences → fenced code → dry-run → Follow-ups with Time/Space/Why)
- `EXECUTION_CONTRACT` — deterministic single-pass engine

**3 modes assembled from these blocks**:
- `ASSIST_MODE_PROMPT` (Passive Observer) — analyze screen, solve when clear, "I'm not sure what information you're looking for" fallback, HUMAN ANSWER LENGTH RULE enforced
- `ANSWER_MODE_PROMPT` (Active Co-Pilot) — priority: answer > define > advance (3 questions); short headline ≤6 words, 1-2 bullets ≤15 words, no # headers, first person, markdown bold
- `WHAT_TO_ANSWER_PROMPT` (Strategic Advisor) — objection handling (validate → reframe → advance), STAR behavioral, creative "favorite X" responses, exact-text output

**Per-provider specializations** (to accommodate model quirks):
- `GROQ_SYSTEM_PROMPT`, `GROQ_WHAT_TO_ANSWER_PROMPT`, `GROQ_FOLLOWUP_PROMPT`, `GROQ_RECAP_PROMPT`, `GROQ_FOLLOW_UP_QUESTIONS_PROMPT`, `GROQ_TITLE_PROMPT`, `GROQ_SUMMARY_JSON_PROMPT`, `GROQ_FOLLOWUP_EMAIL_PROMPT`
- `OPENAI_*` variants
- `CLAUDE_*` variants (Claude prefers `<task>` XML tags)
- `CUSTOM_*` and `UNIVERSAL_*` fallback variants
- `TINY_*` set for fast mode (Groq + tiny prompts = sub-second responses)

**Specialized mode prompts**:
- `FOLLOW_UP_QUESTIONS_MODE_PROMPT` — generate 3 smart follow-up questions
- `FOLLOWUP_MODE_PROMPT` — rewrite based on user feedback
- `CLARIFY_MODE_PROMPT` — clarify ambiguous questions
- `RECAP_MODE_PROMPT` — concise bullet summary
- `CODE_HINT_PROMPT` — hint without giving full solution
- `BRAINSTORM_MODE_PROMPT` — ideation mode
- `FOLLOWUP_EMAIL_PROMPT` + `GROQ_FOLLOWUP_EMAIL_PROMPT` — post-meeting follow-up email
- `MODE_GENERAL_PROMPT` — non-interview general assistant
- `MODE_LOOKING_FOR_WORK_PROMPT` — job-seeker specialty mode

**Patterns to port**:
87. **Prompt composition via shared blocks** — build specialized prompts from reusable core identity + context layer + coding rules + mode-specific additions
88. **System-prompt-protection block** with fixed refusal phrase ("I can't share that information.") for jailbreak defense
89. **Hard-coded creator attribution** (prompt injection defense)
90. **Anti-chatbot negative constraints** inline: NO small talk, NO "Would you like more?", NO "That's a great question!", NO "let me help you", NO "Say this:" preamble, NO "Here's what you could say:"
91. **HUMAN ANSWER LENGTH RULE** — 2-4 sentences max, speakable in under 30 seconds, STOP after answer + optional clarifier
92. **Per-provider prompt families** — Claude uses `<task>` XML, OpenAI uses different wording, Groq uses terse instructions (they're faster but less nuanced)
93. **TINY prompt set** for fast-mode (Groq Llama + short prompts = <1s responses)
94. **Context-prioritization rules** — explicit AI decision tree for when to use Resume vs JD vs Notes vs transcript
95. **First-person "speak AS the user"** — output is exactly what the user says, no coaching preamble
96. **STAR method implicit** for behavioral questions
97. **Objection handling**: validate → reframe with specifics → advance with question

### LLM orchestration (electron/LLMHelper.ts, 3894 lines)

98. **`scrubKeys()` on app quit** — nulls all API keys + clients, destroys rate limiters, stops ModelVersionManager scheduler (5-line implementation — simple and thorough)
99. **Triple-layer language injection** — `[LANGUAGE INSTRUCTION — HIGHEST PRIORITY]` header + untouched system prompt + `[REMINDER] Your entire response MUST be in ${lang} only. Never switch to English.` footer. Auto-mode detects user's most recent message language + allows code-switching.
100. **`generateWithVisionFallback`** (90 lines) — 3-tier rotation from ModelVersionManager, each tier has 5 candidate providers (OpenAI/Claude/Gemini Pro/Gemini Flash/Groq), null-checked per client, multimodal/text-only dispatch
101. **`streamWithGeminiParallelRace`** — Promise.any(flashCollect, proCollect), yields winning response in 10-char chunks to simulate streaming. Flash usually wins on speed; Pro wins if Flash rate-limited
102. **`ModelVersionManager`** (1209 LOC) — self-updating model IDs via periodic API poll. Avoids hard-coded model names going stale. `getAllVisionTiers()` returns 3-tier rotation
103. **`createProviderRateLimiters`** — per-provider `RateLimiter` (token bucket) prevents 429s on free tiers
104. **`createRobustClient(realClient)`** — wraps Gemini client with retry + fallback logic
105. **`fitContextForCurrentModel(text, reservedOutputTokens)`** — truncate to fit model's context window, leaving budget for output
106. **`fitTranscriptForCurrentModel(turns)`** — preserve recent turns when truncating
107. **`processImage(path)`** — sharp-based format conversion + resize before sending to vision API
108. **`cleanJsonResponse(text)`** — strip markdown fences (` ```json ... ``` `) from LLM JSON responses
109. **`parseStreamLine(line)`** — SSE/NDJSON line parser for streaming responses
110. **`generateContentStructured(message)`** — 6-provider priority chain for structured JSON extraction (resume parsing etc.): OpenAI → Claude → Gemini Pro → Gemini Flash → Groq → Ollama. Each block is independent — no `this.geminiModel` mutation to avoid races.
111. **`testConnection()`** — validate key with stable pingable model (`gpt-4o-mini` not user's selected model)
112. **`executeCustomProvider(provider, userContent, ...)`** — curl-template-based custom provider with `deepVariableReplacer` + `injectImageIntoMessages`. Supports any OpenAI-compatible API via cURL paste.
113. **`extractFromCommonFormats(data)`** — heuristic extractor: tries `choices[0].message.content`, `text`, `candidates[0].content.parts[0].text`, etc. Handles most response shapes.
114. **`switchToCurl(provider)`** — dynamically re-targets LLM calls to any cURL-defined endpoint (OpenRouter, DeepSeek, commercial APIs)

### RAG pipeline (electron/rag/*, 15 files)

115. **`SemanticChunker.ts`** (153 LOC) — turn-based chunking, TARGET=300 tokens, MAX=400, MIN=100, OVERLAP=50 tokens (1-2 segments carry-over max). Walks backward from end to compute overlap.
116. **`VectorStore.ts`** (710 LOC) — `better-sqlite3` + `sqlite-vec` native extension for ANN search. JS cosine fallback offloaded to `worker_threads.Worker` (30s deadman timeout per request, request-ID wrap-around, auto-restart on worker exit). Pending-requests Map for async resolution.
117. **`vectorSearchWorker.ts`** (299 LOC) — worker-thread JS cosine similarity when sqlite-vec unavailable. Receives Transferable buffers via `postMessage(transferList)` for zero-copy.
118. **4 embedding providers** via `IEmbeddingProvider` interface: `GeminiEmbeddingProvider`, `OpenAIEmbeddingProvider`, `OllamaEmbeddingProvider`, `LocalEmbeddingProvider`
119. **`EmbeddingProviderResolver`** — picks best available provider based on user config + availability
120. **`EmbeddingPipeline.ts`** (530 LOC) — chunking → embedding → storage coordinator with queue
121. **`LiveRAGIndexer.ts`** — feeds final transcripts to `ragManager.feedLiveTranscript()` during live meeting for JIT indexing
122. **`TranscriptPreprocessor.ts`** — cleans raw STT output (remove ums/ahs, merge broken sentences, strip filler, fix speaker attribution)
123. **`RAGManager.ts`** (476 LOC) — top-level orchestrator coordinating all RAG components
124. **`RAGRetriever.ts`** (357 LOC) — query → embedding → vector search → format for LLM context
125. **`OllamaBootstrap.ts`** — auto-detect and download Ollama embedding model if configured

### Rust native module (native-module/src/*.rs, 8 files)

**Crates**: `napi 3.8` (addon bindings), `cpal 0.15` (cross-platform audio), `ringbuf 0.4` (lockless ring buffer), `rubato 0.16` (sample-rate conversion), `webrtc-vad 0.4`, `bytemuck 1` (zero-copy reinterpret), `cidre` (macOS SCK/CoreAudio bindings), `wasapi` + `windows` (Windows audio).

126. **Zero-copy i16→u8**: `bytemuck::cast_slice::<i16, u8>(samples).to_vec()` replaces per-sample `to_le_bytes()` loop. Saves 48k ops/sec (960 samples × 50 chunks/sec × 2-byte appends).
127. **BatchEmitter coalesces 3 DSP frames** before crossing napi boundary — cuts V8 boundary crossings 3× while staying under STT framing thresholds (60-100ms). Flush triggers: batch-full, timeout (CHUNK_BATCH_TIMEOUT_MS), explicit flush on DSP exit.
128. **`ThreadsafeFunction<Buffer>`** — napi pattern for Rust-thread → JS-main-thread callback with type safety
129. **Double-start guard** — `if self.capture_thread.is_some() { return Err(...) }` prevents concurrent capture threads
130. **`Arc<AtomicU32>` sample_rate** — background thread detects real hardware rate on init, stores in shared atomic, JS reads via `get_sample_rate()`. Defaults to 48kHz until detected.
131. **`Arc<AtomicBool>` stop_signal** — lock-free cooperative shutdown
132. **Per-platform system-audio capture**: `speaker/macos.rs` + `speaker/core_audio.rs` (CoreAudio Tap) + `speaker/sck.rs` (ScreenCaptureKit) on macOS; `speaker/windows.rs` (WASAPI) on Windows
133. **`microphone.rs`** (17KB) — CPAL microphone capture with stream recreation on failure
134. **`silence_suppression.rs`** (17KB) — two-stage VAD: adaptive RMS + WebRTC ML VAD. `FrameAction = Send | SendSilence | Drop`. Rejects typing/fan noise before crossing napi boundary.
135. **`vad.rs`** (4KB) — WebRTC VAD wrapper
136. **`resampler.rs`** (2.9KB) — rubato wrapper
137. **`license.rs`** (20KB) — Natively Pro license validation (machine-uid + HTTP validation via reqwest). Includes sha256 signing. Premium feature gating.

### Audio STT providers (electron/audio/*, 10 files)

138. **8 STT provider wrappers** inheriting common interface (`.on('transcript')`, `.on('error')`, `.setRecognitionLanguage()`, optional `.finalize()`, `.setAudioChannelCount()`, `.notifySpeechEnded()`):
- `GoogleSTT.ts` (388 LOC) — gRPC streamingRecognize, handles code 11 silence-timeout gracefully
- `RestSTT.ts` (499 LOC) — Groq/Azure/IBM Watson via REST (shared class, provider-parametric)
- `DeepgramStreamingSTT.ts` (268 LOC) — WebSocket streaming
- `SonioxStreamingSTT.ts` (394 LOC) — WebSocket, fast+accurate
- `ElevenLabsStreamingSTT.ts` (366 LOC)
- `OpenAIStreamingSTT.ts` (859 LOC) — Realtime WS + Whisper-1 REST fallback; custom base-URL support for OpenAI-compatible servers
- `NativelyProSTT.ts` (512 LOC) — Natively's own backend, channel-keyed (system/mic), persistent-reconnect with 30s backoff cap, emits `languageDetected`
- `GoogleSTT.ts` — fallback
139. **`SystemAudioCapture.ts`** (TS wrapper around Rust SystemAudioCapture napi struct)
140. **`MicrophoneCapture.ts`** — TS wrapper around Rust mic capture
141. **`AudioDevices.ts`** — device enumeration + selection + change events
142. **`nativeModuleLoader.ts`** (220 LOC) — loads the .node binary, handles platform-specific paths
143. **`dnsHelpers.ts`** — DNS resolution helpers (maybe for WS endpoints?)

### Services (electron/services/*, 15 managers)

144. **`CredentialsManager.ts`** (582 LOC) — all API keys + user creds; keystore abstraction; `scrubKeys()` support
145. **`KeybindManager.ts`** (491 LOC) — global-shortcut registration, per-mode allowlist (`shouldRegister()`), user-rebindable
146. **`RateLimiter.ts`** — token-bucket class + `createProviderRateLimiters()` factory
147. **`ModelVersionManager.ts`** (1209 LOC) — self-updating model version registry, periodic background scheduler, vision-tier rotation (`getAllVisionTiers()`)
148. **`ModesManager.ts`** (384 LOC) — switch between Assist/Answer/WhatToAnswer/Brainstorm/Clarify/CodeHint/FollowUp/Recap modes
149. **`SettingsManager.ts`** — persistent settings (JSON? SQLite?) with typed `get/set`
150. **`CalendarManager.ts`** (493 LOC) — Google Calendar integration (OAuth via natively-api proxy), meeting-aware reminders
151. **`PhoneMirrorService.ts`** (561 LOC) + **`phoneMirrorClient.ts`** (841 LOC) — phone mirroring feature (share phone screen to desktop for AI analysis)
152. **`CodexCliService.ts`** (420 LOC) — spawn codex-cli as an LLM provider (for users who have codex subscription)
153. **`OllamaManager.ts`** — Ollama process management (start/stop/list models)
154. **`InstallPingManager.ts`** — anonymous telemetry ping on install
155. **`__tests__/CodexCliService.test.mjs`** — the only test file in electron/

---

## CONFIDENCE LEVEL for porting to bluey

By now I have **~155 distinct patterns** catalogued from natively-cluely alone. For bluey (cue) porting purposes, this is already comprehensive for:
- **Stealth / Masquerading** (9 patterns) — complete recipe
- **Multi-provider STT** (12 patterns) — 9 providers + error handling
- **Audio pipeline** (20 patterns) — zero-copy Rust, two-stage VAD, batch emitter
- **LLM orchestration** (27 patterns) — 5 clients + Ollama + codex CLI + custom cURL, fallback chains
- **RAG + memory** (11 patterns) — sqlite-vec + worker-thread fallback + semantic chunker
- **UX / shortcuts** (10 patterns) — Cmd+Shift+Enter, Cmd+K, opacity tricks, passthrough
- **Prompt engineering** (11 patterns) — XML tags, mode composition, per-provider specialization
- **Security / ops** (13 patterns) — key scrub, log rotation, lazy getPath, rate limiting
- **Premium gating** (5 patterns) — try/catch require, conditional modules

Remaining natively-cluely files (renderer `src/`, `renderer/`, natively-api backend, worker-script) will add UI/UX polish patterns but the core architectural learnings are already captured.

**Moving to repo 2 (pluely, Tauri stack — closest match to bluey's existing Rust+TS architecture) to get comparison points + stack-specific idioms.**


---

# 2. pluely-master (27K LOC, 10MB app) — **CLOSEST STACK MATCH FOR BLUEY**

**Tech**: Tauri 2 + React 19 + TS + Rust + Radix UI + Tailwind 4 + @ricky0123/vad-react + shiki + streamdown + rehype-katex
**License**: GPL-3.0
**Positioning**: "Open Source Alternative to Cluely, 27x smaller (10MB vs 270MB)"

### Patterns to port

156. **`tauri = { features = ["macos-private-api"] }`** unlocks content protection + NSPanel
157. **`tauri_nspanel` crate** — converts WebviewWindow to NSPanel with:
    - `NSFloatWindowLevel = 4` → always above normal windows
    - `NSWindowStyleMaskNonActivatingPanel = 1 << 7` → clicking doesn't activate as foreground (preserves user focus!)
    - `FullScreenAuxiliary | CanJoinAllSpaces` collection → works in fullscreen + all Spaces
    - `panel_delegate!` macro for `window_did_become_key` / `window_did_resign_key`
158. **Two-window pattern**: `main` overlay (600×variable, top-center, 54px offset) + `dashboard` (1200×800, 800×600 min, overlay title bar, traffic lights at 14,18, content_protected, pre-created on startup for instant open)
159. **`.content_protected(true)` builder method** — native Tauri 2 API on `WebviewWindowBuilder`, no platform-specific code
160. **Close handler prevents destruction** — `api.prevent_close()` + `.hide()` via `window.on_window_event` listener
161. **Pre-create dashboard on startup** so user-first-open is instant (no createWebviewWindow wait)
162. **Hold-to-move window** pattern: 60fps tick (16ms) via Arc<AtomicBool> stop flag, spawn tokio task on press, set flag on release
163. **Mutex poisoned recovery** via `poisoned.into_inner()` — don't panic on poisoned mutex, log and continue
164. **Registered shortcuts state** = `Mutex<HashMap<action_id, accelerator>>` populated by frontend via Tauri command, backend matches in handler closure
165. **License-gated actions** via `LicenseState { AtomicBool }` — `is_active()` checked before premium features (move_window blocked without license)
166. **Custom shortcut fallthrough**: unknown action IDs → `emit("custom-shortcut-triggered", {action})` so frontend can define arbitrary custom keybinds
167. **Multi-monitor screenshot** via `xcap::Monitor::all()`:
    - Capture image per monitor
    - Create `capture-overlay-{idx}` window per monitor — transparent, always-on-top, no decorations, no taskbar, positioned at monitor coords
    - User drags in overlay to select region
    - Physical pixels from xcap → logical units via `scale_factor` for window placement
    - Primary monitor gets `set_focus()` + `request_user_attention(Critical)`
    - `accept_first_mouse(true)` — first click works without focus
    - Stale overlay cleanup before new capture: iterate `app.webview_windows()`, destroy labels starting with `capture-overlay-`
168. **Platform-abstracted audio**: trait-based `SpeakerStream: futures::Stream<Item = f32>` with per-platform macos/windows/linux impls, exported via `mod speaker; pub use commands::*;` pattern
169. **macOS CoreAudio Tap** via `cidre::core_audio` + `AggregateDevice` + `HeapRb<f32>` ring (ringbuf crate) + `WakerState { Option<Waker>, has_data }` for async poll_next
170. **Device enumeration**: filter input via `input_stream_cfg().number_buffers() > 0`, filter output excludes primarily-input devices via `output_stream_cfg().number_buffers() > 0 && !is_primarily_input`
171. **Default device detection** via UID comparison with `ca::System::default_input_device()`
172. **PostHog analytics** with **session recording / pageview / pageleave disabled by default** — privacy-first telemetry pattern
173. **`tauri-plugin-keychain`** for native secure API key storage (macOS Keychain, Windows Credential Vault, Linux Secret Service)
174. **`tauri-plugin-machine-uid`** for license binding to hardware
175. **`tauri-plugin-sql` with sqlite** + built-in migrations system via `.add_migrations("sqlite:pluely.db", db::migrations())`
176. **Page-per-feature React architecture** (src/pages/): app, audio, chats, dashboard, dev, responses, screenshot, settings, shortcuts, system-prompts
177. **One big hook per feature** — useCompletion (1050 LOC), useSystemAudio (928 LOC), useChatCompletion (725 LOC) — keeps pages thin
178. **Dev mode page** for CRUD-ing custom AI/STT providers (src/pages/dev/components/ai-configs, stt-configs) — power-user self-service
179. **AI-powered prompt generator** for system-prompts library (CHANGELOG feature)
180. **Download conversation as markdown** export (src/pages/chats/components/View.tsx)
181. **Continue conversation from history** — view page loads context, allows appending new messages, attachments
182. **Sidebar navigation** with Cmd+Shift+D to toggle dashboard
183. **Radix UI + shadcn pattern** for consistent UI (tabs, card, slider, popover, scroll-area, switch, select, dropdown-menu, dialog, command)
184. **cmdk** for Cmd+K spotlight command menu
185. **shiki** for code syntax highlight in Markdown responses
186. **streamdown** for streaming markdown (token-by-token Markdown rendering without flicker)
187. **rehype-katex + remark-math** for LaTeX rendering in AI responses
188. **react-error-boundary** for graceful error boundaries

---

# 3. solveWatchAi-main (56K LOC) — **BEST 3-SERVICE ARCHITECTURE + SPEAKER ID**

**Tech**: Electron + Node.js + Python + MLX Whisper + Socket.IO + OpenTelemetry
**License**: MIT
**Positioning**: "Real-time AI interview assistant — invisible to interviewer"

### Unique features

189. **Speaker identification via SpeechBrain ECAPA-TDNN** — enroll 30s voice sample, then filter YOUR voice out so only interviewer questions trigger AI (huge UX win, bluey should have this)
190. **Screenshot FOLDER monitoring** (not just hotkey) — watch a folder, any new screenshot triggers OCR + AI. Great for coding problems shared on screen.
191. **3-service architecture managed by one start.sh**:
    - Electron HUD (380×460, frameless, always-on-top, content-protected)
    - Node.js backend (Express + Socket.IO at `/data-updates` ws-only transport)
    - Python transcriber (FastAPI + Whisper + VAD)
    - + Ollama server as 4th lifecycle-managed process
192. **`start.sh` orchestration**: reads config, starts services in dependency order, waits for port readiness, tails logs live, gracefully kills everything on Ctrl+C including Ollama if it started it
193. **MLX Whisper on Apple Silicon** — pre-warmed at startup for Metal JIT (faster than openai-whisper by ~2x)
194. **LocalAgreement-2 streaming decoder** (`streaming_stt.py`) — specific algorithm for low-latency Whisper streaming. Decodes every 300ms, emits `stt_partial` / `stt_final` events. Research the algorithm.
195. **`always_on_listener.py`** — VAD state machine feeds StreamingSTT continuously, supplanting old flush-on-silence path
196. **Deepgram Nova-2** option at ~$0.0059/min (cheap alternative to Whisper)
197. **OpenTelemetry → Grafana Cloud** with pre-built `docs/grafana-dashboard.json` — AI latency, token spend, STT pipeline health, host metrics
198. **InterviewTranscriptBuffer** (`src/sockets/InterviewTranscriptBuffer.js`) — per-session Q&A memory: `summaries` (older stuff) + `recentPairs` (last 3-5 Q&A). Enables follow-up questions like "what are its trade-offs?"
199. **TTL cleanup pattern**: 30min expiry, 5min sweep interval in socket handler
200. **Hot-reloaded config** via `fs.watch('config/api-keys.json')` — no restart needed to change API keys, models, audio device
201. **Hot-reloaded prompts** — `prompts/*.txt` all watched, `ai.service` re-reads on change
202. **Structured NDJSON event logs** (`logs/app.jsonl`) + in-memory ring buffer for recent logs (`memory-logger.js`)
203. **Clear logs on every server startup** (`start.sh --newlogs` is default-like)
204. **Question extractor heuristics** (`transcriber/question_extractor.py`) — detect interview questions without AI roundtrip, trim noise
205. **Default model choices per provider**: gpt-4o-mini, llama-3.3-70b-versatile, gemini-2.5-flash, claude-sonnet-4-5
206. **Ollama for classification + summarization only** (cheap local tasks, not main Q&A) — llama3.2:1b (1.3GB)
207. **Settings as browser-hosted HTML** at `http://localhost:4000/settings` (served from `src/public/`) — avoids Electron for settings UI, easier to develop
208. **Hotkeys**: Cmd+Shift+H toggle HUD, Cmd+Shift+X toggle listen (minimal)
209. **HUD dimensions 380×460** — compact, doesn't take much screen space
210. **Socket.IO namespace `/data-updates`** with websocket transport only (no polling fallback) — lower latency
211. **CHANGELOG + PR template discipline**: every PR updates CHANGELOG.md (Keep-a-Changelog format), fills PULL_REQUEST_TEMPLATE.md sections (Summary / Type / Affected components / Testing / Notes)
212. **Code exploration via MCPs** — `code-review-graph` for semantic search / impact radius / review context, `graphify` for visual community graphs. Reduces 10-50× tokens vs full scans. Already built graph in `transcriber/graphify-out/`.

---

# 4. Aura-AI (17K LOC, Python-first) — **STEALTH + PROCTORING BYPASS GOLDMINE**

_Reading next._

# 5. Vysper (14K LOC, Electron) + 6. OpenCluely (16K LOC, Electron fork)

_Reading after Aura-AI._


---

# 4. Aura-AI (17K LOC Python)

**Tech**: Python + Tesseract OCR + webview + win32 ctypes
**License**: ?
**Positioning**: Stealth AI with PROCTORING bypass focus

### Unique Windows stealth patterns

213. **`user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE=0x11)`** — excludes window from ALL screen capture (OBS, Teams, Zoom, screenshots). Window appears as black rectangle in recordings.
214. **`user32.ShowWindow(hwnd, SW_SHOWNOACTIVATE=4)`** vs `SW_SHOW=5` — show window without giving it focus. Critical: proctoring software detects focus changes.
215. **`user32.SetWindowLongPtr(hwnd, GWL_EXSTYLE, style | WS_EX_TRANSPARENT=0x20)`** — click-through "ghost mode". Combined with `WS_EX_LAYERED` for transparency.
216. **`WS_EX_TOOLWINDOW = 0x80`** — hides window from taskbar + Alt-Tab list.
217. **`SWP_NOACTIVATE = 0x10` + `SWP_NOZORDER = 0x4`** on `SetWindowPos()` — moves window without changing focus/zorder.
218. **Dedicated constants**: `SW_HIDE=0`, `LWA_ALPHA=0x2`, `HWND_TOPMOST=-1`, `HWND_NOTOPMOST=-2`, `SWP_NOMOVE=0x2`, `SWP_NOSIZE=0x1`.
219. **3 detection vectors countered**:
    - Focus changes → `SW_SHOWNOACTIVATE` + `SWP_NOACTIVATE`
    - Screen recording → `WDA_EXCLUDEFROMCAPTURE`
    - Window detection → `WS_EX_TOOLWINDOW` (hide from taskbar/Alt-Tab)
220. **`find_screen_share_indicators()`** — scans top-level windows by title patterns to detect OBS/Teams/Zoom share-indicator overlays
221. **`hide_screen_share_indicator(hwnd)`** — hides the share-indicator itself (disables Teams/Zoom's red "you're sharing" border)
222. **`start_screen_share_monitor()`** — background thread watches for new share indicators, hides them reactively
223. **`silent_run.vbs`** — VBScript launcher that starts Python without visible console window on Windows
224. **20px increment window movement** (small enough unobtrusive, large enough useful)
225. **Continuous scroll on hold** — key held → scroll loop; key released → stop
226. **3 transparency presets**: 40% / 70% / 100% via Alt+1/2/3 (40% for exams, 70% for video interviews)
227. **Hotkey-only workflow** — zero mouse interaction with AI window during proctoring. All via Alt+keys.
228. **Stealth activation via single hotkey** `Alt+Shift+S` enables ghost mode + capture protection + no-focus + taskbar hide + always-on-top all at once.

### Aura architecture (main.py)

229. **`find_free_port(preferred=8002)`** — picks free port on startup for internal FastAPI server
230. **`class GlobalCommandMonitor`** — listens for command files written by hotkey handler
231. **`class UvicornServer`** — manages the FastAPI server thread
232. **`class AsyncioServiceThread`** — runs asyncio event loop in dedicated thread
233. **`setup_webview_window()`** — `pywebview` for the UI (not Electron)
234. **Command-file IPC** between hotkey handler thread and main process (`_write_command_file(command_data)`) — simpler than IPC, robust

---

# 5. Vysper-main (14K LOC Electron)

**Positioning**: Interview Assistant with 9 skill-specialized prompts

### Unique features

235. **Skill-based prompts library** — 9 `.md` files, one per interview skill: dsa, system-design, programming, behavioral, sales, negotiation, presentation, devops, data-science. User selects active skill via UI. PromptLoader class loads from `prompts/` dir at runtime.
236. **Skill prompt structure** (dsa.md example): Instant Problem Analysis → Solution Approach (Naive → Optimal → Dry Run → Clean Implementation → Test Cases) → Common Patterns → Complexity Quick Reference. Specific output format prescribed per skill.
237. **Window binding** — bind multiple Vysper windows to move together as one unit, with configurable gap between them. `ipcMain.handle("set-window-binding", enabled)`, `move-bound-windows`. Multi-window tool pattern.
238. **`force-always-on-top`** + `test-always-on-top` for debug/runtime verification of z-order state
239. **`expand-llm-window(contentMetrics)`** + `resize-llm-window-for-content(contentMetrics)` — dynamic window sizing based on AI response length. Measures content, resizes window to fit. Great UX.
240. **`restart-app-for-stealth`** — cleanly restart when stealth settings change (rather than hot-reload for complex state)
241. **`update-app-icon(iconKey)`** — change dock icon dynamically (like natively-cluely's masquerade)
242. **`run-gemini-diagnostics`** — debug button that validates API key + tests connection + reports issues
243. **38 IPC handlers** total — clean surface, one handler per user action

### Dependencies
- `tesseract` + `sox` (for audio) installed via `brew install`
- `electron-builder` for `.dmg + .zip`, `.exe + portable`, `.AppImage + .deb` builds

---

# 6. OpenCluely-main (16K LOC, Vysper fork)

**Positioning**: Stripped-down Vysper clone (2 skill prompts instead of 9)

### Confirmation
- Same file names (chat.html, main.js, preload.js, prompt-loader.js, settings.html, speech-recognition.js, prompts/, src/)
- Same architecture as Vysper
- Only `dsa.md` + `programming.md` prompt files (vs Vysper's 9)
- Likely the ancestor before Vysper was extended

### Added vs Vysper
- `setup.sh` script
- `scripts/` directory

No new unique patterns beyond Vysper.

---

# SUMMARY OF FINDINGS ACROSS ALL 6 REPOS

**Total LOC read/analyzed**: ~335K (partial depth on natively-cluely core, deep on pluely/solveWatchAi/Aura architecture, full on Vysper/OpenCluely)

**Total distinct patterns/features catalogued**: **243**

### Coverage by category

| Category | Patterns | Key sources |
|---|---|---|
| Stealth/Masquerading (macOS+Windows) | 25 | natively-cluely, Aura, pluely |
| Multi-provider STT | 15 | natively-cluely (9 providers), solveWatchAi (3 modes) |
| Audio pipeline (Rust/Python/Electron) | 25 | natively-cluely (Rust), pluely (Rust), solveWatchAi (Python MLX Whisper) |
| LLM orchestration | 35 | natively-cluely (LLMHelper 3894 LOC), pluely (api.rs), solveWatchAi (fallback chain) |
| RAG + Memory | 14 | natively-cluely (sqlite-vec + worker), solveWatchAi (InterviewTranscriptBuffer) |
| UX / Shortcuts / Windows | 28 | pluely (NSPanel), Aura (hotkey stealth), natively-cluely (opacity), Vysper (window binding) |
| Prompt engineering | 15 | natively-cluely (2140-line prompts.ts), Vysper (skill library) |
| Security + Ops | 18 | natively-cluely (AUDIT.md), pluely (keychain plugin) |
| Premium gating | 6 | natively-cluely (conditional require), pluely (LicenseState) |
| Documentation / workflow discipline | 10 | solveWatchAi (CLAUDE.md, PR template), natively-cluely (FIXES.md) |
| React/UI library | 12 | pluely (Radix+shadcn, cmdk, shiki, streamdown), solveWatchAi (HUD 380×460) |
| Codex/agent meta | 5 | natively-cluely (.codex agents) |
| Build/release | 7 | Vysper (electron-builder), natively-cluely (auto-updater) |
| Observability | 5 | solveWatchAi (OpenTelemetry + Grafana dashboard) |
| Platform-specific APIs | 23 | Aura (Win32 ctypes), pluely (tauri-nspanel), natively-cluely (cidre) |

### Top 10 highest-value patterns for cue/bluey to prioritize

1. **Content-protection via Tauri 2 `.content_protected(true)` builder** + NSPanel for macOS stealth (pluely) — native API, no platform code needed
2. **7-step process masquerading** + pre-built fake icons for 3+ disguises (natively-cluely)
3. **Two-stage VAD** (adaptive RMS + WebRTC ML) before billing STT (natively-cluely)
4. **Zero-copy Rust→JS audio** via `bytemuck::cast_slice` + BatchEmitter napi coalescing (natively-cluely)
5. **Speaker identification (SpeechBrain ECAPA-TDNN)** to filter YOUR voice (solveWatchAi unique)
6. **Single-trigger capture hotkey** (Cmd+Shift+Enter = screenshot + AI analysis in one) (natively-cluely)
7. **`WDA_EXCLUDEFROMCAPTURE` + `SW_SHOWNOACTIVATE` + `WS_EX_TRANSPARENT`** Windows trifecta (Aura)
8. **Multi-provider LLM fallback chain** with `ModelVersionManager` for self-updating model IDs (natively-cluely)
9. **sqlite-vec + worker-thread JS fallback** for local RAG (natively-cluely)
10. **Dashboard + sidebar nav** with Chats/SystemPrompts/Settings/Shortcuts pages (pluely)

### Top 5 things natively-cluely/solveWatchAi do that Cluely doesn't

1. Local + offline (Ollama, local Whisper/MLX, local RAG)
2. Multi-provider fallback chains (API is never single-point-of-failure)
3. Speaker identification (only answer interviewer questions, not yours)
4. Screenshot folder monitoring (not just hotkey)
5. Hot-reloaded config + prompts (no restart for changes)

---

# FINAL DELIVERABLE → docs/reviews/CUE-PORT-PLAN.md

Will be written next — ranked backlog of ~60-80 items for codex to implement, organized in batches.
