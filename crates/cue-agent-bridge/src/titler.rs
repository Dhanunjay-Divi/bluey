//! Session **titler** — give every listed session a human title, mechanically
//! first and only falling back to AI when the mechanical result is noise.
//!
//! ## Why this exists
//!
//! When Bluey lists a coding agent's prior sessions so the user can pick one,
//! the quality of the per-agent "title" varies wildly (observed on real
//! machines, see `docs/work` notes):
//!
//! - **claude_code_app** — already has a good pre-computed title ("Bluey
//!   repository setup"). Nothing to do: pass it through.
//! - **claude_code (CLI)** — the raw first user message ("In the Autonomous
//!   Staffing Desk repo (/Users/…)"): readable but verbose. *Mechanical* clean
//!   is enough.
//! - **cursor** — the raw first line, often noise: a shell paste
//!   ("ms@Mac ~ % ssh jetson-orin…") or a typo'd question. *Sometimes*
//!   mechanical, sometimes noise.
//! - **codex** — a system/test prompt ("Reply with exactly … LIVEPROOF7",
//!   `rollout-…`). *Useless*: must fall back.
//! - **copilot** — one identical generic string for every session ("GitHub
//!   Copilot CLI session events"). *Useless*: must fall back.
//!
//! ## The policy (product decision): **mechanical first, AI only if noisy**
//!
//! 1. If the agent already gave a real title, use it ([`TitleSource::Provided`]).
//! 2. Else derive a [`mechanical_title`] from the first user message — strip
//!    paths, shell prompts, wrapper tags, collapse whitespace, drop boilerplate.
//!    Reuses the shared [`crate::sessions`] helpers so every agent rejects the
//!    same junk. ([`TitleSource::Mechanical`].)
//! 3. Else, **only when** the mechanical candidate is obvious noise (a
//!    canary/test prompt, a generic identical string, a shell paste, a path,
//!    gibberish, or empty), ask the **user's own attached agent** to summarize
//!    the first few turns into a short title ([`TitleSource::AiGenerated`]).
//! 4. Else, a meaningful [`crate::sessions::fallback_label`] derived from the
//!    project path ([`TitleSource::Fallback`]).
//!
//! ## Hard invariant
//!
//! **Bluey's own AI is never invoked here.** The only LLM call in this module is
//! [`ai_title_via_agent`], which drives the *user's* already-attached agent over
//! the user's *own* session via [`crate::drive`]. That call is bounded by a
//! short timeout and fails soft (returns `None`) on any error — a missing title
//! is never fatal to listing sessions.

use std::time::{Duration, Instant};

use futures_util::StreamExt;

use crate::sessions::{fallback_label, is_boilerplate_title, snippet, strip_wrapper_tags};
use crate::{AgentKind, AnswerChunk, Question};

/// Max words a derived (mechanical or AI) title should carry. A title is a
/// glance-able label, not a sentence; the product brief caps AI titles at 8.
pub const MAX_TITLE_WORDS: usize = 8;

/// Max characters for a mechanical snippet before it is elided. Keeps a verbose
/// first message (e.g. the Claude CLI's path-prefixed opener) from becoming a
/// wall of text in the picker.
const MAX_TITLE_CHARS: usize = 60;

/// Wall-clock cap for the single AI titling call. A one-line summary returns
/// well within this; past it we abandon the call and fall through rather than
/// block the session list. Deliberately short — titling is best-effort UI sugar.
const AI_TITLE_TIMEOUT: Duration = Duration::from_secs(20);

/// Where a session's final title came from. Surfaced to the harness/UI so a
/// reviewer can see *why* a row reads the way it does (and audit that AI was
/// only used where mechanical genuinely failed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleSource {
    /// The agent's own pre-computed title was good; used verbatim (cleaned).
    Provided,
    /// Derived mechanically from the first user message (no LLM).
    Mechanical,
    /// The user's *own* attached agent summarized the first turns (LLM, but
    /// never Bluey's). Reached only when mechanical yielded noise/nothing.
    AiGenerated,
    /// No usable title or topic; a project-derived placeholder label.
    Fallback,
}

impl TitleSource {
    /// A short, stable tag for logs and matrix output.
    pub fn tag(self) -> &'static str {
        match self {
            TitleSource::Provided => "provided",
            TitleSource::Mechanical => "mechanical",
            TitleSource::AiGenerated => "ai",
            TitleSource::Fallback => "fallback",
        }
    }
}

