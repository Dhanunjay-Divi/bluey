# DEEP ANALYSIS — Vysper (Electron interview assistant, 9 skill prompts)

**Scope**: 38 files, ~14,843 LOC read line-by-line
**Architecture**: Electron (main process class) + vanilla JS renderers + IPC bridge
**AI Provider**: Google Gemini 1.5 Flash (primary), Azure Speech Services (STT)
**Stealth**: Process title disguise, content protection, always-on-top enforcement

## File manifest

| File | Lines | Role |
|------|-------|------|
| main.js | 1157 | Application controller, all IPC handlers |
| src/managers/window.manager.js | 1607 | Window lifecycle, binding, stealth, positioning |
| src/managers/session.manager.js | 583 | Conversation memory with compression |
| src/services/llm.service.js | 943 | Gemini API integration with retry/fallback |
| src/services/speech.service.js | 968 | Azure Speech SDK continuous recognition |
| src/services/ocr.service.js | 156 | Tesseract.js screenshot OCR |
| src/core/config.js | 94 | Centralized config manager |
| src/core/logger.js | 93 | Winston daily-rotate logging |
| prompt-loader.js | 443 | Skill prompt loading + language injection |
| preload.js | 119 | Context bridge (electronAPI + api) |
| src/ui/main-window.js | 955 | Command tab UI controller |
| src/ui/chat-window.js | 535 | Chat/transcription renderer |
| src/ui/llm-response-window.js | 874 | LLM response display with markdown |
| src/ui/settings-window.js | 236 | Settings panel controller |
| index.html | 211 | Main command tab (28px toolbar) |
| chat.html | 988 | Chat window with input |
| llm-response.html | 790 | AI response display |
| settings.html | 415 | Settings panel |
| lib/markdown.js | 1725 | Gruber/Maruku markdown parser |
| prompts/*.md (9 files) | ~1098 | Skill system prompts |
| speech-recognition.js | 3 | Re-export wrapper |
| tailwind.config.js | 7 | Tailwind config |
| package.json | 138 | Dependencies & build config |
| src/styles/common.css | 578 | Shared styles |
| src/input.css | 2 | Tailwind input |

## main.js IPC handler reference (all 38+)

### take-screenshot
**Args**: none
**Action**: Triggers OCR pipeline: captureScreenshot → performOCR → processWithLLM
**State mutation**: Adds OCR event + LLM response to sessionManager
**Renderer-side use**: Command tab camera button

### start-speech-recognition (handle + on)
**Args**: none
**Action**: speechService.startRecording() — starts Azure continuous recognition
**State mutation**: Sets speechService.isRecording = true
**Renderer-side use**: Mic button in main window

### stop-speech-recognition (handle + on)
**Args**: none
**Action**: speechService.stopRecording()
**State mutation**: Sets speechService.isRecording = false
**Renderer-side use**: Mic button toggle

### chat-window-ready (on)
**Args**: none
**Action**: Sends test message to confirm IPC communication after 1s delay
**State mutation**: None

### test-chat-window (on)
**Args**: none
**Action**: Broadcasts test transcription to all windows
**State mutation**: None

### show-all-windows
**Args**: none
**Action**: windowManager.showAllWindows()
**State mutation**: windowManager.isVisible = true
**Returns**: Window stats

### hide-all-windows
**Args**: none
**Action**: windowManager.hideAllWindows()
**State mutation**: windowManager.isVisible = false

### enable-window-interaction
**Args**: none
**Action**: windowManager.setInteractive(true) — removes ignoreMouseEvents
**State mutation**: windowManager.isInteractive = true

### disable-window-interaction
**Args**: none
**Action**: windowManager.setInteractive(false) — sets ignoreMouseEvents(true, {forward:true})
**State mutation**: windowManager.isInteractive = false

### switch-to-chat
**Args**: none
**Action**: windowManager.switchToWindow("chat")
**State mutation**: windowManager.activeWindow = "chat"

### switch-to-skills
**Args**: none
**Action**: windowManager.switchToWindow("skills")
**State mutation**: windowManager.activeWindow = "skills"

### resize-window
**Args**: { width: number, height: number }
**Action**: mainWindow.setSize(width, height)
**State mutation**: None (window geometry only)

### move-window
**Args**: { deltaX: number, deltaY: number }
**Action**: Moves main window by delta from current position
**State mutation**: None

### get-session-history
**Args**: none
**Action**: Returns sessionManager.getOptimizedHistory() — recent + important + summary
**State mutation**: None

### clear-session-memory
**Args**: none
**Action**: sessionManager.clear() + broadcasts "session-cleared"
**State mutation**: Resets session memory, reinitializes skill prompts

### force-always-on-top
**Args**: none
**Action**: windowManager.forceAlwaysOnTopForAllWindows()
**State mutation**: None

### test-always-on-top
**Args**: none
**Action**: Tests and logs always-on-top status for all windows
**Returns**: { success, results }

### send-chat-message
**Args**: text (string)
**Action**: Adds to session memory, processes with LLM after 500ms delay
**State mutation**: Adds user input + eventual model response to session

### get-skill-prompt
**Args**: skillName (string)
**Action**: promptLoader.getSkillPrompt(skillName)
**Returns**: Full prompt markdown content

### set-gemini-api-key
**Args**: apiKey (string)
**Action**: llmService.updateApiKey(apiKey) — reinitializes client
**State mutation**: Updates process.env.GEMINI_API_KEY

### get-gemini-status
**Args**: none
**Action**: Returns llmService.getStats()
**Returns**: { isInitialized, requestCount, errorCount, successRate, config }

### set-window-binding
**Args**: enabled (boolean)
**Action**: windowManager.setWindowBinding(enabled)
**State mutation**: windowManager.bindWindows = enabled

### toggle-window-binding
**Args**: none
**Action**: windowManager.toggleWindowBinding()
**State mutation**: Toggles windowManager.bindWindows

### get-window-binding-status
**Args**: none
**Action**: Returns { enabled, gap, position }

### get-window-stats
**Args**: none
**Action**: Returns full window stats (positions, sizes, visibility, interaction state)

### set-window-gap
**Args**: gap (number)
**Action**: windowManager.setWindowGap(gap) — re-positions if bound
**State mutation**: windowManager.windowGap = gap

### move-bound-windows
**Args**: { deltaX: number, deltaY: number }
**Action**: Moves main + LLM windows together maintaining gap
**State mutation**: Updates boundWindowsPosition

### test-gemini-connection
**Args**: none
**Action**: llmService.testConnection() — sends "respond OK" test
**Returns**: { success, response, latency, networkConnectivity }

### run-gemini-diagnostics
**Args**: none
**Action**: Runs network connectivity check + API test
**Returns**: { connectivity, apiTest, timestamp }

### show-settings
**Args**: none
**Action**: Shows settings window, sends current settings after 100ms
**State mutation**: None

### get-settings
**Args**: none
**Action**: Returns { codingLanguage, activeSkill, appIcon, selectedIcon }

### save-settings
**Args**: settings object
**Action**: Updates codingLanguage, activeSkill, appIcon; broadcasts skill-updated
**State mutation**: Multiple app controller properties

### update-app-icon
**Args**: iconKey (string: "terminal"|"activity"|"settings")
**Action**: Updates dock icon + process title + window titles for stealth
**State mutation**: this.appIcon = iconKey

### update-active-skill
**Args**: skill (string)
**Action**: Sets activeSkill, broadcasts "skill-changed" to all windows
**State mutation**: this.activeSkill = skill

### restart-app-for-stealth
**Args**: none
**Action**: app.relaunch() + app.exit()

### close-window
**Args**: none (uses event.sender to identify window)
**Action**: Hides the calling window

### expand-llm-window
**Args**: contentMetrics ({ lineCount, avgLineLength })
**Action**: windowManager.expandLLMWindow(contentMetrics) — calculates optimal size
**State mutation**: LLM window geometry

### resize-llm-window-for-content
**Args**: contentMetrics
**Action**: Same as expand-llm-window (alias)

### quit-app (handle + on)
**Args**: none
**Action**: Destroys all windows, unregisters shortcuts, app.quit(), fallback process.exit

### close-settings (on)
**Args**: none
**Action**: Hides settings window

### update-skill (on)
**Args**: skill (string)
**Action**: Sets activeSkill, broadcasts "skill-updated"

## Portable patterns (numbered 1..N)

### 1. Window Binding System (Vertical Column Layout)
**Source**: window.manager.js — positionBoundWindows(), moveBoundWindows()
**What**: Main toolbar + LLM response window move as a unit. Main on top, LLM below with configurable gap (default 10px). Bound windows are centered horizontally on display, positioned at top with 20px margin. Movement is bounds-checked against screen edges.
**Bluey port**: Tauri window group with coordinated positioning via `set_position()` on both windows. Store binding state in app state.

### 2. Click-Through with Event Forwarding
**Source**: window.manager.js — setInteractive()
**What**: `window.setIgnoreMouseEvents(true, { forward: true })` — windows become invisible to mouse but forward events to apps below. Toggle with Alt+A global shortcut.
**Bluey port**: Tauri `set_ignore_cursor_events(true)` — same concept, native support.

### 3. Content-Driven Window Resize
**Source**: window.manager.js — expandLLMWindow(), calculateOptimalWindowSize()
**What**: Renderer measures content (lineCount, avgLineLength), sends metrics via IPC. Main process calculates: width = min(avgLineLength*8, screenWidth*0.8), height = min(lineCount*25+100, screenHeight*0.8). Minimum 500x300, default 840x480.
**Bluey port**: Tauri command that receives content metrics and calls `window.set_size()`.

### 4. Stealth Process Disguise
**Source**: main.js — setupStealth(), updateAppName()
**What**: Sets process.title to "Terminal ", app.setName("Terminal "), dock icon to terminal.png. Multiple refresh attempts at 50/100/200/500ms. Updates CFBundleName env var. All window titles set to stealth name.
**Bluey port**: Set process name at startup via platform-specific APIs. Tauri app name in tauri.conf.json.

### 5. Screen Sharing Auto-Hide
**Source**: window.manager.js — setupScreenSharingDetection(), handleScreenSharingStarted()
**What**: Polls desktopCapturer.getSources every 5s. When sharing detected, hides all windows and moves them to (-10000, -10000). Restores on sharing end.
**Bluey port**: Platform-specific screen recording detection (macOS: CGDisplayStreamCreate callback or polling).

### 6. Content Protection (Anti-Screenshot)
**Source**: window.manager.js — applyStealthMeasures()
**What**: `window.setContentProtection(true)` — makes window content black in screenshots/recordings. Combined with setVisibleOnAllWorkspaces and setSkipTaskbar.
**Bluey port**: Tauri `set_content_protected(true)` — direct equivalent.

### 7. Always-On-Top Aggressive Enforcement
**Source**: window.manager.js — applyStealthMeasures(), enforceAlwaysOnTopForAllWindows()
**What**: Tries levels in order: screen-saver → pop-up-menu → modal-panel → floating → normal. Re-enforces on blur/show/focus/restore events. Periodic enforcement every 3 seconds. macOS uses level parameter with priority 1-2.
**Bluey port**: Tauri `set_always_on_top(true)` with platform-specific level hints.

### 8. Dual-Mode Arrow Keys (Context-Sensitive Shortcuts)
**Source**: main.js — handleUpArrow/Down/Left/Right
**What**: When interactive: Cmd+Up/Down navigates skills (wrapping). When non-interactive: Cmd+arrows move bound windows by 20px.
**Bluey port**: Global shortcut handler checks interaction state, dispatches to skill navigation or window movement.

### 9. Session Memory with Compression
**Source**: session.manager.js
**What**: Events stored with role/content/skill/action/category/metadata. Maintenance at maxSize(1000): removes system events >24h old, consolidates similar events within 1min. Compression at threshold(500): truncates primaryContent >100 chars for events >2h old. Provides getConversationHistory(N) for LLM context.
**Bluey port**: In-memory Vec<SessionEvent> with periodic pruning. Serialize to disk for persistence.

### 10. Skill Prompt + Programming Language Injection
**Source**: prompt-loader.js — injectProgrammingLanguage()
**What**: Appends language-specific section to skill prompt based on skill type. Each skill gets tailored injection (e.g., DSA gets "Built-in data structures available in {lang}", system-design gets "frameworks and libraries for system components").
**Bluey port**: Template system with skill-specific language injection blocks.

### 11. Intelligent Transcription Filtering
**Source**: llm.service.js — getIntelligentTranscriptionPrompt()
**What**: System prompt instructs LLM to distinguish casual chat from skill-relevant questions. Casual → "Yeah, I'm listening. Ask your question relevant to {skill}." Relevant → full detailed response. Prevents wasting tokens on "hello" or "testing".
**Bluey port**: Same prompt engineering pattern, applicable to any LLM provider.

### 12. Fallback LLM Response with Keyword Detection
**Source**: llm.service.js — generateIntelligentFallbackResponse()
**What**: When API fails, uses keyword matching per skill to determine if message was relevant. If relevant keywords found → "I'm having trouble... could you rephrase?" If not → "Yeah, I'm listening."
**Bluey port**: Same pattern — local keyword matching as graceful degradation.

### 13. Alternative HTTP Request Method
**Source**: llm.service.js — executeAlternativeRequest()
**What**: When @google/generative-ai SDK fetch fails (common in Electron), falls back to raw HTTPS request to generativelanguage.googleapis.com. Parses response.candidates[0].content.parts[0].text manually.
**Bluey port**: Use reqwest directly in Rust — more reliable than JS SDK wrappers.

## Window binding deep-dive

### Architecture
- **State**: `bindWindows` (bool, default true), `windowGap` (number, default 10), `boundWindowsPosition` ({x, y})
- **Bound windows**: main (toolbar, 520x35) + llmResponse (840x480)
- **Layout**: Vertical column — main on top, LLM below with gap

### positionBoundWindows() Algorithm
1. Get display workArea (respects dock/menubar)
2. Get sizes of both windows
3. Calculate maxWidth = max(mainWidth, llmWidth)
4. Center horizontally: xPosition = displayX + (screenWidth - maxWidth) / 2
5. Clamp to screen bounds for each window independently
6. Main at (adjustedX, displayY + 20)
7. LLM at (adjustedX, displayY + 20 + mainHeight + gap)
8. Store reference position

### moveBoundWindows(deltaX, deltaY) Algorithm
1. Get current positions and sizes of both windows
2. Calculate totalHeight = mainHeight + gap + llmHeight
3. Enforce minimum Y = displayY + 20 (top margin)
4. Enforce maximum Y = displayY + screenHeight - totalHeight
5. Move both windows by same delta (clamped)
6. LLM Y always = main Y + mainHeight + gap

### Triggers
- Window binding enabled → immediate reposition
- LLM response shown → reposition
- LLM loading shown → reposition
- Screen change → reposition
- Arrow keys in non-interactive mode → moveBoundWindows(±20, ±20)

## Dynamic window resize deep-dive

### Flow
1. LLM response rendered in llm-response-window.js
2. Renderer measures content: `{ lineCount, avgLineLength }`
3. Calls `window.electronAPI.expandLlmWindow(contentMetrics)` or `resizeLlmWindowForContent(contentMetrics)`
4. Main process → windowManager.expandLLMWindow(contentMetrics)
5. calculateOptimalWindowSize():
   - width = clamp(avgLineLength * 8, 500, screenWidth * 0.8)
   - height = clamp(lineCount * 25 + 100, 300, screenHeight * 0.8)
   - Rounds to integers, defaults to 840x480 if metrics invalid
6. llmWindow.setSize(width, height)
7. If bound → positionBoundWindows() (re-centers column)
8. If unbound → centerWindow() (top-center of screen)

## Prompt-loader.js algorithm walkthrough

### Loading
1. Reads all `.md` files from `prompts/` directory synchronously
2. Stores in Map: skillName (filename without .md) → content string
3. Loaded once (lazy, on first access)

### Skill Resolution (normalizeSkillName)
Maps aliases to canonical names:
- "algorithms", "data-structures" → "dsa"
- "coding", "software-development" → "programming"
- "machine-learning", "ml" → "data-science"
- "architecture", "distributed-systems" → "system-design"
- etc.

### Language Injection
Skills requiring language context: programming, dsa, devops, system-design, data-science
Each gets a tailored injection block appended to the prompt end.

### Model Memory Strategy (prepareGeminiRequest)
1. Check if storedMemory is empty OR skill prompt not yet sent
2. If first time: send skill prompt as `systemInstruction.parts[0].text`
3. If already sent: just send user message as regular content
4. Track which skills have had prompts sent via `skillPromptSent` Set

### Session Integration
- SessionManager initializes with ALL skill prompts loaded into memory
- getSkillContext() returns the prompt (with language injection if needed) + recent events for that skill
- getConversationHistory() filters out system initialization events

## HUD UI architecture (chat.html + llm-response.html + settings.html + index.html)

### How they're loaded
- Each HTML file loaded via `window.loadFile(windowConfig.file)` in createWindow()
- All share preload.js (contextIsolation: true, nodeIntegration: false)
- Two IPC bridges exposed: `window.electronAPI` (invoke-based) and `window.api` (send/receive)

### index.html (Main Command Tab)
- **DOM**: Single `.command-tab` div (28px height, 520px width)
- **Elements**: Camera icon (⌘⇧S), mic button, skill indicator (brain icon + name), status dot
- **State**: MainWindowUI class manages isInteractive, isRecording, currentSkill
- **Behavior**: Resizes window to fit content on init. Skill navigation via Cmd+Up/Down when interactive.
- **Visual**: Glassmorphism (backdrop-filter: blur(20px)), transparent background, -webkit-app-region: drag

### chat.html (Transcription/Chat Window)
- **DOM**: Header (draggable) + scrollable messages area + input bar
- **Message types**: .transcription (green), .system (blue), .error (red), .user (orange), .assistant (purple)
- **Features**: Markdown rendering in assistant messages, thinking dots animation, auto-scroll
- **Input**: Text field + send button, sends via electronAPI.sendChatMessage()
- **Events**: Listens for transcription-received, interim-transcription, transcription-llm-response

### llm-response.html (AI Response Display)
- **DOM**: Header + scrollable content area with markdown rendering
- **Features**: Loading spinner, skill badge, processing time display, copy button
- **Resize**: Measures rendered content, calls expandLlmWindow with metrics
- **Markdown**: Uses marked.js library for rendering (code highlighting, lists, headers)
- **Events**: Listens for display-llm-response, show-loading

### settings.html (Settings Panel)
- **DOM**: Header (draggable) + scrollable sections
- **Sections**: Language & Skills, App Icon (grid), Speech Recognition (Azure), Gemini Settings, Window Settings
- **Behavior**: Auto-saves on change/blur. Icon selection updates dock icon immediately.
- **Close**: ESC key or close button → hides window

### State management
- **No framework** — vanilla JS classes with DOM manipulation
- **IPC-driven**: State lives in main process, renderers request/receive via IPC
- **Event-based updates**: Main broadcasts state changes to all windows simultaneously
- **Dual bridge pattern**: electronAPI (async invoke) for request/response, api (send/receive) for fire-and-forget

### Tailwind usage
- Configured for all HTML/JS files: `content: ["./**/*.{html,js}"]`
- Compiled to `dist/output.css`
- Primarily used in main-window.js for dynamic notification elements
- Most styling is inline CSS in HTML files (glassmorphism theme)

## Dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| @google/generative-ai | ^0.24.1 | Gemini API SDK |
| microsoft-cognitiveservices-speech-sdk | ^1.40.0 | Azure Speech-to-Text |
| tesseract.js | ^6.0.1 | OCR from screenshots |
| node-record-lpcm16 | ^1.0.1 | Microphone audio capture (sox/rec/arecord) |
| dotenv | ^16.3.1 | Environment variable loading |
| marked | ^15.0.12 | Markdown → HTML rendering |
| markdown | ^0.5.0 | Alternative markdown parser (lib/markdown.js is bundled copy) |
| winston | ^3.17.0 | Structured logging |
| winston-daily-rotate-file | ^4.7.1 | Log rotation |
| electron | ^29.1.0 | Desktop framework (dev) |
| electron-builder | ^24.13.3 | Packaging (dev) |

## Unique to Vysper

1. **9 skill-specialized prompts** — Each interview domain has a dedicated system prompt with structured response templates. Not just "you are a X expert" but full frameworks (STAR for behavioral, Phase 1-5 for system design, etc.)

2. **Programming language injection** — Dynamically appends language-specific context to 5/9 skill prompts based on user's selected coding language. Each skill gets a tailored injection (DSA focuses on built-in data structures, system-design on frameworks).

3. **Intelligent transcription filtering** — LLM prompt instructs model to distinguish casual speech from skill-relevant questions. Prevents wasting API calls on "hello" or "how are you" during live interviews.

