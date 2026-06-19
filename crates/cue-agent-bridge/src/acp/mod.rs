//! ACP — the Agent Client Protocol substrate for Bluey.
//!
//! ACP ([agentclientprotocol.com](https://agentclientprotocol.com/), created by
//! Zed Industries, Apache-2.0) is a JSON-RPC-2.0-over-stdio protocol that
//! standardizes the GUI↔agent interface — "LSP for coding agents". Bluey is
//! migrating off the fragile "spawn `claude -p` and scrape human stdout" model
//! onto ACP: agents are spawned as long-lived stdio subprocesses we speak
//! structured JSON-RPC to, via the official [`agent_client_protocol`] SDK.
//!
//! This module is the **Phase-1 foundation** that the rest of the migration
//! builds on:
//!
//! - [`client`] — [`AcpClient`], a thin wrapper over the SDK that spawns an
//!   agent, runs `initialize` + `session/new` (or `session/load` for resume),
//!   sends a prompt, and exposes the streaming response as the crate's existing
//!   [`crate::drive::AnswerStream`] of [`crate::drive::AnswerChunk`]s.
//! - [`neutral`] — [`NeutralConversation`] / [`NeutralMessage`] / [`Role`], a
//!   provider-agnostic transcript model with lossless [`From`] conversions to
//!   and from the crate's on-disk [`crate::Transcript`] / [`crate::Role`]. This
//!   is the substrate for cross-agent continuation.
//!
//! # What is deliberately *not* here (Phase 2, other agents)
//!
//! - **Per-agent binary mapping.** [`AcpAgentSpec`] takes the executable + args
//!   explicitly; the `AgentKind → spec` registry lands in Phase 2. See the
//!   `// PHASE 2` note on [`AcpAgentSpec`].
//! - **Daemon drive integration.** Nothing here registers with the daemon's
//!   answer ladder or the generic [`crate::drive`] dispatcher yet. Because
//!   [`AcpClient::prompt`]/[`resume`](AcpClient::resume) already return an
//!   [`AnswerStream`], wiring is a thin adapter (see the note below).
//! - **Session-store decoders.** Turning an on-disk session into a
//!   [`NeutralConversation`] to seed a resume is Phase 2/3; the
//!   `Transcript → NeutralConversation` conversion that path will use already
//!   lives in [`neutral`].
//!
//! ## PHASE 2 plug-in points (precise)
//!
//! 1. **Binary map:** add `impl TryFrom<&crate::AgentKind> for
//!    client::AcpAgentSpec` (or a `registry`-keyed lookup) so a discovered
//!    agent yields its ACP entrypoint. The SDK ships ready-made constructors
//!    for the common agents (Claude Code / Gemini / Codex) — see the
//!    `AcpAgentSpec` doc.
//! 2. **Daemon drive:** add a `Driver` impl (or a branch in [`crate::drive`])
//!    that, for ACP-capable agents, constructs an [`AcpClient`] from the spec +
//!    the [`crate::drive::Question`]'s `cwd`, then calls `prompt` (or `resume`
//!    when `Question::resume` is set) and returns the resulting
//!    [`AnswerStream`] straight through — the chunk contract already matches.
//! 3. **Decoders → continuation:** when seeding a cross-agent resume, decode
//!    the source session to a [`crate::Transcript`] (existing readers) and
//!    `.into()` a [`NeutralConversation`]; render that back to prompt text (or,
//!    once agents expose it, an ACP `session/load`) for the destination agent.
//!
//! [`AcpClient`]: client::AcpClient
//! [`AcpAgentSpec`]: client::AcpAgentSpec
//! [`NeutralConversation`]: neutral::NeutralConversation
//! [`NeutralMessage`]: neutral::NeutralMessage
//! [`Role`]: neutral::Role
//! [`AnswerStream`]: crate::drive::AnswerStream
//! [`resume`]: client::AcpClient::resume

pub mod client;
pub mod decode;
pub mod drive;
pub mod neutral;
pub mod spec_map;

pub use client::{AcpAgentSpec, AcpClient};
pub use decode::{
    encode_claude_project_dir, load_session_neutral, load_session_neutral_bounded,
    neutral_from_transcript,
};
pub use drive::drive_acp;
pub use neutral::{NeutralConversation, NeutralMessage, Role};
pub use spec_map::acp_spec_for_discovered;
