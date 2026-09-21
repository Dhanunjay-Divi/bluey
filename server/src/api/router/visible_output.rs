//! Output shaping shared by regular and streamed answer delivery.
//!
//! This module deliberately owns the rolling disclosure holdback and the
//! interview-only coaching-appendix guard so every visible delivery path has
//! the same terminal answer.

use super::{
    contains_internal_disclosure_quarantine_anchor, extract_search_question,
    fold_guardrail_compatibility_char, fold_guardrail_confusable, internal_disclosure_leak_start,
    internal_disclosure_quarantine_anchor_start, looks_like_internal_disclosure_leak,
    normalize_guardrail_text, INTERNAL_DISCLOSURE_LEAK_SIGNALS,
    INTERNAL_DISCLOSURE_QUARANTINE_ANCHORS, INTERNAL_DISCLOSURE_REFUSAL,
    INTERNAL_PLAN_DISCLOSURE_MARKERS,
};

const INTERVIEW_COACHING_HEADINGS: &[&str] =
    &["why this works", "why it works", "reasoning", "rationale"];

const META_OFFER_ACTORS: &[&str] = &[
    "i can", "i could", "i will", "we can", "we could", "we will",
];

const META_OFFER_ACTIONS: &[&str] = &[
    "turn", "rewrite", "shorten", "expand", "give", "provide", "show", "explain", "draft", "adapt",
    "walk", "help", "make", "convert", "tailor", "sketch", "create", "share", "generate",
    "outline", "produce", "prepare", "format", "send", "map",
];

// These are every fixed prefix that can make
// `terminal_meta_offer_needs_quarantine` retain a suffix before the complete
// terminal offer is known. Actor-shaped offers are handled separately because
// they require an action before they become a potential offer.
const META_OFFER_QUARANTINE_LEADS: &[&str] = &[
    "if you want",
    "if you would like",
    "if you'd like",
    "if helpful",
    "if it helps",
    "if it would help",
    "i'm happy to",
    "i am happy to",
    "we're happy to",
    "we are happy to",
    "happy to",
    "would you like",
    "let me know if",
    "tell me if",
];

const STREAM_BOUNDARY_CONTEXT_ALNUM_CHARS: usize = 1;

const fn ascii_alnum_count(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut count = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if (byte >= b'0' && byte <= b'9')
            || (byte >= b'A' && byte <= b'Z')
            || (byte >= b'a' && byte <= b'z')
        {
            count += 1;
        }
        index += 1;
    }
    count
}

const fn max_ascii_alnum_count(phrases: &[&str]) -> usize {
    let mut index = 0usize;
    let mut maximum = 0usize;
    while index < phrases.len() {
        let count = ascii_alnum_count(phrases[index]);
        if count > maximum {
            maximum = count;
        }
        index += 1;
    }
    maximum
}

const fn max_usize(left: usize, right: usize) -> usize {
    if left > right {
        left
    } else {
        right
    }
}

/// The rolling window is one alphanumeric character longer than every finite
/// earliest-recognition trigger. That extra character keeps the safe side of a
/// Markdown/sentence boundary private until a coaching appendix or meta-offer
/// is classified, so stripping cannot invalidate bytes already delivered.
const fn disclosure_stream_holdback_alnum_chars() -> usize {
    let disclosure = max_ascii_alnum_count(&INTERNAL_DISCLOSURE_LEAK_SIGNALS);
    let plan_anchor = max_ascii_alnum_count(&INTERNAL_PLAN_DISCLOSURE_MARKERS);
    let quarantine_anchor = max_ascii_alnum_count(&INTERNAL_DISCLOSURE_QUARANTINE_ANCHORS);
    let coaching_heading = max_ascii_alnum_count(INTERVIEW_COACHING_HEADINGS);
    let meta_offer_lead = max_ascii_alnum_count(META_OFFER_QUARANTINE_LEADS);
    let actor_offer = max_ascii_alnum_count(META_OFFER_ACTORS)
        + ascii_alnum_count("also")
        + max_ascii_alnum_count(META_OFFER_ACTIONS);

    max_usize(
        max_usize(
            max_usize(disclosure, plan_anchor),
            max_usize(quarantine_anchor, coaching_heading),
        ),
        max_usize(meta_offer_lead, actor_offer),
    ) + STREAM_BOUNDARY_CONTEXT_ALNUM_CHARS
}