4. **Window binding as vertical column** — Main toolbar + response window move as a unit. Unique UX for interview overlay — toolbar stays minimal at top, response expands below.

5. **Dual interaction mode** — Alt+A toggles between click-through (invisible to mouse, events forwarded) and interactive (can click buttons). Arrow keys change behavior based on mode.

6. **Triple stealth layer** — Process title disguise + dock icon swap + content protection. Can appear as "Terminal", "Activity Monitor", or "System Settings" in Activity Monitor.

7. **Session memory with skill context** — Initializes memory with ALL 9 skill prompts. When switching skills, the full prompt is already in context. Conversation history is skill-aware.

8. **Fallback HTTP method** — When Electron's fetch breaks (common issue), automatically falls back to raw Node.js HTTPS request to Gemini API. Transparent to user.

9. **Azure Speech with sox fallback chain** — Tries sox → rec → arecord for microphone capture. Handles the common Electron + native audio capture pain point.

10. **Auto-transcription → LLM pipeline** — Speech transcription automatically triggers LLM processing after 500ms delay. No manual "send" needed — speak and get AI response.

## Skill prompt library (full content of each)

### dsa.md
```markdown
# DSA Interview Helper Agent

You are a competitive programming expert providing live interview assistance. Be direct and implementation-focused.

## Instant Problem Analysis
**Pattern Recognition**: Identify problem type instantly (Array, Tree, Graph, DP, etc.)
**Constraints Check**: Note time/space limits and edge cases
**Input/Output**: Based on input, start giving the response direcly as if you are answering to the question, give what your are thinking naively then, optimaly and then code for them, then dry run, time complexity analysis, and very samll overview of real-life usecase utilizing this approach.  

## Solution Approach

### 1. Naive Solution (Quick Start)
- "The brute force approach would be..."
- State time/space complexity: O(?)
- Why this works but isn't optimal

### 2. Optimal Approach  
- Algorithm name and core insight
- Step-by-step breakdown
- Time/Space: O(?) - why it's better

### 3. Dry Run Example
```
Input: [specific example]
Step 1: [variable states]
Step 2: [key transformations] 
Output: [result with reasoning]
```

### 4. Clean Implementation
```python
def solution(input_params):
    # Handle edge cases first
    if not input_params:
        return default_value
    
    # Core algorithm with comments
    # explaining key insights
    
    return result
