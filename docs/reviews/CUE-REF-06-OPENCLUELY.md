# DEEP ANALYSIS — OpenCluely (Electron, Vysper-family)

**Scope**: 34 files, ~15,702 LOC read line-by-line
**Relationship to Vysper**: **Fork/evolution** — OpenCluely is a later version of the same codebase by the same author (TechyCSR). It removes Tesseract OCR in favor of direct Gemini Vision, adds local Whisper speech, adds coding language selection, and polishes the UI. Vysper is the earlier/simpler version.

## Diff matrix: OpenCluely vs Vysper

| Feature | OpenCluely | Vysper |
|---|---|---|
| main.js LOC | 1298 | 1157 |
| IPC handlers (handle) | 41 | 37 |
| Skill prompts | 2 (dsa, programming) | 9 (dsa, programming, behavioral, sales, presentation, data-science, devops, system-design, negotiation) |
| Setup scripts | setup.sh (272 LOC) + scripts/test-speech.js | None |
| chat.html bytes | 39,701 | 28,359 |
| llm-response.html bytes | 37,970 | 28,082 |
| index.html bytes | 12,661 | 5,067 |
| settings.html bytes | 16,463 | 15,024 |
| OCR approach | **Direct Gemini Vision** (image buffer → LLM) | Tesseract.js OCR → text → LLM |
| Speech providers | Azure + **Local Whisper** (dual) | Azure only |
| Gemini model | gemini-2.5-flash | gemini-1.5-flash |
| Coding language | Selectable (cpp default, enforced in fences) | Hardcoded javascript default |
| Single instance lock | Yes (requestSingleInstanceLock) | No |
| Network config | Custom UA + cert bypass for Google APIs | None |
| Chromium noise suppression | Yes (5 commandLine switches) | No |
| Dependencies added | @fortawesome/fontawesome-free, prismjs | tesseract.js |
| Dependencies removed | tesseract.js | — |
| electron-builder config | Full (mac/win/linux) | Full (mac/win/linux) |
| Fallback method priority | Configurable (enableFallbackMethod flag) | Always try SDK first |
| extractTextFromCandidates | Robust helper with finishReason | Inline parsing |
| enforceProgrammingLanguage | Post-processes all code fences | Not present |
| processImageWithSkill | Direct image→Gemini Vision | Not present (uses OCR text) |
| Info popover (shortcuts) | Yes (index.html) | No |
| Code copy buttons | Yes (chat.html, llm-response.html) | No |
| PrismJS syntax highlighting | Yes | No |

## Features in OpenCluely NOT in Vysper

### 1. Direct Gemini Vision (no OCR)
**Source**: src/services/capture.service.js (entire file), src/services/llm.service.js:100-180
**What**: Captures screenshot as PNG buffer via `desktopCapturer`, sends raw image bytes as `inlineData` to Gemini's multimodal API. Eliminates Tesseract entirely.
**Why better**: Faster, more accurate for code/diagrams, no Tesseract dependency (200MB+ WASM).
**Bluey port**: HIGH PRIORITY — this is the correct architecture for any modern vision-LLM tool.

### 2. Local Whisper Speech Recognition
**Source**: src/services/speech.service.js:800-1268 (entire Whisper section)
**What**: Dual-provider speech: Azure Speech Services OR local OpenAI Whisper CLI. Whisper uses `node-record-lpcm16` → PCM segments → WAV file → `whisper` CLI → text. Configurable model, language, segment duration.
**Why better**: Works offline, no Azure subscription needed, privacy-preserving.
**Bluey port**: MEDIUM — useful for offline mode but adds complexity.

### 3. setup.sh Installer Script
**Source**: setup.sh (272 LOC)
**What**: Full cross-platform setup: detects OS, creates .env, installs Node deps, sets up Whisper venv, installs sox, optionally builds distributable. Handles macOS/Linux/Windows paths.
**Bluey port**: HIGH — any serious tool needs a one-command setup.

