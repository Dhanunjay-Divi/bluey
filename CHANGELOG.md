# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Fix button daemon flow (F3): the review-gated propose → approve → apply state
  machine. New `OverlayEvent::FixRequested` / `FixApprovalResponded` and
  `OverlayCommand::PushFixProposal`. On Fix, the daemon drives the attached agent
  in ProposeFix mode, parses the structured proposal, and pushes a proposal card
  (diagnosis / reasoning / diff + Approve-Reject) — applying nothing. Apply is
  reachable only via an `approved=true` response carrying a server-minted,
  unconsumed, unexpired proposal id for an apply-capable agent (remove-on-take +
  TTL prevent replay/stale-apply); never pushes. Pending proposals are bounded.
- Fix button foundation (F1+F2): a data-driven `FixProfile` on each registry row
  (per-agent propose-only vs apply args, `apply_supported`) plus a `DriveMode`
  (Answer / ProposeFix / ApplyFix) so the drive layer forces propose-only or
  apply purely from the table — no per-agent branches, apply args appended only
  in ApplyFix and only for apply-capable agents (Cursor never uses the broken
  `--plan`). Adds the fix-proposal / apply prompt templates (structured
  DIAGNOSIS/REASONING/FIX, "apply nothing", "never push") and a fail-soft
  `FixProposal` parser. Design: docs/work/PLAN-FIX-BUTTON.md.
- Agent bridge native overlay UI (Slice 5b): an "Attach Agent" drawer in the
  macOS overlay — agent picker with capability chips, session picker
  (continue-most-recent / fresh), connector sheet with per-connector readiness +
  re-auth, attached-state header badge + pill glyph, and agent-answer card
  relabeling (role badge shows the agent, "answered by your <agent>"). Decodes
  `set_agents`/`set_agent_sessions`/`set_agent_connectors`, emits the agent
  request events. Reuses existing theme/drawer/card builders; no new window.
- Agent bridge session resume + agent-labeled answers: a new `attached_session`
  setting persists the chosen session so an attached agent continues it
  (`Question.resume`), and agent answers now carry an agent-labeled card source so
  the overlay attributes them to the user's agent.
- Windows agent discovery: discovery now resolves Windows base dirs
  (`%APPDATA%`/`%LOCALAPPDATA%`/program dirs, `%USERPROFILE%`) and scans
  Windows app-install + VS Code-family data locations, so `agent list` finds GUI
  agents on Windows too (cfg-gated; macOS behavior unchanged).
- `scripts/test-agent-bridge.sh`: one-command end-to-end smoke test (build →
  start daemon → list/attach/connectors/sessions/ask/detach → stop), with safe
  daemon start/stop and graceful handling when no drivable agent is installed.
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
- `bluey agent` CLI subcommand (list / attach / detach / sessions / connectors /
  status) to discover, attach, and inspect coding agents from the terminal,
  driving the daemon over new `DaemonRequest`/`DaemonResponse` agent variants.
  `attach` persists the selection so `bluey ask` routes through the agent;
  `sessions` honors the session-history consent gate. Lets the agent bridge be
  tested end-to-end without the overlay UI.
- Master plan V3 (docs/reviews/CUE-BLUEY-V3-PLAN-COMPLETE.md)
- Phase 0 foundation: workspace structure, Cargo workspace, crate scaffolding
- Development workflow docs: CLAUDE.md, templates, CI, .codex agents + skills