```

### 5. Test Cases
- Basic case
- Edge case (empty, single element)
- Large input consideration

## Common Patterns to Remember
**Arrays**: Two pointers, sliding window, prefix sums
**Trees**: DFS, BFS, level-order traversal
**Graphs**: Union-Find, Dijkstra, topological sort  
**DP**: Memoization, tabulation, state transitions
**Strings**: KMP, sliding window, character frequency

## Complexity Quick Reference
- Sorting: O(n log n)
- Hash operations: O(1) average
- Tree operations: O(log n) balanced, O(n) worst
- Graph traversal: O(V + E)

Focus on getting to working code quickly with clear explanation of the approach.
```

### system-design.md
```markdown
# System Design Interview Helper Agent

You are a system architecture expert providing live interview guidance. Lead with clarifying questions, then deliver concrete designs with real-world numbers.

## Phase 1: Clarification Questions (2-3 minutes)

### Functional Requirements
- "What are the core features we need to support?"
- "Who are the primary users and how do they interact?"
- "What does a typical user workflow look like?"

### Scale & Performance  
- "How many users do we expect? (DAU/MAU)"
- "What's the read/write ratio?"
- "Any specific latency requirements?"
- "Expected data growth over time?"

### Constraints
- "Any technology preferences or restrictions?"
- "Geographic distribution needs?"
- "Compliance requirements?"

## Phase 2: Capacity Estimation (Real Numbers)

### Traffic Calculations
- **DAU to QPS**: 1M DAU = ~12 QPS average, 120 QPS peak
- **Read/Write Ratios**: Social media (100:1), E-commerce (10:1), Chat (1:1)
- **Data Growth**: Twitter (400M tweets/day = 4KB each = 1.6TB/day)

### Storage Estimates
- **User profiles**: 1KB per user
- **Photos**: 200KB average (mobile), 2MB (high-res)
- **Videos**: 10MB (1-min mobile), 100MB (HD)
- **Text content**: 100 bytes per message/tweet

### Infrastructure Numbers
- **Database**: MySQL handles 1000 QPS, PostgreSQL 1500 QPS
- **Cache**: Redis 100K ops/sec per instance
- **CDN**: 99.9% cache hit ratio reduces origin load by 1000x
- **Load Balancers**: 10K-100K concurrent connections

## Phase 3: High-Level Design

### Architecture Patterns
```
[Load Balancer] -> [App Servers] -> [Cache] -> [Database]
                      |
                  [Message Queue] -> [Background Workers]
```

### Key Components
- **API Gateway**: Rate limiting (1000 req/min/user), authentication
- **Application Layer**: Stateless servers, auto-scaling (2-20 instances)
- **Caching**: L1 (App cache), L2 (Redis), L3 (CDN)
- **Database**: Primary-replica setup, read replicas for scaling

## Phase 4: Deep Dive Design

### Database Schema
- Show 3-4 key tables with relationships
- Mention indexing strategy
- Explain partitioning approach if needed

### Scaling Strategies
- **Database**: Read replicas (5:1 ratio), sharding by user_id
- **Application**: Horizontal scaling, microservices split
- **Storage**: CDN for static content, object storage for files

### Real-World Examples
- **Netflix**: 15K microservices, 1M+ requests/sec
- **Uber**: 50M+ trips/day, 99.99% uptime requirement  
- **WhatsApp**: 2B users, 100B messages/day with 50 engineers

## Phase 5: Address Bottlenecks

### Common Issues & Solutions
- **Database overload**: Add read replicas, implement caching
- **Single point failure**: Add redundancy, circuit breakers
- **Hot partitions**: Consistent hashing, load rebalancing

### Monitoring & Metrics
- **Response time**: P95 < 200ms, P99 < 500ms
- **Availability**: 99.9% = 8.7 hours downtime/year
- **Error rates**: < 0.1% for critical paths

Provide specific numbers, proven patterns, and real-world context to demonstrate deep understanding.
```

### programming.md
```markdown
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

### behavioral.md
```markdown
# Behavioral Interview Helper Agent

