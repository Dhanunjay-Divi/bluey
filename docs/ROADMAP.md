# Roadmap

## Version 0.1: macOS Arm64 Local-First Overlay

- Native overlay sidecar.
- Capture-excluded overlay on macOS.
- Compact pill-first launch, movable/resizable panel, click-through readable feed,
  and opacity-adjustable background glass.
- CLI/daemon process model.
- Durable active session state.
- Manual transcript ingestion.
- User-selected screenshot/diagram/code/document context attachments.
- macOS user-triggered screenshot capture with preview and confirmation.
- Explicit support-only periodic screen context capture, separate from the primary user-confirmed Analyse Screen action.
- Deterministic question/action/decision cards.
- Native macOS audio helper path, two-stage VAD, Deepgram/OpenAI Realtime/LocalWhisper-capable STT routing, and source-labeled transcript storage.
- Streaming LLM answer cards with compacted transcript/context/attachment memory.
- Local SQLite session storage, FTS search, export, local RAG primitives, and OS-keyring-backed settings where wired.
- Terminal tarball package and installer script for macOS arm64.
- Recap and action-item commands.

## Version 0.2: Reliability And Local RAG

- Clean-machine macOS arm64 install validation.
- Long-session stress tests for overlay, audio, STT reconnects, and answer streaming.
- Device hot-swap, sleep/wake, permission repair, and health diagnostics.
- sqlite-vec or another ANN index for local RAG.
- Attachment drawer and visible context management.
- Provider cancellation/abort semantics for superseded answer requests.
- Local workspace/repo attachment as a first-class context source, with file
  tree, summaries, search, and RAG feeding the live meeting/ask path.
- Context coverage model for audio, screen, docs, repo, page, memory, and cloud
  sources.
- Missing-context prompts that guide the user to attach the right approved
  source instead of guessing from weak screenshot context.

## Version 0.3: Platform Expansion

- macOS x86_64 artifact if Intel support is required.
- Windows whisper.cpp integration and Windows 10/11 QA for overlay, audio,
  page capture, installer, and update paths.
- Linux build decision after audio/capture feasibility review.
- Signed installers and auto-update only after the supported platform matrix is real.

## Version 0.4: Commercial Cloud Memory And Meeting Intelligence

- Authenticated Bluey cloud account.
- Secure cloud sync for meetings, artifacts, and recaps.
- Cloud RAG index with tenant/workspace scoped retrieval.
- Background recap and memory extraction.
- Meeting history dashboard.
- Proactive meeting cards that surface relevant repo/docs/ticket/PR/deploy
  context when a topic is mentioned in the live transcript.
- Meeting intent detection for project names, branches, tickets, owners,
  customers, incidents, releases, and action items.
- Source cards on answers: used sources, missing sources, freshness, and
  confidence.
- Research-backed context quality reports that compare screen-only answers
  against structured workspace/repo/doc context.
- Export and deletion jobs backed by the cloud policy model.
- Retention, export, and deletion controls.
- Billing and plan enforcement.

## Version 0.5: Agent Context Bridge And Context Intelligence Lab

- Attach local coding-agent/workspace sessions where technically feasible:
  Cursor, Claude Code, Codex, Kiro, GitHub Copilot/Gemini CLI-style workflows.
- Reuse user-approved local workspace context and MCP-style connectors instead
  of rebuilding every integration from scratch.
- Context-source router chooses between transcript, screen/page, local repo,
  attached docs, session memory, and approved agent/connector context.
- Team meeting mode merges multiple participants' approved context with
  explicit workspace boundaries and audit logs.
- Post-meeting pushbacks create or update Jira/GitHub/Notion-style artifacts
  through approved connectors or agent-owned tools.
- Controlled mock IDE/work-app research harness for measuring context channels:
  screenshot/OCR, selection, clipboard, accessibility, local folder, repo index,
  browser-page text, and approved connectors.
- Public research/trust paper derived from the harness, explaining Bluey's
  consent-based context architecture and defensive recommendations.

### Future Addition: Browser Connector

Bluey should eventually have a user-installed browser connector for Chrome, and
later other browsers where feasible. The goal is Codex-style browser context for
normal user-approved work, not hidden surveillance or credential extraction.

What it should enable:

- Read the current tab title, URL, visible text, selected text, basic DOM/layout
  structure, links/buttons/forms, and console errors when the user grants tab
  access.
- Let the user ask Bluey to summarize the current page, compare it with the
  meeting transcript, or use it alongside screen context and attached docs.
- Let Bluey propose browser actions such as click, type, open tab, navigate, or
  copy text, then require user approval before sensitive actions.
- Keep an overlay indicator such as `Chrome connected` and `current tab used`
  whenever browser context is included in an answer.
- Log browser actions at a user-readable level: tab read, button clicked, text
  typed, form submitted, navigation opened.

Non-goals and safety limits:

- Do not scrape cookies, passwords, session tokens, hidden fields, browser
  storage, or data the page did not visibly expose to the user.
- Do not bypass site restrictions, assessment environments, CAPTCHAs, payment
  flows, or authorization boundaries.
- Require confirmation before form submit, message send, upload, delete,
  permission accept, payment, account/security changes, or any irreversible
  action.
- Keep all provider keys server-side. The extension should talk only to the
  local Bluey daemon or a local native-messaging bridge.

Candidate architecture:

```text
Chrome extension
  -> native messaging or local websocket
Bluey daemon
  -> sanitized BrowserContext artifact
Bluey server / router
  -> answer or proposed action
Overlay UI
  -> user approves risky action
Chrome extension
  -> executes click/type/navigation only after approval
```

Implementation plan when we return to this:

1. Add `extensions/chrome-bluey/` as a Manifest V3 extension.
2. Add daemon bridge commands for `browser_context`, `browser_action_preview`,
   and `browser_action_confirmed`.
3. Add a `BrowserContext` artifact type that can be combined with transcript,
   screen, attached documents, and session memory.
4. Add overlay UI for `Chrome connected`, `Use current tab`, and action preview.
5. Teach server prompts and routing to distinguish browser context from screen
   OCR and document context.
6. Add policy tests so unsafe data and unsafe actions are blocked before any
   model call or extension execution.
7. Add integration tests with mocked extension events before enabling real
   browser control.

## Reliability Principles

- The live meeting path must never block on recap, storage compaction, embeddings, or network retries.
- Overlay failure should not crash the daemon.
- Audio capture should be restartable independently from the meeting engine.
- Provider errors should degrade to local cards/status, not silence.
- State should be recoverable after daemon restart.
