//! Per-agent session-store readers (Slice 3).
//!
//! Each installed agent persists its conversation history in its own on-disk
//! format (see `PLAN-AGENT-BRIDGE.md` §3). This module defines a single
//! [`SessionReader`] abstraction and one decoder per [`SessionFormat`]:
//!
//! - [`jsonl`] — Claude Code / Codex newline-delimited JSON.
//! - [`vscdb`] — Cursor / VS Code-family `state.vscdb` SQLite store.
//! - [`json_files`] — VS Code / Copilot `chatSessions/*.json` files.
//! - [`antigravity`] — Antigravity plaintext conversation index (+ SQLite/JSONL
//!   bodies).
//!
//! Security invariants enforced by every decoder (design §6):
//!
//! - **Read-only.** SQLite opens with `SQLITE_OPEN_READ_ONLY` + `immutable=1`;
//!   file formats read via `std::fs` only. Nothing here ever writes, locks, or
//!   spawns a subprocess.
//! - **Bounded & lazy.** `limit` and `max_turns` are honored; the multi-GB
//!   `vscdb` is queried with SQL `LIMIT`, never slurped into memory.
//! - **Fail-soft.** A malformed line / row / file is skipped; the rest still
//!   returns. No decoder panics on bad input, and none uses `unwrap`/`expect`.
//! - **No secrets.** Only conversation text, titles, and timestamps are read.

use crate::{SessionFormat, SessionRef, SessionStore, Transcript};

pub mod antigravity;
pub mod antigravity_ide;
pub mod claude_app;
pub mod json_files;
pub mod jsonl;
pub mod summaries;
pub mod vscdb;

/// The maintainability signal for a reader against a real store. Session reading
/// has no UNIFORM protocol (ACP `session/list` shipped in Cursor/Zed in 2026 but
/// can't yet cover all six agent surfaces), so every reader reverse-engineers the
/// vendor's undocumented on-disk format — and vendors change those formats on
/// roughly every major/minor (Claude v2.1.128 nulled the legacy `messages` field
/// → P0 data loss; Cursor moved `ItemTable`→`cursorDiskKV` across 3 generations;
/// Codex's `RolloutLine`/`thread_source` broke old rollouts; Antigravity 2.0 split
/// into two dirs). So format drift is an OPERATIONAL CERTAINTY, not paranoia.
///
/// The fail-soft readers all return an empty list on a problem, which is correct
/// for the user but makes a genuinely-empty store and a SILENTLY-DRIFTED format
/// look identical. [`ReaderHealth`] disambiguates them with a **parse-success
/// RATIO** (`parsed` of `raw_total`) — the research-confirmed drift signal: a
/// store that holds raw records but parses 0 is the page-worthy break; a declining
/// ratio is the early warning. (Zero-based "flags empty" was rejected — it's a
/// false negative on real drift and a false positive on a legitimately empty
/// store.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReaderHealth {
    /// The store path doesn't exist / is empty on disk — nothing to read, and not
    /// a format problem (the agent simply has no sessions here).
    EmptyStore,
    /// The store holds `raw_total` raw records (files / rows / index entries) and
    /// the reader successfully parsed `parsed` of them. `parsed == raw_total` is
    /// healthy; `parsed < raw_total` means some records were skipped (per-row
    /// degradation — the rest still surface); **`parsed == 0` while
    /// `raw_total > 0` is the DRIFT signal** (the format changed under us). The
    /// canary asserts on this ratio, never on absolute zero.
    Parsed { parsed: usize, raw_total: usize },
}

impl ReaderHealth {
    /// The fraction of raw records the reader understood (1.0 = all, 0.0 = total
    /// drift). `EmptyStore` is `1.0` (nothing to parse is not a failure).
    pub fn parse_ratio(&self) -> f64 {
        match self {
            ReaderHealth::EmptyStore => 1.0,
            ReaderHealth::Parsed { raw_total: 0, .. } => 1.0,
            ReaderHealth::Parsed { parsed, raw_total } => *parsed as f64 / *raw_total as f64,
        }
    }

    /// The page-worthy drift signal: the store has raw records but the reader
    /// understood NONE of them — the format almost certainly changed.
    pub fn is_total_drift(&self) -> bool {
        matches!(self, ReaderHealth::Parsed { parsed: 0, raw_total } if *raw_total > 0)
    }
}

/// Decodes one agent's session store, read-only and bounded.
///
/// Implementations must be fail-soft (skip malformed input) and must never
/// write to or lock the underlying store.
pub trait SessionReader {
    /// The `limit` most-recent sessions, cheaply — avoid full-body decode where
    /// the format permits (file mtime / metadata only).
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>>;

