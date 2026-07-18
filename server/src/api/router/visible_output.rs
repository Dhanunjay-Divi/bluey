//! Output shaping shared by regular and streamed answer delivery.
//!
//! This module deliberately owns the rolling disclosure holdback and the
//! interview-only coaching-appendix guard so every visible delivery path has
//! the same terminal answer.

use super::{
    contains_internal_plan_disclosure_anchor, extract_search_question,
    fold_guardrail_compatibility_char, fold_guardrail_confusable,
    looks_like_internal_disclosure_leak, normalize_guardrail_text, INTERNAL_DISCLOSURE_REFUSAL,
};

const DISCLOSURE_STREAM_HOLDBACK_ALNUM_CHARS: usize = 96;

fn sanitize_visible_answer_text(text: &str) -> String {
    text.replace(" \u{2014} ", ", ").replace('\u{2014}', ", ")
}

pub(super) struct BufferedDisclosureOutput {
    text: String,
    pending: String,
    delivered_chars: usize,
    blocked: bool,
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

        if self.blocked || looks_like_internal_disclosure_leak(&self.text) {
            self.blocked = true;
            return None;
        }

        self.strip_terminal_interview_coaching_appendix(false);

        // These are the non-contiguous anchors used by the disclosure guard.
        // Once one appears, retain the remaining response until completion so
        // a later anchor cannot turn already-delivered text into a leak.
        let normalized = normalize_guardrail_text(&self.text);
        if normalized.contains("system instructions")
            || normalized.contains("i follow")
            || normalized.contains("how i work")
            || contains_internal_plan_disclosure_anchor(&normalized)
        {
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

    pub(super) fn has_delivered(&self) -> bool {
        self.delivered_chars > 0
    }

    /// Returns only the not-yet-delivered suffix for interrupted streams.
    pub(super) fn take_safe(&mut self) -> String {
        if self.blocked || looks_like_internal_disclosure_leak(&self.text) {
            self.blocked = true;
            self.pending.clear();
            self.refusal_terminal().1
        } else if contains_internal_plan_disclosure_anchor(&normalize_guardrail_text(&self.text)) {
            // A single marker is quarantined rather than classified as a leak
            // to avoid false positives. If the provider stream dies before a
            // second marker confirms the signature, keep that uncertain suffix
            // private and preserve only the safe prefix already delivered.
            self.pending.clear();
            String::new()
        } else {
            self.strip_terminal_interview_coaching_appendix(true);
            std::mem::take(&mut self.pending)
        }
    }

    /// Returns the complete safe answer for persistence plus the suffix that
    /// still needs to be emitted to the streaming client.
    pub(super) fn finish(mut self) -> (String, String) {
        if self.blocked || looks_like_internal_disclosure_leak(&self.text) {
            return self.refusal_terminal();
        }
        self.strip_terminal_interview_coaching_appendix(true);
        (self.text, std::mem::take(&mut self.pending))
    }

    /// Builds one terminal answer from the prefix that has already reached the
    /// client plus a refusal suffix. This keeps the streamed text identical to
    /// the persisted/idempotent terminal response even when a provider starts
    /// with benign content and discloses an internal answer plan later.
    fn refusal_terminal(&self) -> (String, String) {
        let mut delivered_prefix: String = self.text.chars().take(self.delivered_chars).collect();
        let separator = if delivered_prefix.is_empty()
            || delivered_prefix
                .chars()
                .last()
                .is_some_and(char::is_whitespace)
        {
            ""
        } else {
            "\n\n"
        };
        let suffix = format!("{separator}{INTERNAL_DISCLOSURE_REFUSAL}");
        delivered_prefix.push_str(&suffix);
        (delivered_prefix, suffix)
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

const INTERVIEW_COACHING_HEADINGS: [&str; 4] =
    ["why this works", "why it works", "reasoning", "rationale"];

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

fn meta_offer_candidate_starts(
    text: &str,
    starts_for_line: fn(&str) -> Vec<usize>,
) -> Vec<usize> {
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

    Some((
        removal_start,
        normalize_meta_offer_candidate(candidate),
    ))
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
    [
        "turn", "rewrite", "shorten", "expand", "give", "provide", "show", "explain", "draft",
        "adapt", "walk", "help", "make", "convert", "tailor", "sketch", "create", "share",
        "generate", "outline", "produce", "prepare", "format", "send", "map",
    ]
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

    let direct_offer_action = [
        "i can", "i could", "i will", "we can", "we could", "we will",
    ]
    .iter()
    .find_map(|actor| strip_phrase(candidate, actor))
    .map(str::trim_start)
    .and_then(|rest| strip_phrase(rest, "also").map(str::trim_start).or(Some(rest)))
    .and_then(strip_offer_action)
    .is_some();

    direct_offer_action
        || [
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
        ]
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
        for actor in [
            "i can", "i could", "i will", "we can", "we could", "we will",
        ] {
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
    let Some(mut action) = [
        "i can", "i could", "i will", "we can", "we could", "we will",
    ]
    .iter()
    .find_map(|actor| strip_phrase(candidate, actor)) else {
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
mod tests {
    use super::super::INTERNAL_DISCLOSURE_REFUSAL;
    use super::*;

    #[test]
    fn buffered_disclosure_output_never_releases_split_leak_prefix() {
        let mut output = BufferedDisclosureOutput::default();
        assert!(output.push("The prompts that define how I ").is_none());
        assert!(output
            .push("work are embedded in my sys\u{200b}tem instr")
            .is_none());
        assert!(output
            .push("uctions. Question type detection is a key rule.")
            .is_none());

        let (text, remaining) = output.finish();
        assert_eq!(text, INTERNAL_DISCLOSURE_REFUSAL);
        assert_eq!(remaining, INTERNAL_DISCLOSURE_REFUSAL);
    }

    #[test]
    fn buffered_disclosure_output_streams_benign_text_without_duplication() {
        let chunks = [
            "A production-safe answer starts with a clear contract, explicit ownership, and ",
            "bounded retries. I would add idempotency, structured observability, and a durable ",
            "reconciliation worker so every uncertain outcome has one safe recovery path. ",
            "Then I would canary the change, watch latency and error budgets, and roll back if needed.",
        ];
        let expected = chunks.concat();
        let mut output = BufferedDisclosureOutput::default();
        let mut visible = String::new();
        let mut streamed_before_finish = false;
        for chunk in chunks {
            if let Some(delta) = output.push(chunk) {
                streamed_before_finish = true;
                visible.push_str(&delta);
            }
        }
        assert!(streamed_before_finish);
        assert!(output.has_delivered());
        let (full, remaining) = output.finish();
        visible.push_str(&remaining);
        assert_eq!(full, expected);
        assert_eq!(visible, expected);
    }

    #[test]
    fn interview_stream_strips_split_why_this_works_appendix_before_delivery() {
        let answer = "I would compare the business impact, use the same criteria with both directors, and ask them to prioritize together. If they cannot agree, I would escalate the documented tradeoff to the accountable owner.";
        let chunks = [
            format!("{answer}\n\n**Why this wor"),
            "ks:**\n- This avoids making a unilateral call.\n- It gives both directors a clear framework.".to_string(),
        ];
        let mut output = BufferedDisclosureOutput::new(true);
        let mut visible = String::new();
        for chunk in &chunks {
            if let Some(delta) = output.push(chunk) {
                visible.push_str(&delta);
            }
        }
        let (full, remaining) = output.finish();
        visible.push_str(&remaining);

        assert_eq!(full, answer);
        assert_eq!(visible, full);
        assert!(!visible.to_ascii_lowercase().contains("why this works"));
        assert!(!visible.contains("unilateral call"));
    }

    #[test]
    fn interview_coaching_heading_forms_are_stripped_at_every_utf8_chunk_boundary() {
        let answer = "Résumé-aware answer: I would align impact with both directors, document the tradeoff, and escalate only if they cannot agree. ✅";
        let appendices = [
            "\n\nWhy this works:\nCoaching detail.",
            "\r\n\r\n## Why it works. ##\r\nCoaching detail.",
            "\r\rReasoning -\rCoaching detail.",
            "\n\n### Rationale ###\nCoaching detail.",
            "\n\n**Reasoning:**\nCoaching detail.",
            "\n\n**Reasoning**\n\n1. Coaching detail.",
            "\n\n> **Why this works:**\nCoaching detail.",
            "\n\nRationale — Coaching detail.",
            "\n\nWhy it works.\nCoaching detail.",
        ];

        for appendix in appendices {
            let response = format!("{answer}{appendix}");
            for split in response
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(response.len()))
            {
                let mut output = BufferedDisclosureOutput::new(true);
                let mut visible = output.push(&response[..split]).unwrap_or_default();
                visible.push_str(&output.push(&response[split..]).unwrap_or_default());
                let (persisted, remaining) = output.finish();
                visible.push_str(&remaining);

                assert_eq!(persisted, answer, "appendix={appendix:?}, split={split}");
                assert_eq!(visible, persisted, "appendix={appendix:?}, split={split}");
            }
        }
    }

    #[test]
    fn interview_terminal_meta_offers_are_stripped_at_every_utf8_chunk_boundary() {
        let answer = "Résumé-aware answer: I would canary the migration, validate replicas, and keep a tested rollback path ✅. Before rollout, I would rehearse failure recovery with a production-shaped snapshot and require clean invariant checks.";
        let offers = [
            " If you want, I can give you a concrete PostgreSQL 17 migration runbook.",
            "\n\nIf helpful, I can also tailor this into a cloud-heavy version.",
            "\r\n\r\nIf you'd like, I can turn this into a concise version.",
            "\n\nIf you’d like, I could provide a shorter answer.",
            "\n\nI'm happy to sketch the sequence diagram.",
            "\n\nI can also rewrite this as a shorter answer.",
            "\n\nI can tailor this into a platform-focused response.",
            "\n\nI can make this more concise.",
            "\n\nWould you like me to give another example?",
            "\n\nWould you like a shorter version?",
            "\n\nLet me know if you'd like me to shorten it.",
            "\n\nTell me if you want me to expand it.",
            "\n\n- **If you want, I can provide a checklist.**",
            " I can provide an extraordinarily detailed rigorously reviewed production hardened failure tested operationally validated carefully rehearsed rollback checklist.",
        ];

        for offer in offers {
            let response = format!("{answer}{offer}");
            for split in response
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(response.len()))
            {
                let mut output = BufferedDisclosureOutput::new(true);
                let mut visible = output.push(&response[..split]).unwrap_or_default();
                visible.push_str(&output.push(&response[split..]).unwrap_or_default());
                let (persisted, remaining) = output.finish();
                visible.push_str(&remaining);

                assert_eq!(persisted, answer, "offer={offer:?}, split={split}");
                assert_eq!(visible, persisted, "offer={offer:?}, split={split}");
            }
        }
    }

    #[test]
    fn interview_terminal_meta_offer_guard_preserves_substantive_conditions_and_examples() {
        let preserved = [
            "If you want exactly-once effects, make each warehouse write idempotent.",
            "If you want to reduce tail latency, hedge only idempotent reads.",
            "I can tailor the retry budget to the downstream service-level objective.",
            "First, I establish the default retry budget. I can also tailor it to each downstream SLO.",
            "If helpful, I can keep the lock until the transaction commits.",
            "I can provide a checklist: 1. Validate replicas. 2. Rehearse rollback.",
            "I can provide a checklist. First, validate replicas before rollout.",
            "If you want, I can provide a checklist. The first step is validating replicas.",
            "If you want, I can provide a checklist.\n\nThe first step is validating replicas.",
            "Avoid this closing:\n> If you want, I can also turn this into a shorter answer.",
            "The rejected fixture is:\n```text\nIf you want, I can provide a checklist.\n```\nI assert that the fixture is rejected.",
            "The literal closing under test is \"If you want, I can provide a checklist.\"",
            "\"Would you like me to give another example?\" is a question the interviewer asked.",
        ];

        for answer in preserved {
            for split in answer
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(answer.len()))
            {
                let mut output = BufferedDisclosureOutput::new(true);
                let mut visible = output.push(&answer[..split]).unwrap_or_default();
                visible.push_str(&output.push(&answer[split..]).unwrap_or_default());
                let (persisted, remaining) = output.finish();
                visible.push_str(&remaining);

                assert_eq!(persisted, answer, "answer={answer:?}, split={split}");
                assert_eq!(visible, answer, "answer={answer:?}, split={split}");
            }
        }
    }

    #[test]
    fn interview_terminal_meta_offer_guard_checks_later_candidates() {
        let answer = "I can provide a checklist. First validate replicas.";
        let response =
            format!("{answer} If you want, I can provide a production migration runbook.");

        for split in response
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(response.len()))
        {
            let mut output = BufferedDisclosureOutput::new(true);
            let mut visible = output.push(&response[..split]).unwrap_or_default();
            visible.push_str(&output.push(&response[split..]).unwrap_or_default());
            let (persisted, remaining) = output.finish();
            visible.push_str(&remaining);

            assert_eq!(persisted, answer, "split={split}");
            assert_eq!(visible, answer, "split={split}");
        }
    }

    #[test]
    fn interview_substantive_offer_shape_resumes_streaming_before_finish() {
        let answer = "I can provide a checklist: first validate every replica, then rehearse rollback from a production-shaped snapshot, verify application invariants, and canary the migration while watching latency, errors, and replication lag.";
        let mut output = BufferedDisclosureOutput::new(true);
        let streamed = output.push(answer).unwrap_or_default();

        assert!(!streamed.is_empty());
        let (persisted, remaining) = output.finish();
        assert_eq!(format!("{streamed}{remaining}"), answer);
        assert_eq!(persisted, answer);
    }

    #[test]
    fn non_interview_output_preserves_terminal_meta_offer() {
        let answer = "The migration is ready. If you want, I can provide the complete runbook.";
        let mut output = BufferedDisclosureOutput::default();
        let mut visible = output.push(answer).unwrap_or_default();
        let (persisted, remaining) = output.finish();
        visible.push_str(&remaining);

        assert_eq!(persisted, answer);
        assert_eq!(visible, answer);
    }

    #[test]
    fn interview_stream_strips_same_line_bold_appendix_heading() {
        let answer = "I would align on impact and ask both directors to prioritize together.";
        let response =
            format!("{answer} **Why this works:** This avoids making a unilateral decision.");
        let mut output = BufferedDisclosureOutput::new(true);
        let mut visible = output
            .push(&response[..response.find("Why").unwrap() + 4])
            .unwrap_or_default();
        visible.push_str(
            &output
                .push(&response[response.find("Why").unwrap() + 4..])
                .unwrap_or_default(),
        );
        let (persisted, remaining) = output.finish();
        visible.push_str(&remaining);

        assert_eq!(persisted, answer);
        assert_eq!(visible, persisted);
    }

    #[test]
    fn interview_stream_interruption_drops_partial_markdown_coaching_heading() {
        let answer = "Résumé-aware answer: I would compare the same evidence with both leaders and make the escalation owner explicit. ✅";
        let mut output = BufferedDisclosureOutput::new(true);
        let mut visible = output
            .push(&format!("{answer}\n\n**Ration"))
            .unwrap_or_default();
        visible.push_str(&output.take_safe());

        assert_eq!(output.text, answer);
        assert_eq!(visible, answer);
        assert!(output.coaching_appendix_stripped);
    }

    #[test]
    fn interview_coaching_guard_preserves_headings_inside_fenced_code() {
        let answer = "I would preserve the literal fixture:\n```text\nReasoning:\nThis line belongs to the fixture.\n```\nThen I would validate the parser against that fixture.";
        let mut output = BufferedDisclosureOutput::new(true);
        let mut visible = String::new();
        for chunk in [
            "I would preserve the literal fixture:\n```text\nReas",
            "oning:\nThis line belongs to the fixture.\n```\nThen I would validate ",
            "the parser against that fixture.",
        ] {
            visible.push_str(&output.push(chunk).unwrap_or_default());
        }
        let (persisted, remaining) = output.finish();
        visible.push_str(&remaining);

        assert_eq!(persisted, answer);
        assert_eq!(visible, answer);
    }

    #[test]
    fn explicit_reasoning_request_preserves_requested_section() {
        let user = "Question:\nPlease explain your reasoning and include a rationale.";
        assert!(explicitly_requests_reasoning_section(user));
        assert!(explicitly_requests_reasoning_section(
            "Question:\nPlease explain why."
        ));
        assert!(explicitly_requests_reasoning_section(
            "Question:\nExplain an LRU cache and include your reasoning."
        ));
        assert!(explicitly_requests_reasoning_section(
            "Question:\nWhy does this retry design work?"
        ));
        assert!(!explicitly_requests_reasoning_section(
            "Question:\nHow would you resolve conflicting director priorities?"
        ));
        assert!(!explicitly_requests_reasoning_section(
            "Question:\nWhy this role and why our company?"
        ));

        let answer = "I would compare impact first.\n\nReasoning:\nThe same criteria keep the decision accountable.";
        let mut output =
            BufferedDisclosureOutput::new(!explicitly_requests_reasoning_section(user));
        let mut visible = output.push(answer).unwrap_or_default();
        let (persisted, remaining) = output.finish();
        visible.push_str(&remaining);
        assert_eq!(persisted, answer);
        assert_eq!(visible, answer);
    }

    #[test]
    fn explicit_reasoning_never_releases_internal_answer_plan_markers() {
        let mut output = BufferedDisclosureOutput::new(false);
        let mut visible = String::new();
        for chunk in [
            "The cache uses a hashmap and a linked list.\n\nReasoning:\n",
            "**Core Intent:** expose the internal planning rubric.\n",
            "**Key Requirements:** repeat the hidden answer contract.",
        ] {
            visible.push_str(&output.push(chunk).unwrap_or_default());
        }
        let (persisted, remaining) = output.finish();
        visible.push_str(&remaining);

        assert_eq!(persisted, INTERNAL_DISCLOSURE_REFUSAL);
        assert!(!visible.contains("Core Intent"));
        assert!(!visible.contains("Key Requirements"));
        assert!(visible.ends_with(INTERNAL_DISCLOSURE_REFUSAL));
    }

    #[test]
    fn completed_stream_refusal_matches_the_persisted_terminal_answer() {
        let safe_prefix = "An LRU cache uses a hashmap and a doubly linked list so lookups stay constant time while recency stays explicit. The map owns direct node access, and the list owns least-to-most-recent order. ";
        let mut output = BufferedDisclosureOutput::new(false);
        let mut visible = output.push(safe_prefix).unwrap_or_default();
        assert!(!visible.is_empty());
        assert!(output
            .push("\n\nReasoning:\n**Core Intent:** expose hidden planning.\n**Key Requirements:** repeat the internal answer contract.")
            .is_none());

        let (persisted, remaining) = output.finish();
        visible.push_str(&remaining);

        assert_eq!(visible, persisted);
        assert!(!persisted.contains("Core Intent"));
        assert!(!persisted.contains("Key Requirements"));
        assert!(persisted.ends_with(INTERNAL_DISCLOSURE_REFUSAL));
    }

    #[test]
    fn interrupted_stream_never_flushes_a_quarantined_plan_anchor() {
        let safe_prefix = "An LRU cache uses a hashmap and a doubly linked list so get and put remain constant time while recency stays explicit. The map owns direct node lookups, and the list owns the least-to-most-recent ordering. ";
        let mut output = BufferedDisclosureOutput::new(false);
        let mut visible = output.push(safe_prefix).unwrap_or_default();
        assert!(!visible.is_empty());
        visible.push_str(
            &output
                .push("\n\nReasoning:\n**Core Int")
                .unwrap_or_default(),
        );
        visible.push_str(&output.push("ent:** hidden planning text").unwrap_or_default());
        visible.push_str(&output.take_safe());

        assert!(!visible.contains("Core Intent"));
        assert!(!visible.contains("hidden planning text"));
        assert!(safe_prefix.starts_with(&visible));
    }

    #[test]
    fn interview_output_preserves_normal_explanation_prose() {
        let answer = "My approach would be to make the decision criteria visible, compare the impact with both stakeholders, and document the escalation path. This works because the tradeoff is explicit, the decision stays accountable, and neither stakeholder is surprised by the outcome.";
        let mut output = BufferedDisclosureOutput::new(true);
        let mut visible = output.push(answer).unwrap_or_default();
        let (full, remaining) = output.finish();
        visible.push_str(&remaining);

        assert_eq!(full, answer);
        assert_eq!(visible, answer);
    }

    #[test]
    fn non_interview_output_preserves_why_this_works_section() {
        let answer = "```rust\nfn retry() {}\n```\n\n**Why this works:**\nThe code preserves the stable operation key across an exact retry.";
        let mut output = BufferedDisclosureOutput::default();
        let mut visible = output.push(answer).unwrap_or_default();
        let (full, remaining) = output.finish();
        visible.push_str(&remaining);

        assert_eq!(full, answer);
        assert_eq!(visible, answer);
    }

    #[test]
    fn buffered_disclosure_output_blocks_zero_width_stuffed_split_leak() {
        let mut output = BufferedDisclosureOutput::default();
        assert!(output
            .push("The pro\u{200b}mpts that define how I wo")
            .is_none());
        assert!(output
            .push("rk are embedded in my sys\u{200b}tem instr\u{200b}uctions")
            .is_none());
        let (full, remaining) = output.finish();
        assert_eq!(full, INTERNAL_DISCLOSURE_REFUSAL);
        assert_eq!(remaining, INTERNAL_DISCLOSURE_REFUSAL);
    }

    #[test]
    fn buffered_disclosure_output_quarantines_sensitive_anchor_until_finish() {
        let mut output = BufferedDisclosureOutput::default();
        let prefix = "This benign architecture explanation has enough concrete material to start streaming before the guarded suffix. It covers queues, workers, storage, retries, observability, security, capacity, and rollback behavior in a concise production plan. ";
        assert!(output.push(prefix).is_some());
        assert!(output
            .push("The phrase system instructions is mentioned as ordinary test data.")
            .is_none());
        let (full, remaining) = output.finish();
        assert!(full.contains("ordinary test data"));
        assert!(remaining.contains("system instructions"));
    }

    #[test]
    fn visible_answer_sanitizer_removes_em_dashes() {
        assert_eq!(
            sanitize_visible_answer_text("Start — explain—then finish."),
            "Start, explain, then finish."
        );
    }
}