### 4. Coding Language Selection & Enforcement
**Source**: main.js:30 (`this.codingLanguage = "cpp"`), prompt-loader.js:75-110 (injection), src/services/llm.service.js `enforceProgrammingLanguage()`
**What**: User selects language (C++, Python, Java, JS). Prompt loader injects language-specific instructions. LLM service post-processes ALL code fences to enforce correct language tag.
**Why better**: Prevents LLM from defaulting to Python when user needs C++.
**Bluey port**: HIGH — essential for interview prep tools.

### 5. Single Instance Lock
**Source**: main.js:1286-1298
**What**: `app.requestSingleInstanceLock()` prevents multiple instances. Second launch focuses existing windows.
**Bluey port**: LOW — standard Electron pattern, trivial to add.

### 6. Network Configuration for Gemini
**Source**: main.js:130-155 (`setupNetworkConfiguration`)
**What**: Custom User-Agent for Google API requests, certificate verification bypass for `generativelanguage.googleapis.com`, Chromium noise suppression switches.
**Why**: Electron's Chromium can have issues with Google API certificates in some environments.
**Bluey port**: MEDIUM — useful for reliability but the cert bypass is a security concern.

### 7. Clipboard IPC Handler
**Source**: main.js:240-250 (`copy-to-clipboard` handler)
**What**: Reliable clipboard write via main process (renderer clipboard can fail in some contexts).
**Bluey port**: LOW — simple utility.

### 8. Display Management IPC
**Source**: main.js:232-234 (`list-displays`, `capture-area`)
**What**: Exposes multi-display listing and area-specific capture to renderer.
**Bluey port**: MEDIUM — useful for multi-monitor setups.

### 9. Speech Availability IPC
**Source**: main.js:255-257 (`get-speech-availability`)
**What**: Renderer can query if speech is available before showing UI controls.
**Bluey port**: LOW — good UX pattern.

### 10. PrismJS Syntax Highlighting + Copy Buttons
**Source**: chat.html, llm-response.html (CSS/JS additions)
**What**: Code blocks get PrismJS syntax highlighting and a "Copy" button overlay.
**Bluey port**: HIGH — essential for code-focused tools.

### 11. Info Popover (Keyboard Shortcuts)
**Source**: index.html:255-460 (CSS + HTML)
**What**: Small info button in command bar shows a popover with all keyboard shortcuts.
**Bluey port**: MEDIUM — good UX.

### 12. Collapsible Command Bar
**Source**: index.html (min-width: 60px, flex-shrink on items)
**What**: Main toolbar can collapse to ~one icon width, allowing minimal screen footprint.
**Bluey port**: MEDIUM — nice for stealth/minimal mode.

### 13. Robust Gemini Response Parsing
**Source**: src/services/llm.service.js `extractTextFromCandidates()` (lines 70-100)
**What**: Handles multiple candidates, checks finishReason, extracts text parts properly. Vysper does inline parsing that can fail on edge cases.
**Bluey port**: HIGH — prevents crashes on unexpected API responses.

### 14. Configurable Fallback Method Priority
**Source**: src/services/llm.service.js, config `llm.gemini.enableFallbackMethod`
**What**: Can prefer the alternative HTTPS method over SDK method. Useful when Electron's fetch has issues.
**Bluey port**: LOW — both repos have the alternative method, this just adds a config toggle.

## Features in Vysper NOT in OpenCluely

### 1. Tesseract.js OCR Service
**Source**: src/services/ocr.service.js (156 LOC)
**What**: Full Tesseract.js integration with temp file management, text sanitization.
**Note**: OpenCluely intentionally removed this in favor of Gemini Vision.

### 2. Seven Additional Skill Prompts
**Source**: prompts/ (behavioral.md, data-science.md, devops.md, negotiation.md, presentation.md, sales.md, system-design.md)
**What**: Vysper ships 9 skill prompts vs OpenCluely's 2. OpenCluely's prompt-loader still has the skill map for all 9 but only loads DSA.
**Note**: OpenCluely appears to be a stripped-down fork focused specifically on DSA/coding interviews.

