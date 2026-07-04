use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

impl Speaker {
    /// Human, conversational label for transcript rendering and the
    /// question→trigger heuristic ("did *They* address *You*?"). The mic is the
    /// local user ("You"); system audio is the remote side of the call
    /// ("They"). Distinct from [`Display`], which emits the lowercase technical
    /// tag used for serialization and logs.
    pub fn display_label(self) -> &'static str {
        match self {
            Self::User => "You",
            Self::System => "They",
            Self::Other => "Other",
            Self::Unknown => "Speaker",
        }
    }

    /// Whether this speaker is the local user (the mic / "me"). The trigger in
    /// the daemon uses this to only fire on lines spoken by *others*.
    pub fn is_me(self) -> bool {
        matches!(self, Self::User)
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
    /// Diarized per-meeting speaker id (0-based), when speaker diarization is
    /// active. ORTHOGONAL to the coarse `speaker` channel tag (mic-vs-system):
    /// `speaker` says which side, `speaker_id` says which individual. `None`
    /// when diarization is off or hasn't labeled this segment yet. `#[serde
    /// (default)]` so transcripts persisted before this field deserialize.
    #[serde(default)]
    pub speaker_id: Option<i64>,
    /// Audio position (seconds since capture start) for this segment, on the SAME
    /// sample clock the diarizer uses (retention buffer's cumulative sample count
    /// ÷ 16 kHz). This is what lets live diarization align a diarized speaker-time
    /// segment to this transcript segment EXACTLY, rather than guessing via
    /// wall-clock `created_at` (a different clock — the root cause of segments
    /// staying unlabeled). `None` when diarization is off / no audio clock was
    /// available. `#[serde(default)]` for back-compat with older transcripts.
    #[serde(default)]
    pub audio_start_secs: Option<f64>,
    /// Estimated audio duration (seconds) of this segment's speech, so it forms an
    /// interval `[audio_start_secs, +audio_dur_secs]` for max-total-overlap speaker
    /// assignment (WhisperX-style) instead of a single-point test. `None` falls
    /// back to a default span. `#[serde(default)]` for back-compat.
    #[serde(default)]
    pub audio_dur_secs: Option<f64>,
}

