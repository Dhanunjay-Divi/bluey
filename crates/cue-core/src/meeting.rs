use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cards::CueCardArtifact;
use crate::clock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    System,
    User,
    Other,
    Unknown,
}

impl Default for Speaker {
    fn default() -> Self {
        Self::Unknown
    }
}

impl std::fmt::Display for Speaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::User => write!(f, "user"),
            Self::Other => write!(f, "other"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: Uuid,
    pub speaker: Speaker,
    pub text: String,
    pub created_at: String,
    pub is_final: bool,
}

impl TranscriptSegment {
    pub fn new(speaker: Speaker, text: impl Into<String>, is_final: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            speaker,
            text: text.into(),
            created_at: clock::now_epoch_ms_string(),
            is_final,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: Uuid,
    pub text: String,
    pub owner: Option<String>,
    pub source_segment_id: Option<Uuid>,
    pub created_at: String,
    pub done: bool,
}

impl ActionItem {
    pub fn new(
        text: impl Into<String>,
        owner: Option<String>,
        source_segment_id: Option<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            text: text.into(),
            owner,
            source_segment_id,
            created_at: clock::now_epoch_ms_string(),
            done: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: Uuid,
    pub text: String,
    pub source_segment_id: Option<Uuid>,
    pub created_at: String,
}

impl Decision {
    pub fn new(text: impl Into<String>, source_segment_id: Option<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            text: text.into(),
            source_segment_id,
            created_at: clock::now_epoch_ms_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextKind {
    Image,
    Diagram,
    Code,
    Document,
    Text,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextProcessingStatus {
    Pending,
    Ready,
    Unsupported,
    Failed,
}

impl Default for ContextProcessingStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl std::fmt::Display for ContextProcessingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Ready => write!(f, "ready"),
            Self::Unsupported => write!(f, "unsupported"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::fmt::Display for ContextKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Image => write!(f, "image"),
            Self::Diagram => write!(f, "diagram"),
            Self::Code => write!(f, "code"),
            Self::Document => write!(f, "document"),
            Self::Text => write!(f, "text"),
            Self::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextArtifact {
    pub id: Uuid,
    pub kind: ContextKind,
    pub path: String,
    pub title: String,
    pub note: Option<String>,
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub text_preview: Option<String>,
    #[serde(default)]
    pub markdown_path: Option<String>,
    #[serde(default)]
    pub processing_status: ContextProcessingStatus,
    #[serde(default)]
    pub processing_error: Option<String>,
    pub created_at: String,
    /// Monotonic local revision used by cloud sync. Older records deserialize
    /// with an empty value and fall back to `created_at` at the sync boundary.
    #[serde(default)]
    pub updated_at: String,
}

impl ContextArtifact {
    pub fn new(
        kind: ContextKind,
        path: impl Into<String>,
        title: impl Into<String>,
        note: Option<String>,
        size_bytes: Option<u64>,
    ) -> Self {
        let now = clock::now_epoch_ms_string();
        Self {
            id: Uuid::new_v4(),
            kind,
            path: path.into(),
            title: title.into(),
            note,
            size_bytes,
            text_preview: None,
            markdown_path: None,
            processing_status: ContextProcessingStatus::Pending,
            processing_error: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn touch(&mut self) {
        let now = clock::now_epoch_ms_string()
            .parse::<i64>()
            .unwrap_or_default();
        let current = self
            .updated_at
            .parse::<i64>()
            .or_else(|_| self.created_at.parse::<i64>())
            .unwrap_or_default();
        self.updated_at = now.max(current.saturating_add(1)).to_string();
    }

    pub fn with_text_preview(mut self, preview: impl Into<String>) -> Self {
        let preview = preview.into();
        if preview.trim().is_empty() {
            return self;
        }

        self.text_preview = Some(preview);
        self.processing_status = ContextProcessingStatus::Ready;
        self.processing_error = None;
        self.touch();
        self
    }

    pub fn with_markdown_path(mut self, path: impl Into<String>) -> Self {
        self.markdown_path = Some(path.into());
        self.touch();
        self
    }

    pub fn with_processing_status(mut self, status: ContextProcessingStatus) -> Self {
        self.processing_status = status;
        self.touch();
        self
    }

    pub fn with_processing_error(mut self, error: impl Into<String>) -> Self {
        self.processing_status = ContextProcessingStatus::Failed;
        self.processing_error = Some(error.into());
        self.touch();
        self
    }

    pub fn with_unsupported_error(mut self, error: impl Into<String>) -> Self {
        self.processing_status = ContextProcessingStatus::Unsupported;
        self.processing_error = Some(error.into());
        self.touch();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub id: Uuid,
    pub question: String,
    pub answer: String,
    #[serde(default)]
    pub attachment_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<CueCardArtifact>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    pub created_at: String,
}

impl ConversationTurn {
    pub fn new(
        question: impl Into<String>,
        answer: impl Into<String>,
        source: Option<String>,
        provider: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            question: question.into(),
            answer: answer.into(),
            attachment_ids: Vec::new(),
            artifact: None,
            source,
            provider,
            created_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn with_attachment_ids(mut self, attachment_ids: Vec<Uuid>) -> Self {
        self.attachment_ids = attachment_ids;
        self
    }

    pub fn with_artifact(mut self, artifact: Option<CueCardArtifact>) -> Self {
        self.artifact = artifact;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingRecord {
    pub id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_account_id: Option<String>,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub transcript: Vec<TranscriptSegment>,
    /// Number of finalized transcript segments already used by a live-caption
    /// answer. The transcript remains in session history; this cursor only
    /// prevents the next Answer action from submitting the same speech again.
    #[serde(default)]
    pub live_answer_transcript_cursor: usize,
    pub action_items: Vec<ActionItem>,
    pub decisions: Vec<Decision>,
    #[serde(default)]
    pub context: Vec<ContextArtifact>,
    #[serde(default)]
    pub conversation: Vec<ConversationTurn>,
    #[serde(default)]
    pub answer_instructions: Option<String>,
    #[serde(default)]
    pub diagnostics: MeetingDiagnostics,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeetingDiagnostics {
    #[serde(default)]
    pub listen_runs: u64,
    #[serde(default)]
    pub stt_parse_errors: u64,
    #[serde(default)]
    pub stt_provider_errors: u64,
    #[serde(default)]
    pub audio_start_errors: u64,
    #[serde(default)]
    pub audio_source_errors: u64,
    #[serde(default)]
    pub last_audio_session_id: Option<String>,
    #[serde(default)]
    pub last_stt_provider: Option<String>,
    #[serde(default)]
    pub last_error_kind: Option<String>,
    #[serde(default)]
    pub last_error_message: Option<String>,
    #[serde(default)]
    pub last_error_at: Option<String>,
}

impl MeetingDiagnostics {
    pub fn record_listen_start(
        &mut self,
        audio_session_id: impl Into<String>,
        stt_provider: Option<impl Into<String>>,
    ) {
        self.listen_runs = self.listen_runs.saturating_add(1);
        self.last_audio_session_id = Some(audio_session_id.into());
        if let Some(provider) = stt_provider {
            let provider = provider.into();
            if !provider.trim().is_empty() {
                self.last_stt_provider = Some(provider);
            }
        }
    }

    pub fn record_error(&mut self, kind: impl Into<String>, message: impl Into<String>) {
        let kind = kind.into();
        match kind.as_str() {
            "stt_parse_error" => self.stt_parse_errors = self.stt_parse_errors.saturating_add(1),
            "stt_provider_error" => {
                self.stt_provider_errors = self.stt_provider_errors.saturating_add(1)
            }
            "audio_start_error" => {
                self.audio_start_errors = self.audio_start_errors.saturating_add(1)
            }
            "audio_source_error" => {
                self.audio_source_errors = self.audio_source_errors.saturating_add(1)
            }
            _ => {}
        }
        self.last_error_kind = Some(kind);
        self.last_error_message = Some(message.into());
        self.last_error_at = Some(clock::now_epoch_ms_string());
    }
}

impl MeetingRecord {
    pub fn new(title: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            owner_account_id: None,
            title: title
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "New recording".to_string()),
            started_at: clock::now_epoch_ms_string(),
            ended_at: None,
            transcript: Vec::new(),
            live_answer_transcript_cursor: 0,
            action_items: Vec::new(),
            decisions: Vec::new(),
            context: Vec::new(),
            conversation: Vec::new(),
            answer_instructions: None,
            diagnostics: MeetingDiagnostics::default(),
            summary: None,
        }
    }

    pub fn session_code(&self) -> String {
        short_session_code(self.id)
    }

    pub fn last_transcript_text(&self, count: usize) -> String {
        let start = self.transcript.len().saturating_sub(count);
        self.transcript[start..]
            .iter()
            .map(|segment| format!("{}: {}", segment.speaker, segment.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn last_transcript_text_bounded(&self, count: usize, max_chars: usize) -> String {
        self.transcript_text_bounded_from(0, count, max_chars)
    }

    pub fn unanswered_live_transcript_text_bounded(
        &self,
        count: usize,
        max_chars: usize,
    ) -> String {
        self.transcript_text_bounded_from(self.live_answer_transcript_cursor, count, max_chars)
    }

    pub fn has_unanswered_live_transcript(&self) -> bool {
        self.transcript
            .get(
                self.live_answer_transcript_cursor
                    .min(self.transcript.len())..,
            )
            .is_some_and(|segments| {
                segments
                    .iter()
                    .any(|segment| !segment.text.trim().is_empty())
            })
    }

    pub fn mark_live_transcript_answered(&mut self) {
        self.mark_live_transcript_answered_through(self.transcript.len());
    }

    /// Marks only the transcript prefix that was included in an answer.
    /// Segments arriving while the answer is streaming remain available for
    /// the next Answer request instead of being consumed accidentally.
    pub fn mark_live_transcript_answered_through(&mut self, segment_count: usize) {
        self.live_answer_transcript_cursor = segment_count.min(self.transcript.len());
    }

    fn transcript_text_bounded_from(
        &self,
        cursor: usize,
        count: usize,
        max_chars: usize,
    ) -> String {
        if count == 0 || max_chars == 0 {
            return String::new();
        }

        let cursor = cursor.min(self.transcript.len());
        let start = self.transcript.len().saturating_sub(count).max(cursor);
        let mut selected = Vec::new();
        let mut used_chars = 0usize;

        for segment in self.transcript[start..].iter().rev() {
            let line = format!("{}: {}", segment.speaker, segment.text.trim());
            if line.trim().is_empty() {
                continue;
            }

            let separator_chars = usize::from(!selected.is_empty());
            let line_chars = line.chars().count();
            if used_chars + separator_chars + line_chars <= max_chars {
                selected.push(line);
                used_chars += separator_chars + line_chars;
                continue;
            }

            let remaining = max_chars.saturating_sub(used_chars + separator_chars);
            if selected.is_empty() || remaining >= 96 {
                let truncated =
                    truncate_transcript_line_tail(segment.speaker, &segment.text, remaining);
                if !truncated.trim().is_empty() {
                    selected.push(truncated);
                }
            }
            break;
        }

        selected.reverse();
        selected.join("\n")
    }

    pub fn push_conversation_turn(&mut self, turn: ConversationTurn) {
        self.conversation.push(turn);
        let excess = self.conversation.len().saturating_sub(80);
        if excess > 0 {
            self.conversation.drain(0..excess);
        }
    }

    pub fn last_conversation_text(&self, count: usize) -> String {
        let start = self.conversation.len().saturating_sub(count);
        self.conversation[start..]
            .iter()
            .map(|turn| {
                let mut text = format!("you: {}\nbluey: {}", turn.question, turn.answer);
                if let Some(provider) = turn
                    .provider
                    .as_ref()
                    .filter(|provider| !provider.trim().is_empty())
                {
                    text.push_str(&format!("\nprovider: {provider}"));
                }
                text
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

pub fn short_session_code(id: Uuid) -> String {
    id.as_simple()
        .to_string()
        .chars()
        .take(8)
        .collect::<String>()
        .to_ascii_uppercase()
}

fn truncate_transcript_line_tail(speaker: Speaker, text: &str, max_chars: usize) -> String {
    let prefix = format!("{speaker}: ...");
    let prefix_chars = prefix.chars().count();
    if max_chars <= prefix_chars {
        return tail_chars(&format!("{speaker}: {}", text.trim()), max_chars);
    }

    let tail_budget = max_chars - prefix_chars;
    format!(
        "{prefix}{}",
        tail_chars(text.trim(), tail_budget).trim_start()
    )
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let mut chars = text.chars().rev().take(max_chars).collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingRecap {
    pub meeting_id: Uuid,
    pub title: String,
    pub summary: String,
    pub transcript_segments: usize,
    pub context: Vec<ContextArtifact>,
    pub answer_instructions: Option<String>,
    pub action_items: Vec<ActionItem>,
    pub decisions: Vec<Decision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHit {
    pub meeting_id: Uuid,
    pub meeting_title: String,
    pub source: String,
    pub snippet: String,
    pub score: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_conversation_text_keeps_follow_up_context() {
        let mut meeting = MeetingRecord::new(Some("Follow-up".to_string()));
        meeting.push_conversation_turn(ConversationTurn::new(
            "What is the plan?",
            "Use the current session context.",
            Some("overlay ask".to_string()),
            Some("Bluey managed".to_string()),
        ));
        meeting.push_conversation_turn(ConversationTurn::new(
            "Can you expand step two?",
            "Step two is to wire the provider route.",
            Some("overlay ask".to_string()),
            Some("Bluey managed".to_string()),
        ));

        let text = meeting.last_conversation_text(2);
        assert!(text.contains("you: What is the plan?"));
        assert!(text.contains("bluey: Step two is to wire the provider route."));
        assert!(text.contains("provider: Bluey managed"));
    }

    #[test]
    fn bounded_transcript_keeps_recent_turns_within_budget() {
        let mut meeting = MeetingRecord::new(Some("Budget".to_string()));
        for index in 0..8 {
            meeting.transcript.push(TranscriptSegment::new(
                Speaker::User,
                format!("turn {index} with enough words to consume budget"),
                true,
            ));
        }

        let text = meeting.last_transcript_text_bounded(8, 120);
        assert!(text.chars().count() <= 120);
        assert!(!text.contains("turn 0"));
        assert!(text.contains("turn 7"));
    }

    #[test]
    fn live_answer_cursor_keeps_history_but_excludes_already_answered_speech() {
        let mut meeting = MeetingRecord::new(Some("Live answer".to_string()));
        meeting.transcript.push(TranscriptSegment::new(
            Speaker::User,
            "explain the old question",
            true,
        ));
        meeting.mark_live_transcript_answered();
        meeting.transcript.push(TranscriptSegment::new(
            Speaker::User,
            "explain the new question",
            true,
        ));

        assert_eq!(meeting.transcript.len(), 2);
        assert!(meeting.has_unanswered_live_transcript());
        let text = meeting.unanswered_live_transcript_text_bounded(8, 1_000);
        assert!(!text.contains("old question"));
        assert!(text.contains("new question"));

        meeting.mark_live_transcript_answered();
        assert!(!meeting.has_unanswered_live_transcript());
        assert!(meeting
            .unanswered_live_transcript_text_bounded(8, 1_000)
            .is_empty());
    }

    #[test]
    fn live_answer_cursor_does_not_consume_speech_arriving_during_generation() {
        let mut meeting = MeetingRecord::new(Some("Live answer boundary".to_string()));
        meeting.transcript.push(TranscriptSegment::new(
            Speaker::User,
            "question included in the request",
            true,
        ));
        let request_high_water_mark = meeting.transcript.len();
        meeting.transcript.push(TranscriptSegment::new(
            Speaker::User,
            "follow-up spoken while Bluey was answering",
            true,
        ));

        meeting.mark_live_transcript_answered_through(request_high_water_mark);

        assert!(meeting.has_unanswered_live_transcript());
        let remaining = meeting.unanswered_live_transcript_text_bounded(8, 1_000);
        assert!(!remaining.contains("included in the request"));
        assert!(remaining.contains("while Bluey was answering"));
    }

    #[test]
    fn bounded_transcript_truncates_single_long_latest_turn() {
        let mut meeting = MeetingRecord::new(Some("Long".to_string()));
        meeting.transcript.push(TranscriptSegment::new(
            Speaker::System,
            format!("{} important ending", "filler ".repeat(80)),
            true,
        ));

        let text = meeting.last_transcript_text_bounded(4, 80);
        assert!(text.chars().count() <= 80);
        assert!(text.starts_with("system: ..."));
        assert!(text.contains("important ending"));
    }
}
