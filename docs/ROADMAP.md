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

## Version 0.6 (future): Chat-Surface Answer Assist (Slack / Teams)

> **Separate from the meeting overlay.** This is a NEW frontend surface for the
> *meeting-assistant* product, not the interview overlay. The spine
> (`cue-agent-bridge` → user's own agent + its MCP) is unchanged; only a new
> input trigger + output sink are added. See PLAN-PRODUCTION-VISION.md §9 for the
> full feasibility study and design.

**The flow.** The user is talking to someone (a live call, or an async thread).
A question comes up. From **Slack or Microsoft Teams**, the user invokes Bluey
(slash command / @mention / DM) → Bluey answers **using their own agent +
connectors** → the answer is delivered **into that same chat** so the user can
read it and answer the other person with it. It is an **assist to the human**
(helps them know what to say), not Bluey speaking autonomously on their behalf.

**Confirmed feasible (researched June 2026):**
- **Slack** — a Slack app responds to a slash command or an `app_mention` and
  posts the answer back into the **same channel/thread**, and can make it
  **ephemeral** (`response_type: "ephemeral"` → visible ONLY to the invoking
  user, not the channel) — ideal for the "help me privately" assist model.
  Async answers use the `response_url` (past the 3s ack window). **Socket Mode**
  removes the need for a public HTTPS endpoint (events over a WebSocket), which
  fits Bluey's local-first posture. Scopes: `commands`, `chat:write`,
  `app_mentions:read`.
- **Microsoft Teams** — a Bot Framework / Azure Bot replies in the same
  channel/group when `@mentioned`, supports user mentions + Adaptive Cards, and
  can send a **1:1 (personal) message visible only to the user**. Requires Azure
  Bot registration + app manifest + a public HTTPS messaging endpoint, and the
  app must be **installed into the team/tenant** (admin or user per policy).

**The honest privacy boundary (the key design constraint).** A channel reply is
visible to everyone in that channel — that is the answer LEAVING to the tool. So
the default assist mode is the **private/ephemeral path** (Slack ephemeral /
Teams 1:1), which keeps the answer to the user only — preserving the "no data
retention beyond what the user chose" principle. Posting visibly into a shared
channel is a separate, explicit, per-action consented choice. Never silent.

**Why this is its own version, not part of the core meeting loop.** The core
product (interview + live work-meeting) is **fully local**: local system audio →
the user's own agent → invisible overlay — nothing leaves the machine. The
Slack/Teams surface is the case where the conversation *lives in a tool* and an
answer in that tool is what helps; it carries a real third-party data-egress
decision, so it is sequenced AFTER the local loop is solid and gated by explicit
consent. (Teams also needs cloud bot infra — a public endpoint — unlike the
local-only core.)

**Surface coverage (researched June 2026 — each verified to support a PRIVATE /
ephemeral answer, the feature this assist model requires).** Every surface below
can deliver an answer visible ONLY to the invoking user, so the default stays
"help me privately" with no channel-wide egress:

- **Tier 1 — committed (the corporate standard):**
  - **Slack** — slash command / `app_mention`; ephemeral reply (`response_type:
    "ephemeral"`); Socket Mode = no public server. Best fit, build first.
  - **Microsoft Teams** — Bot Framework reply on @mention; 1:1 private message;
    needs Azure bot + public HTTPS endpoint + tenant install. Enterprise default.
- **Tier 2 — planned fast-followers (cover the rest of corporate + dev):**
  - **Discord** — native EPHEMERAL message flag built for slash commands
    ("only you can see"); webhook/gateway interactions. Easiest after Slack;
    strong in dev/startup/OSS teams.
  - **Google Chat** — slash commands; `privateMessageViewer` sends a message to
    one user; command content visible only to the user + the app. Covers the
    **Google Workspace** half of corporate (the Meet-not-Teams orgs), as Teams
    covers the Office 365 half.
- **Tier 3 — possible, note-only (build only if a customer asks):**
  - **Telegram** — excellent bot API, inline queries with a `private` flag, but
    more consumer than corporate-eng.
  - **Mattermost / Zulip / Rocket.Chat** — self-hosted Slack-likes (ephemeral +
    `response_url` confirmed on Mattermost); niche but real in security-conscious
    / regulated eng orgs.
- **Explicitly OUT of scope (wrong audience / wrong API):** WhatsApp, iMessage,
  SMS (consumer, not corporate-eng meetings); Zoom Team Chat / Webex messaging
  (low primary-chat adoption — those orgs are in Slack/Teams anyway).

## Reliability Principles

- The live meeting path must never block on recap, storage compaction, embeddings, or network retries.
- Overlay failure should not crash the daemon.
- Audio capture should be restartable independently from the meeting engine.
- Provider errors should degrade to local cards/status, not silence.
- State should be recoverable after daemon restart.
