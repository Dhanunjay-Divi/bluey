# Bluey — Research Scope by Element

> **Purpose of this doc.** Break the whole product into its distinct **elements**, and for
> each one capture: (1) what it is, (2) how it's *supposed* to work, (3) what to look for
> **online** — products/projects/people who have already built this element (in Bluey's
> exact shape OR in a completely different product we can borrow a part from), and how we'd
> reuse it.
>
> This is a **research-scoping doc**, not a build plan and not a bug list. The "gap" for each
> element is the research target: *who has solved this piece, and what can we lift?*
>
> **How to use it.** For each element, fill the **"Look for / reuse"** column with links,
> repos, products, blog posts, and notes as you find them. `[[double-bracketed]]` names point
> at deeper existing docs in this repo. Leave a `TODO(you)` where I've left a gap for you to fill.
>
> Related existing docs: [[HOW-IT-WORKS]], [[FEATURE-MAP]], [[AI-COMPETITIVE-STUDY]],
> [[COMPETITIVE-GAPS]], [[PRODUCT-STRATEGY]], [[REFERENCE-MAP]].

---

## 0. What we're building (one paragraph)

Bluey (codename Cue) is a **local-first, screen-share-invisible AI copilot for macOS (Windows
next)**. It floats an always-on-top overlay over any app/meeting that the user can talk to,
and it has two answer modes: **(A) Managed Bluey AI** (Bluey's own LLM routing) and **(B)
Borrow-your-own-agent** — it discovers the coding agents the user *already has installed*
(Claude Code, Cursor, Copilot, Codex, Gemini, Antigravity, …), reads their past sessions, and
drives them through their own CLIs to answer — including **resuming a specific past session**
and, the marquee capability, **continuing the same conversation even after you switch to a
different agent** (start in Claude, keep going in Cursor, with the thread intact). It captures
audio/screen for meeting context, runs local RAG over that context, and is distributed as one
lean `.app` (system WebView, no bundled browser).

The pillars that make it distinct:
- **Invisible to screen capture** (you can screenshare and the overlay doesn't show).
- **Borrow the user's existing agents** instead of shipping/proxying our own model — no vendor
  SDK bundled, no API keys required for the agent path.
- **Seamless continuation** — resume a past session, and carry a conversation *across* a change
  of agent, with the GUI loading the thread and letting you keep chatting.

---

## How this research works (the three passes)

1. **Pass 1 — define the element.** What it is + how it's supposed to work. (Mostly done below;
   you correct/extend.)
2. **Pass 2 — find prior art.** For each element, who built this already? Same-product clones
   (e.g. other "invisible meeting copilots") AND cross-product sources (e.g. a screen-recorder
   that solved capture-exclusion, an IDE that solved session resume).
3. **Pass 3 — decide reuse.** For each found source: is it a library we vendor, a technique we
   copy, an API we call, or just a design reference? Note license + integration cost.

---

## The Elements

> **Scope note.** Out of research scope for now (working / not the priority): invisibility,
> overlay UI, audio+STT, local RAG, MCP connectors, distribution. The elements below are the
> ones we ARE researching. Numbering is stable (E3–E8, E11, E13, E15) plus the new ones:
> **E16** cross-agent continuation, **E17** continue-from-GUI, **E18** GUI answer rendering
> (the raw-markdown problem), **E19** the architecture question (GUI↔CLI hybrid — right approach?).
>
> **The four that matter most right now (the "global" problems):** E16, E17, E18, E19. These are
> architectural/cross-cutting, not per-agent bugs. E19 should be decided *before* we build much
> more — it's the substrate everything else sits on.

### E3 — Borrow-your-agent: discovery
- **What it is:** detect which coding agents are installed (CLI on PATH, app bundles, session
  stores, MCP config) and classify capability (drive / read-only / needs-reauth / cloud-blocked).
- **How it's supposed to work:** `cue-agent-bridge/discover.rs` scans PATH + known dirs; per
  registry row. Live-proven: finds Claude/Cursor/Copilot/Codex/Gemini/Antigravity/VS Code.
- **Look for / reuse online:**
  - How tools like `mise`/`asdf`, `which`-style detectors, or IDE "detect installed toolchains"
    features enumerate installs robustly across macOS/Windows.
  - Any project that already enumerates AI-CLI installs (there may be a "list my AI tools" util).
  - `TODO(you)`: cross-platform install-detection patterns (Windows registry + app data).

### E4 — Borrow-your-agent: reading past sessions
- **What it is:** read each agent's prior conversations (titles, projects, timestamps) from its
  own on-disk store — many different formats.
- **How it's supposed to work:** per-format readers in `cue-agent-bridge/sessions/`:
  `jsonl.rs` (Claude CLI), `claude_app.rs` (desktop), `vscdb.rs` (VS Code SQLite),
  `antigravity.rs` (proto index), `json_files.rs`. Live-proven: real titles + projects.
- **Look for / reuse online:**
  - Reverse-engineering notes for each store (people who've parsed `~/.claude/projects`,
    Cursor's `state.vscdb`, Copilot session state, Codex sessions, Gemini/Antigravity stores).
  - SQLite/`vscdb` readers, protobuf index parsers others wrote for these exact files.
  - `TODO(you)`: community write-ups on each agent's session-file schema (they change per version).

### E5 — Borrow-your-agent: driving + streaming an answer
- **What it is:** run the agent's headless CLI with the user's question and stream the answer
  token-by-token into the overlay.
- **How it's supposed to work:** `cue-agent-bridge/drive/cli.rs` spawns `claude -p …`,
  `cursor-agent -p …`, `copilot -p …`, `codex exec …`, `gemini -p …`; parses each output format;
  120s timeout; streams `Delta`s. Live-proven: Claude/Copilot/Gemini answer; streams.
- **Look for / reuse online:**
  - Each CLI's documented headless/programmatic mode (flags, output formats, exit codes).
  - Wrappers/SDKs others built around these CLIs (e.g. "Claude Code as a library" shims).
  - Stream-JSON / SSE parsing examples for each tool.
  - `TODO(you)`: which agents have a stable non-interactive contract vs. fragile TTY behavior.

### E6 — Session continuation / resume of the SAME agent (the hard one)
- **What it is:** continue a *specific* past session of the agent it belongs to — either natively
  (`--resume <id>`) or by replaying its transcript as context when the CLI can't resume by id.
- **How it's supposed to work:** registry `ContinuationTier` (`NativeResume` ×5, `Replay` ×6);
  `apply_continuation_tier` sets cwd to the session's project (Claude resolves `--resume` against
  cwd) or degrades to Replay with a compacted transcript. Big-file compaction (recent-turns-from-tail)
  proven on a 141MB file.
  - **Known failure (found in testing):** when the session's project folder is gone/moved,
    `claude --resume` silently starts fresh; the Replay safety-net didn't catch it. Also gemini's
    `--resume` only takes "latest"/index, not a UUID.
- **Look for / reuse online:**
  - How each agent documents resume + cwd-scoping (Claude's project-dir encoding, Cursor's
    `--resume=`, Codex `exec resume`, gemini index semantics).
  - **Conversation-compaction / context-window-management** techniques from other products
    (summarize-older-turns, sliding window, map-reduce summarize) — directly reusable for Replay.
  - Anyone who built a "resume any AI CLI session" or "portable chat history" tool.
  - `TODO(you)`: best prior art for cross-tool session portability + compaction.
- **Findings (research, 2026-06):**
  - **This is a known, documented Claude Code limitation** — `--resume <id>` is scoped to the
    cwd at launch: it only finds sessions in `~/.claude/projects/<hash-of-cwd>/`, so a session
    started in another folder reports "No conversation found with session ID". **Exactly our
    bug.** Confirms the fix must set cwd to the session's project dir (we do) AND degrade to
    Replay when that dir is gone (the part that's failing).
    - [anthropics/claude-code#35226](https://github.com/anthropics/claude-code/issues/35226) —
      "Session Resume Fails With Misleading Error When Working Directory Differs From Launch
      Directory" (our exact symptom).
    - [anthropics/claude-code#58591](https://github.com/anthropics/claude-code/issues/58591) —
      feature request for a `--cwd` flag to resume in a different working dir (not yet shipped).
    - [anthropics/claude-code#41021](https://github.com/anthropics/claude-code/issues/41021) —
      request to let `/resume` find sessions across ALL projects.
    - [Claude Code Sessions docs](https://code.claude.com/docs/en/sessions) — official resume/
      session-storage behavior.
  - **`clauhist`** — a community tool that browses the full Claude Code history and **resumes
    sessions across projects** (works around the cwd-scoping). Reference for how to do
    cross-project resume from the outside.
    [dev.to write-up](https://dev.to/lef237/clauhist-browse-full-claude-code-history-and-resume-sessions-across-projects-1c1o).
  - **Storage format confirmed:** each session is a `.jsonl` (one event per line) under
    `~/.claude/projects/<hash-of-absolute-project-path>/<session-id>.jsonl`. Matches our E4 reader.
  - **Takeaway for us:** since `--resume` is hard-scoped to the project dir, the robust path when
    the dir is missing/moved is **read the `.jsonl` ourselves and Replay it** (our E6 Replay
    tier / E16 neutral-transcript) — don't rely on native resume there.
  - *(Fuller cross-agent / E16 findings pending from the deep-research workflow run.)*

### E16 — SEAMLESS continuation across a DIFFERENT agent (switch mid-conversation) — **NEW**
- **What it is:** the marquee capability — continue a conversation **even after switching the
  underlying agent.** Start a session in Claude, then switch to Cursor (or Copilot, or Gemini) and
  keep going *with the prior conversation intact*. The user changes the engine; the conversation
  doesn't reset. This is fundamentally harder than E6 (same-agent resume) because **no agent's
  native `--resume` can load *another* agent's session** — the only portable substrate is the
  **transcript itself**, replayed into the new agent.
- **How it's supposed to work (intended):**
  - Treat the **conversation transcript as the source of truth**, decoupled from any single agent.
  - When the user switches agents mid-conversation, take the accumulated transcript (across
    whatever agents produced it), **compact it** (E6's compaction), and **Replay** it as context
    into the newly-selected agent's fresh drive — regardless of that agent's native resume support.
  - Normalize each agent's on-disk transcript format (E4 readers) into ONE neutral conversation
    representation so any agent's history can feed any other agent.
  - Keep appending new turns to that neutral transcript so the *next* switch also carries
    everything.
  - **Status:** the pieces exist (transcript readers E4, compaction E6, Replay tier), but
    "carry the conversation across an agent switch" is **not wired end-to-end** — today switching
    the attached agent starts that agent fresh; it does not inherit the prior agent's conversation.
- **Look for / reuse online:**
  - **"Portable conversation / provider-agnostic chat history"** — any tool that lets you move a
    chat between models/providers and keep context (some chat front-ends, LLM gateways with
    "conversation objects", agent frameworks with a shared memory store).
  - **Agent frameworks with a model-agnostic memory/transcript layer** (LangChain/LlamaIndex
    "memory", AutoGen/CrewAI shared context, OpenAI/Anthropic "messages" portability patterns) —
    reuse the *neutral transcript + replay* design, not necessarily the framework.
  - **Multi-model chat apps** (LibreChat, OpenWebUI, Chatbox, etc.) that switch models mid-thread
    — how they keep the thread when the backend changes.
  - **Context-compaction at scale** (how Claude Code / Cursor / others summarize long histories) so
    a replayed cross-agent transcript fits the new agent's window.
  - `TODO(you)`: who has actually shipped "switch the agent, keep the conversation"? Closest prior art?
- **Findings (research, 2026-06):**
  - **LibreChat is the closest shipped prior art — and it validates the whole approach.** A SINGLE
    conversation can switch between GPT-4o, Claude, Gemini, local Llama (Ollama), Mistral — **same
    thread, same UI, same history format**, via a model dropdown, with full context preserved
    across the switch. Broad provider support (OpenAI/Anthropic/Google/Azure/Bedrock/Groq/Mistral/
    Ollama/OpenRouter + custom). It also has **conversation forking** (branch at any message).
    [openwebui vs librechat comparison](https://onyx.app/insights/openwebui-vs-librechat-vs-onyx),
    [LibreChat repo (search)](https://github.com/danny-avila/LibreChat).
    - **The reusable design (confirmed):** keep ONE neutral conversation/history format; the model
      is just a swappable backend; on switch, replay the same history to the new backend. **This is
      exactly E16's intended design — LibreChat proves it works at scale.** Difference for us: our
      "backends" are *agent CLIs* (Claude Code, Cursor, …) not raw model APIs, so our replay target
      is `claude -p`/`cursor-agent -p` with the transcript as context, not a chat-completions call.
    - **Reuse class:** *design reference* (architecture + the neutral-history idea). LibreChat is
      MIT — its conversation-schema and model-switch UX are worth studying directly; we don't vendor
      it (it's a server, not an agent-CLI bridge), but we mirror its "one history, swappable engine."
  - **OpenWebUI** — Ollama + any OpenAI-compatible endpoint; a pipeline architecture that routes
    between models. Reference for the *router/pipeline* layer (more E8 than E16).
  - *(LangChain/LlamaIndex memory + AutoGen/CrewAI shared-context not yet pulled — TODO next pass.)*

### E17 — Continue/resume FROM THE GUI (the overlay can't, today) — **NEW**
- **What it is:** the ability to pick a past session in the **GUI** (overlay/dashboard) and have it
  actually **continue in the GUI** — the thread loads, you keep chatting, and follow-ups carry the
  context. Distinct from E6/E16 (the *engine* mechanics): this is the **GUI-side wiring + UX** of
  resume/continuation.
- **The problem (found in testing):**
  - Clicking a **Meeting** session in History runs `open_meeting_session`, which loads the record
    but **does not render the conversation turns** — it only reports "Continuing X. 0 transcript,
    0 context" (ignores `conversation.len()`), so a real 6-turn session looks empty and the user
    re-clicks (fires repeatedly). The conversation isn't shown, so it *feels* like resume doesn't
    work from the GUI.
  - Clicking an **Agent** session attaches + resumes the engine (E6) but the **thread isn't
    rendered into the overlay** either — you get an answer, not a visible continued conversation.
  - Net: **the GUI can pick a session, but it can't visibly *continue* one** — the conversation
    history never repopulates the overlay thread, and "In context: N turns" stays stale (shows 0).
- **How it's supposed to work (intended):**
  - Picking any session (meeting OR agent) should **rehydrate the overlay thread** with that
    session's prior turns (rendered as cards), set the context indicator correctly, and route the
    next ask as a continuation (E6/E16) of *that* session.
  - One click = one open (no repeated-fire); clear loading/empty states.
- **Look for / reuse online:**
  - How chat UIs **rehydrate a thread on open** (load history → render messages → continue) — any
    chat app's "open conversation" flow is reusable design.
  - Desktop AI apps that let you **reopen a past chat and keep going** (the open-thread-then-append
    pattern), especially ones bridging to a CLI/agent backend.
  - `TODO(you)`: best reference for "open past session → see it → continue it" UX in a floating/HUD UI.
- **Findings (research, 2026-06): there's a whole category of "GUI on top of an agent CLI" we can learn from.**
  - **CodePilot** (op7418) — *"multi-model AI agent desktop client — connect any AI provider,
    extend with MCP & skills, control from your phone."* Electron + Next.js. Directly analogous to
    Bluey's GUI-over-agents idea. [github.com/op7418/CodePilot](https://github.com/op7418/CodePilot).
  - **Nimbalyst** — multi-agent workspace supporting Claude Code natively + Codex via provider
    config; markdown editor, mockup/diagram surfaces. Reference for multi-agent GUI UX.
    [nimbalyst.com](https://nimbalyst.com/blog/best-multi-agent-desktop-apps-claude-code-codex-2026/).
  - **AgentPlane** — local CLI that wraps Claude Code, Codex, Cursor, Aider in a Git-native,
    auditable workflow. Reference for the *wrap-multiple-agent-CLIs* pattern (same as ours).
  - **OpenAI Codex App** (official) — parallel agents, worktrees, diff review — reference for a
    polished agent-desktop UX.
  - **`awesome-cli-coding-agents`** (bradAGI) — a curated directory of terminal agents + the
    harnesses that orchestrate them; good index for "who wraps what."
    [github.com/bradAGI/awesome-cli-coding-agents](https://github.com/bradAGI/awesome-cli-coding-agents).
  - **Rendering pattern (the answer to E18, reusable):** these apps render assistant text **as
    streaming Markdown**, with **reasoning blocks collapsible**, **tool calls as expandable cards**,
    and **tool output specialized** (bash → terminal blocks, file reads → syntax-highlighted code).
    That's the GUI-correct way to display agent output — see E18.

### E18 — Rendering agent answers in the GUI (markdown/tool-output, not raw terminal text) — **NEW**
- **What it is:** agent CLIs emit output formatted for a **terminal** — Markdown (`**bold**`,
  `*italics*`, `#` headings, lists), and structured events (reasoning, tool calls, tool output,
  diffs). Today Bluey's overlay shows this **raw**, so answers look wrong (stray `**`/`*` markers,
  unrendered lists) — they're meant for a coding-agent terminal, not a chat bubble.
- **The problem (found in testing):** the user sees literal `*`/`**` and terminal-shaped text in
  the overlay; it reads as broken/ugly. The markers "mean something" (Markdown) but aren't being
  rendered, and tool-call/reasoning chatter isn't separated from the actual answer.
- **How it's supposed to work (intended):**
  - **Render assistant text as Markdown** in the overlay (bold/italic/headings/lists/inline code).
  - **Syntax-highlight code blocks**; render **diffs** as diffs.
  - **Separate the answer from the machinery:** collapse/hide reasoning + tool-call noise (or show
    tool calls as expandable cards), so the chat bubble shows the *answer*, not the raw stream.
  - Decide per-stream-format: `claude` stream-json, `cursor` json, `codex` jsonl, `copilot`/`gemini`
    plain — each needs its parser to pull *just the answer text* + structured events.
- **Look for / reuse online:**
  - The rendering pattern from E17's GUI-over-agent apps (CodePilot/Nimbalyst/Codex App):
    **streaming Markdown + collapsible reasoning + tool-call cards + specialized tool output**
    (bash → terminal block, file read → highlighted code). This is the confirmed-correct approach.
  - Lightweight JS Markdown renderers safe for streaming (markdown-it, marked, micromark) +
    a syntax highlighter (Shiki/highlight.js) — pick small ones (fits the lean-app constraint).
  - How chat UIs handle **partial/streaming Markdown** (don't break on half-rendered `**`).
  - `TODO(you)`: which renderer is smallest/safest for the overlay; do we strip tool noise or show it?

### E19 — Architecture question: is GUI↔CLI hybrid the right way? (or is there a better substrate) — **THINK THIS THROUGH**
- **What it is:** the core architecture decision. Today Bluey = **GUI (overlay) → daemon →
  drive the agent's CLI headlessly** (`claude -p …`), parse the output, stream it back. You asked:
  *is wrapping the CLI the right substrate, or is there a better one?*
- **The options to weigh:**
  1. **CLI-drive (current):** spawn the agent's own CLI per ask, parse stdout. **Pro:** zero
     install of our own model, inherits the user's auth/MCP/context, "borrow your agent" literally.
     **Con:** fragile per-tool contracts, cwd-scoped resume (E6), no per-id resume on some (E16),
     terminal-shaped output (E18), startup latency per call, hard to keep a *live* session.
  2. **Persistent CLI session (PTY/long-lived process):** keep the agent CLI running in a pseudo-
     terminal and feed turns to it (instead of one-shot `-p` per ask). **Pro:** real in-session
     continuity (the CLI keeps its own context → fixes a lot of E6/E16 *for the same agent*),
     closer to how a human uses it. **Con:** PTY parsing is messy; cross-agent switch still needs
     a neutral transcript.
  3. **Agent SDK / library (where one exists):** e.g. Claude Code's SDK/programmatic API, Codex
     app integrations — call the agent as a library, not a CLI. **Pro:** structured events, no
     stdout parsing. **Con:** not every agent has one; bundling a vendor SDK conflicts with the
     "no vendor SDK, one lean binary" constraint ([[project_sdk_as_spec_not_dep]]) — need to check
     if it can be a thin runtime dependency vs. a bundled dep.
  4. **Neutral-transcript + replay as the PRIMARY substrate (not a fallback):** make the portable
     transcript the source of truth (E16) and treat *every* answer as "replay history + ask",
     regardless of native resume. **Pro:** one code path, agent-agnostic, fixes cross-agent by
     design. **Con:** loses the agent's *native* in-session state/compaction; more tokens replayed.
  5. **MCP / gateway angle:** route through an LLM gateway or expose agents over a protocol. Likely
     more relevant to E8 than to "drive the user's installed agent."
- **What to research / decide:**
  - Do the popular GUI-over-agent tools (CodePilot, Nimbalyst, AgentPlane, Codex App) use one-shot
    CLI, persistent PTY, or an SDK? **That tells us the proven substrate.** (`TODO`: pull this — the
    Codex App is official and may reveal the intended integration path.)
  - Which agents expose a **library/SDK or a persistent/JSON-RPC mode** vs. only a CLI?
  - Cost/latency of replay-everything (option 4) vs. native in-session (option 2).
  - **Recommendation placeholder:** likely a **hybrid of 2 + 4** — persistent session for the
    *current* agent (native continuity, good UX) **plus** a maintained neutral transcript for
    *cross-agent* switches and resume-when-the-dir-is-gone. CLI one-shot (option 1) stays as the
    floor for agents with no better mode. *(Confirm with research before committing.)*
- **Look for / reuse online:**
  - `TODO(you)/(me)`: how each GUI-over-agent product actually drives the agent (one-shot vs PTY
    vs SDK) — this is the single most important thing to learn before locking the architecture.

- **🔴 FINDINGS — DECISIVE (deep research, 2026-06-16; 107 agents, 122 claims, 22 adversarially verified):**

  **The concern was correct. Our current substrate (drive each CLI one-shot with `-p`, parse human
  stdout) is NOT the proven industry approach and is now obsolete for every major agent.** Every
  major agent we target (Claude Code, Codex, Copilot, Cursor, Gemini) ships a **structured
  programmatic surface that emits typed JSON message objects, not terminal text.** Two proven
  substrates have emerged — and they fix the exact walls we hit:

  **Substrate A — each vendor's SDK (typed streaming session + resume-by-id):**
  - **Claude Agent SDK** (`@anthropic-ai/claude-agent-sdk` / `claude_agent_sdk`) — `query()` returns
    a typed `AsyncGenerator` of `SDKMessage` objects (NOT stdout). **Streaming-input mode = a
    persistent long-lived session** (turns queued, mid-session interrupt, persisted FS state).
    **`resume: <session_id>` is a first-class documented API** → directly fixes our "no per-id
    resume" wall. Auto-persists full transcript as JSONL; `listSessions()`/`getSessionMessages()`
    read them. *Library we vendor.*
    - https://code.claude.com/docs/en/agent-sdk/typescript ·
      https://code.claude.com/docs/en/agent-sdk/sessions
  - **OpenAI Codex SDK** (`@openai/codex-sdk`, Apache-2.0) — drives a local Codex **app-server over
    JSON-RPC 2.0 (NDJSON/stdio)**, returns structured objects. `startThread()`/`resumeThread(id)` —
    **first-class resume-by-thread-id, independent of cwd.** Threads persist in `~/.codex/sessions`.
    *Library we vendor* (note: Python pkg is beta). https://developers.openai.com/codex/sdk
  - **GitHub Copilot** — official `@github/copilot-sdk` **spawns the CLI as a long-lived stdio
    subprocess** speaking **ACP** (`copilot --acp --stdio`, NDJSON JSON-RPC; public preview
    2026-01-28). *Technique we copy.*
  - **Cursor** — `--output-format stream-json` is structured BUT **one-shot, no resume-by-id**;
    persistent sessions require its **native ACP** (`cursor-agent agent acp`). https://cursor.com/docs/cli/acp
  - **Gemini CLI** — official **ACP mode** (`--acp`, JSON-RPC 2.0 over stdio); Google's reference
    ACP impl. https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/acp-mode.md

  **Substrate B — ACP (Agent Client Protocol), the neutral one-protocol-for-all answer:**
  - **ACP** (Apache-2.0, created by Zed) is a **JSON-RPC-2.0-over-stdio wire protocol that
    standardizes the GUI↔agent interface the way LSP standardized editor↔language-server.** It
    exists explicitly to kill the N×M per-agent custom-integration problem — *exactly* the problem
    our per-CLI parsers are. Persistent sessions: `session/load` (replays history) + a **stabilized
    `session/resume`** (reconnect to a live agent-held session by id, no replay; stabilized
    2026-04-22).
  - **25+ agents support it**, including the ones we target: **Gemini CLI (native), Cursor (native),
    Copilot CLI (native preview)**, Cline, Goose, OpenHands, and **Claude (via Zed's open-source
    `claude-agent-acp` adapter, Apache-2.0).** → **One ACP client could drive most of our agents
    through a single neutral code path.** *Protocol to adopt; Zed adapters are libraries we vendor.*
  - https://agentclientprotocol.com/get-started/agents ·
    https://agentclientprotocol.com/announcements/session-resume-stabilized ·
    https://github.com/zed-industries/claude-agent-acp

  **What this means for Bluey (the architecture call):**
  - **Replace per-CLI stdout parsers** with either (A) each vendor's SDK or (B) a single ACP client.
    Either gives us **typed JSON events (fixes E18's raw-markdown problem at the source — the answer
    text arrives as a structured field, separate from tool/reasoning events) + persistent sessions
    + resume-by-id (fixes E6/E16 same-agent resume).**
  - **Likely direction: ACP as the primary substrate** (one protocol → Gemini/Cursor/Copilot
    native + Claude/Codex via Apache-2.0 adapters), because it's *also* the neutral layer E16
    (cross-agent) wants — one client, many agents, one session/transcript model. Fall back to a
    vendor SDK where ACP coverage is weak.

  **Honest caveats (do NOT over-read the finding):**
  1. **"SDK" ≠ no subprocess.** Copilot's and Codex's SDKs still **spawn the CLI as a long-lived
     stdio subprocess** — so spawning a subprocess was never the anti-pattern; **scraping human
     stdout was.** The win is the structured JSON-RPC/NDJSON/JSONL layer.
  2. **cwd-scoping is NOT solved by the SDK.** Claude's SDK has the *same*
     `~/.claude/projects/<encoded-cwd>/*.jsonl` constraint as the CLI — our cwd bug persists; we
     must launch from the original cwd or relocate the jsonl. (Encoding confirmed on this machine.)
  3. **ACP is not turnkey for our full set today** — Claude/Codex/Cursor go through Zed-built or
     community **adapters** (Apache-2.0, vendorable, but per-agent maintenance remains); real-world
     `session/resume` robustness varies by adapter maturity (the one claim that split 2-1).
  4. **Hangs are a substrate-independent problem.** Zombie/hung subprocess bugs exist even in the
     structured modes (Codex stdin-never-closed, claude-agent-sdk JSONL flush race on SIGTERM,
     claude-code zombie-on-initialize) — **we must engineer subprocess lifecycle (graceful
     shutdown, idle timeout, EOF) regardless.** This is our E6/#6 "Thinking… hang" — it doesn't
     vanish by switching substrate.
  5. **Fast-moving / flag churn.** Copilot `--headless --stdio`→`--acp --stdio` (broke integrations,
     #1606); Claude removed a V2 session API in v0.2.142; Gemini `--experimental-acp`→`--acp`. **Pin
     versions; re-verify exact flags/package versions at implementation time.**
  6. **Aider: unknown.** No claim about Aider's scripting/Python API survived verification — its
     substrate (lib vs CLI vs ACP) is an **open item** needing dedicated investigation.

  **Open decisions (for you):**
  - **One ACP client** (neutral, Apache-2.0 adapters) **vs. per-vendor native SDKs** (richest
    features, N integrations)? Trade-off: neutral protocol + maintained adapters vs. vendor-specific
    capabilities the adapters may not expose.
  - Does ACP's `session/resume` reliably reconnect across *our specific* adapters, or fall back to
    load-and-replay? (Verify per-agent before committing.)
  - Aider's programmatic surface — research separately.

  **Other substrate references found:** PTY long-lived process (`node-pty`, `tauri-plugin-pty` —
  for agents with only a TUI), wrapper post-mortems (agentgui, TermHive, avasdream's claude wrapper),
  and survey of vibe-kanban / crystal (how shipped multi-agent GUIs drive agents).

### E7 — Two answer modes + the mode toggle (Managed AI ↔ Your agent)
- **What it is:** one app that switches between Bluey-managed LLM answers and borrow-your-agent
  answers; routing keys off whether an agent is attached.
- **How it's supposed to work:** `AiProviderKind` (`CueManaged` | `Agent`); daemon dispatches on
  `attached_agent`. Dashboard has `useAgentMode` + a toggle; overlay routes ask through the agent
  when attached. ~90% wired (per [[INTEGRATION-PLAN]]).
- **Look for / reuse online:**
  - Apps with a "use my own key / use our hosted" switch (BYOK products) — the UX + plumbing.
  - `TODO(you)`: cleanest BYOK/managed toggle reference.

### E8 — LLM routing & failover (managed side)
- **What it is:** provider abstraction + auto-router that picks/falls-back across providers.
- **How it's supposed to work:** `cue-llm` (anthropic/openai/ollama/bluey_managed + `router.rs`),
  `cue-router/auto.rs` task classifier. See [[MODEL-ROUTING]], [[AUTO-ROUTING-USP]],
  [[PROVIDER-429-PLAYBOOK]].
- **Look for / reuse online:**
  - LLM gateways/routers (LiteLLM, OpenRouter, Portkey, etc.) — patterns for failover, retries,
    cost-aware routing. Reuse design, maybe a self-host gateway.
  - `TODO(you)`: which router lib (if any) is worth depending on vs. our own.

### E11 — Meeting/session model & History
- **What it is:** Bluey's own meeting records (transcript + conversation + context + summary),
  shown in History; titling; open/continue. (The GUI *continuation* of these lives in E17.)
- **How it's supposed to work:** `MeetingRecord` in cue-core; `storage.rs`; titler
  (`mechanical_title`) now wired. **Known gaps (testing):** opening a meeting doesn't render its
  conversation (reports "0 transcript/0 context" ignoring conversation turns — see E17); old titles
  generic on pre-fix records.
- **Look for / reuse online:**
  - How notetakers structure + display a session timeline (transcript + Q&A + summary).
  - `TODO(you)`: best session-detail UX reference.

### E13 — Process masquerading / anti-detection (stealth)
- **What it is:** the dashboard process presents under an innocuous identity.
- **How it's supposed to work:** `cue-stealth` (process masquerading).
- **Look for / reuse online:**
  - Legit process-naming/relaunch techniques (note: keep this to legitimate, non-malicious uses).
  - `TODO(you)`: scope what's acceptable here.

### E15 — Cloud sync / commercial tier (later)
- **What it is:** optional paid cloud sync of sessions/RAG.
- **How it's supposed to work:** `cue-cloud-client`; [[COMMERCIAL-PATH]], [[PRICING-MODEL]],
  [[CLOUD-RAG]].
- **Look for / reuse online:**
  - Sync engines (CRDT/keep-it-simple), e2e-encrypted sync references.
  - `TODO(you)`: sync approach that doesn't compromise the local-first promise.

---

## Cross-cutting research targets (whole-product comparables)

These are *whole products* to study end-to-end because they overlap Bluey's shape — useful for
many elements at once:

- **Multi-agent / agent-router desktop tools** → E3, E5, E7, E8.
- **Provider-agnostic / multi-model chat apps** (LibreChat, OpenWebUI, Chatbox, …) → E16, E17
  (switch model mid-thread + keep context; reopen-and-continue UX).
- **Agent frameworks with a shared memory/transcript layer** (LangChain/LlamaIndex memory,
  AutoGen/CrewAI shared context) → E16 (neutral transcript + replay).
- **LLM gateways** (LiteLLM/OpenRouter/Portkey) → E8, and E16 if they expose portable
  "conversation" objects.
- **"Resume any AI CLI session" / portable chat-history utilities** → E6, E16.
- `TODO(you)`: add the specific products you have in mind to clone-or-borrow-from.

---

## The continuation story, in one place (E6 → E16 → E17)

Because continuation is the heart of the product, here's how the three pieces relate:

- **E6** = continue the SAME agent's session (native `--resume`, else Replay). *Engine, same agent.*
- **E16** = continue across a DIFFERENT agent (switch engine, keep the conversation via a neutral
  transcript + Replay). *Engine, any agent — the marquee feature, not yet wired end-to-end.*
- **E17** = do either of the above **from the GUI** so the thread visibly loads and continues.
  *UX/wiring — today the GUI can pick a session but can't visibly continue one.*

A reusable design that satisfies all three: **one neutral conversation transcript** (normalized
from any agent's store), **a compaction step**, and **a Replay path** that any agent can consume —
with the GUI rehydrating the thread from that transcript on open and appending new turns to it.

---

## What I may have missed (for you to fill)

- Any element above where my description of "how it's supposed to work" is wrong — correct it.
- Elements not listed at all (e.g. onboarding flow, permissions/consent UX, billing/disclosure,
  auto-recap, the "auto-router USP").
- The **specific products/people** you already know solved a piece — drop names/links under the
  matching element's "Look for / reuse."
- Priorities: which elements are worth researching first (highest leverage vs. effort).
