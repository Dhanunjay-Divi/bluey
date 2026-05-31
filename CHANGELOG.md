# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `cue-agent-bridge` crate (Slice 1): read-only discovery of installed coding
  agents (GUI + CLI) via registry + generic VS Code-fork detector, JSONC-tolerant
  MCP connector reader with auth-tier classification, and the `AgentSource` trait
  foundation. Design: docs/work/PLAN-AGENT-BRIDGE.md.
- `cue-agent-bridge` drive layer (Slice 2): data-driven per-agent CLI command map
  (claude/copilot/cursor-agent/gemini/codex), safe subprocess execution (args array,
  no shell injection), wall-clock timeout, output-size cap, kill-on-drop, and
  stream-json + plain-text answer parsers streaming `AnswerChunk`s. `AgentSource::ask`
  wired to the runner.
- `cue-agent-bridge` session readers (Slice 3): per-format decoders behind a
  `SessionReader` trait — JSONL (Claude/Codex), SQLite `state.vscdb` (Cursor/VS Code,
  read-only/immutable, bounded), JSON-files (VS Code/Copilot), and a deferred
  Antigravity protobuf stub. Normalizes to `Transcript`/`SessionRef`.
- Agent bridge daemon wiring (Slice 4): new `AiProviderKind::Agent` provider route
  so an attached coding agent answers through the existing
  `resolve_answer_route`/`OverlayAnswerStream` machinery. Selection is driven by two
  new `CueSettings` fields (`attached_agent`, `allow_agent_session_history`); when an
  agent is attached, answers stream from it and the turn is recorded with an agent
  provider label. If the agent CLI is missing or not signed in, Bluey shows a
  guidance `Warning` card and never silently falls back to its own AI.
- Agent bridge IPC + discovery surface (Slice 5a): overlay-facing DTOs
  (`AgentSummary`/`AgentConnectorInfo`/`AgentSessionSummary`), new `OverlayCommand`
  (`SetAgents`/`SetAgentSessions`/`SetAgentConnectors`) and `OverlayEvent`
  (`AgentListRequested`/`AgentAttachRequested`/`AgentDetachRequested`/
  `AgentSessionsRequested`/`AgentConnectorsRequested`/`ConnectorReauthRequested`)
  variants, and daemon handlers that discover agents, map them to summaries,
  persist attach/detach to settings, and list sessions (gated on the
  `allow_agent_session_history` consent toggle). Discovery/IO runs off the async
  runtime; connector readiness reported without exposing secrets.
- Master plan V3 (docs/reviews/CUE-BLUEY-V3-PLAN-COMPLETE.md)
- Phase 0 foundation: workspace structure, Cargo workspace, crate scaffolding
- Development workflow docs: CLAUDE.md, templates, CI, .codex agents + skills
