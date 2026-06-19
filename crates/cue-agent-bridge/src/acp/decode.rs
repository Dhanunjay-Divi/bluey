//! Session-store → [`NeutralConversation`] glue (Phase 2).
//!
//! Cross-agent continuation needs to read *any* agent's past session into the
//! provider-agnostic [`NeutralConversation`] so it can be replayed into a
//! *different* agent. Almost the entire path already exists from Phase 1:
//!
//! - the per-agent [`crate::sessions`] readers decode an on-disk store into the
//!   crate's [`crate::Transcript`], and
//! - [`super::neutral`] supplies a lossless `From<Transcript>` for
//!   [`NeutralConversation`].
//!
//! This module is the thin, obvious seam between the two: one call that, given a
//! [`SessionFormat`] + a located [`SessionStore`] + a session id, hands back the
//! neutral conversation, plus a trivial transcript→neutral entry point so
//! callers never have to reach for `.into()` directly.
//!
//! It also hosts [`encode_claude_project_dir`] — the canonical, public, tested
//! form of Claude Code's project-dir path encoding (see its doc for why it lives
//! here and how it relates to the existing private encoder in
//! [`crate::sessions`]).

use crate::{reader_for, SessionFormat, SessionStore, Transcript};

use super::NeutralConversation;

/// Default upper bound on turns when a caller does not specify one.
///
/// [`crate::SessionReader::read`] requires an explicit `max_turns` (there is no
/// unbounded read by design — the multi-GB `vscdb` store must never be slurped).
/// Continuation seeds the *whole* conversation, so this is generous; callers
/// that want a tighter bound use [`load_session_neutral_bounded`].
pub const DEFAULT_MAX_TURNS: usize = 100_000;

/// Wrap an already-decoded [`Transcript`] as a [`NeutralConversation`].
///
/// This is the one obvious entry point for the common case where a caller
/// already holds a `Transcript` (e.g. it drove a reader itself) and just wants
/// the neutral form. It is a total, lossless conversion — it delegates to the
/// Phase-1 `From<Transcript>` impl in [`super::neutral`].
#[must_use]
pub fn neutral_from_transcript(transcript: Transcript) -> NeutralConversation {
    transcript.into()
}

/// Read a single session from its on-disk store into a [`NeutralConversation`],
/// bounded to [`DEFAULT_MAX_TURNS`] turns.
///
/// This reuses the existing per-format readers verbatim ([`reader_for`] +
/// [`crate::SessionReader::read`]) — it does **not** re-implement any
/// SQLite/JSONL parsing. The decoded [`Transcript`] is then converted with the
/// lossless Phase-1 `From` impl.
///
/// `session_ref` is the store-local session **id** the reader keys on (the same
/// value carried by [`crate::SessionRef::id`] from a prior listing) — for the
/// JSONL store that is the `<session-id>` of `…/<session-id>.jsonl`; for the
/// `vscdb` store it is the composer/session row id; and so on per format.
///
/// # Errors
///
/// Propagates any error from the underlying reader (e.g. an unreadable or
/// missing store). The readers are fail-soft *within* a session — a malformed
/// line/row is skipped, not fatal — so a successful call against a store with no
/// matching/decodable turns yields an empty conversation rather than an error.
pub fn load_session_neutral(
    format: SessionFormat,
    store: &SessionStore,
    session_ref: &str,
) -> anyhow::Result<NeutralConversation> {
    load_session_neutral_bounded(format, store, session_ref, DEFAULT_MAX_TURNS)
}

/// Like [`load_session_neutral`], but with an explicit `max_turns` cap.
///
/// Use this when seeding continuation from only the tail of a long
/// conversation, or to keep a preview cheap. `max_turns` is passed straight to
/// [`crate::SessionReader::read`], which honors it at the source (the `vscdb`
/// store applies it as a SQL `LIMIT`, never loading the whole DB).
///
/// # Errors
///
/// Propagates any error from the underlying reader; see [`load_session_neutral`].
pub fn load_session_neutral_bounded(
    format: SessionFormat,
    store: &SessionStore,
    session_ref: &str,
    max_turns: usize,
) -> anyhow::Result<NeutralConversation> {
    let transcript = reader_for(format).read(store, session_ref, max_turns)?;
    Ok(neutral_from_transcript(transcript))
}

