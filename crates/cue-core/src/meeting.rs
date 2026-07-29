use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::clock;

/// The boilerplate summary/title a meeting gets when it ends with no captured
/// content. It is NOT real content, so [`MeetingRecord::meeting_is_substantive`]
/// must not count it — otherwise every empty meeting clutters the History list
/// as a "0 lines · 0 Q&A" row. Referenced by the recap generator too.
pub const EMPTY_MEETING_SUMMARY: &str = "No transcript was captured for this meeting.";

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
    /// Other diarized speakers who had a real share of THIS fragment's audio
    /// (talk-over / interruption). One ASR fragment carries a single lexical
    /// stream, so the text is attributed to `speaker_id` (the dominant voice),
    /// but co-speakers are surfaced here rather than discarded — the honest
    /// "more than one person was speaking in this line" signal. Empty for the
    /// normal single-speaker case. `#[serde(default)]` for back-compat.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secondary_speaker_ids: Vec<i64>,
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
            secondary_speaker_ids: Vec::new(),
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
    ///
    /// Speaker ids are shown **1-based** to match every user-facing surface (the
    /// overlay label, the wire, the dev socket) — so the AI and the user name the
    /// same person identically ("Speaker 2" in the app == "Speaker 2" to the AI).
    /// A talk-over fragment surfaces its co-speakers ("Speaker 2 + 3") so the AI
    /// knows more than one voice was in the line when extracting decisions/owners.
    pub fn context_label(&self) -> String {
        match (self.speaker.is_me(), self.speaker_id) {
            // The mic channel is always the user — keep the reliable "You".
            (true, _) => self.speaker.display_label().to_string(),
            // A resolved individual on the far side (1-based; + co-speakers).
            (false, Some(id)) => speaker_display_label(id, &self.secondary_speaker_ids),
            // No diarization yet — coarse channel label ("They"/"Other"/…).
            (false, None) => self.speaker.display_label().to_string(),
        }
    }
}