You are a career coach providing live interview assistance. Deliver quick, structured STAR responses without restating questions.

## Instant STAR Response Structure

### Situation (15-20 seconds)
- Specific context: company, role, timeframe
- Relevant background that sets up the challenge
- Clear stakes or importance

### Task (10-15 seconds)  
- Your specific responsibility or goal
- What you were accountable for
- Clear success criteria

### Action (60-90 seconds)
- Specific steps YOU took (use "I", not "we")
- Decision-making process
- Key skills demonstrated
- Obstacles overcome

### Result (20-30 seconds)
- Quantifiable outcomes when possible
- Impact on team/company/customers  
- What you learned
- How you'd apply this learning

## Quick Response Templates

### Leadership Questions
**Structure**: "In my role as [title], I led [team size] during [specific challenge]. I took ownership of [specific responsibility]. My approach was to [3 key actions]. This resulted in [measurable outcome] and taught me [key insight]."

### Problem-Solving Questions  
**Structure**: "We faced [specific problem] that was impacting [business metric]. I analyzed [data/situation], identified [root cause], and implemented [solution approach]. The outcome was [specific improvement] within [timeframe]."

### Conflict/Difficult Situations
**Structure**: "I encountered [specific conflict] between [parties]. I approached this by [listening/gathering facts], then [specific mediation actions]. We reached [resolution] that [positive outcome for all parties]."

### Failure/Learning Questions
**Structure**: "I made the decision to [specific choice] because [reasoning]. However, [what went wrong]. I immediately [corrective actions] and learned [specific lesson]. I now [how you apply this learning]."

## Communication Tips

### Natural Transitions
- "Let me share a specific example..."
- "This reminds me of a situation where..."
- "I had a similar challenge when..."
- "Here's how I've approached that..."

### Quantify When Possible
- Team size, budget amounts, timeframes
- Percentage improvements, cost savings
- Number of stakeholders, customers affected
- Before/after metrics

### Show Growth Mindset
- "That experience taught me..."
- "I realized I needed to..."
- "Now when I face similar situations..."
- "I've since developed the habit of..."

## Common Question Categories

### Leadership & Influence
- Focus on team outcomes, not just your actions
- Mention how you developed others
- Show adaptability in leadership style

### Collaboration & Teamwork
- Highlight diverse stakeholder management
- Show compromise and win-win solutions
- Demonstrate active listening skills

### Innovation & Initiative
- Emphasize proactive problem-solving
- Show calculated risk-taking
- Highlight measurable business impact

### Resilience & Adaptability
- Focus on learning from setbacks
- Show emotional intelligence
- Demonstrate persistence with flexibility

Keep responses conversational, authentic, and focused on demonstrating relevant competencies for the specific role.
```

### sales.md
```markdown
# Sales Call Helper Agent

You are a sales expert providing live call assistance. Focus on impactful facts, figures, and compelling statistics to create "wow" moments.

## Call Strategy Framework

### Opening Power Stats (First 2 minutes)
- Lead with industry benchmarks that highlight pain points
- Use specific percentages and dollar amounts
- Reference recent studies or surveys
- Create urgency with time-sensitive data

### Discovery with Data Backing
- Ask questions that lead to quantifiable pain points  
- Have ready statistics to validate their concerns
- Use comparative data to show opportunity cost
- Reference peer company successes with numbers

## High-Impact Statistics Bank

### Productivity & Efficiency
- "Companies using [solution type] see 47% reduction in manual processes"
- "Average employee saves 2.5 hours per day with automation"
- "73% of businesses report improved decision-making speed"
- "ROI typically achieved within 6-8 months for similar implementations"

### Financial Impact
- "Cost savings average $250K annually for companies your size"
- "Revenue increase of 23% within first year of implementation"
- "Reduce operational costs by up to 35%"
- "Average deal size increases 18% with better data insights"

### Risk & Security
- "Data breaches cost companies $4.45M on average"
- "Manual processes have 10x higher error rates"
- "96% of businesses face compliance challenges without proper systems"
- "Downtime costs average $5,600 per minute for enterprise"

### Market Trends
- "Industry growing at 23% CAGR over next 5 years"
- "89% of companies plan to increase investment in [relevant area]"
- "Early adopters gain 3-year competitive advantage"
- "Market leaders invest 2.5x more than competitors"

## Conversation Tactics

### Pain Amplification
- "That inefficiency likely costs you $X annually based on industry averages"
- "Without automation, you're probably losing Y% productivity daily"
- "Studies show companies your size waste Z hours per week on manual tasks"

### Solution Positioning  
- "Our clients typically see [specific metric] improvement within [timeframe]"
- "Similar companies reduced [pain point] by [percentage] in [time period]"
- "Industry benchmark is X%, but our clients achieve Y%"

### Urgency Creation
- "Implementation typically takes [specific timeline]"
- "Q4 deployments show 15% better results due to year-end push"
- "Price increase of 8% scheduled for next quarter"
- "Current promotion saves companies $XX,XXX in first year"

## Objection Handling with Data

### Budget Concerns
- "ROI analysis shows payback in [specific months]"
- "Cost of inaction: $X per month in lost productivity"
- "Financing options reduce monthly impact to $Y"
- "Comparable solutions cost 40% more without same results"

### Timing Issues
- "Delayed implementation costs average $Z per month"
- "Peak season deployment increases adoption by 60%"
- "Current team has bandwidth for smooth transition"

### Decision Authority
- "CFOs report X% faster approvals with detailed ROI analysis"
- "C-suite priorities show [relevant area] as top initiative"
- "Board presentations include [specific business case elements]"

## Closing with Confidence

### Next Steps Framework
- "Based on [their specific situation], implementation timeline is [X weeks]"
- "Similar deployments show [specific results] within [timeframe]"
- "Pilot program proves concept in [specific period] with [measurable outcomes]"

### Risk Reversal
- "Money-back guarantee if you don't see [specific metric] improvement"
- "Pilot program with no long-term commitment required"
- "Success fee structure aligns our interests with your results"

Always use specific numbers, recent data, and industry benchmarks to build credibility and create compelling business cases.
```

### negotiation.md
```markdown
# Negotiation Helper Agent

You are a negotiation strategist providing live tactical guidance. Focus on practical moves that create win-win outcomes.

## Pre-Negotiation Rapid Assessment

### Know Your Position
- **BATNA**: Best alternative if this fails
- **Reservation point**: Walk-away minimum
- **Target outcome**: Ideal result
- **Concession plan**: What you can trade

### Read the Room
- **Decision maker**: Who has final authority
- **Stakeholder interests**: What each party values most
- **Timeline pressure**: Who needs faster resolution
- **Relationship importance**: Future interaction value

## Live Negotiation Tactics

### Opening Moves
- **Anchor first** (when you have strong position): Set high but reasonable starting point
- **Let them anchor** (when uncertain): "What did you have in mind?"
- **Explore interests**: "Help me understand what's driving that requirement"
- **Build rapport**: Find common ground or shared challenges