    /// Decode a single session identified by `id` into a normalized
    /// [`Transcript`], bounded to at most `max_turns` turns.
    fn read(&self, store: &SessionStore, id: &str, max_turns: usize) -> anyhow::Result<Transcript>;

    /// Drift health against a real store: count the raw records present and how
    /// many parsed, as a [`ReaderHealth::Parsed`] ratio. Each reader overrides
    /// this with a cheap count of its own raw unit (jsonl files in the dir,
    /// `cursorDiskKV` composer rows, index entries, …) so the canary can tell
    /// "genuinely empty" from "format changed under us." The DEFAULT is a
    /// conservative fallback that treats `list()` output as both parsed and raw
    /// (so it never false-alarms, but also can't detect drift) — readers SHOULD
    /// override it.
    fn health(&self, store: &SessionStore) -> ReaderHealth {
        if !store.path.exists() {
            return ReaderHealth::EmptyStore;
        }
        let n = self.list(store, usize::MAX).map(|v| v.len()).unwrap_or(0);
        ReaderHealth::Parsed {
            parsed: n,
            raw_total: n,
        }
    }
}

/// List a store's sessions AND surface format-drift in one call: runs the
/// reader, then checks [`SessionReader::health`] and emits exactly ONE structured
/// `unrecognized_format` warning if the store holds raw records but the reader
/// understood none of them ([`ReaderHealth::is_total_drift`]). This is the
/// production list entry point — bare `list()` stays available for callers that
/// don't want the health side effect (tests, the canary, which asserts directly).
///
/// Per-row degradation is already handled inside every reader (a malformed
/// line/row/file is skipped, the rest still return); this adds the MISSING piece:
/// turning a silent all-empty result into a single actionable log line naming the
/// agent, the store path, and the parse ratio, so a vendor format change is
/// caught in telemetry the day it ships rather than weeks later.
pub fn list_with_health_check(
    format: SessionFormat,
    store: &SessionStore,
    limit: usize,
) -> anyhow::Result<Vec<SessionRef>> {
    let reader = reader_for(format);
    let refs = reader.list(store, limit)?;
    // Probe health when the listing is empty OR when every row came back
    // title-less. The latter matters for the resilient file readers (jsonl):
    // they ALWAYS surface a row from id + mtime, so a body-format break shows up
    // not as an empty list but as rows that all lost their titles. A populated,
    // titled listing is self-evidently healthy and skips the re-scan, keeping the
    // common path free.
    let all_untitled = !refs.is_empty() && refs.iter().all(|r| r.title.is_none());
    if refs.is_empty() || all_untitled {
        let health = reader.health(store);
        if health.is_total_drift() {
            if let ReaderHealth::Parsed { parsed, raw_total } = health {
                tracing::warn!(
                    target: "cue_agent_bridge::sessions",
                    event = "unrecognized_format",
                    agent = ?format,
                    store_path = %store.path.display(),
                    parsed,
                    raw_total,
                    "session store holds {raw_total} raw record(s) but the reader \
                     parsed none — the on-disk format likely changed; sessions for \
                     this agent will be missing or bodyless until the reader is \
                     updated"
                );
            }
        }
    }
    Ok(refs)
}

/// Return the decoder for a given storage [`SessionFormat`].
pub fn reader_for(format: SessionFormat) -> Box<dyn SessionReader> {
    match format {
        SessionFormat::Jsonl => Box::new(jsonl::JsonlReader),
        SessionFormat::SqliteVscdb => Box::new(vscdb::VscdbReader),
        SessionFormat::JsonFiles => Box::new(json_files::JsonFilesReader),
        SessionFormat::AntigravityIndex => Box::new(antigravity::AntigravityReader),
        SessionFormat::AntigravityIdeIndex => Box::new(antigravity_ide::AntigravityIdeReader),
        SessionFormat::ClaudeAppIndex => Box::new(claude_app::ClaudeAppReader),
    }
}

/// Minimal percent-decoder for `file://` URL paths (`%20` → space, …). Shared so
/// every reader decodes project paths the same way. UTF-8-lossy, fail-soft: a
/// bad/short `%` escape is left as-is rather than panicking.
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Best-effort last-modified marker for a path, as an epoch-seconds string.
///
/// `SessionRef::updated_at` is documented as "RFC3339 or epoch string, per
/// source"; the file-backed decoders use epoch seconds to stay dependency-free.
/// Returns `"0"` when the mtime is unavailable rather than failing the listing.
pub(crate) fn mtime_epoch_string(path: &std::path::Path) -> String {
    use std::time::UNIX_EPOCH;
    let secs = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

/// Truncate a single-line snippet for use as a session title.
///
/// Collapses internal whitespace and caps length so a title never carries a
/// huge message body. Char-boundary safe.
pub(crate) fn snippet(text: &str, max_chars: usize) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(max_chars).collect();
    out.push('…');
    out
}

