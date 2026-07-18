//! Output shaping shared by regular and streamed answer delivery.
//!
//! This module deliberately owns the rolling disclosure holdback and the
//! interview-only coaching-appendix guard so every visible delivery path has
//! the same terminal answer.

use super::{
    extract_search_question, fold_guardrail_compatibility_char, fold_guardrail_confusable,
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
            INTERNAL_DISCLOSURE_REFUSAL.to_string()
        } else {
            self.strip_terminal_interview_coaching_appendix(true);
            std::mem::take(&mut self.pending)
        }
    }

    /// Returns the complete safe answer for persistence plus the suffix that
    /// still needs to be emitted to the streaming client.
    pub(super) fn finish(mut self) -> (String, String) {
        if self.blocked || looks_like_internal_disclosure_leak(&self.text) {
            return (
                INTERNAL_DISCLOSURE_REFUSAL.to_string(),
                INTERNAL_DISCLOSURE_REFUSAL.to_string(),
            );
        }
        self.strip_terminal_interview_coaching_appendix(true);
        (self.text, std::mem::take(&mut self.pending))
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

/// Returns the start of a narrow, heading-shaped coaching appendix outside a
/// fenced code block. Ordinary prose such as "This works because..." is not a
/// heading. Same-line detection requires explicit Markdown bold markers.
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
    None
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
            "add reasoning",
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