const DISCLOSURE_STREAM_HOLDBACK_ALNUM_CHARS: usize = disclosure_stream_holdback_alnum_chars();

fn sanitize_visible_answer_text(text: &str) -> String {
    text.replace(" \u{2014} ", ", ").replace('\u{2014}', ", ")
}

fn confirmed_safe_prefix_bytes(text: &str, disclosure_start: usize) -> usize {
    let prefix = &text[..disclosure_start];
    unsolicited_coaching_appendix_start(prefix, true)
        .filter(|appendix_start| {
            let normalized = normalize_guardrail_text(&prefix[*appendix_start..]);
            INTERVIEW_COACHING_HEADINGS
                .iter()
                .any(|heading| normalized == *heading)
        })
        .map(|appendix_start| prefix[..appendix_start].trim_end().len())
        .unwrap_or(disclosure_start)
}

pub(super) struct BufferedDisclosureOutput {
    text: String,
    pending: String,
    delivered_chars: usize,
    blocked: bool,
    blocked_safe_prefix_bytes: Option<usize>,
    strip_interview_coaching_appendix: bool,
    coaching_appendix_stripped: bool,
}

impl Default for BufferedDisclosureOutput {
    fn default() -> Self {
        Self::new(false)
    }
}

impl BufferedDisclosureOutput {
    pub(super) fn new(strip_interview_coaching_appendix: bool) -> Self {
        Self {
            text: String::new(),
            pending: String::new(),
            delivered_chars: 0,
            blocked: false,
            blocked_safe_prefix_bytes: None,
            strip_interview_coaching_appendix,
            coaching_appendix_stripped: false,
        }
    }

    /// Appends an upstream delta and returns the prefix that is safe to expose
    /// now. A rolling suffix stays private so a disclosure phrase split across
    /// provider events is inspected before any part of that phrase is sent.
    pub(super) fn push(&mut self, delta: &str) -> Option<String> {
        if self.coaching_appendix_stripped {
            return None;
        }

        let sanitized = sanitize_visible_answer_text(delta);
        self.text.push_str(&sanitized);
        self.pending.push_str(&sanitized);

        if self.block_if_disclosure_leak() {
            return None;
        }

        self.strip_terminal_interview_coaching_appendix(false);

        // These are the non-contiguous anchors used by the disclosure guard.
        // Once one appears, retain the remaining response until completion so
        // a later anchor cannot turn already-delivered text into a leak.
        let normalized = normalize_guardrail_text(&self.text);
        if contains_internal_disclosure_quarantine_anchor(&normalized) {
            return None;
        }
        // Once an offer-shaped sentence begins, quarantine the not-yet-
        // delivered suffix until completion. At finish we remove it only when
        // it is truly terminal; if substantive content follows, the complete
        // quarantined suffix is released unchanged.
        if self.strip_interview_coaching_appendix
            && terminal_meta_offer_needs_quarantine(&self.text)
        {
            return None;
        }

        let release_bytes =
            disclosure_safe_release_bytes(&self.pending, DISCLOSURE_STREAM_HOLDBACK_ALNUM_CHARS);
        if release_bytes == 0 {
            return None;
        }
        let released: String = self.pending.drain(..release_bytes).collect();
        self.delivered_chars = self
            .delivered_chars
            .saturating_add(released.chars().count());
        (!released.is_empty()).then_some(released)
    }

    pub(super) fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    #[allow(dead_code)] // Exercised by the focused stream-state tests.
    pub(super) fn has_delivered(&self) -> bool {
        self.delivered_chars > 0
    }

