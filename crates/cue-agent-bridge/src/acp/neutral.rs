//! Provider-agnostic conversation model — the substrate for cross-agent
//! continuation.
//!
//! ACP, Claude Code JSONL, Cursor's `state.vscdb`, Antigravity's protobuf
//! index, and the cloud vendors all describe "a conversation" differently.
//! [`NeutralConversation`] is the single shape every one of them is mapped
//! *into* before Bluey reasons about continuation: when we want to carry a
//! conversation from one agent to another (the core "your own agent" promise),
//! we normalize the source transcript into this model, then render it back into
//! whatever the destination agent expects.
//!
//! This is intentionally a *lossy, display-oriented* model: roles + text, no
//! tool-call internals, no provider-specific metadata. It is the lowest common
//! denominator that every agent can both produce and consume. Richer structure
//! (tool calls, reasoning, attachments) stays in the source format and is
//! re-derived on demand; it does not belong here.
//!
//! The crate already has a [`crate::Transcript`] type used by the on-disk
//! session readers. [`NeutralConversation`] is a distinct type on purpose:
//! `Transcript` is "what we decoded from a store", `NeutralConversation` is
//! "the canonical thing we continue from", and the [`From`] conversions below
//! are the bridge. Keeping them separate means a Phase-2 change to the ACP
//! event model never silently reshapes the on-disk decoders, and vice versa.

use crate::{Role as CrateRole, Transcript, Turn};

/// Speaker role in a [`NeutralMessage`].
///
/// Deliberately a closed set of three: every source role collapses into one of
/// these. The crate's [`crate::Role::Other`] (used by session readers for
/// roles they could not classify, e.g. a tool/observer turn) maps to
/// [`Role::System`] — an unclassified turn is grounding context, never a user
/// or assistant utterance, so treating it as a system note is the safe default
/// for replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A human/user turn.
    User,
    /// An agent/assistant turn.
    Assistant,
    /// System or otherwise-unclassified grounding context.
    System,
}

impl Role {
    /// Stable lowercase label, handy for rendering a transcript back into a
    /// plain-text prompt for a destination agent.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        }
    }
}

/// One message in a [`NeutralConversation`]: a role plus its text content.
///
/// Text is already flattened — any structured content from the source
/// (multiple content blocks, tool output, etc.) is concatenated into a single
/// string by the mapper that produced this message. This keeps continuation
/// rendering trivial and provider-agnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeutralMessage {
    /// Who spoke.
    pub role: Role,
    /// The flattened text of the turn.
    pub text: String,
}

impl NeutralMessage {
    /// Construct a message from any string-like content.
    #[must_use]
    pub fn new(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
        }
    }
}

/// A provider-agnostic transcript: an ordered list of [`NeutralMessage`]s.
///
/// This is the canonical representation Bluey continues a conversation *from*,
/// regardless of which agent produced it or which agent will consume it next.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NeutralConversation {
    /// The conversation's messages, oldest first.
    pub messages: Vec<NeutralMessage>,
}

impl NeutralConversation {
    /// An empty conversation.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a message, returning `self` for chaining.
    #[must_use]
    pub fn with_message(mut self, message: NeutralMessage) -> Self {
        self.messages.push(message);
        self
    }

    /// Append a message in place.
    pub fn push(&mut self, role: Role, text: impl Into<String>) {
        self.messages.push(NeutralMessage::new(role, text));
    }

    /// `true` when the conversation has no messages.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Number of messages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.messages.len()
    }
}

// --- Conversions to/from the crate's on-disk `Transcript`/`Role` model. -----
//
// These are the bridge between "what a session reader decoded" and "the thing
// we continue from". They are total (never fail) and lossless for the fields
// the neutral model carries (role + text).

impl From<CrateRole> for Role {
    fn from(role: CrateRole) -> Self {
        match role {
            CrateRole::User => Role::User,
            CrateRole::Assistant => Role::Assistant,
            // System *and* the catch-all `Other` collapse to System: an
            // unclassified turn is grounding context, not a user/assistant
            // utterance.
            CrateRole::System | CrateRole::Other => Role::System,
        }
    }
}

impl From<Role> for CrateRole {
    fn from(role: Role) -> Self {
        match role {
            Role::User => CrateRole::User,
            Role::Assistant => CrateRole::Assistant,
            Role::System => CrateRole::System,
        }
    }
}

