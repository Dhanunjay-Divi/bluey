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
- Master plan V3 (docs/reviews/CUE-BLUEY-V3-PLAN-COMPLETE.md)
- Phase 0 foundation: workspace structure, Cargo workspace, crate scaffolding
- Development workflow docs: CLAUDE.md, templates, CI, .codex agents + skills
