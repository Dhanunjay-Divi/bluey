use crate::{
    ActionItem, CardKind, CueCard, Decision, MeetingRecap, MeetingRecord, TranscriptSegment,
};

#[derive(Debug, Clone)]
pub struct SegmentAnalysis {
    pub cards: Vec<CueCard>,
    pub action_items: Vec<ActionItem>,
    pub decisions: Vec<Decision>,
}

impl SegmentAnalysis {
    fn empty() -> Self {
        Self {
            cards: Vec::new(),
            action_items: Vec::new(),
            decisions: Vec::new(),
        }
    }
}

pub fn analyze_segment(segment: &TranscriptSegment, meeting: &MeetingRecord) -> SegmentAnalysis {
    let text = segment.text.trim();
    if text.is_empty() {
        return SegmentAnalysis::empty();
    }

    let mut analysis = SegmentAnalysis::empty();

    if is_question(text) {
        let answer = local_answer(text, meeting);
        analysis.cards.push(
            CueCard::new(CardKind::Answer, question_title(text), answer)
                .with_source("live transcript"),
        );
    }

    // NOTE: action items + decisions are NOT extracted here anymore. The old
    // per-segment keyword heuristic ("please", "follow up", "we decided")
    // produced fragment garbage ("do", "follow up on") because it matched
    // trigger words on chopped transcript pieces. They now come from the
    // verified AI ledger (crate::ledger — Owner→action item, Decision→decision),
    // which reads a whole transcript window and copies verbatim quotes. This
    // function only surfaces live answer + context cards.

    if analysis.cards.is_empty() && meeting.transcript.len() % 5 == 4 {
        analysis.cards.push(
            CueCard::new(CardKind::Context, "Context", compact_context(meeting, 4))
                .with_source("rolling meeting context"),
        );
    }

    analysis
}

pub fn local_answer(question: &str, meeting: &MeetingRecord) -> String {
    let recent = meeting.last_transcript_text(6);
    let conversation = meeting.last_conversation_text(4);
    let has_instructions = meeting
        .answer_instructions
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty());
    if recent.is_empty()
        && conversation.is_empty()
        && meeting.context.is_empty()
        && !has_instructions
    {
        return "I do not have enough meeting context yet. Keep listening and I will build a better answer.".to_string();
    }

    let lower = question.to_lowercase();
    if lower.contains("instruction") || lower.contains("answer style") || lower.contains("style") {
        return meeting.answer_instructions.clone().unwrap_or_else(|| {
            "No answer instructions are set yet. Add them from the overlay text button or `bluey instructions set \"...\"`.".to_string()
        });
    }

    if asks_for_context(&lower) {
        if meeting.context.is_empty() {
            return "No screenshots, diagrams, code files, or other context artifacts have been attached yet.".to_string();
        }
        return format_context_items(meeting, 6);
    }

    if lower.contains("action") || lower.contains("todo") || lower.contains("follow up") {
        if meeting.action_items.is_empty() {
            return "No action items have been detected yet in this meeting.".to_string();
        }
        return meeting
            .action_items
            .iter()
            .rev()
            .take(4)
            .map(|item| format!("- {}", item.text))
            .collect::<Vec<_>>()
            .join("\n");
    }

    if lower.contains("decision") || lower.contains("decide") {
        if meeting.decisions.is_empty() {
            return "No explicit decisions have been detected yet.".to_string();
        }
        return meeting
            .decisions
            .iter()
            .rev()
            .take(4)
            .map(|decision| format!("- {}", decision.text))
            .collect::<Vec<_>>()
            .join("\n");
    }

    let mut answer = String::new();
    if !recent.is_empty() {
        answer.push_str("Based on the current meeting context, the most relevant thread is:\n");
        answer.push_str(&recent);
    }
    if !conversation.is_empty() {
        if !answer.is_empty() {
            answer.push_str("\n\n");
        }
        answer.push_str("Recent Bluey Q&A:\n");
        answer.push_str(&conversation);
    }
    if !meeting.context.is_empty() {
        if !answer.is_empty() {
            answer.push_str("\n\n");
        }
        answer.push_str("Attached context:\n");
        answer.push_str(&format_context_items(meeting, 3));
    }
    if let Some(instructions) = meeting
        .answer_instructions
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        answer.push_str("\n\nAnswer instructions:\n");
        answer.push_str(instructions);
    }
    format!(
        "Local fallback context for: {question}\n\n{answer}\n\nSet a live provider key to generate a full answer from this context."
    )
}