/// Encode an absolute project path into Claude Code's on-disk project-dir name.
///
/// Claude Code stores each project's sessions at
/// `~/.claude/projects/<ENCODED_CWD>/<session-id>.jsonl`, where `ENCODED_CWD` is
/// the absolute project path with **every non-alphanumeric character** replaced
/// by `-` (the Agent SDK source uses the regex `[^a-zA-Z0-9] → -`). So:
///
/// - `/Users/me/proj` → `-Users-me-proj`
/// - `/Users/ms/Developer/Divi's Agenda` → `-Users-ms-Developer-Divi-s-Agenda`
///   (the apostrophe **and** the space **and** every slash all become `-`)
/// - a `/.hidden` segment → `--hidden` (slash then dot, both non-alphanumeric)
///
/// The subtlety is that a *naive* `path.replace('/', "-")` is **wrong**: it
/// leaves spaces, dots, apostrophes, underscores, and parentheses untouched, so
/// for any project whose path contains one of those it computes a directory that
/// does not exist on disk → the resume reads an empty transcript.
///
/// # Relationship to existing code
///
/// This is the **canonical, public** home for the encoding. The Phase-1
/// [`crate::sessions`] code (`claude_app::encode_project_dir`) already
/// implements the *correct* full-non-alphanumeric rule — its predecessor's
/// `/`-and-`.`-only bug was fixed in Phase 1, with tests — but that function is
/// private to its module and so is not reusable by continuation code. The
/// naive-replace bug is therefore **not present** in the live readers; there is
/// nothing to rip out. When the reconciliation step lands, the private
/// `claude_app::encode_project_dir` should be collapsed onto this one public
/// helper so there is a single source of truth; until then the two are
/// byte-for-byte equivalent and covered by the same tricky-case tests.
#[must_use]
pub fn encode_claude_project_dir(abs_path: &str) -> String {
    abs_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Role as CrateRole, Transcript, Turn};

    #[test]
    fn encode_claude_project_dir_plain_path_just_swaps_slashes() {
        assert_eq!(
            encode_claude_project_dir("/Users/me/proj"),
            "-Users-me-proj"
        );
        assert_eq!(
            encode_claude_project_dir("/Users/ms/Developer/Bluey"),
            "-Users-ms-Developer-Bluey"
        );
    }

    #[test]
    fn encode_claude_project_dir_handles_space_apostrophe_slash() {
        // The headline tricky case: apostrophe AND space AND slash all → '-'.
        assert_eq!(
            encode_claude_project_dir("/Users/ms/Developer/Divi's Agenda"),
            "-Users-ms-Developer-Divi-s-Agenda"
        );
    }

    #[test]
    fn encode_claude_project_dir_handles_dot_hidden_segment() {
        // A '/.hidden' segment: slash then dot, both non-alphanumeric → "--".
        assert_eq!(
            encode_claude_project_dir("/Users/ms/.config/x"),
            "-Users-ms--config-x"
        );
        // A trailing extension dot is encoded too.
        assert_eq!(encode_claude_project_dir("/a/b.rs"), "-a-b-rs");
    }

    #[test]
    fn encode_claude_project_dir_handles_underscore_and_parens() {
        assert_eq!(
            encode_claude_project_dir("/a/my_proj (v2)"),
            "-a-my-proj--v2-"
        );
    }

    #[test]
    fn encode_claude_project_dir_differs_from_naive_slash_replace_on_tricky_paths() {
        // Guard the actual contract: the correct encoding must differ from a
        // naive `.replace('/', "-")` exactly when the path has a non-alnum,
        // non-slash char (here: the space and the apostrophe).
        let path = "/Users/ms/Developer/Divi's Agenda";
        let naive = path.replace('/', "-");
        assert_ne!(encode_claude_project_dir(path), naive);
        // ...and a path of only letters/digits/slashes is unaffected by the
        // difference (the correct encoding is a strict superset of the naive one).
        let plain = "/Users/me/proj";
        assert_eq!(encode_claude_project_dir(plain), plain.replace('/', "-"));
    }

    #[test]
    fn neutral_from_transcript_round_trips_roles_and_text_in_order() {
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
                    // The catch-all role collapses to System in the neutral model.
                    role: CrateRole::Other,
                    text: "tool ran".into(),
                },
            ],
        };

        let conv = neutral_from_transcript(transcript);

        assert_eq!(conv.len(), 4);
        assert_eq!(conv.messages[0].role, super::super::Role::System);
        assert_eq!(conv.messages[0].text, "you are a helpful agent");
        assert_eq!(conv.messages[1].role, super::super::Role::User);
        assert_eq!(conv.messages[1].text, "fix the bug");
        assert_eq!(conv.messages[2].role, super::super::Role::Assistant);
        assert_eq!(conv.messages[2].text, "done");
        // Role::Other collapsed to System (documented lossy direction).
        assert_eq!(conv.messages[3].role, super::super::Role::System);
        assert_eq!(conv.messages[3].text, "tool ran");
    }

    #[test]
    fn neutral_from_empty_transcript_is_empty() {
        let conv = neutral_from_transcript(Transcript::default());
        assert!(conv.is_empty());
        assert_eq!(conv.len(), 0);
    }
}
