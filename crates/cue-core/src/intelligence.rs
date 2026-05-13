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

    let lower = text.to_ascii_lowercase();
    let mut analysis = SegmentAnalysis::empty();

    if is_question(text) {
        let answer = local_answer(text, meeting);
        analysis.cards.push(
            CueCard::new(CardKind::Answer, question_title(text), answer)
                .with_source("live transcript"),
        );
    }

    if let Some(action_text) = extract_action_item(text, &lower) {
        let action = ActionItem::new(action_text, extract_owner(text), Some(segment.id));
        analysis.cards.push(
            CueCard::new(CardKind::ActionItem, "Action item", action.text.clone())
                .with_source("live transcript"),
        );
        analysis.action_items.push(action);
    }

    if let Some(decision_text) = extract_decision(text, &lower) {
        let decision = Decision::new(decision_text, Some(segment.id));
        analysis.cards.push(
            CueCard::new(CardKind::Decision, "Decision", decision.text.clone())
                .with_source("live transcript"),
        );
        analysis.decisions.push(decision);
    }

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
            "No transcript was captured for this meeting.".to_string()
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
        .map(|segment| format!("{}: {}", segment.speaker, segment.text))
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

fn question_title(text: &str) -> String {
    let clean = text.trim();
    if clean.len() <= 96 {
        return format!("Q: {clean}");
    }
    format!("Q: {}...", &clean[..96])
}

fn extract_action_item(text: &str, lower: &str) -> Option<String> {
    let trimmed = text.trim().trim_matches('-').trim();
    for marker in ["action item", "todo", "to do"] {
        if let Some(index) = lower.find(marker) {
            let start = index + marker.len();
            let action = text
                .get(start..)
                .unwrap_or(trimmed)
                .trim()
                .trim_start_matches([':', '-', ' '])
                .trim();
            if !action.is_empty() {
                return Some(action.to_string());
            }
        }
    }

    for prefix in ["please ", "can you ", "could you "] {
        if lower.starts_with(prefix) {
            let action = text
                .get(prefix.len()..)
                .unwrap_or(trimmed)
                .trim()
                .trim_start_matches([':', '-', ' '])
                .trim();
            if !action.is_empty() {
                return Some(action.to_string());
            }
        }
    }

    let inline_markers = [
        "action item",
        "todo",
        "to do",
        "please ",
        "can you ",
        "could you ",
        "i will ",
        "i'll ",
        "we need to ",
        "let's follow up",
        "follow up",
    ];

    if inline_markers.iter().any(|marker| lower.contains(marker)) {
        return Some(trimmed.to_string());
    }

    None
}

fn extract_decision(text: &str, lower: &str) -> Option<String> {
    let markers = [
        "we decided",
        "decision:",
        "decided to",
        "let's go with",
        "we will use",
        "we are going with",
        "approved",
    ];

    if markers.iter().any(|marker| lower.contains(marker)) {
        return Some(text.trim().to_string());
    }

    None
}

fn extract_owner(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    if lower.contains("i will") || lower.contains("i'll") {
        return Some("you".to_string());
    }
    None
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
        analyze_segment, local_answer, ContextArtifact, ContextKind, MeetingRecord, Speaker,
        TranscriptSegment,
    };

    #[test]
    fn detects_question_action_and_decision_cards() {
        let mut meeting = MeetingRecord::new(Some("Test".to_string()));
        let segment = TranscriptSegment::new(
            Speaker::System,
            "Can you explain the decision? Action item: I will update the runbook.",
            true,
        );
        meeting.transcript.push(segment.clone());

        let analysis = analyze_segment(&segment, &meeting);

        assert!(analysis.cards.len() >= 2);
        assert_eq!(analysis.action_items.len(), 1);
        assert_eq!(analysis.action_items[0].text, "I will update the runbook.");
    }

    #[test]
    fn detects_decision() {
        let meeting = MeetingRecord::new(Some("Test".to_string()));
        let segment = TranscriptSegment::new(
            Speaker::System,
            "We decided to ship the lightweight native overlay first.",
            true,
        );

        let analysis = analyze_segment(&segment, &meeting);
        assert_eq!(analysis.decisions.len(), 1);
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