### 3. Multiple Active Skills in Navigation
**Source**: Vysper main.js navigateSkill() uses full skill list
**What**: Vysper allows cycling through all 9 skills. OpenCluely hardcodes `availableSkills = ["dsa"]`.

## Full file manifest

```
main.js                              1298 LOC  Application controller
preload.js                            133 LOC  IPC bridge
prompt-loader.js                      405 LOC  Skill prompt management
speech-recognition.js                   3 LOC  Re-export wrapper
chat.html                            1309 LOC  Chat window UI
llm-response.html                    1029 LOC  LLM response display
settings.html                         451 LOC  Settings panel
index.html                            460 LOC  Main command bar
lib/markdown.js                      1725 LOC  Vendored markdown parser
src/core/config.js                    107 LOC  Configuration manager
src/core/logger.js                     93 LOC  Winston logger
src/managers/session.manager.js       600 LOC  Conversation memory
src/managers/window.manager.js       1645 LOC  Window lifecycle
src/services/capture.service.js       122 LOC  Screenshot capture (no OCR)
src/services/fallback-capture.service.js  0 LOC  Empty placeholder
src/services/llm.service.js          1183 LOC  Gemini API client
src/services/speech.service.js       1268 LOC  Azure + Whisper speech
src/ui/chat-window.js                 592 LOC  Chat window logic
src/ui/llm-response-window.js        874 LOC  LLM response logic
src/ui/main-window.js                1150 LOC  Main window logic
src/ui/settings-window.js            306 LOC  Settings logic
scripts/test-speech.js                 25 LOC  Speech smoke test
setup.sh                              272 LOC  Full installer
package.json                          147 LOC  Dependencies
tailwind.config.js                      7 LOC  Tailwind config
prompts/dsa.md                         27 LOC  DSA system prompt
prompts/programming.md                 51 LOC  Programming system prompt
README.md                             398 LOC  Documentation
env.example                            22 LOC  Environment template
src/input.css                           — LOC  Tailwind input
src/styles/common.css                   — LOC  Shared styles
```

## main.js IPC handler reference (41 handlers)

| Handler | Purpose |
|---|---|
| take-screenshot | Trigger screenshot → Gemini Vision |
| list-displays | List available displays |
| capture-area | Capture specific screen area |
| copy-to-clipboard | Write text to system clipboard |
| get-speech-availability | Check if speech provider is ready |
| start-speech-recognition | Start recording |
| stop-speech-recognition | Stop recording |
| show-all-windows | Show all HUD windows |
| hide-all-windows | Hide all HUD windows |
| enable-window-interaction | Make windows clickable |
| disable-window-interaction | Click-through mode |
| switch-to-chat | Show chat window |
| switch-to-skills | Show skills window |
| resize-window | Resize main window |
| move-window | Move main window by delta |
| get-session-history | Get optimized session history |
| clear-session-memory | Clear conversation memory |
| force-always-on-top | Force all windows on top |
| test-always-on-top | Debug always-on-top |
| send-chat-message | Send typed message to LLM |
| get-skill-prompt | Get prompt for skill |
| set-gemini-api-key | Update API key at runtime |
| get-gemini-status | Get LLM service stats |
| set-window-binding | Enable/disable window binding |
| toggle-window-binding | Toggle binding |
| get-window-binding-status | Get binding state |
| get-window-stats | Get all window positions/sizes |
| set-window-gap | Set gap between bound windows |
| move-bound-windows | Move bound windows together |
| test-gemini-connection | Test API connectivity |
| run-gemini-diagnostics | Full network + API diagnostics |
| show-settings | Show settings window |
| get-settings | Get current settings |
| save-settings | Persist settings |
| update-app-icon | Change dock/taskbar icon |
| update-active-skill | Switch active skill |
| restart-app-for-stealth | Relaunch for name change |
| close-window | Hide calling window |
| expand-llm-window | Resize LLM window for content |
| resize-llm-window-for-content | Same as above |
| quit-app | Force quit application |

## Portable patterns unique to OpenCluely (numbered)