    /// Returns only the not-yet-delivered suffix for interrupted streams.
    pub(super) fn take_safe(&mut self) -> String {
        if self.block_if_disclosure_leak() {
            self.pending.clear();
            self.refusal_terminal().1
        } else if contains_internal_disclosure_quarantine_anchor(&normalize_guardrail_text(
            &self.text,
        )) {
            // A single marker is quarantined rather than classified as a leak
            // to avoid false positives. If the provider stream dies before a
            // second marker confirms the signature, keep that uncertain suffix
            // private while returning every preceding safe byte.
            let detected_safe_prefix_chars =
                internal_disclosure_quarantine_anchor_start(&self.text)
                    .map(|bytes| confirmed_safe_prefix_bytes(&self.text, bytes))
                    .map(|bytes| self.text[..bytes].chars().count());
            debug_assert!(
                detected_safe_prefix_chars.is_none_or(|chars| chars >= self.delivered_chars),
                "internal disclosure anchor escaped the streaming holdback"
            );
            let safe_prefix_chars = detected_safe_prefix_chars
                .filter(|chars| *chars >= self.delivered_chars)
                .unwrap_or(self.delivered_chars);
            let safe_delta = self
                .text
                .chars()
                .skip(self.delivered_chars)
                .take(safe_prefix_chars - self.delivered_chars)
                .collect();
            self.pending.clear();
            safe_delta
        } else {
            self.strip_terminal_interview_coaching_appendix(true);
            std::mem::take(&mut self.pending)
        }
    }

    /// Returns the complete safe answer for persistence plus the suffix that
    /// still needs to be emitted to the streaming client.
    pub(super) fn finish(mut self) -> (String, String) {
        if self.block_if_disclosure_leak() {
            return self.refusal_terminal();
        }
        self.strip_terminal_interview_coaching_appendix(true);
        (self.text, std::mem::take(&mut self.pending))
    }

    fn block_if_disclosure_leak(&mut self) -> bool {
        if self.blocked {
            return true;
        }
        if !looks_like_internal_disclosure_leak(&self.text) {
            return false;
        }

        self.blocked = true;
        self.blocked_safe_prefix_bytes = internal_disclosure_leak_start(&self.text)
            .map(|disclosure_start| confirmed_safe_prefix_bytes(&self.text, disclosure_start))
            .filter(|safe_prefix_bytes| {
                let safe_prefix_chars = self.text[..*safe_prefix_bytes].chars().count();
                let guard_held_the_complete_leak = safe_prefix_chars >= self.delivered_chars;
                debug_assert!(
                    guard_held_the_complete_leak,
                    "internal disclosure signature escaped the streaming holdback"
                );
                guard_held_the_complete_leak
            });
        true
    }

    /// Builds one terminal answer from every confirmed-safe byte before the
    /// leak plus a refusal. The returned delta starts exactly after the prefix
    /// already delivered, keeping streamed and persisted responses identical.
    fn refusal_terminal(&self) -> (String, String) {
        let safe_prefix_bytes = self.blocked_safe_prefix_bytes.or_else(|| {
            internal_disclosure_leak_start(&self.text)
                .map(|start| confirmed_safe_prefix_bytes(&self.text, start))
        });
        let safe_prefix_chars = safe_prefix_bytes
            .map(|bytes| self.text[..bytes].chars().count())
            .filter(|chars| *chars >= self.delivered_chars)
            .unwrap_or(self.delivered_chars);
        let mut terminal: String = self.text.chars().take(safe_prefix_chars).collect();
        let separator =
            if terminal.is_empty() || terminal.chars().last().is_some_and(char::is_whitespace) {
                ""
            } else {
                "\n\n"
            };
        terminal.push_str(separator);
        terminal.push_str(INTERNAL_DISCLOSURE_REFUSAL);
        let suffix = terminal.chars().skip(self.delivered_chars).collect();
        (terminal, suffix)
    }