/// A session title plus the provenance of how it was produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleResult {
    pub title: String,
    pub source: TitleSource,
}

impl TitleResult {
    fn new(title: impl Into<String>, source: TitleSource) -> Self {
        Self {
            title: title.into(),
            source,
        }
    }
}

/// Phrases that betray a **canary / test / harness** prompt rather than a real
/// conversation topic. Matched case-insensitively as substrings. These are the
/// openers Bluey itself (and similar tools) inject when probing an agent — they
/// must never become a user-facing title.
const CANARY_MARKERS: &[&str] = &[
    "reply with exactly",
    "respond with exactly",
    "reply with only",
    "say exactly",
    "echo back",
    "liveproof",
    "rollout-",
    "canary",
    "this is a test prompt",
    "ignore this message",
];

/// Generic, content-free strings some agents write as a placeholder "title" for
/// *every* session (so they carry zero topic signal). Matched case-insensitively
/// against the whole trimmed string (substring), e.g. Copilot's identical
/// per-session banner.
const GENERIC_MARKERS: &[&str] = &[
    "github copilot cli session events",
    "copilot cli session",
    "session events",
    "new session",
    "untitled session",
    "untitled",
    "new conversation",
    "new chat",
];

/// Detect whether a candidate title is **noise** — i.e. it carries no real topic
/// and must not be shown to the user. Noise classes (from observed real data):
///
/// - empty / whitespace-only,
/// - a **canary/test** prompt (`"Reply with exactly … LIVEPROOF7"`, `rollout-…`),
/// - a **generic identical** string (`"GitHub Copilot CLI session events"`),
/// - a **shell paste** prefix (`"ms@Mac ~ % ssh …"`, `"user@host:~$ …"`),
/// - a **path-only** string (`"/Users/ms/Developer/Bluey"` and nothing else),
/// - **pure gibberish** with no alphabetic topic content.
///
/// Boilerplate banners (session-continuation notices, system role prompts,
/// Bluey's own propose/apply prompts) are *also* noise; that judgement is shared
/// with every session reader via [`crate::sessions::is_boilerplate_title`], so we
/// defer to it rather than duplicate its marker list here.
pub fn is_noise_title(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return true;
    }

    // Shared boilerplate judgement (continuation banners, system prompts,
    // Bluey's own propose/summary prompts, wrapper-tag-only). One source of
    // truth across the crate.
    if is_boilerplate_title(trimmed) {
        return true;
    }

    let lower = trimmed.to_lowercase();

    // Canary / harness / test prompts.
    if CANARY_MARKERS.iter().any(|m| lower.contains(m)) {
        return true;
    }

    // Generic identical placeholder strings (carry no topic).
    if GENERIC_MARKERS.iter().any(|m| lower.contains(m)) {
        return true;
    }

    // Shell-prompt paste: starts with a `user@host` shell preamble, or with a
    // bare prompt sigil. Cursor's "ms@Mac ~ % ssh …" is the canonical case.
    if looks_like_shell_paste(trimmed) {
        return true;
    }

    // Path-only: the whole thing is a single filesystem path with no prose.
    if looks_like_bare_path(trimmed) {
        return true;
    }

    // Gibberish: no run of alphabetic characters long enough to be a word, so
    // there is no topic to show (e.g. punctuation/hex soup). A typo'd *question*
    // with real words ("are oyu ble acesssupasbe mcp tools?") is NOT caught here
    // — it has words and a topic; the AI step (or the user) can make sense of it.
    if !has_wordlike_run(trimmed) {
        return true;
    }

    false
}