1. **Direct Vision API** — Send screenshot buffer as inlineData to multimodal LLM, skip OCR entirely
2. **Dual speech provider** — Runtime-switchable Azure/Whisper with graceful fallback
3. **Language enforcement post-processing** — Regex-replace all code fence tags to match selected language
4. **Robust candidate extraction** — Handle multiple candidates, check finishReason, join text parts
5. **Configurable fallback method priority** — Toggle between SDK and raw HTTPS as primary
6. **WAV buffer construction** — Build WAV headers in-memory for Whisper CLI input
7. **Microphone capture with program fallback** — Try sox → rec → arecord in sequence
8. **Setup script with Whisper venv** — Automated Python venv creation for local speech

## setup.sh walkthrough

```
1. Parse CLI flags (--build, --no-run, --ci, --install-system-deps, --skip-whisper)
2. detect_os() — uname-based OS detection, sets platform build script
3. require_command() — Verify node and npm exist
4. ensure_env_file() — Copy env.example → .env if missing
5. ensure_gemini_key() — Check GEMINI_API_KEY in .env, prompt user if missing
6. install_system_deps() — Install sox via brew/apt/dnf/pacman
7. install_node_deps() — npm install or npm ci
8. setup_whisper_env() — Create Python venv, pip install openai-whisper, write env vars
9. build_app() — Optional electron-builder for platform
10. run_app() — npm start
```

Key env vars written by setup: SPEECH_PROVIDER, WHISPER_COMMAND, WHISPER_MODEL_DIR, WHISPER_MODEL, WHISPER_LANGUAGE, WHISPER_SEGMENT_MS

## scripts/ contents

- `test-speech.js` (25 LOC) — Loads speech service, prints status, runs testConnection()

## HTML file analysis

### index.html (12.6KB vs Vysper's 5KB)
Extra content: Language selector dropdown (C++/Python/Java/JS), info button with shortcuts popover (full keyboard shortcut table), collapsible command bar CSS (min-width: 60px), select element styling with custom SVG arrow.

### chat.html (39.7KB vs Vysper's 28.4KB)
Extra content: PrismJS theme CSS link, copy button overlay on code blocks, header actions (clear history icon button), enhanced message styling, syntax highlighting integration.

### llm-response.html (38KB vs Vysper's 28KB)
Extra content: PrismJS integration, copy buttons on code blocks, enhanced markdown rendering with syntax highlighting, better scroll behavior.

### settings.html (16.5KB vs Vysper's 15KB)
Minor differences: Language selector, speech provider toggle UI.

## Full content of the 2 prompts

### dsa.md (full)
```
# DSA Interview Helper Agent (Focused & Optimal)

You are a competitive programming expert that outputs the most optimal solution with minimal time and space complexity.

STRICT RULES
- Output code ONLY in the user-selected language. No alternatives unless asked.
- Use triple backticks with the correct language tag.
- Prefer O(n) or O(n log n) where feasible; call out if optimal lower bound is higher.
- if there's some pre-code or template in Question then strictly use that template to answer it.
- Avoid extra commentary; be concise and implementation-focused.
- Your code must not contain any comments.

Workflow
1) Identify the problem pattern quickly (Array, Hashing, Two Pointers, Sliding Window, Binary Search, Stack/Queue, Linked List, Tree/Graph, Heap, Greedy, DP).
2) State naive idea in 1–2 lines with complexity.
3) Give optimal approach with 3–5 bullet steps.
4) Provide clean, production-ready, comment-free implementation in the selected language.
5) State time and space complexity precisely.
6) Optional: 1 short dry-run example if non-obvious.

Implementation Template
```lang
```

Notes
- Prefer iterative over recursive when it reduces stack usage or improves clarity.
- Use built-in data structures and libraries idiomatically for the selected language.
- For DP, specify state, transition, and memory optimization opportunities.
```