    /// Drops an unsolicited, terminal coaching section before it reaches the
    /// client or persisted answer. The stream holdback above is deliberately
    /// longer than the heading, so even a heading split across provider deltas
    /// remains in `pending` until this check has seen it in full.
    fn strip_terminal_interview_coaching_appendix(&mut self, allow_incomplete_terminal: bool) {
        if !self.strip_interview_coaching_appendix || self.coaching_appendix_stripped {
            return;
        }
        let Some(appendix_start) =
            unsolicited_coaching_appendix_start(&self.text, allow_incomplete_terminal)
        else {
            return;
        };

        // Separating blank lines and spaces are part of the appendix boundary,
        // not the spoken answer. They are still inside the rolling holdback at
        // this point, so trimming them cannot invalidate already-streamed text.
        let retained_bytes = self.text[..appendix_start].trim_end().len();
        let retained_chars = self.text[..retained_bytes].chars().count();
        // `pending` is exactly the not-yet-delivered suffix of `text`. The
        // rolling holdback guarantees a complete heading is never delivered
        // before it can be recognized here.
        if retained_chars < self.delivered_chars {
            debug_assert!(
                false,
                "interview coaching heading escaped the streaming holdback"
            );
            return;
        }
        let pending_chars = retained_chars - self.delivered_chars;
        let pending_bytes = self
            .pending
            .char_indices()
            .nth(pending_chars)
            .map(|(byte, _)| byte)
            .unwrap_or(self.pending.len());
        self.text.truncate(retained_bytes);
        self.pending.truncate(pending_bytes);
        self.coaching_appendix_stripped = true;
    }
}

/// Returns the start of a narrow coaching appendix or closing meta-offer
/// outside a fenced code block. Ordinary prose such as "This works because..."
/// and substantive technical conditions are preserved. Same-line coaching
/// heading detection requires explicit Markdown bold markers.
fn unsolicited_coaching_appendix_start(
    text: &str,
    allow_incomplete_terminal: bool,
) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut line_start = 0usize;
    let mut open_fence: Option<(u8, usize)> = None;

    while line_start < bytes.len() {
        let mut line_end = line_start;
        while line_end < bytes.len() && !matches!(bytes[line_end], b'\r' | b'\n') {
            line_end += 1;
        }
        let line = &text[line_start..line_end];
        let terminal_unterminated_line = line_end == bytes.len();

        if let Some((marker, length)) = markdown_fence_marker(line) {
            match open_fence {
                Some((open_marker, open_length))
                    if marker == open_marker && length >= open_length =>
                {
                    open_fence = None;
                }
                None => open_fence = Some((marker, length)),
                _ => {}
            }
        } else if open_fence.is_none() {
            let (heading, had_markdown_prefix) = strip_heading_prefix(line);
            if coaching_heading(heading)
                || (allow_incomplete_terminal
                    && terminal_unterminated_line
                    && had_markdown_prefix
                    && incomplete_coaching_heading(heading))
            {
                return Some(line_start);
            }
            if let Some(inline_start) = inline_bold_coaching_heading_start(
                line,
                allow_incomplete_terminal && terminal_unterminated_line,
            ) {
                return Some(line_start + inline_start);
            }
        }

        if line_end == bytes.len() {
            break;
        }
        line_start = line_end + 1;
        if bytes[line_end] == b'\r' && line_start < bytes.len() && bytes[line_start] == b'\n' {
            line_start += 1;
        }
    }
    if allow_incomplete_terminal {
        return terminal_meta_offer_candidate_starts(text)
            .into_iter()
            .find(|start| terminal_meta_offer_suffix_is_terminal(&text[*start..]));
    }
    None
}

fn terminal_meta_offer_candidate_starts(text: &str) -> Vec<usize> {
    meta_offer_candidate_starts(text, terminal_meta_offer_starts)
}

fn potential_terminal_meta_offer_candidate_starts(text: &str) -> Vec<usize> {
    meta_offer_candidate_starts(text, potential_terminal_meta_offer_starts)
}

fn meta_offer_candidate_starts(text: &str, starts_for_line: fn(&str) -> Vec<usize>) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut line_start = 0usize;
    let mut open_fence: Option<(u8, usize)> = None;
    let mut starts = Vec::new();

    while line_start < bytes.len() {
        let mut line_end = line_start;
        while line_end < bytes.len() && !matches!(bytes[line_end], b'\r' | b'\n') {
            line_end += 1;
        }
        let line = &text[line_start..line_end];
        if let Some((marker, length)) = markdown_fence_marker(line) {
            match open_fence {
                Some((open_marker, open_length))
                    if marker == open_marker && length >= open_length =>
                {
                    open_fence = None;
                }
                None => open_fence = Some((marker, length)),
                _ => {}
            }
        } else if open_fence.is_none() {
            starts.extend(
                starts_for_line(line)
                    .into_iter()
                    .map(|start| line_start + start),
            );
        }

        if line_end == bytes.len() {
            break;
        }
        line_start = line_end + 1;
        if bytes[line_end] == b'\r' && line_start < bytes.len() && bytes[line_start] == b'\n' {
            line_start += 1;
        }
    }
    starts
}