/// Heuristic for a pasted shell line: a leading `user@host…%`/`$`/`#` prompt, or
/// a bare leading sigil. Read-only string inspection; runs nothing.
fn looks_like_shell_paste(s: &str) -> bool {
    let first = s.lines().next().unwrap_or(s).trim();

    // `user@host … <sigil> rest` — classic copied prompt. Two shapes occur:
    //   "ms@Mac ~ % ssh foo"     (zsh: sigil is its own token)
    //   "user@host:~$ git status" (bash: sigil glued to the cwd, e.g. ":~$")
    // The reliable signal is: an '@', a hostname-ish chunk right after it, and a
    // prompt sigil ('%'/'$'/'#') somewhere before the first space, immediately
    // followed by a space. Requiring the trailing space rejects emails
    // ("a@b.com") and addresses with no command after them.
    if let Some(at) = first.find('@') {
        let after = &first[at + 1..];
        // The token right after '@' is the host (+ optional ":cwd" / "sigil").
        let host_token = after.split_whitespace().next().unwrap_or("");
        // Hostname/cwd chars legitimately include ':' '~' '/' '.' '-' '_' plus
        // the trailing sigil itself; reject anything with no host at all.
        let first_host_char_ok = host_token
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric());
        let sigil_then_space = after.contains("% ") || after.contains("$ ") || after.contains("# ");
        if first_host_char_ok && sigil_then_space {
            return true;
        }
    }

    // Bare leading sigil: "% cmd", "$ cmd", "> cmd".
    let mut chars = first.chars();
    if let Some(c0) = chars.next() {
        if matches!(c0, '%' | '$' | '>' | '#') && matches!(chars.next(), Some(' ')) {
            return true;
        }
    }

    false
}

/// Heuristic for a bare filesystem path with no surrounding prose: the trimmed
/// string is a single whitespace-free token that starts like a path and contains
/// a separator. (`mechanical_title` separately strips *inline* paths from prose.)
fn looks_like_bare_path(s: &str) -> bool {
    if s.split_whitespace().count() != 1 {
        return false;
    }
    let starts_pathish = s.starts_with('/')
        || s.starts_with("~/")
        || s.starts_with("./")
        || s.starts_with("../")
        || s.starts_with(r"\\")
        || (s.len() >= 3 && s.as_bytes()[1] == b':' && (s.starts_with('C') || s.starts_with('c')));
    starts_pathish && s.contains(['/', '\\'])
}