pub fn generate_recap(meeting: &MeetingRecord) -> MeetingRecap {
    let summary = meeting.summary.clone().unwrap_or_else(|| {
        if meeting.transcript.is_empty()
            && meeting.context.is_empty()
            && meeting.conversation.is_empty()
        {
            crate::meeting::EMPTY_MEETING_SUMMARY.to_string()
        } else {
            let mut summary = format!(
                "Captured {} transcript segment(s).\n\nRecent context:\n{}",
                meeting.transcript.len(),
                recent_transcript_highlights(meeting, 8)
            );
            let conversation = meeting.last_conversation_text(6);
            if !conversation.is_empty() {
                summary.push_str("\n\nRecent Bluey Q&A:\n");
                summary.push_str(&conversation);
            }
            if !meeting.context.is_empty() {
                summary.push_str("\n\nAttached context:\n");
                summary.push_str(&format_context_items(meeting, 6));
            }
            if let Some(instructions) = meeting
                .answer_instructions
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                summary.push_str("\n\nAnswer instructions:\n");
                summary.push_str(instructions);
            }
            summary
        }
    });

    MeetingRecap {
        meeting_id: meeting.id,
        title: meeting.title.clone(),
        summary,
        transcript_segments: meeting.transcript.len(),
        context: meeting.context.clone(),
        answer_instructions: meeting.answer_instructions.clone(),
        action_items: meeting.action_items.clone(),
        decisions: meeting.decisions.clone(),
    }
}