fn terminal_meta_offer_needs_quarantine(text: &str) -> bool {
    let confirmed = terminal_meta_offer_candidate_starts(text);
    if confirmed
        .iter()
        .any(|start| terminal_meta_offer_suffix_is_terminal(&text[*start..]))
    {
        return true;
    }

    potential_terminal_meta_offer_candidate_starts(text)
        .into_iter()
        .filter(|start| !confirmed.contains(start))
        .any(|start| potential_meta_offer_suffix_is_unfinished(&text[start..]))
}

fn potential_meta_offer_suffix_is_unfinished(suffix: &str) -> bool {
    let trimmed = suffix.trim();
    if trimmed.is_empty()
        || trimmed
            .split_once(':')
            .is_some_and(|(_, after)| !after.trim().is_empty())
    {
        return false;
    }
    !trimmed.char_indices().any(|(index, ch)| {
        matches!(ch, '.' | '!' | '?')
            && trimmed[index + ch.len_utf8()..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
    })
}

fn terminal_meta_offer_suffix_is_terminal(suffix: &str) -> bool {
    let trimmed = suffix.trim();
    if trimmed.is_empty() {
        return false;
    }
    // A colon followed by content is normally the start of the actual answer
    // or checklist, not a disposable invitation.
    if trimmed
        .split_once(':')
        .is_some_and(|(_, after)| !after.trim().is_empty())
    {
        return false;
    }
    let Some((terminal_index, terminal_char)) = trimmed
        .char_indices()
        .find(|(_, ch)| matches!(ch, '.' | '!' | '?'))
    else {
        return true;
    };
    trimmed[terminal_index + terminal_char.len_utf8()..]
        .trim()
        .trim_matches(['*', '_', '`'])
        .trim()
        .is_empty()
}

/// Finds an unsolicited offer that starts a line or a new terminal sentence.
///
/// Detection deliberately requires an explicit offer shape and, where the
/// phrase could also describe real work, an offer action. This keeps ordinary
/// technical prose such as "If you want exactly-once effects..." and "I can
/// tailor the retry budget..." intact. Quoted examples and blockquotes are
/// evidence, not answer appendices, so they are never stripped here.
fn terminal_meta_offer_starts(line: &str) -> Vec<usize> {
    meta_offer_starts(line, meta_offer_at_sentence_start)
}

fn potential_terminal_meta_offer_starts(line: &str) -> Vec<usize> {
    meta_offer_starts(line, potential_meta_offer_at_sentence_start)
}

fn meta_offer_starts(
    line: &str,
    candidate_at_sentence_start: fn(&str, usize) -> Option<usize>,
) -> Vec<usize> {
    if line.trim_start().starts_with('>') {
        return Vec::new();
    }

    let mut starts = Vec::new();
    if let Some(start) = candidate_at_sentence_start(line, 0) {
        starts.push(start);
    }
    for (byte_index, ch) in line.char_indices() {
        if !matches!(ch, '.' | '!' | '?') {
            continue;
        }
        let after_boundary = byte_index + ch.len_utf8();
        if line[after_boundary..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            if let Some(start) = candidate_at_sentence_start(line, after_boundary) {
                starts.push(start);
            }
        }
    }
    starts
}

fn meta_offer_at_sentence_start(line: &str, raw_start: usize) -> Option<usize> {
    let (removal_start, candidate) = normalized_meta_offer_at_sentence_start(line, raw_start)?;
    is_terminal_meta_offer(&candidate).then_some(removal_start)
}

fn potential_meta_offer_at_sentence_start(line: &str, raw_start: usize) -> Option<usize> {
    let (removal_start, candidate) = normalized_meta_offer_at_sentence_start(line, raw_start)?;
    is_potential_terminal_meta_offer(&candidate).then_some(removal_start)
}

fn normalized_meta_offer_at_sentence_start(
    line: &str,
    raw_start: usize,
) -> Option<(usize, String)> {
    let suffix = &line[raw_start..];
    let leading_whitespace = suffix.len() - suffix.trim_start().len();
    let removal_start = raw_start + leading_whitespace;
    let mut candidate = &line[removal_start..];

    // Allow an offer rendered as its own Markdown bullet/heading or in bold,
    // but retain the marker in the removed suffix so no dangling syntax leaks.
    if raw_start == 0 {
        if candidate
            .as_bytes()
            .get(0..2)
            .is_some_and(|prefix| matches!(prefix, b"- " | b"* " | b"+ "))
        {
            candidate = &candidate[2..];
        } else if candidate.starts_with('#') {
            let marker_len = candidate.bytes().take_while(|byte| *byte == b'#').count();
            if candidate[marker_len..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                candidate = candidate[marker_len..].trim_start();
            }
        }
    }
    candidate = candidate.trim_start();
    if candidate.starts_with("**") || candidate.starts_with("__") {
        candidate = candidate[2..].trim_start();
    }

    // A quote or inline-code marker means the phrase is being discussed, not
    // offered to the user. Fenced code is excluded by the caller.
    if candidate.chars().next().is_some_and(|ch| {
        matches!(
            ch,
            '"' | '\'' | '\u{2018}' | '\u{2019}' | '\u{201c}' | '\u{201d}' | '`' | '>'
        )
    }) {
        return None;
    }

    Some((removal_start, normalize_meta_offer_candidate(candidate)))
}

fn normalize_meta_offer_candidate(candidate: &str) -> String {
    let mut normalized = String::with_capacity(candidate.len());
    let mut pending_space = false;
    for original in candidate.chars() {
        if original.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        let original = match original {
            '\u{2018}' | '\u{2019}' => '\'',
            other => other,
        };
        normalized.extend(original.to_lowercase());
    }
    normalized
}

fn strip_phrase<'a>(text: &'a str, phrase: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(phrase)?;
    rest.chars()
        .next()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '\'')
        .then_some(rest)
}