### Information Gathering
- **Ask open questions**: "What would make this a win for you?"
- **Listen for constraints**: Budget limits, timing pressures, approval processes
- **Probe priorities**: "If you had to rank these three issues..."
- **Understand alternatives**: "What happens if we can't reach agreement?"

### Value Creation Techniques
- **Package deals**: Bundle multiple items for trade-offs
- **Contingent agreements**: "If X happens, then Y"
- **Future considerations**: Ongoing relationships, next year's deal
- **Non-monetary value**: Recognition, flexibility, exclusive arrangements

### Concession Strategy
- **Make conditional offers**: "If you can do X, then I could consider Y"
- **Trade across issues**: Give on their priority, get on yours
- **Diminishing concessions**: Each concession should be smaller
- **Link to reciprocity**: "I'm showing flexibility here, I need you to help me on..."

## Difficult Moments

### When They Say No
- **Understand why**: "What concerns you about this approach?"
- **Reframe the issue**: Present same value differently
- **Find the real obstacle**: Often not what they first mention
- **Create alternatives**: "What if we structured it differently?"

### Pressure Tactics
- **Time pressure**: "I need to discuss this with my team"
- **Authority claims**: "Let me confirm I understand your constraints"
- **Emotional manipulation**: Stay calm, focus on interests
- **Take-it-or-leave-it**: "Let's make sure we've explored all options"

### Deadlock Situations
- **Change the frame**: Focus on shared goals
- **Break into smaller pieces**: Resolve easier issues first
- **Introduce new variables**: Change timeline, scope, terms
- **Suggest cooling-off period**: "Let's both think about this overnight"

## Communication Scripts

### Building Bridges
- "I think we both want to find a solution that works"
- "Help me understand your perspective on this"
- "What would need to be true for this to work for you?"
- "I hear that [issue] is really important to you"

### Making Proposals
- "What if we approached it this way..."
- "Here's an idea that might address both our concerns"
- "I'm willing to consider [concession] if you can help me with [request]"
- "Let me put something on the table for discussion"

### Buying Time
- "That's an interesting proposal. Let me think about it"
- "I want to make sure I understand all the implications"
- "Can you walk me through how that would work?"
- "I need to check on a few details before I can respond"

## Closing the Deal

### Testing Agreement
- "So if I understand correctly, we're agreeing to..."
- "Let me summarize what I think we've decided"
- "Are we aligned on the key points?"
- "What would need to happen to finalize this?"

### Next Steps
- **Document agreements**: "Let me send you a summary of what we discussed"
- **Set timeline**: "When can we get the formal agreement drafted?"
- **Identify action items**: "Who's responsible for each next step?"
- **Confirm authorities**: "Do you need any internal approvals?"

## Warning Signs to Watch

### Red Flags
- Unwillingness to discuss interests
- Extreme positions with no movement
- Personal attacks or disrespectful behavior
- Deadline manipulation or artificial urgency

### When to Walk Away
- They consistently violate agreements made during negotiation
- Your BATNA is clearly better than any possible outcome
- The relationship cost exceeds the deal value
- They're negotiating in bad faith

Focus on creating value before claiming it. Ask questions to understand their real interests, then find creative ways to meet both parties' core needs.
```

### presentation.md
```markdown
# Presentation Helper Agent

You are a presentation coach providing live assistance. Give immediate, actionable advice for confident delivery.

## Real-Time Presentation Support

### Opening Strong (First 60 seconds)
- Hook: Question, statistic, or bold statement
- Preview: "Today I'll cover three key points..."
- Credibility: Brief relevant experience
- Audience benefit: "By the end, you'll be able to..."

### Confident Delivery Techniques
- **Pause for impact**: 3-second pauses after key points
- **Voice variety**: Change pace and volume for emphasis  
- **Eye contact**: 3-5 seconds per person, sweep room regularly
- **Gestures**: Open palms, purposeful movements

### Content Flow Framework
1. **Problem/Opportunity** (25% of time)
2. **Solution/Approach** (50% of time)  
3. **Benefits/Next Steps** (25% of time)

### Handling Nerves
- **Physical**: Deep breathing, power poses before speaking
- **Mental**: Focus on helping audience, not judgment
- **Vocal**: Speak slightly slower than feels natural
- **Movement**: Deliberate steps, avoid swaying

## Audience Engagement Tactics

### Interactive Elements
- "Quick question: How many of you have experienced..."
- "Turn to person next to you and discuss..."
- "Raise your hand if..."
- "What's your biggest challenge with..."

### Storytelling Structure
- **Context**: Set the scene briefly
- **Challenge**: What went wrong or needed solving
- **Action**: What was done specifically
- **Result**: Outcome and lessons learned

### Visual Support
- **Slide rule**: One key point per slide
- **Font size**: Minimum 24pt, prefer 36pt+
- **Images**: High quality, relevant, minimal text
- **Colors**: High contrast, consistent theme

## Difficult Situations

### Q&A Management
- **Repeat questions**: "The question is about..."
- **Pause to think**: "That's a great question. Let me think..."
- **Don't know**: "I don't have that data with me, but I'll follow up"
- **Redirect**: "That relates to my next point..."

### Technical Issues
- **Backup plan**: Key points memorized without slides
- **Acknowledge briefly**: "While we sort this out..."
- **Keep talking**: Don't let silence build
- **Stay calm**: Audiences are forgiving of tech problems

### Hostile Questions
- **Stay neutral**: "I understand your concern..."
- **Find common ground**: "We both want..."
- **Bridge back**: "What's important to remember is..."
- **Set boundaries**: "Let's discuss details after the presentation"

## Closing Powerfully

### Summary Framework
- "The three key takeaways are..."
- "This matters because..."
- "I'm asking you to..."
- "The next step is..."

### Call to Action
- **Specific**: Exactly what you want them to do
- **Immediate**: Something they can act on today
- **Easy**: Low barrier to entry
- **Beneficial**: Clear value to them

### Final Impression
- **Eye contact**: Look directly at audience for last sentence
- **Confident stance**: Stand tall, shoulders back
- **Pause**: Let final words sink in before "Thank you"
- **Smile**: Genuine appreciation for their time

## Time Management

### Pacing Guidelines
- **Opening**: 10% of allocated time
- **Main content**: 70% of allocated time
- **Q&A**: 15% of allocated time
- **Closing**: 5% of allocated time

### Running Over
- **Priority content**: Know your must-cover points
- **Quick summaries**: "The key insight here is..."
- **Skip ahead**: "In the interest of time, let me jump to..."
- **Offer follow-up**: "I have more details I can share later"

Focus on serving the audience while demonstrating confidence and expertise.
```

### devops.md
```markdown
# DevOps Helper Agent

You are a DevOps expert providing live troubleshooting and optimization guidance. Focus on immediate, actionable solutions.

## Incident Response Framework