fn asks_for_context(lower: &str) -> bool {
    [
        "screenshot",
        "screen shot",
        "diagram",
        "image",
        "code",
        "file",
        "attached",
        "attachment",
        "context",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn format_context_items(meeting: &MeetingRecord, count: usize) -> String {
    meeting
        .context
        .iter()
        .rev()
        .take(count)
        .map(|item| {
            let mut line = format!("- [{}] {}", item.kind, item.title);
            if let Some(note) = item.note.as_ref().filter(|note| !note.trim().is_empty()) {
                line.push_str(&format!(" - {note}"));
            }
            if let Some(preview) = item
                .text_preview
                .as_ref()
                .filter(|preview| !preview.trim().is_empty())
            {
                line.push_str("\n  preview: ");
                line.push_str(&compact_snippet(preview, 280));
            } else if let Some(error) = item
                .processing_error
                .as_ref()
                .filter(|error| !error.trim().is_empty())
            {
                line.push_str("\n  processing: ");
                line.push_str(error);
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn recent_transcript_highlights(meeting: &MeetingRecord, count: usize) -> String {
    if meeting.transcript.is_empty() {
        return "No transcript text captured yet.".to_string();
    }

    meeting
        .transcript
        .iter()
        .rev()
        .take(count)
        .map(|segment| format!("{}: {}", segment.speaker.display_label(), segment.text))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact_snippet(text: &str, max_chars: usize) -> String {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= max_chars {
        return clean;
    }

    let mut snippet = clean.chars().take(max_chars).collect::<String>();
    snippet.push_str("...");
    snippet
}

fn is_question(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    lower.ends_with('?')
        || [
            "what ", "why ", "how ", "when ", "where ", "who ", "which ", "can ", "could ",
            "should ", "would ", "do ", "does ", "did ", "is ", "are ", "will ",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Why a spoken line was flagged as "this is for me" (master doc §6). Carried
/// back to the daemon so it can decide to suggest vs auto-fire and label the
/// surfaced card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForMeQuestion {
    /// The question text, trimmed.
    pub question: String,
    /// The name that matched (the user's own name that was spoken). `None` when
    /// `my_names` is empty but the line is still question-shaped from another
    /// speaker — kept `None` so the daemon can apply a lower-precision policy.
    pub matched_name: Option<String>,
}

/// High-precision "this question is for me" detector (master doc §6).
///
/// A line triggers when ALL hold:
/// 1. It was spoken by *someone other than the local user* (`!speaker.is_me()`)
///    — you don't trigger on your own speech.
/// 2. It is question-shaped (`is_question`).
/// 3. If `my_names` is non-empty, the line mentions one of those names
///    (word-boundary, case-insensitive). When `my_names` is empty this name
///    check is skipped, so the detector falls back to "any question from
///    another speaker" — lower precision, which the daemon gates behind
///    explicit opt-in.
///
/// Pure and side-effect free so it is cheap to unit test; the daemon owns the
/// action (suggest card vs drive the agent).
pub fn detect_for_me_question(
    segment: &TranscriptSegment,
    my_names: &[String],
) -> Option<ForMeQuestion> {
    let text = segment.text.trim();
    detect_for_me_question_given(segment, my_names, is_question(text))
}

/// [`detect_for_me_question`] with the question-shape decision INJECTED — the
/// two-stage seam (PLAN-CONTEXT-WARMUP SET 1). The daemon's detector runs the
/// lexical `is_question` first (fast, precise) and, on its rejects, an ONNX
/// classifier that catches the disfluent/declarative questions regex misses
/// ("so um do we need the flag or not", "wait is this thread safe" — measured:
/// regex 41% recall on real meeting speech, classifier 63%+). Speaker + name
/// gating stay identical regardless of who decided the question shape.
pub fn detect_for_me_question_given(
    segment: &TranscriptSegment,
    my_names: &[String],
    is_question_shaped: bool,
) -> Option<ForMeQuestion> {
    if segment.speaker.is_me() {
        return None;
    }
    let text = segment.text.trim();
    if text.is_empty() || !is_question_shaped {
        return None;
    }

    let matched_name = if my_names.is_empty() {
        None
    } else {
        let matched = my_names
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .find(|name| text_mentions_name(text, name))
            .map(|name| name.to_string());
        // Names configured but none mentioned → not addressed to me.
        matched.as_ref()?;
        matched
    };

    Some(ForMeQuestion {
        question: text.to_string(),
        matched_name,
    })
}

/// The lexical question-shape check (stage 1 of the two-stage detector): ends
/// with `?` or starts with a wh/aux word. Public so the daemon can run it
/// FIRST and only pay for the ONNX classifier on its rejects.
pub fn is_question_shaped(text: &str) -> bool {
    is_question(text)
}

/// Case-insensitive, word-boundary name match. Avoids firing on "Alexander"
/// for the name "Alex" by requiring the surrounding characters to be
/// non-alphanumeric.
fn text_mentions_name(text: &str, name: &str) -> bool {
    let haystack = text.to_lowercase();
    let needle = name.to_lowercase();
    if needle.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(&needle) {
        let start = from + rel;
        let end = start + needle.len();
        let before_ok = start == 0
            || !haystack[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric());
        let after_ok = end >= haystack.len()
            || !haystack[end..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        from = start + needle.len();
        if from >= haystack.len() {
            break;
        }
    }
    false
}

fn question_title(text: &str) -> String {
    let clean = text.trim();
    if clean.len() <= 96 {
        return format!("Q: {clean}");
    }
    format!("Q: {}...", &clean[..96])
}

fn compact_context(meeting: &MeetingRecord, count: usize) -> String {
    let context = meeting.last_transcript_text(count);
    if context.is_empty() {
        "Listening for useful meeting context.".to_string()
    } else {
        context
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        analyze_segment, detect_for_me_question, local_answer, ContextArtifact, ContextKind,
        MeetingRecord, Speaker, TranscriptSegment,
    };

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn for_me_question_fires_on_name_and_question_from_other() {
        let seg = TranscriptSegment::new(Speaker::System, "Alex, what's the status on auth?", true);
        let hit = detect_for_me_question(&seg, &names(&["Alex"])).expect("should fire");
        assert_eq!(hit.matched_name.as_deref(), Some("Alex"));
        assert!(hit.question.contains("status on auth"));
    }

    #[test]
    fn for_me_question_ignores_my_own_speech() {
        // Even if I say my own name in a question, it's not "for me".
        let seg = TranscriptSegment::new(Speaker::User, "Alex, should we ship?", true);
        assert!(detect_for_me_question(&seg, &names(&["Alex"])).is_none());
    }

    #[test]
    fn for_me_question_requires_question_shape() {
        let seg = TranscriptSegment::new(Speaker::System, "Alex is handling the deploy.", true);
        assert!(detect_for_me_question(&seg, &names(&["Alex"])).is_none());
    }

    #[test]
    fn for_me_question_name_match_is_word_bounded() {
        // "Alexander" must NOT match the name "Alex".
        let seg = TranscriptSegment::new(Speaker::System, "Is Alexander joining the call?", true);
        assert!(detect_for_me_question(&seg, &names(&["Alex"])).is_none());
    }

    #[test]
    fn for_me_question_without_configured_names_falls_back_to_any_other_question() {
        let seg = TranscriptSegment::new(Speaker::System, "How do we handle retries?", true);
        let hit = detect_for_me_question(&seg, &[]).expect("fallback should fire");
        assert!(hit.matched_name.is_none());
    }

    #[test]
    fn for_me_question_with_names_set_but_unmentioned_does_not_fire() {
        let seg = TranscriptSegment::new(Speaker::System, "How do we handle retries?", true);
        assert!(detect_for_me_question(&seg, &names(&["Alex"])).is_none());
    }

    #[test]
    fn detects_question_but_not_action_or_decision() {
        // analyze_segment surfaces a live ANSWER card for a question, but no
        // longer extracts action items / decisions per-segment — those now come
        // from the verified AI ledger (the old keyword heuristic produced
        // fragment garbage). So the structured fields stay empty here.
        let mut meeting = MeetingRecord::new(Some("Test".to_string()));
        let segment = TranscriptSegment::new(
            Speaker::System,
            "Can you explain the decision? Action item: I will update the runbook.",
            true,
        );
        meeting.transcript.push(segment.clone());

        let analysis = analyze_segment(&segment, &meeting);

        // A question still produces an answer card.
        assert!(analysis
            .cards
            .iter()
            .any(|c| matches!(c.kind, crate::CardKind::Answer)));
        // Action items / decisions are NOT extracted here anymore.
        assert!(analysis.action_items.is_empty());
        assert!(analysis.decisions.is_empty());
    }

    #[test]
    fn analyze_segment_does_not_heuristically_extract_decisions() {
        let meeting = MeetingRecord::new(Some("Test".to_string()));
        let segment = TranscriptSegment::new(
            Speaker::System,
            "We decided to ship the lightweight native overlay first.",
            true,
        );

        let analysis = analyze_segment(&segment, &meeting);
        // No per-segment decision extraction — the AI ledger owns this now.
        assert!(analysis.decisions.is_empty());
    }

    #[test]
    fn answers_with_attached_context() {
        let mut meeting = MeetingRecord::new(Some("Test".to_string()));
        meeting.context.push(ContextArtifact::new(
            ContextKind::Code,
            "/tmp/app.rs",
            "app.rs",
            Some("router implementation".to_string()),
            Some(128),
        ));

        let answer = local_answer("what code is attached?", &meeting);
        assert!(answer.contains("app.rs"));
        assert!(answer.contains("router implementation"));
    }

    #[test]
    fn answers_with_answer_instructions() {
        let mut meeting = MeetingRecord::new(Some("Test".to_string()));
        meeting.answer_instructions = Some("Be concise and mention risks.".to_string());

        let answer = local_answer("what answer style is set?", &meeting);
        assert!(answer.contains("Be concise"));
    }

    #[test]
    fn general_local_answer_names_fallback_context() {
        let mut meeting = MeetingRecord::new(Some("Test".to_string()));
        meeting.push_conversation_turn(crate::ConversationTurn::new(
            "What did we ship?",
            "The native overlay.",
            Some("test".to_string()),
            Some("local".to_string()),
        ));

        let answer = local_answer("what did we improve?", &meeting);
        assert!(answer.contains("Local fallback context"));
        assert!(answer.contains("Set a live provider key"));
    }
}