fn strip_offer_action(text: &str) -> Option<&str> {
    let mut text = text.trim_start();
    if let Some(rest) = strip_phrase(text, "also") {
        text = rest.trim_start();
    }
    META_OFFER_ACTIONS
        .iter()
        .find_map(|action| strip_phrase(text, action))
}

fn is_terminal_meta_offer(candidate: &str) -> bool {
    conditional_meta_offer(candidate)
        || direct_answer_transform_offer(candidate)
        || happy_to_offer(candidate)
        || would_you_like_offer(candidate)
        || let_me_know_offer(candidate)
}

fn is_potential_terminal_meta_offer(candidate: &str) -> bool {
    if is_terminal_meta_offer(candidate) {
        return true;
    }

    let direct_offer_action = META_OFFER_ACTORS
        .iter()
        .find_map(|actor| strip_phrase(candidate, actor))
        .map(str::trim_start)
        .and_then(strip_offer_action)
        .is_some();

    direct_offer_action
        || META_OFFER_QUARANTINE_LEADS
            .iter()
            .any(|lead| strip_phrase(candidate, lead).is_some())
}

fn conditional_meta_offer(candidate: &str) -> bool {
    for condition in [
        "if you want",
        "if you would like",
        "if you'd like",
        "if helpful",
        "if it helps",
        "if it would help",
    ] {
        let Some(mut rest) = strip_phrase(candidate, condition) else {
            continue;
        };
        rest = rest.trim_start();
        if let Some(without_comma) = rest.strip_prefix(',') {
            rest = without_comma.trim_start();
        }
        for actor in META_OFFER_ACTORS {
            if strip_phrase(rest, actor)
                .and_then(strip_offer_action)
                .is_some()
            {
                return true;
            }
        }
    }
    false
}