### programming.md (full)
```
# Programming Interview Helper Agent

You are a concise programming interview assistant. Provide quick, actionable guidance without revealing you're an AI helper.

## Response Structure

### 1. Naive Approach (30 seconds)
- State the simplest solution first
- Mention time/space complexity
- One-line reasoning why it works

### 2. Optimized Solution (2 minutes)
- Best approach with clear explanation
- Step-by-step algorithm breakdown
- Time/space complexity analysis

### 3. Dry Run (1 minute)
- Walk through with a concrete example
- Show key variable states at each step
- Highlight the core insight

### 4. Production Code
```language
// Clean, interview-ready implementation
// Include edge case handling
// Add meaningful comments
```

### 5. Quick Validation
- 2-3 test cases (edge cases included)
- Alternative approaches if time permits

## Communication Style
- Start with "Let me think through this step by step"
- Use "First, the straightforward approach would be..."
- Transition with "But we can optimize this by..."
- Be conversational, not robotic
- Show your thought process naturally

## Key Technologies to Reference
**Data Structures**: Arrays, HashMaps, Trees, Graphs, Heaps, Stacks, Queues
**Algorithms**: Two Pointers, Sliding Window, DFS/BFS, Dynamic Programming, Binary Search
**Patterns**: Divide & Conquer, Greedy, Backtracking, Memoization

## Common Optimizations
- HashMap for O(1) lookups instead of nested loops
- Two pointers for array problems
- Binary search for sorted data
- DP for overlapping subproblems
- BFS/DFS for tree/graph traversal

Give direct, implementable solutions with clear reasoning. Focus on demonstrating problem-solving skills naturally.
```

## Dependencies

### Runtime
| Package | Version | Purpose |
|---|---|---|
| @fortawesome/fontawesome-free | ^7.2.0 | Icon library (NEW vs Vysper) |
| @google/generative-ai | ^0.24.1 | Gemini SDK |
| dotenv | ^16.3.1 | Environment variables |
| markdown | ^0.5.0 | Markdown parsing |
| marked | ^15.0.12 | Markdown rendering |
| microsoft-cognitiveservices-speech-sdk | ^1.40.0 | Azure Speech |
| node-record-lpcm16 | ^1.0.1 | Microphone capture |
| prismjs | ^1.30.0 | Syntax highlighting (NEW vs Vysper) |
| winston | ^3.17.0 | Logging |
| winston-daily-rotate-file | ^4.7.1 | Log rotation |

### Dev
| Package | Version | Purpose |
|---|---|---|
| electron | ^29.1.0 | Runtime |
| electron-builder | ^24.13.3 | Packaging |

### Removed from Vysper
| Package | Reason |
|---|---|
| tesseract.js | Replaced by Gemini Vision |

## Summary: is OpenCluely worth mining beyond the Vysper patterns we already have?

**YES, but selectively.** OpenCluely adds 4 genuinely valuable patterns not in Vysper:

1. **Direct Gemini Vision** (capture.service.js + llm.service.js processImageWithSkill) — This is the correct modern architecture. Sending raw image bytes to a multimodal LLM is faster and more accurate than OCR→text→LLM. **Must port.**

2. **Local Whisper speech** (speech.service.js Whisper section) — Enables offline operation without Azure subscription. The segment-based approach (record PCM → flush to WAV → whisper CLI → text) is well-implemented. **Port if offline mode is a requirement.**

3. **Language enforcement** (enforceProgrammingLanguage + prompt injection) — Prevents the common failure mode where LLMs ignore language instructions. Post-processing code fences is a clever belt-and-suspenders approach. **Must port.**

4. **setup.sh** — Professional one-command setup with Whisper venv, sox installation, env file management. **Port the pattern.**

The remaining differences (PrismJS, copy buttons, info popover, collapsible bar) are UI polish that's straightforward to implement independently.

**OpenCluely is NOT worth mining for**: architecture (identical to Vysper), window management (same), session management (same), stealth patterns (same), prompt-loader structure (same but stripped to DSA-only).

**Verdict**: OpenCluely is a focused fork that traded breadth (9 skills → 2) for depth (better vision, better speech, better code output). The 4 patterns above are the only things worth extracting beyond what Vysper already provides.