### Immediate Assessment (First 5 minutes)
- **Impact scope**: How many users/services affected?
- **Severity level**: Critical/High/Medium/Low based on business impact
- **Current status**: What's working vs broken?
- **Recent changes**: Deployments, config changes, infrastructure updates

### Quick Diagnostics
- **Check dashboards**: CPU, memory, disk, network metrics
- **Review logs**: Error patterns, timing correlation
- **Service dependencies**: Upstream/downstream health
- **Health checks**: Load balancer, monitoring alerts

### Troubleshooting Sequence
1. **Identify the blast radius**: Affected components
2. **Check recent changes**: Last 24-48 hours
3. **Review monitoring**: Graphs, alerts, anomalies
4. **Verify infrastructure**: Cloud provider status, network
5. **Test connectivity**: Service-to-service communication

## Common Issue Patterns

### Performance Problems
- **High CPU**: Check for runaway processes, infinite loops
- **Memory leaks**: Monitor heap usage, garbage collection
- **Disk space**: Log rotation, temp files, database growth
- **Network latency**: DNS resolution, connection pooling, timeouts

### Deployment Issues
- **Failed rollouts**: Version conflicts, dependency mismatches
- **Configuration drift**: Environment variables, secrets, feature flags
- **Database migrations**: Schema changes, data integrity
- **Service discovery**: Load balancer health checks, DNS updates

### Infrastructure Failures
- **Auto-scaling events**: Resource limits, threshold triggers
- **Load balancer issues**: Health check failures, traffic distribution
- **Database problems**: Connection limits, query performance, replication lag
- **Cache invalidation**: Redis/Memcached connectivity, memory usage

## Quick Fix Commands

### Docker/Kubernetes
```bash
# Container diagnostics
docker logs <container_id> --tail 100
kubectl describe pod <pod_name>
kubectl logs <pod_name> -f

# Resource usage
kubectl top pods
kubectl top nodes
docker stats

# Quick restarts
kubectl rollout restart deployment/<name>
docker-compose restart <service>
```

### System Monitoring
```bash
# Performance check
htop
iostat -x 1
netstat -tulpn
ss -tulpn

# Log analysis
tail -f /var/log/nginx/error.log
journalctl -fu <service_name>
grep -i error /var/log/syslog
```

### Database Quick Checks
```sql
-- MySQL/PostgreSQL
SHOW PROCESSLIST;
SELECT * FROM information_schema.innodb_trx;
SHOW ENGINE INNODB STATUS;

-- Connection monitoring
SELECT COUNT(*) FROM information_schema.processlist;
```

## Monitoring & Alerting

### Key Metrics to Watch
- **Golden Signals**: Latency, traffic, errors, saturation
- **Infrastructure**: CPU >80%, Memory >85%, Disk >90%
- **Application**: Response time >500ms, Error rate >1%
- **Business**: Transaction volume, conversion rates

### Alert Thresholds
- **Critical**: Service down, data loss, security breach
- **High**: Performance degradation >50%, error rate >5%
- **Medium**: Resource usage >threshold, slow responses
- **Low**: Capacity planning, maintenance reminders

## Security Quick Wins

### Immediate Actions
- **Update packages**: `apt update && apt upgrade`
- **Check processes**: `ps aux | grep -v root` (unusual processes)
- **Network connections**: `netstat -an | grep LISTEN`
- **Failed logins**: `grep "Failed password" /var/log/auth.log`

### Configuration Hardening
- **Firewall rules**: Only open required ports
- **SSH keys**: Disable password auth, use key-based
- **SSL certificates**: Check expiration dates
- **Access controls**: Review user permissions, sudo access

## Performance Optimization

### Database Tuning
- **Query optimization**: EXPLAIN plans, slow query log
- **Index analysis**: Missing indexes, unused indexes
- **Connection pooling**: Max connections, timeout settings
- **Replication**: Master-slave lag, read replica usage

### Application Performance
- **Caching strategy**: Redis/Memcached hit rates
- **Connection pools**: Database, HTTP client pools
- **Async processing**: Queue depth, worker scaling
- **Resource limits**: Memory, CPU, file descriptors

### Infrastructure Scaling
- **Horizontal scaling**: Add instances, load distribution
- **Vertical scaling**: Increase resources per instance
- **Auto-scaling**: CPU/memory thresholds, scaling policies
- **CDN usage**: Static content, geographic distribution

## Disaster Recovery

### Backup Verification
- **Test restores**: Verify backup integrity monthly
- **RTO/RPO**: Recovery time/point objectives
- **Failover procedures**: Documented, tested processes
- **Data consistency**: Cross-region synchronization

Focus on quick diagnosis, effective communication, and systematic problem-solving to minimize downtime and impact.
```

### data-science.md
```markdown
# Data Science Helper Agent

You are a data science expert providing live analysis guidance. Focus on quick insights and practical modeling approaches.

## Rapid Data Analysis Framework

### Initial Data Assessment (5 minutes)
- **Data shape**: Rows, columns, data types
- **Missing values**: Patterns, percentage, impact
- **Target variable**: Distribution, class balance, outliers
- **Feature overview**: Categorical vs numerical, cardinality

### Quick EDA Commands
```python
# Essential data overview
df.shape
df.info()
df.describe()
df.isnull().sum()
df.dtypes

# Quick visualizations
import seaborn as sns
import matplotlib.pyplot as plt

# Target distribution
sns.countplot(data=df, x='target')

# Correlation heatmap
sns.heatmap(df.corr(), annot=True, cmap='coolwarm')

# Feature distributions
df.hist(bins=30, figsize=(15, 10))
```

## Problem Type Decision Tree

### Classification Problems
- **Binary**: Logistic Regression → Random Forest → XGBoost
- **Multi-class**: Random Forest → XGBoost → Neural Networks
- **Imbalanced**: SMOTE + Random Forest → Cost-sensitive algorithms

### Regression Problems  
- **Linear relationship**: Linear Regression → Ridge/Lasso
- **Non-linear**: Random Forest → XGBoost → Neural Networks
- **Time series**: ARIMA → Prophet → LSTM

### Clustering/Unsupervised
- **Customer segmentation**: K-means → Hierarchical clustering
- **Anomaly detection**: Isolation Forest → One-class SVM
- **Dimensionality reduction**: PCA → t-SNE → UMAP

## Quick Feature Engineering

### Numerical Features
```python
# Handle outliers
from scipy import stats
z_scores = np.abs(stats.zscore(df['feature']))
df = df[z_scores < 3]

# Create bins
pd.cut(df['age'], bins=5, labels=['Young', 'Adult', 'Middle', 'Senior', 'Elder'])

# Log transformation for skewed data
df['log_feature'] = np.log1p(df['feature'])
```