/// Whether a candidate first-message is **boilerplate**, not a real topic —
/// shared by every reader so all agents reject the same junk titles. Catches
/// session-continuation banners, system/agent prompts, and Bluey's own
/// propose/apply/summary prompts that would otherwise become the "title".
///
/// Cross-agent by design: Claude, Codex, Cursor, VS Code all surface some of
/// these, so the rule lives here, not per-reader.
pub(crate) fn is_boilerplate_title(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_lowercase();
    const MARKERS: &[&str] = &[
        "this session is being continued",
        "continued from a previous conversation",
        // Bluey's OWN generated turns must never become titles: the Replay banner
        // prepended to fork/replay continuations, and the 8-word summary prompt
        // the titler/compactor sends. Both were leaking as titles across
        // Copilot/Gemini/Codex.
        "context from a prior conversation",
        "summarize this conversation",
        "summarize the earlier part of our conversation", // the compaction prompt
        "you are proposing a fix",
        "propose-only",
        "===diagnosis===",
        "one sentence: what is this session about", // Bluey's own summary prompt
        "summarize what this",
        "this is the gemini cli",
        "<session_context>",
        "<environment_context>", // Codex injects this as the first turn
        "<cwd>",
        "<user_instructions>",
        "# agents.md instructions for", // Codex injects this AGENTS.md context block
        "you are a ",                   // system role prompts ("You are a Rust architect…")
        "you are the github copilot cli", // Copilot system prompt
        "system:",
        "caveat: the messages below", // Claude Code system caveat banner
    ];
    MARKERS
        .iter()
        .any(|m| lower.starts_with(m) || lower.contains(m))
}

/// A meaningful fallback label when no real title is available — uses the
/// project's last path segment (e.g. `…/Developer/Bluey` → "Bluey") so the row
/// is identifiable instead of blank or a raw id/filename. Returns `None` only
/// when there is no project to derive from (the caller then shows a short id).
/// `_updated_at` is accepted for a future date suffix (no date lib in-tree yet).
pub(crate) fn fallback_label(project: Option<&str>, _updated_at: &str) -> Option<String> {
    let project = project?;
    let leaf = project
        .trim_end_matches('/')
        .rsplit('/')
        .find(|s| !s.is_empty())?;
    if leaf.is_empty() {
        None
    } else {
        Some(format!("{leaf} session"))
    }
}

