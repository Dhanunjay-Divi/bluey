//! Drive layer — ask an installed agent a question and stream its answer.
//!
//! Slice 2 scope: **one CLI runner plus a per-agent command map.** A
//! [`Question`] is turned into a subprocess invocation (args array, never a
//! shell string), the child's stdout is parsed into [`AnswerChunk`]s, and the
//! result is exposed as an [`AnswerStream`].
//!
//! Security posture (see PLAN §6.8): args array only (no command injection),
//! wall-clock timeout, output-size cap, kill-on-drop, and no prompt content
//! logged above `debug`/`trace`.
//!
//! This module is self-contained: it does not modify the [`AgentSource`] trait
//! in `lib.rs`. The parent wires `mod drive;` and a trait `ask()` method on top
//! of [`drive`].
//!
//! [`AgentSource`]: crate::AgentSource

use std::pin::Pin;

use futures_util::Stream;

use crate::{AgentKind, Transcript};

pub mod cli;

pub use cli::{drive, drive_with_options, DriveOptions};

/// A question to put to an agent, plus optional grounding context and a native
/// session id to resume.
#[derive(Debug, Clone, Default)]
pub struct Question {
    /// The prompt text. Passed to the CLI as a single argument, verbatim.
    pub prompt: String,
    /// Bounded prior context (meeting summary + optional prior session). When
    /// present it is prepended to the prompt; the daemon decides how much to
    /// include.
    pub context: Option<Transcript>,
    /// Native session id to continue, when the agent supports `--resume`.
    pub resume: Option<String>,
}

impl Question {
    /// Convenience constructor for a bare prompt with no context or resume.
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            context: None,
            resume: None,
        }
    }

    /// Render the full prompt text that is handed to the CLI: any [`context`]
    /// transcript is flattened ahead of the prompt. This is pure string
    /// assembly — the result is still passed as a single argv entry, never
    /// through a shell.
    ///
    /// [`context`]: Question::context
    pub(crate) fn render_prompt(&self) -> String {
        match &self.context {
            None => self.prompt.clone(),
            Some(t) if t.turns.is_empty() => self.prompt.clone(),
            Some(t) => {
                let mut out = String::new();
                out.push_str("Context from a prior conversation:\n");
                for turn in &t.turns {
                    let role = match turn.role {
                        crate::Role::User => "User",
                        crate::Role::Assistant => "Assistant",
                        crate::Role::System => "System",
                        crate::Role::Other => "Note",
                    };
                    out.push_str(role);
                    out.push_str(": ");
                    out.push_str(&turn.text);
                    out.push('\n');
                }
                out.push_str("\nQuestion:\n");
                out.push_str(&self.prompt);
                out
            }
        }
    }
}

/// One streamed piece of an agent's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum AnswerChunk {
    /// The run started; carries the native session id when the agent reports
    /// one (used to resume later).
    Started { session_id: Option<String> },
    /// A piece of answer text, in order.
    Delta(String),
    /// The run finished cleanly; carries the reported cost when available.
    Done { cost_usd: Option<f64> },
    /// The run failed (spawn error, non-zero exit, timeout, output cap, parse
    /// failure). Never silent — always surfaced as a terminal chunk.
    Error(String),
}

/// An ordered, owned stream of [`AnswerChunk`]s. The underlying subprocess is
/// killed when this stream is dropped (see [`DriveOptions`] / kill-on-drop).
pub type AnswerStream = Pin<Box<dyn Stream<Item = AnswerChunk> + Send>>;

/// Drive an agent: turn a [`Question`] into a streamed answer.
///
/// Implemented for the CLI path by [`cli::drive`]. Kept as a trait so a future
/// non-CLI driver (e.g. the zero-retention router fallback) can slot in behind
/// the same entry point without changing callers.
#[async_trait::async_trait]
pub trait Driver: Send + Sync {
    /// Run `question` against `agent` and return its streamed answer.
    async fn drive(&self, agent: AgentKind, question: Question) -> anyhow::Result<AnswerStream>;
}
