//! Session-continuation spine: the policy for turning a pinned prior session
//! into a ready-to-drive prompt.
//!
//! Continuing a conversation has two halves:
//!
//! - **Compaction** ([`compaction`]) — when a Replay-tier transcript is too big
//!   to replay whole, summarize its older turns (via a drive through the user's
//!   OWN agent — Bluey runs no AI) and keep the recent "hot layer" verbatim.
//! - **Recoverability** ([`recoverable`]) — classify an agent's resume error so
//!   the caller knows whether a fresh (no-resume) retry can still answer.
//!
//! The tier *decision* (NativeResume vs Replay, true-resume vs fork) is data on
//! the registry row ([`crate::registry::ContinuationTier`]); this module is the
//! mechanism that decision drives. Kept dependency-light and agent-agnostic —
//! nothing here names a specific agent.

pub mod compaction;
pub mod recoverable;
pub mod tier;

pub use compaction::{
    maybe_compact, split_for_compaction, summarize_older_prompt, transcript_chars,
};
pub use recoverable::is_resume_recoverable_error;
pub use tier::{
    apply_tier, continuation_bridge_kind, resolve_session, CONTINUATION_READ_MAX_TURNS,
};