/// The single source of truth for a diarized speaker's display label. Ids are
/// **1-based** ("Speaker 2", not 1) so the overlay, the rehydrate wire, the dev
/// socket, AND the AI-context transcript all name the same person identically.
/// A talk-over fragment appends its co-speakers ("Speaker 2 + 3"). Used by
/// `TranscriptSegment::context_label` (AI), the daemon's live overlay upgrade,
/// and `to_wire_line` (snapshot) — one place to change the format.
pub fn speaker_display_label(primary: i64, secondary: &[i64]) -> String {
    if secondary.is_empty() {
        return format!("Speaker {}", primary + 1);
    }
    // Talk-over: list all speaker numbers comma-separated ("Speaker 2, 1"),
    // not "Speaker 2 + 1" (which reads like arithmetic).
    let mut nums: Vec<i64> = std::iter::once(primary)
        .chain(secondary.iter().copied())
        .collect();
    nums.dedup();
    let joined = nums
        .iter()
        .map(|s| (s + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("Speaker {joined}")
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
    /// The id of the LAST finalized transcript segment present when this artifact
    /// was attached — its ANCHOR into the timeline. The overlay renders each
    /// attachment inline right after the transcript line CONTAINING this segment,
    /// so a screenshot/file stays pinned where it was added (like a Q&A turn) and
    /// the conversation flows below it. A segment id (not a raw index) because the
    /// UI GROUPS segments into fewer lines — an index would overshoot to the tail.
    /// `None` on pre-existing artifacts (they fall to the end).
    #[serde(default)]
    pub anchor_segment_id: Option<String>,
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
            anchor_segment_id: None,
        }
    }

    /// Pin this artifact to the transcript segment it was attached after (the
    /// last finalized segment at attach time). See [`ContextArtifact::anchor_segment_id`].
    pub fn with_anchor_segment_id(mut self, id: impl Into<String>) -> Self {
        self.anchor_segment_id = Some(id.into());
        self
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

    /// The most recent activity time (epoch ms) — the newest transcript segment's
    /// `created_at`, falling back to `started_at`. Used by boot recovery to tell a
    /// genuine mid-session restart (resume) from stale leftover junk (archive).
    pub fn last_activity_ms(&self) -> i64 {
        self.transcript
            .iter()
            .filter_map(|s| s.created_at.parse::<i64>().ok())
            .max()
            .or_else(|| self.started_at.parse::<i64>().ok())
            .unwrap_or(0)
    }

    /// A STRICTER bar than [`has_content`] for what belongs in the History list.
    /// A meeting is "substantive" — worth showing to the user — iff it has real
    /// conversational weight: at least two committed units (final transcript
    /// segments + Q&A turns combined), OR a written summary, OR attached context.
    /// This hides the 1-line / 0-turn fragments and empty active shells that a
    /// too-eager create path used to spawn, without discarding anything already
    /// archived.
    pub fn meeting_is_substantive(&self) -> bool {
        let final_transcript = self
            .transcript
            .iter()
            .filter(|segment| segment.is_final)
            .count();
        if final_transcript + self.conversation.len() >= 2 {
            return true;
        }
        !self.context.is_empty()
            || self
                .summary
                .as_ref()
                // The "no transcript was captured" placeholder is NOT real
                // content — a meeting that ended empty gets that boilerplate
                // summary + title, and counting it as substantive is exactly what
                // clutters the list with "0 lines · 0 Q&A" rows. Ignore it.
                .is_some_and(|summary| {
                    let s = summary.trim();
                    !s.is_empty() && s != EMPTY_MEETING_SUMMARY
                })
    }

    pub fn last_transcript_text(&self, count: usize) -> String {
        // `count` = speaker TURNS. Coalesce consecutive same-speaker fragments into
        // one labeled line first (the streaming STT emits many tiny fragments per
        // utterance; labeling each produced "They: … They: … They: …" mid-turn),
        // then take the last `count` turns.
        let mut turns: Vec<(String, String)> = Vec::new();
        for segment in self.transcript.iter() {
            let label = segment.context_label().to_string();
            let text = segment.text.trim();
            if text.is_empty() {
                continue;
            }
            match turns.last_mut() {
                Some((prev_label, prev_text)) if *prev_label == label => {
                    if !prev_text.ends_with(' ') && !text.starts_with(' ') {
                        prev_text.push(' ');
                    }
                    prev_text.push_str(text);
                }
                _ => turns.push((label, text.to_string())),
            }
        }
        let start = turns.len().saturating_sub(count);
        turns[start..]
            .iter()
            .map(|(label, text)| format!("{label}: {text}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn last_transcript_text_bounded(&self, count: usize, max_chars: usize) -> String {
        if count == 0 || max_chars == 0 {
            return String::new();
        }

        // `count` is a count of speaker TURNS, not raw segments. The streaming STT
        // emits many tiny fragments PER utterance (each ~10 chars is its own
        // segment), so labeling per-segment produced "They: could actually give
        // They: feedback They: but" — the label repeated mid-sentence. Coalesce
        // consecutive same-speaker segments into ONE turn line first, THEN take the
        // last `count` turns and bound by chars. The label now appears once per
        // turn, as a reader expects.
        let mut turns: Vec<(String, String)> = Vec::new(); // (label, joined text)
        for segment in self.transcript.iter() {
            let label = segment.context_label().to_string();
            let text = segment.text.trim();
            if text.is_empty() {
                continue;
            }
            match turns.last_mut() {
                Some((prev_label, prev_text)) if *prev_label == label => {
                    // Same speaker → extend the turn. RAW concat with a single
                    // space keeps fragment word boundaries without doubling.
                    if !prev_text.ends_with(' ') && !text.starts_with(' ') {
                        prev_text.push(' ');
                    }
                    prev_text.push_str(text);
                }
                _ => turns.push((label, text.to_string())),
            }
        }

        let start = turns.len().saturating_sub(count);
        let mut selected = Vec::new();
        let mut used_chars = 0usize;

        for (label, text) in turns[start..].iter().rev() {
            let line = format!("{label}: {text}");
            let separator_chars = usize::from(!selected.is_empty());
            let line_chars = line.chars().count();
            if used_chars + separator_chars + line_chars <= max_chars {
                selected.push(line);
                used_chars += separator_chars + line_chars;
                continue;
            }

            // Turn too long for the remaining budget: keep its TAIL (the most
            // recent words) so the newest content survives the char cap.
            let remaining = max_chars.saturating_sub(used_chars + separator_chars);
            if selected.is_empty() || remaining >= 96 {
                let tail = tail_chars(text, remaining.saturating_sub(label.len() + 2));
                if !tail.trim().is_empty() {
                    selected.push(format!("{label}: {tail}"));
                }
            }
            break;
        }

        selected.reverse();
        selected.join("\n")
    }

    /// A labeled block of the meeting's NON-transcript content — user notes,
    /// attached files, and screenshots — so the summarizer and ledger extractor
    /// see them ALONGSIDE the spoken transcript, not just the words. Each entry is
    /// tagged by kind (`[Note]` / `[Attachment]` / `[Screenshot]`) so the LLM knows
    /// it is reference material added to the meeting, not something spoken. Bounded
    /// to `max_chars`; returns an empty string when there is nothing to include.
    pub fn context_block_bounded(&self, max_chars: usize) -> String {
        if max_chars == 0 || self.context.is_empty() {
            return String::new();
        }
        let mut lines: Vec<String> = Vec::new();
        let mut used = 0usize;
        // Newest first so the most recent context survives the cap.
        for artifact in self.context.iter().rev() {
            let tag = match artifact.kind {
                ContextKind::Image | ContextKind::Diagram => "Screenshot",
                ContextKind::Text if artifact.path.trim().is_empty() => "Note",
                _ => "Attachment",
            };
            // The body: a typed note's text is its text_preview; an attachment
            // contributes its title + any caption note + extracted text preview.
            let mut body = String::new();
            if tag == "Note" {
                if let Some(t) = artifact.text_preview.as_deref() {
                    body.push_str(t.trim());
                }
            } else {
                body.push_str(artifact.title.trim());
                if let Some(n) = artifact.note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                    body.push_str(" — ");
                    body.push_str(n);
                }
                if let Some(t) = artifact
                    .text_preview
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                {
                    body.push_str(": ");
                    body.push_str(t);
                }
            }
            let body = body.trim();
            if body.is_empty() {
                continue;
            }
            let line = format!("[{tag}] {body}");
            let line = if line.chars().count() > 600 {
                line.chars().take(600).collect::<String>()
            } else {
                line
            };
            let sep = usize::from(!lines.is_empty());
            let cost = sep + line.chars().count();
            if used + cost > max_chars {
                break;
            }
            lines.push(line);
            used += cost;
        }
        if lines.is_empty() {
            return String::new();
        }
        lines.reverse();
        format!("MEETING NOTES & ATTACHMENTS:\n{}", lines.join("\n"))
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
    fn context_label_is_1_based_and_surfaces_co_speakers() {
        // Mic side is always "You" regardless of any diarized id.
        let me = TranscriptSegment::new(Speaker::User, "hi", true).with_speaker_id(Some(3));
        assert_eq!(me.context_label(), "You");

        // Far side, single speaker: 1-based to match the UI ("Speaker 2", not 1).
        let solo = TranscriptSegment::new(Speaker::System, "hi", true).with_speaker_id(Some(1));
        assert_eq!(solo.context_label(), "Speaker 2");

        // Talk-over: co-speakers appended (comma-separated) so the AI sees the
        // line was mixed. 1-based ids: primary 1→"2", secondaries 2,4→"3","5".
        let mut mixed =
            TranscriptSegment::new(Speaker::System, "hi", true).with_speaker_id(Some(1));
        mixed.secondary_speaker_ids = vec![2, 4];
        assert_eq!(mixed.context_label(), "Speaker 2, 3, 5");

        // No diarization yet → coarse channel label, not "Speaker N".
        let unlabeled = TranscriptSegment::new(Speaker::System, "hi", true);
        assert_eq!(unlabeled.context_label(), unlabeled.speaker.display_label());
    }

    #[test]
    fn test_meeting_is_substantive_thresholds() {
        // Empty shell → not substantive (the fragment/empty-active case).
        let empty = MeetingRecord::new(Some("Meeting 10:00".to_string()));
        assert!(!empty.meeting_is_substantive());

        // A single final segment (1 unit) is below the >= 2 bar → not substantive.
        let mut one_line = MeetingRecord::new(Some("Meeting 10:00".to_string()));
        one_line
            .transcript
            .push(TranscriptSegment::new(Speaker::System, "hello", true));
        assert!(!one_line.meeting_is_substantive());

        // Non-final partials don't count toward the transcript weight.
        let mut only_partials = MeetingRecord::new(Some("Meeting 10:00".to_string()));
        only_partials.transcript.push(TranscriptSegment::new(
            Speaker::System,
            "partial one",
            false,
        ));
        only_partials.transcript.push(TranscriptSegment::new(
            Speaker::System,
            "partial two",
            false,
        ));
        assert!(
            !only_partials.meeting_is_substantive(),
            "partials must not count toward substance"
        );

        // Two final segments → substantive.
        let mut two_finals = MeetingRecord::new(Some("Meeting 10:00".to_string()));
        two_finals
            .transcript
            .push(TranscriptSegment::new(Speaker::System, "a", true));
        two_finals
            .transcript
            .push(TranscriptSegment::new(Speaker::User, "b", true));
        assert!(two_finals.meeting_is_substantive());

        // One final segment + one Q&A turn (1 + 1 = 2 units) → substantive.
        let mut mixed = MeetingRecord::new(Some("Meeting 10:00".to_string()));
        mixed
            .transcript
            .push(TranscriptSegment::new(Speaker::System, "a", true));
        mixed.push_conversation_turn(ConversationTurn::new(
            "q",
            "a",
            Some("overlay ask".to_string()),
            Some("Bluey managed".to_string()),
        ));
        assert!(mixed.meeting_is_substantive());

        // A written summary alone → substantive (a whitespace-only summary is not).
        let mut summarized = MeetingRecord::new(Some("Meeting 10:00".to_string()));
        summarized.summary = Some("Recap text.".to_string());
        assert!(summarized.meeting_is_substantive());
        let mut blank_summary = MeetingRecord::new(Some("Meeting 10:00".to_string()));
        blank_summary.summary = Some("   ".to_string());
        assert!(!blank_summary.meeting_is_substantive());

        // The "no transcript was captured" placeholder summary is NOT real
        // content — an empty meeting that ended with it must stay hidden (the
        // "0 lines · 0 Q&A" clutter bug).
        let mut placeholder = MeetingRecord::new(Some(EMPTY_MEETING_SUMMARY.to_string()));
        placeholder.summary = Some(EMPTY_MEETING_SUMMARY.to_string());
        assert!(
            !placeholder.meeting_is_substantive(),
            "the empty-meeting placeholder summary must not make a meeting substantive"
        );

        // Attached context alone → substantive.
        let mut with_context = MeetingRecord::new(Some("Meeting 10:00".to_string()));
        with_context.context.push(ContextArtifact::new(
            ContextKind::Text,
            "pasted.txt",
            "pasted spec",
            Some("pasted spec text".to_string()),
            None,
        ));
        assert!(with_context.meeting_is_substantive());
    }

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
        // System audio renders with the conversational "They" label — once, at the
        // start of the (single) turn, not repeated per fragment.
        assert!(text.starts_with("They: "));
        assert_eq!(text.matches("They:").count(), 1);
        // The TAIL (newest words) survives the char cap.
        assert!(text.contains("important ending"));
    }

    #[test]
    fn bounded_transcript_coalesces_same_speaker_fragments() {
        // The bug: streaming STT emits many tiny fragments per utterance, each its
        // own segment. Labeling per-segment produced "They: could actually give
        // They: feedback They: but". Coalescing must render ONE label per turn.
        let mut meeting = MeetingRecord::new(Some("Frag".to_string()));
        for frag in ["could actually give ", "feedback ", "but"] {
            meeting.transcript.push(TranscriptSegment::new(
                Speaker::System,
                frag.to_string(),
                true,
            ));
        }
        let text = meeting.last_transcript_text_bounded(3, 220);
        // One label, one line — the whole utterance under a single "They:".
        assert_eq!(text.matches("They:").count(), 1);
        assert_eq!(text, "They: could actually give feedback but");
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