fn direct_answer_transform_offer(candidate: &str) -> bool {
    let Some(mut action) = META_OFFER_ACTORS
        .iter()
        .find_map(|actor| strip_phrase(candidate, actor))
    else {
        return false;
    };
    action = action.trim_start();
    if let Some(rest) = strip_phrase(action, "also") {
        action = rest.trim_start();
    }

    for verb in ["turn", "rewrite", "adapt", "convert", "tailor"] {
        let Some(rest) = strip_phrase(action, verb) else {
            continue;
        };
        if answer_object_then(rest, &["into", "as"]) {
            return true;
        }
    }
    if strip_phrase(action, "shorten")
        .is_some_and(|rest| starts_with_answer_object(rest.trim_start()))
    {
        return true;
    }
    if let Some(rest) = strip_phrase(action, "make") {
        if let Some(rest) = strip_answer_object(rest.trim_start()) {
            let rest = rest.trim_start();
            if [
                "concise",
                "more concise",
                "short",
                "shorter",
                "long",
                "longer",
                "technical",
                "behavioral",
            ]
            .iter()
            .any(|shape| strip_phrase(rest, shape).is_some())
                || (["a", "an"]
                    .iter()
                    .any(|article| strip_phrase(rest, article).is_some())
                    && nearby_deliverable(rest))
            {
                return true;
            }
        }
    }

    ["give", "provide", "show", "sketch"].iter().any(|verb| {
        strip_phrase(action, verb).is_some_and(|rest| nearby_deliverable(rest.trim_start()))
    })
}

fn answer_object_then(text: &str, continuations: &[&str]) -> bool {
    strip_answer_object(text.trim_start()).is_some_and(|rest| {
        let rest = rest.trim_start();
        continuations
            .iter()
            .any(|continuation| strip_phrase(rest, continuation).is_some())
    })
}

fn starts_with_answer_object(text: &str) -> bool {
    strip_answer_object(text).is_some()
}

fn strip_answer_object(text: &str) -> Option<&str> {
    ["this", "it", "the answer", "the response"]
        .iter()
        .find_map(|object| strip_phrase(text, object))
}

fn nearby_deliverable(text: &str) -> bool {
    let nearby = text
        .split_ascii_whitespace()
        .take(16)
        .collect::<Vec<_>>()
        .join(" ");
    [
        "answer",
        "response",
        "version",
        "example",
        "diagram",
        "checklist",
        "implementation",
        "protocol",
        "state machine",
    ]
    .iter()
    .any(|deliverable| nearby.contains(deliverable))
}

fn happy_to_offer(candidate: &str) -> bool {
    [
        "i'm happy to",
        "i am happy to",
        "we're happy to",
        "we are happy to",
        "happy to",
    ]
    .iter()
    .any(|lead| {
        strip_phrase(candidate, lead)
            .and_then(strip_offer_action)
            .is_some()
    })
}

fn would_you_like_offer(candidate: &str) -> bool {
    if strip_phrase(candidate, "would you like me to")
        .and_then(strip_offer_action)
        .is_some()
    {
        return true;
    }
    ["a", "an", "the"].iter().any(|article| {
        strip_phrase(candidate, "would you like")
            .map(str::trim_start)
            .and_then(|rest| strip_phrase(rest, article))
            .is_some_and(|rest| nearby_deliverable(rest.trim_start()))
    })
}

fn let_me_know_offer(candidate: &str) -> bool {
    for lead in ["let me know if", "tell me if"] {
        let Some(rest) = strip_phrase(candidate, lead) else {
            continue;
        };
        for preference in ["you want", "you would like", "you'd like"] {
            let Some(mut rest) = strip_phrase(rest.trim_start(), preference) else {
                continue;
            };
            rest = rest.trim_start();
            if let Some(after_me_to) = strip_phrase(rest, "me to") {
                rest = after_me_to.trim_start();
            }
            if strip_offer_action(rest).is_some() {
                return true;
            }
        }
    }
    false
}

fn markdown_fence_marker(line: &str) -> Option<(u8, usize)> {
    let mut bytes = line.as_bytes();
    let indent = bytes.iter().take_while(|byte| **byte == b' ').count();
    if indent > 3 {
        return None;
    }
    bytes = &bytes[indent..];
    let marker = *bytes.first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let length = bytes.iter().take_while(|byte| **byte == marker).count();
    (length >= 3).then_some((marker, length))
}