impl From<Turn> for NeutralMessage {
    fn from(turn: Turn) -> Self {
        NeutralMessage {
            role: turn.role.into(),
            text: turn.text,
        }
    }
}

impl From<NeutralMessage> for Turn {
    fn from(message: NeutralMessage) -> Self {
        Turn {
            role: message.role.into(),
            text: message.text,
        }
    }
}

impl From<Transcript> for NeutralConversation {
    fn from(transcript: Transcript) -> Self {
        NeutralConversation {
            messages: transcript.turns.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<NeutralConversation> for Transcript {
    fn from(conversation: NeutralConversation) -> Self {
        Transcript {
            turns: conversation.messages.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_role_other_maps_to_system() {
        // The explicit requirement: Role::Other → System.
        assert_eq!(Role::from(CrateRole::Other), Role::System);
        assert_eq!(Role::from(CrateRole::System), Role::System);
        assert_eq!(Role::from(CrateRole::User), Role::User);
        assert_eq!(Role::from(CrateRole::Assistant), Role::Assistant);
    }

    #[test]
    fn neutral_role_maps_back_to_crate_role() {
        // The neutral model has no `Other`, so the round-trip for System lands
        // on System (not Other) — this is the documented, lossy direction.
        assert_eq!(CrateRole::from(Role::User), CrateRole::User);
        assert_eq!(CrateRole::from(Role::Assistant), CrateRole::Assistant);
        assert_eq!(CrateRole::from(Role::System), CrateRole::System);
    }

    #[test]
    fn transcript_converts_to_neutral_conversation_preserving_order_and_text() {
        let transcript = Transcript {
            turns: vec![
                Turn {
                    role: CrateRole::System,
                    text: "you are a helpful agent".into(),
                },
                Turn {
                    role: CrateRole::User,
                    text: "fix the bug".into(),
                },
                Turn {
                    role: CrateRole::Assistant,
                    text: "done".into(),
                },
                Turn {
                    role: CrateRole::Other,
                    text: "tool ran".into(),
                },
            ],
        };

        let conv: NeutralConversation = transcript.into();

        assert_eq!(conv.len(), 4);
        assert_eq!(conv.messages[0].role, Role::System);
        assert_eq!(conv.messages[0].text, "you are a helpful agent");
        assert_eq!(conv.messages[1].role, Role::User);
        assert_eq!(conv.messages[1].text, "fix the bug");
        assert_eq!(conv.messages[2].role, Role::Assistant);
        assert_eq!(conv.messages[2].text, "done");
        // Role::Other collapsed to System.
        assert_eq!(conv.messages[3].role, Role::System);
        assert_eq!(conv.messages[3].text, "tool ran");
    }

    #[test]
    fn neutral_conversation_converts_back_to_transcript() {
        let conv = NeutralConversation::new()
            .with_message(NeutralMessage::new(Role::User, "hi"))
            .with_message(NeutralMessage::new(Role::Assistant, "hello"));

        let transcript: Transcript = conv.into();

        assert_eq!(transcript.turns.len(), 2);
        assert_eq!(transcript.turns[0].role, CrateRole::User);
        assert_eq!(transcript.turns[0].text, "hi");
        assert_eq!(transcript.turns[1].role, CrateRole::Assistant);
        assert_eq!(transcript.turns[1].text, "hello");
    }

    #[test]
    fn round_trip_through_neutral_is_stable_for_user_assistant_system() {
        // User/Assistant/System survive a full Transcript → Neutral → Transcript
        // round-trip unchanged (Other is the only lossy role, by design).
        let original = Transcript {
            turns: vec![
                Turn {
                    role: CrateRole::System,
                    text: "ctx".into(),
                },
                Turn {
                    role: CrateRole::User,
                    text: "q".into(),
                },
                Turn {
                    role: CrateRole::Assistant,
                    text: "a".into(),
                },
            ],
        };
        let round_tripped: Transcript = NeutralConversation::from(original.clone()).into();
        assert_eq!(round_tripped, original);
    }

    #[test]
    fn empty_transcript_yields_empty_conversation() {
        let conv: NeutralConversation = Transcript::default().into();
        assert!(conv.is_empty());
        assert_eq!(conv.len(), 0);
    }
}