/// True if the string contains at least one run of >=3 alphabetic characters —
/// our proxy for "has a real word / topic". Below that it is symbol/number soup.
fn has_wordlike_run(s: &str) -> bool {
    let mut run = 0usize;
    for c in s.chars() {
        if c.is_alphabetic() {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// Remove inline filesystem paths and shell-prompt preambles from otherwise-real
/// prose, so a verbose-but-meaningful opener (the Claude CLI case) cleans up to a
/// tidy title instead of leaking `/Users/…`.
///
/// Conservative: only drops tokens that are unambiguously a path (contain a `/`
/// and start path-ish) or a leading `user@host %`/sigil preamble. Ordinary words
/// and punctuation are preserved.
fn strip_paths_and_prompts(text: &str) -> String {
    // Drop a leading shell-prompt preamble up to and including the sigil, e.g.
    // "ms@Mac ~ % ssh foo" -> "ssh foo". Only when it really looks like one.
    let mut work = text.trim().to_string();
    if let Some(idx) = work.find("% ") {
        let head = &work[..idx];
        if head.contains('@') && head.split_whitespace().count() <= 4 {
            work = work[idx + 2..].trim().to_string();
        }
    }

    // Drop path-like tokens from the remaining prose.
    let kept: Vec<&str> = work
        .split_whitespace()
        .filter(|tok| !token_is_path(tok))
        .collect();
    kept.join(" ")
}

/// Whether a single whitespace-delimited token is a filesystem path we should
/// drop from a title. Strips surrounding parens/brackets/punctuation first so
/// "(/Users/ms/x)" is recognized.
fn token_is_path(tok: &str) -> bool {
    let t = tok.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ',' | '.'));
    if !t.contains('/') {
        return false;
    }
    t.starts_with('/') || t.starts_with("~/") || t.starts_with("./") || t.starts_with("../")
}

/// Keep at most `max_words` words from a string (whitespace-delimited), preserving
/// order. Used to bound a derived title's length in *words* (the brief's unit).
fn take_words(s: &str, max_words: usize) -> String {
    s.split_whitespace()
        .take(max_words)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build a short, human title **mechanically** from a session's first user
/// message — no LLM. Strips wrapper tags and inline paths/shell preambles,
/// collapses whitespace, drops boilerplate, and caps the result to `max_words`.
///
/// Returns `None` when the input is empty or resolves to [`is_noise_title`] noise
/// (so the caller knows to fall through to the AI step). Reuses the shared
/// [`crate::sessions`] cleaners ([`strip_wrapper_tags`], [`snippet`],
/// [`is_boilerplate_title`]) so the titler and the readers agree on what "clean"
/// means.
pub fn mechanical_title(first_user_message: &str, max_words: usize) -> Option<String> {
    // 1) Unwrap any XML-ish wrapper tags agents add (Codex's
    //    <user_instructions>, Antigravity's <USER_REQUEST>, …).
    let unwrapped = strip_wrapper_tags(first_user_message);

    // 2) Drop inline paths and a leading shell-prompt preamble from real prose.
    let deprompted = strip_paths_and_prompts(&unwrapped);

    // 3) If what remains is noise (canary, generic, bare path, shell paste,
    //    gibberish, boilerplate, empty) there is no mechanical title to give.
    //    Check the *original* unwrapped text too: stripping a path off a
    //    path-only string would leave "" which is already covered, but a generic
    //    banner ("GitHub Copilot CLI session events") survives stripping and
    //    must still be rejected.
    if is_noise_title(&unwrapped) || is_noise_title(&deprompted) {
        return None;
    }

    // 4) Cap length two ways: a hard char ceiling (so one giant word can't blow
    //    the budget) and the word count (the glance-able unit the brief uses).
    //    `snippet` already collapses whitespace and adds '…' if it elides on
    //    chars; we then trim to `max_words`.
    let char_capped = snippet(&deprompted, MAX_TITLE_CHARS);
    let char_elided = char_capped.ends_with('…');
    let core = char_capped.trim_end_matches('…');

    let total_words = core.split_whitespace().count();
    let title = take_words(core, max_words);
    let title = title.trim();
    if title.is_empty() {
        return None;
    }

    // Append a single '…' if we dropped anything — either the char cap elided,
    // or the word cap left words behind. Never doubles up the mark.
    let word_elided = total_words > max_words;
    let final_title = if char_elided || word_elided {
        format!("{title}…")
    } else {
        title.to_string()
    };

    Some(final_title)
}

/// Ask the **user's own** attached agent to summarize the opening of a session
/// into a short title. This is the *only* LLM call in the titler, and it is the
/// user's agent reading the user's own conversation — **Bluey's AI is never
/// used**.
///
/// Drives `agent` via [`crate::drive`] with a tightly-scoped prompt, consumes the
/// streamed answer, and post-processes it back through [`mechanical_title`] so an
/// over-eager model (quotes, trailing punctuation, a path it echoed) still yields
/// a clean, capped title. Fails **soft**: any spawn error, terminal
/// [`AnswerChunk::Error`], timeout, or empty/again-noisy answer returns `None`,
/// and the caller falls through to [`crate::sessions::fallback_label`].
pub async fn ai_title_via_agent(agent: &AgentKind, first_turns: &[String]) -> Option<String> {
    if first_turns.iter().all(|t| t.trim().is_empty()) {
        return None;
    }

    let prompt = build_ai_prompt(first_turns);
    let started = Instant::now();

    let stream = match crate::drive(agent.clone(), Question::new(prompt)).await {
        Ok(s) => s,
        Err(_) => return None, // missing CLI / no credential / spawn failure
    };

    futures_util::pin_mut!(stream);
    let mut body = String::new();

    loop {
        let remaining = AI_TITLE_TIMEOUT.checked_sub(started.elapsed())?;
        match tokio::time::timeout(remaining, stream.next()).await {
            Err(_) => return None, // overall timeout elapsed
            Ok(None) => break,     // stream ended
            Ok(Some(chunk)) => match chunk {
                AnswerChunk::Started { .. } => {}
                AnswerChunk::Delta(d) => body.push_str(&d),
                AnswerChunk::Done { .. } => break,
                AnswerChunk::Error(_) => return None, // fail soft on agent error
            },
        }
    }

    // Take only the first non-empty line — models sometimes add a preamble or a
    // second explanatory line despite instructions.
    let first_line = body.lines().map(str::trim).find(|l| !l.is_empty())?;

    // Run the model's output back through the mechanical cleaner: strips quotes
    // the model may have added, caps words, and rejects it if the model echoed
    // noise back at us.
    let cleaned = strip_surrounding_quotes(first_line);
    mechanical_title(cleaned, MAX_TITLE_WORDS)
}

/// The prompt put to the user's agent. Bounded: only the first few turns are
/// included, each truncated, so we never ship a huge body to the CLI.
fn build_ai_prompt(first_turns: &[String]) -> String {
    const MAX_TURNS: usize = 6;
    const MAX_TURN_CHARS: usize = 500;

    let mut convo = String::new();
    for turn in first_turns
        .iter()
        .filter(|t| !t.trim().is_empty())
        .take(MAX_TURNS)
    {
        let one_line: String = turn.split_whitespace().collect::<Vec<_>>().join(" ");
        let snippet: String = one_line.chars().take(MAX_TURN_CHARS).collect();
        convo.push_str("- ");
        convo.push_str(&snippet);
        convo.push('\n');
    }

    format!(
        "Summarize this conversation's topic in at most {MAX_TITLE_WORDS} words. \
         Reply with only the title — no punctuation, no quotes, no preamble.\n\n{convo}"
    )
}

/// Strip a single pair of surrounding straight or smart quotes a model may wrap
/// its answer in. Inner quotes are left alone.
fn strip_surrounding_quotes(s: &str) -> &str {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = s.chars().next().unwrap();
        let last = s.chars().next_back().unwrap();
        let is_quote = |c: char| matches!(c, '"' | '\'' | '“' | '”' | '‘' | '’' | '`');
        if is_quote(first) && is_quote(last) {
            return s
                .strip_prefix(first)
                .and_then(|x| x.strip_suffix(last))
                .unwrap_or(s)
                .trim();
        }
    }
    s
}

/// Orchestrate titling for one session: **mechanical first, AI only if noisy**,
/// with a project-derived fallback.
///
/// Decision order (each step records its [`TitleSource`]):
///
/// 1. `raw_title` is present and **not** noise → use it cleaned
///    ([`TitleSource::Provided`]).
/// 2. The first user turn yields a [`mechanical_title`] →
///    ([`TitleSource::Mechanical`]).
/// 3. The first turn *is* noise, so ask the user's own agent
///    ([`ai_title_via_agent`]) → ([`TitleSource::AiGenerated`]). This is the only
///    branch that spends a model call, and only the user's own agent.
/// 4. Otherwise a [`crate::sessions::fallback_label`] from the project path, or a
///    short literal if even that is unavailable ([`TitleSource::Fallback`]).
///
/// `first_turns[0]` (if any) is treated as the first **user** message for the
/// mechanical attempt; the full slice is handed to the AI step for context.
pub async fn title_for_session(
    agent: &AgentKind,
    raw_title: Option<String>,
    first_turns: &[String],
) -> TitleResult {
    title_for_session_with(agent, raw_title, first_turns, None).await
}

/// Test seam for [`title_for_session`]: `project` lets a unit test exercise the
/// fallback branch deterministically (production callers pass the real project
/// path through the public wrapper once wired — see Agent C's harness).
async fn title_for_session_with(
    agent: &AgentKind,
    raw_title: Option<String>,
    first_turns: &[String],
    project: Option<&str>,
) -> TitleResult {
    // 1) A provided title that is actually a topic wins outright — no work,
    //    no model call. We still clean it (cap length, strip wrappers).
    if let Some(raw) = raw_title.as_deref() {
        if !is_noise_title(raw) {
            if let Some(t) = mechanical_title(raw, MAX_TITLE_WORDS) {
                return TitleResult::new(t, TitleSource::Provided);
            }
        }
    }

    let first_user = first_turns.first().map(String::as_str).unwrap_or("");

    // 2) Mechanical from the first user message.
    if let Some(t) = mechanical_title(first_user, MAX_TITLE_WORDS) {
        return TitleResult::new(t, TitleSource::Mechanical);
    }

    // 3) Mechanical produced nothing. If the first message is *noise* (as opposed
    //    to simply empty), it's worth spending the user's own agent on it.
    if is_noise_title(first_user) && !first_user.trim().is_empty() {
        if let Some(t) = ai_title_via_agent(agent, first_turns).await {
            return TitleResult::new(t, TitleSource::AiGenerated);
        }
    }

    // 4) Fallback to a project-derived label.
    if let Some(label) = fallback_label(project, "") {
        return TitleResult::new(label, TitleSource::Fallback);
    }
    TitleResult::new("Untitled session", TitleSource::Fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prove_drive::{CANARY, PROVE_PROMPT};

    // ---- is_noise_title: the real observed noise classes ------------------

    #[test]
    fn noise_flags_codex_canary_prompt() {
        // The exact prompt Bluey's own harness sends (and codex's first message
        // often is) must be rejected as a title.
        assert!(is_noise_title(PROVE_PROMPT));
        assert!(PROVE_PROMPT.contains(CANARY));
        // The bare canary token alone is noise too.
        assert!(is_noise_title(CANARY));
        // A codex rollout id.
        assert!(is_noise_title("rollout-2026-06-14T10-22-01-abc123"));
    }

    #[test]
    fn noise_flags_copilot_generic_string() {
        // Copilot writes this identical string for EVERY session — zero topic.
        assert!(is_noise_title("GitHub Copilot CLI session events"));
        // Case-insensitive.
        assert!(is_noise_title("github copilot cli session events"));
    }

    #[test]
    fn noise_flags_cursor_shell_paste() {
        // Cursor's raw first line is sometimes a pasted shell prompt.
        assert!(is_noise_title("ms@Mac ~ % ssh jetson-orin"));
        assert!(is_noise_title("user@host:~$ git status"));
        assert!(is_noise_title("$ cargo build"));
    }

    #[test]
    fn noise_flags_empty_path_and_gibberish() {
        assert!(is_noise_title(""));
        assert!(is_noise_title("   \n\t  "));
        assert!(is_noise_title("/Users/ms/Developer/Bluey")); // path only
        assert!(is_noise_title("~/code/project")); // path only
        assert!(is_noise_title("()[]{}<>!!! 123 :: ---")); // no word-like run
    }

    #[test]
    fn noise_flags_shared_boilerplate() {
        // Deferred to sessions::is_boilerplate_title — spot-check it's wired.
        assert!(is_noise_title(
            "This session is being continued from a previous conversation"
        ));
        assert!(is_noise_title("You are a Rust architect."));
    }

    #[test]
    fn real_topics_are_not_noise() {
        // A clean topic.
        assert!(!is_noise_title("Refactor the audio capture pipeline"));
        // The Claude CLI's verbose-but-real opener (has prose + a topic).
        assert!(!is_noise_title(
            "In the Autonomous Staffing Desk repo (/Users/ms/x) add a login form"
        ));
        // A typo'd *question* still has words + a topic — NOT noise; the AI step
        // or the user can interpret it. (Real cursor example.)
        assert!(!is_noise_title("are oyu ble acesssupasbe mcp tools?"));
    }

    // ---- mechanical_title -------------------------------------------------

    #[test]
    fn mechanical_handles_claude_cli_raw_message() {
        // THE claude_code (CLI) case: verbose first message with an inline path.
        let raw = "In the Autonomous Staffing Desk repo (/Users/ms/dev/asd) set up CI";
        let title = mechanical_title(raw, MAX_TITLE_WORDS).expect("should produce a title");
        // The inline path is stripped...
        assert!(!title.contains("/Users/"));
        assert!(!title.contains("/dev/asd"));
        // ...and the real words survive.
        assert!(title.to_lowercase().contains("autonomous"));
        // Capped to the word budget.
        assert!(title.split_whitespace().count() <= MAX_TITLE_WORDS);
    }

    #[test]
    fn mechanical_caps_word_count() {
        let raw = "one two three four five six seven eight nine ten eleven twelve";
        let title = mechanical_title(raw, 8).unwrap();
        assert_eq!(title.split_whitespace().count(), 8);
        // Word-capped output ends with an elision mark to signal truncation.
        assert!(title.ends_with('…'));
    }

    #[test]
    fn mechanical_strips_wrapper_tags() {
        // Codex/Antigravity wrap user content; the tag must not appear.
        let raw = "<user_instructions>fix the flaky login test</user_instructions>";
        let title = mechanical_title(raw, MAX_TITLE_WORDS).unwrap();
        assert!(!title.contains('<'));
        assert!(title.to_lowercase().contains("flaky"));
    }

    #[test]
    fn mechanical_returns_none_for_noise() {
        assert_eq!(mechanical_title(PROVE_PROMPT, MAX_TITLE_WORDS), None); // canary
        assert_eq!(
            mechanical_title("GitHub Copilot CLI session events", MAX_TITLE_WORDS),
            None
        ); // generic
        assert_eq!(
            mechanical_title("ms@Mac ~ % ssh jetson-orin", MAX_TITLE_WORDS),
            None
        ); // shell paste
        assert_eq!(mechanical_title("", MAX_TITLE_WORDS), None); // empty
        assert_eq!(
            mechanical_title("/Users/ms/Developer/Bluey", MAX_TITLE_WORDS),
            None
        ); // path only
    }

    #[test]
    fn strip_surrounding_quotes_unwraps_one_pair() {
        assert_eq!(strip_surrounding_quotes("\"hello world\""), "hello world");
        assert_eq!(strip_surrounding_quotes("“smart quotes”"), "smart quotes");
        assert_eq!(strip_surrounding_quotes("no quotes"), "no quotes");
        // Inner quotes preserved.
        assert_eq!(strip_surrounding_quotes("say \"hi\" now"), "say \"hi\" now");
    }

    // ---- title_for_session orchestration: Provided > Mechanical > AI > Fallback

    #[tokio::test]
    async fn orchestrator_prefers_provided_when_good() {
        // A good pre-computed title (the claude_code_app case) is used verbatim,
        // never overridden by the first message.
        let res = title_for_session(
            &AgentKind::ClaudeCodeApp,
            Some("Bluey repository setup".to_string()),
            &["something totally different".to_string()],
        )
        .await;
        assert_eq!(res.source, TitleSource::Provided);
        assert!(res.title.to_lowercase().contains("bluey repository"));
    }

    #[tokio::test]
    async fn orchestrator_falls_to_mechanical_when_provided_is_noise() {
        // Provided title is noise (copilot generic) -> mechanical from 1st turn.
        let res = title_for_session(
            &AgentKind::Copilot,
            Some("GitHub Copilot CLI session events".to_string()),
            &["help me debug the websocket reconnect logic".to_string()],
        )
        .await;
        assert_eq!(res.source, TitleSource::Mechanical);
        assert!(res.title.to_lowercase().contains("websocket"));
    }

    #[tokio::test]
    async fn orchestrator_uses_mechanical_when_no_raw_title() {
        let res = title_for_session(
            &AgentKind::ClaudeCode,
            None,
            &["In the asd repo (/Users/ms/asd) wire up auth".to_string()],
        )
        .await;
        assert_eq!(res.source, TitleSource::Mechanical);
        assert!(!res.title.contains("/Users/"));
    }

    #[tokio::test]
    async fn orchestrator_reaches_ai_then_fallback_for_noisy_first_turn() {
        // First turn is noise (canary), no raw title, no installed agent in the
        // test env -> AI branch is REACHED but the drive fails soft (no CLI) ->
        // we land on Fallback derived from the project path. This asserts the AI
        // step is attempted for noisy input and that failure degrades cleanly.
        let res = title_for_session_with(
            &AgentKind::Codex,
            None,
            &[PROVE_PROMPT.to_string()],
            Some("/Users/ms/Developer/Bluey"),
        )
        .await;
        // The AI call cannot succeed without a real agent, so source is Fallback,
        // and the label is project-derived (proves we passed through the AI gate
        // and into the project fallback, not an early empty return).
        assert_eq!(res.source, TitleSource::Fallback);
        assert_eq!(res.title, "Bluey session");
    }

    #[tokio::test]
    async fn orchestrator_fallback_label_from_project() {
        // Empty everything -> project-derived fallback label.
        let res = title_for_session_with(
            &AgentKind::Cursor,
            None,
            &[],
            Some("/Users/ms/Developer/Bluey"),
        )
        .await;
        assert_eq!(res.source, TitleSource::Fallback);
        assert_eq!(res.title, "Bluey session");
    }

    #[tokio::test]
    async fn orchestrator_fallback_literal_when_no_project() {
        // No raw title, empty turns, no project -> a literal placeholder.
        let res = title_for_session_with(&AgentKind::Cursor, None, &[], None).await;
        assert_eq!(res.source, TitleSource::Fallback);
        assert_eq!(res.title, "Untitled session");
    }

    #[test]
    fn title_source_tags_are_stable() {
        assert_eq!(TitleSource::Provided.tag(), "provided");
        assert_eq!(TitleSource::Mechanical.tag(), "mechanical");
        assert_eq!(TitleSource::AiGenerated.tag(), "ai");
        assert_eq!(TitleSource::Fallback.tag(), "fallback");
    }
}