fn strip_heading_prefix(mut line: &str) -> (&str, bool) {
    line = line.trim_start();
    let mut had_markdown_prefix = false;
    loop {
        let stripped = line.trim_start_matches(['#', '*', '_', '`', '-', '>']);
        if stripped.len() != line.len() {
            had_markdown_prefix = true;
        }
        let stripped = stripped.trim_start();
        if stripped.len() == line.len() {
            break;
        }
        line = stripped;
    }
    (line, had_markdown_prefix)
}

fn matching_coaching_heading_len(candidate: &str) -> Option<usize> {
    INTERVIEW_COACHING_HEADINGS.iter().find_map(|heading| {
        candidate
            .get(..heading.len())
            .filter(|title| title.eq_ignore_ascii_case(heading))
            .map(|_| heading.len())
    })
}

fn coaching_heading(candidate: &str) -> bool {
    let Some(title_len) = matching_coaching_heading_len(candidate) else {
        return false;
    };
    coaching_heading_suffix(&candidate[title_len..])
}

fn coaching_heading_suffix(suffix: &str) -> bool {
    let mut suffix = suffix.trim_start();
    loop {
        let stripped = suffix.trim_start_matches(['#', '*', '_', '`']).trim_start();
        if stripped.len() == suffix.len() {
            break;
        }
        suffix = stripped;
    }
    matches!(
        suffix.chars().next(),
        None | Some(':') | Some('.') | Some('-') | Some('\u{2013}') | Some('\u{2014}') | Some(',')
    )
}

fn incomplete_coaching_heading(candidate: &str) -> bool {
    let candidate = candidate.trim().to_ascii_lowercase();
    candidate.len() >= 3
        && INTERVIEW_COACHING_HEADINGS
            .iter()
            .any(|heading| heading.starts_with(&candidate))
}

fn inline_bold_coaching_heading_start(line: &str, allow_incomplete: bool) -> Option<usize> {
    ["**", "__"].iter().find_map(|marker| {
        let mut search_start = 0usize;
        while let Some(relative_start) = line[search_start..].find(marker) {
            let marker_start = search_start + relative_start;
            let candidate_start = marker_start + marker.len();
            let candidate = &line[candidate_start..];
            let boundary_ok = marker_start == 0
                || line[..marker_start]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch.is_whitespace() || ch.is_ascii_punctuation());
            if boundary_ok {
                if let Some(close) = candidate.find(marker) {
                    if coaching_heading(candidate[..close].trim()) {
                        return Some(marker_start);
                    }
                } else if allow_incomplete && incomplete_coaching_heading(candidate) {
                    return Some(marker_start);
                }
            }
            search_start = candidate_start;
        }
        None
    })
}

pub(super) fn explicitly_requests_reasoning_section(user_text: &str) -> bool {
    let question = normalize_guardrail_text(&extract_search_question(user_text));
    let asks_why_solution_works = (question.starts_with("why does ")
        || question.starts_with("why would "))
        && question
            .split_ascii_whitespace()
            .any(|word| matches!(word, "work" | "works"));
    asks_why_solution_works
        || [
            "explain why",
            "explain the reasoning",
            "explain your reasoning",
            "show your reasoning",
            "include reasoning",
            "include your reasoning",
            "add reasoning",
            "add your reasoning",
            "give your reasoning",
            "include a rationale",
            "add a rationale",
            "give me the rationale",
            "why this works",
            "why that works",
            "why the approach works",
            "why the design works",
            "why the answer works",
            "why this works section",
            "why it works section",
        ]
        .iter()
        .any(|signal| question.contains(signal))
}

fn disclosure_safe_release_bytes(text: &str, holdback_alnum_chars: usize) -> usize {
    if holdback_alnum_chars == 0 {
        return text.len();
    }

    let mut alnum_chars = 0usize;
    for (byte_index, original) in text.char_indices().rev() {
        let folded = fold_guardrail_compatibility_char(original);
        let is_alnum = folded
            .to_lowercase()
            .map(fold_guardrail_confusable)
            .any(|ch| ch.is_ascii_alphanumeric());
        if is_alnum {
            alnum_chars += 1;
            if alnum_chars >= holdback_alnum_chars {
                return byte_index;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests;