/// Pick a human title for a session: the candidate first-message if it is a
/// real topic; else `None` so the caller falls back to project + date. Applied
/// uniformly across readers.
pub(crate) fn clean_title(candidate: Option<String>, max_chars: usize) -> Option<String> {
    let text = strip_wrapper_tags(&candidate?);
    if is_boilerplate_title(&text) {
        return None;
    }
    let s = snippet(&text, max_chars);
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Strip XML-ish wrapper tags some agents wrap user content in (e.g.
/// Antigravity's `<USER_REQUEST>…</USER_REQUEST>`, Codex's `<user_instructions>`)
/// so the title shows the actual message, not the tag. Generic — applies to any
/// agent; leaves normal text (and inline `<` in code) intact by only removing
/// whole `<TAG>`/`</TAG>` tokens.
pub(crate) fn strip_wrapper_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            // Consume a tag only if it looks like <word ...> or </word> — a
            // letter/slash right after '<'. Otherwise keep the '<' (e.g. `a < b`).
            if matches!(chars.peek(), Some(n) if n.is_ascii_alphabetic() || *n == '/') {
                for t in chars.by_ref() {
                    if t == '>' {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{is_boilerplate_title, list_with_health_check, ReaderHealth};
    use crate::{SessionFormat, SessionStore};
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A minimal tracing layer that counts WARN events whose message mentions the
    /// `unrecognized_format` drift signal — so the canary warning is asserted to
    /// fire EXACTLY once on drift and zero times on a healthy store.
    struct DriftWarnCounter(Arc<AtomicUsize>);

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for DriftWarnCounter {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if *event.metadata().level() != tracing::Level::WARN {
                return;
            }
            struct Vis(bool);
            impl tracing::field::Visit for Vis {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "event"
                        && format!("{value:?}").contains("unrecognized_format")
                    {
                        self.0 = true;
                    }
                }
                fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                    if field.name() == "event" && value.contains("unrecognized_format") {
                        self.0 = true;
                    }
                }
            }
            let mut v = Vis(false);
            event.record(&mut v);
            if v.0 {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn count_drift_warnings(body: impl FnOnce()) -> usize {
        use tracing_subscriber::prelude::*;
        let n = Arc::new(AtomicUsize::new(0));
        let subscriber = tracing_subscriber::registry().with(DriftWarnCounter(Arc::clone(&n)));
        tracing::subscriber::with_default(subscriber, body);
        n.load(Ordering::SeqCst)
    }

    fn jsonl_store(path: std::path::PathBuf) -> SessionStore {
        SessionStore {
            path,
            format: SessionFormat::Jsonl,
        }
    }

    #[test]
    fn list_with_health_check_warns_once_on_body_drift() {
        // Files present + listable (id + mtime), but no body decodes → the
        // wrapper must emit the structured warning exactly once.
        let dir = tempfile::tempdir().expect("tempdir");
        for i in 0..3 {
            let mut f = std::fs::File::create(dir.path().join(format!("s{i}.jsonl"))).unwrap();
            writeln!(f, r#"{{"v":2,"unknownEnvelope":{{"x":true}}}}"#).unwrap();
        }
        let store = jsonl_store(dir.path().to_path_buf());

        let warnings = count_drift_warnings(|| {
            let refs = list_with_health_check(SessionFormat::Jsonl, &store, 100).expect("list");
            // Rows still surface (sessions exist) — drift is about the BODY.
            assert_eq!(refs.len(), 3);
        });
        assert_eq!(
            warnings, 1,
            "exactly one unrecognized_format warning on drift"
        );
    }

    #[test]
    fn list_with_health_check_is_silent_on_healthy_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut f = std::fs::File::create(dir.path().join("good.jsonl")).unwrap();
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"real question"}}}}"#
        )
        .unwrap();
        let store = jsonl_store(dir.path().to_path_buf());

        let warnings = count_drift_warnings(|| {
            let refs = list_with_health_check(SessionFormat::Jsonl, &store, 100).expect("list");
            assert_eq!(refs.len(), 1);
            assert!(refs[0].title.is_some(), "healthy store has a titled row");
        });
        assert_eq!(warnings, 0, "no warning when the store parses fine");
    }

    #[test]
    fn list_with_health_check_is_silent_on_empty_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = jsonl_store(dir.path().to_path_buf());
        let warnings = count_drift_warnings(|| {
            let refs = list_with_health_check(SessionFormat::Jsonl, &store, 100).expect("list");
            assert!(refs.is_empty());
        });
        assert_eq!(warnings, 0, "an empty store is not drift");
    }

    #[test]
    fn reader_health_ratio_and_drift_math() {
        // Empty store: not a failure, ratio 1.0, not drift.
        let e = ReaderHealth::EmptyStore;
        assert_eq!(e.parse_ratio(), 1.0);
        assert!(!e.is_total_drift());

        // All parsed: ratio 1.0, not drift.
        let all = ReaderHealth::Parsed {
            parsed: 7,
            raw_total: 7,
        };
        assert_eq!(all.parse_ratio(), 1.0);
        assert!(!all.is_total_drift());

        // Partial: per-row degradation — some skipped, NOT total drift.
        let partial = ReaderHealth::Parsed {
            parsed: 3,
            raw_total: 4,
        };
        assert_eq!(partial.parse_ratio(), 0.75);
        assert!(!partial.is_total_drift());

        // The drift signal: raw records present, none parsed.
        let drift = ReaderHealth::Parsed {
            parsed: 0,
            raw_total: 5,
        };
        assert_eq!(drift.parse_ratio(), 0.0);
        assert!(drift.is_total_drift());

        // Zero raw is divide-by-zero-safe and not drift (genuinely empty).
        let zero_raw = ReaderHealth::Parsed {
            parsed: 0,
            raw_total: 0,
        };
        assert_eq!(zero_raw.parse_ratio(), 1.0);
        assert!(!zero_raw.is_total_drift());
    }

    #[test]
    fn bluey_own_generated_turns_are_boilerplate_not_titles() {
        // C7: Bluey's own Replay banner + summary/compaction prompts were leaking
        // as session titles across Copilot/Gemini/Codex. They must classify as
        // boilerplate so the reader falls back to a real title or a sane default.
        for junk in [
            "Context from a prior conversation: User: what was the bug?",
            "context from a prior conversation\nNote: ...",
            "Summarize this conversation's topic in at most 8 words. Reply with only the title.",
            "Summarize the earlier part of our conversation below in at most 400 words.",
        ] {
            assert!(
                is_boilerplate_title(junk),
                "should be boilerplate, was treated as a real title: {junk:?}"
            );
        }
        // A genuine user title must NOT be flagged.
        assert!(!is_boilerplate_title("Fix the chunker boundary bug"));
        assert!(!is_boilerplate_title("Refactor the auth module"));
    }
}