impl TranscriptSegment {
    pub fn new(speaker: Speaker, text: impl Into<String>, is_final: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            speaker,
            text: text.into(),
            created_at: clock::now_epoch_ms_string(),
            is_final,
            speaker_id: None,
            audio_start_secs: None,
            audio_dur_secs: None,
        }
    }

    /// Attach a diarized speaker id (builder-style).
    pub fn with_speaker_id(mut self, speaker_id: Option<i64>) -> Self {
        self.speaker_id = speaker_id;
        self
    }

    /// Attach the audio-clock position (seconds since capture start), builder-style.
    pub fn with_audio_start_secs(mut self, secs: Option<f64>) -> Self {
        self.audio_start_secs = secs;
        self
    }

    /// Attach the estimated audio duration (seconds), builder-style.
    pub fn with_audio_dur_secs(mut self, secs: Option<f64>) -> Self {
        self.audio_dur_secs = secs;
        self
    }

    /// Label for AI context / display. Prefers the diarized individual when
    /// present ("You" stays "You" since the mic side is 100%-reliably the user;
    /// the system side becomes "Speaker N" when diarization has resolved it),
    /// else falls back to the coarse channel label.
    pub fn context_label(&self) -> String {
        match (self.speaker.is_me(), self.speaker_id) {
            // The mic channel is always the user — keep the reliable "You".
            (true, _) => self.speaker.display_label().to_string(),
            // A resolved individual on the far side.
            (false, Some(id)) => format!("Speaker {id}"),
            // No diarization yet — coarse channel label ("They"/"Other"/…).
            (false, None) => self.speaker.display_label().to_string(),
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
    pub processing_status: ContextProcessingStatus,
    #[serde(default)]
    pub processing_error: Option<String>,
    pub created_at: String,
}

impl ContextArtifact {
    pub fn new(
        kind: ContextKind,
        path: impl Into<String>,
        title: impl Into<String>,
        note: Option<String>,
        size_bytes: Option<u64>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            path: path.into(),
            title: title.into(),
            note,
            size_bytes,
            text_preview: None,
            processing_status: ContextProcessingStatus::Pending,
            processing_error: None,
            created_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn with_text_preview(mut self, preview: impl Into<String>) -> Self {
        let preview = preview.into();
        if preview.trim().is_empty() {
            return self;
        }

        self.text_preview = Some(preview);
        self.processing_status = ContextProcessingStatus::Ready;
        self.processing_error = None;
        self
    }

    pub fn with_processing_status(mut self, status: ContextProcessingStatus) -> Self {
        self.processing_status = status;
        self
    }

    pub fn with_processing_error(mut self, error: impl Into<String>) -> Self {
        self.processing_status = ContextProcessingStatus::Failed;
        self.processing_error = Some(error.into());
        self
    }

    pub fn with_unsupported_error(mut self, error: impl Into<String>) -> Self {
        self.processing_status = ContextProcessingStatus::Unsupported;
        self.processing_error = Some(error.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub id: Uuid,
    pub question: String,
    pub answer: String,
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
            source,
            provider,
            created_at: clock::now_epoch_ms_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingRecord {
    pub id: Uuid,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub transcript: Vec<TranscriptSegment>,
    pub action_items: Vec<ActionItem>,
    pub decisions: Vec<Decision>,
    #[serde(default)]
    pub context: Vec<ContextArtifact>,
    #[serde(default)]
    pub conversation: Vec<ConversationTurn>,
    #[serde(default)]
    pub answer_instructions: Option<String>,
    pub summary: Option<String>,
    /// The agent session id this meeting was chained to, when the user asked
    /// through an attached agent (Claude/Codex/…). Recorded so opening the
    /// meeting later can offer to RESUME that agent thread, not just show old
    /// text. `#[serde(default)]` keeps every legacy on-disk meeting decodable.
    #[serde(default)]
    pub agent_session_id: Option<String>,
    /// The agent KIND ("claude_code", "cursor", …) paired with
    /// [`agent_session_id`]. Both are needed to resume the right thread: the
    /// session id alone is meaningless without knowing which agent owns it, and
    /// resuming it onto whatever agent happens to be attached would mis-target
    /// (a Claude id attached under Cursor → a dead thread). `#[serde(default)]`
    /// for legacy meetings (and meetings linked before this field existed —
    /// their `agent_session_id` is present but kind is `None`, so the UI must
    /// fall back to the attached agent for those).
    #[serde(default)]
    pub agent_kind: Option<String>,
}

impl MeetingRecord {
    pub fn new(title: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Ad hoc meeting".to_string()),
            started_at: clock::now_epoch_ms_string(),
            ended_at: None,
            transcript: Vec::new(),
            action_items: Vec::new(),
            decisions: Vec::new(),
            context: Vec::new(),
            conversation: Vec::new(),
            answer_instructions: None,
            summary: None,
            agent_session_id: None,
            agent_kind: None,
        }
    }

    /// Whether this meeting holds anything worth keeping: spoken transcript,
    /// attached context, a conversation, or a written summary. Empty meetings
    /// are auto-created shells (e.g. from a stray screen capture) that should
    /// not be resumed on boot or shown in History.
    pub fn has_content(&self) -> bool {
        !self.transcript.is_empty()
            || !self.context.is_empty()
            || !self.conversation.is_empty()
            || self
                .summary
                .as_ref()
                .is_some_and(|summary| !summary.trim().is_empty())
    }

    pub fn last_transcript_text(&self, count: usize) -> String {
        let start = self.transcript.len().saturating_sub(count);
        self.transcript[start..]
            .iter()
            .map(|segment| format!("{}: {}", segment.context_label(), segment.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn last_transcript_text_bounded(&self, count: usize, max_chars: usize) -> String {
        if count == 0 || max_chars == 0 {
            return String::new();
        }

        let start = self.transcript.len().saturating_sub(count);
        let mut selected = Vec::new();
        let mut used_chars = 0usize;

        for segment in self.transcript[start..].iter().rev() {
            let line = format!("{}: {}", segment.context_label(), segment.text.trim());
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

fn truncate_transcript_line_tail(speaker: Speaker, text: &str, max_chars: usize) -> String {
    let label = speaker.display_label();
    let prefix = format!("{label}: ...");
    let prefix_chars = prefix.chars().count();
    if max_chars <= prefix_chars {
        return tail_chars(&format!("{label}: {}", text.trim()), max_chars);
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
    fn bounded_transcript_truncates_single_long_latest_turn() {
        let mut meeting = MeetingRecord::new(Some("Long".to_string()));
        meeting.transcript.push(TranscriptSegment::new(
            Speaker::System,
            format!("{} important ending", "filler ".repeat(80)),
            true,
        ));

        let text = meeting.last_transcript_text_bounded(4, 80);
        assert!(text.chars().count() <= 80);
        // System audio renders with the conversational "They" label.
        assert!(text.starts_with("They: ..."));
        assert!(text.contains("important ending"));
    }

    #[test]
    fn speaker_display_label_distinguishes_me_from_them() {
        assert_eq!(Speaker::User.display_label(), "You");
        assert_eq!(Speaker::System.display_label(), "They");
        assert!(Speaker::User.is_me());
        assert!(!Speaker::System.is_me());
        assert!(!Speaker::Other.is_me());
    }

    #[test]
    fn new_meeting_has_no_agent_session() {
        let meeting = MeetingRecord::new(Some("Fresh".to_string()));
        assert_eq!(meeting.agent_session_id, None);
    }

    #[test]
    fn legacy_meeting_json_without_agent_session_decodes() {
        // A meeting persisted before agent_session_id existed carries none of
        // the field; #[serde(default)] must decode it to None rather than fail,
        // so old on-disk meetings keep loading.
        let legacy = r#"{
            "id":"00000000-0000-0000-0000-000000000000",
            "title":"Legacy",
            "started_at":"1718000000000",
            "ended_at":null,
            "transcript":[],
            "action_items":[],
            "decisions":[],
            "summary":null
        }"#;
        let meeting: MeetingRecord =
            serde_json::from_str(legacy).expect("legacy meeting must decode");
        assert_eq!(meeting.agent_session_id, None);
    }

    #[test]
    fn meeting_with_agent_session_round_trips() {
        let mut meeting = MeetingRecord::new(Some("Chained".to_string()));
        meeting.agent_session_id = Some("sess-xyz".to_string());
        let json = serde_json::to_string(&meeting).expect("serialize");
        let decoded: MeetingRecord = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.agent_session_id.as_deref(), Some("sess-xyz"));
    }
}