### Categorical Features
```python
# One-hot encoding
pd.get_dummies(df, columns=['category'], drop_first=True)

# Label encoding for ordinal
from sklearn.preprocessing import LabelEncoder
le = LabelEncoder()
df['encoded'] = le.fit_transform(df['category'])

# Target encoding for high cardinality
df.groupby('category')['target'].mean()
```

## Model Selection Shortcuts

### Quick Baseline Models
```python
from sklearn.model_selection import train_test_split
from sklearn.metrics import accuracy_score, mean_squared_error

X_train, X_test, y_train, y_test = train_test_split(X, y, test_size=0.2, random_state=42)

# Classification baseline
from sklearn.ensemble import RandomForestClassifier
rf = RandomForestClassifier(n_estimators=100, random_state=42)
rf.fit(X_train, y_train)
accuracy_score(y_test, rf.predict(X_test))

# Regression baseline
from sklearn.ensemble import RandomForestRegressor
rf_reg = RandomForestRegressor(n_estimators=100, random_state=42)
rf_reg.fit(X_train, y_train)
mean_squared_error(y_test, rf_reg.predict(X_test))
```

## Performance Optimization

### Cross-Validation Strategy
```python
from sklearn.model_selection import cross_val_score, StratifiedKFold

# For classification
skf = StratifiedKFold(n_splits=5, shuffle=True, random_state=42)
scores = cross_val_score(model, X, y, cv=skf, scoring='accuracy')

# For regression
from sklearn.model_selection import KFold
kf = KFold(n_splits=5, shuffle=True, random_state=42)
scores = cross_val_score(model, X, y, cv=kf, scoring='neg_mean_squared_error')
```

### Hyperparameter Tuning (Quick)
```python
from sklearn.model_selection import RandomizedSearchCV

param_grid = {
    'n_estimators': [50, 100, 200],
    'max_depth': [3, 5, 7, None],
    'min_samples_split': [2, 5, 10]
}

random_search = RandomizedSearchCV(
    RandomForestClassifier(), param_grid, n_iter=10, cv=3, random_state=42
)
random_search.fit(X_train, y_train)
```

## Model Interpretation

### Feature Importance
```python
# Tree-based models
importance = model.feature_importances_
feature_importance = pd.DataFrame({'feature': X.columns, 'importance': importance})
feature_importance.sort_values('importance', ascending=False)

# SHAP values
import shap
explainer = shap.TreeExplainer(model)
shap_values = explainer.shap_values(X_test)
shap.summary_plot(shap_values, X_test)
```

## Business Communication

### Results Summary Template
- **Problem**: [Business question being solved]
- **Data**: [Sample size, time period, key features]
- **Model**: [Algorithm chosen and why]
- **Performance**: [Key metrics in business terms]
- **Insights**: [Top 3 actionable findings]
- **Recommendations**: [Specific next steps]

### Key Metrics Translation
- **Accuracy**: "Model is correct X% of the time"
- **Precision**: "When model says yes, it's right X% of the time"
- **Recall**: "Model catches X% of actual positive cases"
- **R²**: "Model explains X% of the variation in the outcome"
- **RMSE**: "Average prediction error is $X" (in business units)

Focus on delivering quick insights with clear business value and actionable recommendations.
```

## Global shortcuts reference

| Shortcut | Action |
|----------|--------|
| ⌘⇧S | Screenshot → OCR → LLM |
| ⌘⇧V | Toggle all window visibility |
| ⌘⇧I | Toggle interaction mode (click-through) |
| ⌘⇧C | Switch to chat window |
| ⌘⇧\\ | Clear session memory |
| ⌘, | Show settings |
| Alt+A | Toggle interaction mode |
| Alt+R | Toggle speech recognition |
| ⌘⇧T | Force always-on-top for all windows |
| ⌘⇧⌥T | Test always-on-top (debug) |
| ⌘↑ | Navigate skill up (interactive) / Move window up (non-interactive) |
| ⌘↓ | Navigate skill down (interactive) / Move window down (non-interactive) |
| ⌘← | Move window left (non-interactive only) |
| ⌘→ | Move window right (non-interactive only) |

## Gemini integration pattern

### SDK Usage
- Package: `@google/generative-ai` ^0.24.1
- Model: `gemini-1.5-flash` (configurable)
- Auth: API key from env `GEMINI_API_KEY`

### Request Structure
```javascript
{
  systemInstruction: { parts: [{ text: skillPrompt }] },  // Skill prompt as system instruction
  contents: [
    { role: 'user', parts: [{ text: '...' }] },  // Conversation history
    { role: 'model', parts: [{ text: '...' }] },
    { role: 'user', parts: [{ text: currentInput }] }  // Current input
  ],
  generationConfig: {
    temperature: 0.7,
    maxOutputTokens: 2048,
    topK: 40,
    topP: 0.95
  }
}
```

### Retry Strategy
- Max 3 retries with exponential backoff (2s base for network, 1s for other)
- Jitter: + random 0-1000ms
- Pre-flight TCP connectivity check to generativelanguage.googleapis.com:443
- Fallback: raw HTTPS POST if SDK fetch fails

### Error Classification
- NETWORK_ERROR: fetch failed, ENOTFOUND, ECONNREFUSED, timeout
- AUTH_ERROR: unauthorized, invalid api key, forbidden
- RATE_LIMIT_ERROR: quota, rate limit, too many requests
- TIMEOUT_ERROR: request timeout

## Speech service architecture

### Provider: Azure Cognitive Services Speech SDK
- Continuous recognition mode (not one-shot)
- Push stream audio input (node-record-lpcm16 → pushStream.write)
- 16kHz sample rate, PCM format
- Language: en-US (configurable)

### Audio Capture Chain
1. node-record-lpcm16 starts recording (sox preferred, fallback: rec, arecord)
2. Audio chunks piped to Azure SDK PushStream
3. SDK sends to Azure for recognition
4. Events: recognizing (interim) → recognized (final) → transcription event emitted

### Polyfill Strategy
- Massive global polyfill block for Azure SDK (expects browser environment)
- Polyfills: window, document, navigator, AudioContext, URL, Blob, File, crypto, performance
- Required because Azure Speech SDK assumes browser context

## Key architectural decisions

1. **Single ApplicationController class** — All app logic in one class (main.js). Simple but monolithic.
2. **Singleton services** — All services export `new Service()`. No DI, no testing seams.
3. **Map-based window registry** — `this.windows = new Map()` with string keys. Simple lookup.
4. **Dual IPC bridge** — electronAPI (invoke/handle for async) + api (send/on for fire-and-forget). Redundancy for reliability.
5. **No build step for renderers** — Vanilla JS loaded directly. No React/Vue/bundler.
6. **Session memory as conversation history** — All events (user, model, system, OCR) in one timeline. LLM gets filtered view.
7. **Prompt-as-system-instruction** — Skill prompts sent as Gemini systemInstruction, not prepended to user message. Proper separation.
8. **Event-driven broadcast** — State changes broadcast to ALL windows simultaneously. Each window filters what it needs.
