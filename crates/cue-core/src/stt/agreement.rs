//! Bounded LocalAgreement-style transcript stability tracking.
//!
//! The tracker is deliberately independent from any STT provider. A caller
//! snapshots [`TranscriptAgreementCursor`] before starting an asynchronous
//! decode, then supplies that cursor with the result. Resetting or finalizing
//! advances the cursor, so late results are rejected instead of leaking into a
//! newer utterance.

use serde::{Deserialize, Serialize};

/// Hard defaults keep memory and emitted metadata bounded for pathological
/// provider hypotheses.
pub const DEFAULT_MAX_AGREEMENT_WORDS: usize = 512;
pub const DEFAULT_MAX_AGREEMENT_TEXT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAgreementConfig {
    pub max_words: usize,
    pub max_text_bytes: usize,
    pub timestamp_tolerance_ms: u64,
    pub fast_endpoint_silence_ms: u64,
    pub slow_endpoint_silence_ms: u64,
    pub fast_endpoint_stable_ticks: u32,
}

impl Default for LocalAgreementConfig {
    fn default() -> Self {
        Self {
            max_words: DEFAULT_MAX_AGREEMENT_WORDS,
            max_text_bytes: DEFAULT_MAX_AGREEMENT_TEXT_BYTES,
            timestamp_tolerance_ms: 300,
            fast_endpoint_silence_ms: 300,
            slow_endpoint_silence_ms: 1_000,
            fast_endpoint_stable_ticks: 2,
        }
    }
}

impl LocalAgreementConfig {
    fn bounded(self) -> Self {
        Self {
            max_words: self.max_words.max(1),
            max_text_bytes: self.max_text_bytes.max(1),
            fast_endpoint_stable_ticks: self.fast_endpoint_stable_ticks.max(1),
            ..self
        }
    }
}

/// Identifies the exact utterance generation a decode belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptAgreementCursor {
    pub generation_id: u64,
    pub segment_id: u64,
}

/// An optional word timestamp. Local helpers without word timings use
/// [`AgreementWord::untimed`], which falls back to exact token agreement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementWord {
    pub text: String,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}

impl AgreementWord {
    pub fn untimed(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            start_ms: None,
            end_ms: None,
        }
    }

    pub fn timed(text: impl Into<String>, start_ms: u64, end_ms: u64) -> Self {
        Self {
            text: text.into(),
            start_ms: Some(start_ms),
            end_ms: Some(end_ms),
        }
    }
}

/// Describes what changed in this stability update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptAgreementPhase {
    /// The provider hypothesis is still fully or partly replaceable.
    Tentative,
    /// This update extended the prefix agreed across consecutive hypotheses.
    Committed,
    /// The provider declared the utterance final.
    Final,
}

/// Stability metadata carried beside, never in place of, the legacy
/// `TranscriptEvent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptAgreementUpdate {
    pub generation_id: u64,
    pub segment_id: u64,
    pub revision: u64,
    pub phase: TranscriptAgreementPhase,
    pub committed_text: String,
    pub tentative_text: String,
    pub newly_committed_text: String,
    pub stable_ticks: u32,
    /// True when the provider's final result revised an already-committed
    /// prefix. The final remains authoritative, but consumers can avoid
    /// treating the correction as an ordinary append.
    pub final_corrected_committed_prefix: bool,
    /// Stability metadata was clipped to configured bounds. The unchanged
    /// legacy event can still carry the provider's complete hypothesis.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgreementOutcome {
    Applied(TranscriptAgreementUpdate),
    RejectedStale {
        active: TranscriptAgreementCursor,
        received: TranscriptAgreementCursor,
    },
}

impl AgreementOutcome {
    pub fn into_update(self) -> Option<TranscriptAgreementUpdate> {
        match self {
            Self::Applied(update) => Some(update),
            Self::RejectedStale { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointDecision {
    pub should_finalize: bool,
    pub required_silence_ms: u64,
    pub fast_path: bool,
}

/// Bounded consecutive-hypothesis agreement tracker.
#[derive(Debug, Clone)]
pub struct LocalAgreementTracker {
    config: LocalAgreementConfig,
    cursor: TranscriptAgreementCursor,
    next_segment_id: u64,
    revision: u64,
    previous: Vec<AgreementWord>,
    committed: Vec<AgreementWord>,
    stable_ticks: u32,
}

impl Default for LocalAgreementTracker {
    fn default() -> Self {
        Self::new(LocalAgreementConfig::default())
    }
}

impl LocalAgreementTracker {
    pub fn new(config: LocalAgreementConfig) -> Self {
        Self::with_generation(config, 1)
    }

    pub fn with_generation(config: LocalAgreementConfig, generation_id: u64) -> Self {
        let generation_id = generation_id.max(1);
        Self {
            config: config.bounded(),
            cursor: TranscriptAgreementCursor {
                generation_id,
                segment_id: 1,
            },
            next_segment_id: 2,
            revision: 0,
            previous: Vec::new(),
            committed: Vec::new(),
            stable_ticks: 0,
        }
    }

    pub fn config(&self) -> LocalAgreementConfig {
        self.config
    }

    pub fn cursor(&self) -> TranscriptAgreementCursor {
        self.cursor
    }

    /// Invalidates all outstanding decode cursors and starts a new segment.
    pub fn reset(&mut self) -> TranscriptAgreementCursor {
        self.cursor.generation_id = next_nonzero(self.cursor.generation_id);
        self.cursor.segment_id = self.allocate_segment_id();
        self.clear_segment();
        self.cursor
    }

    pub fn observe_partial_text(
        &mut self,
        cursor: TranscriptAgreementCursor,
        text: &str,
    ) -> AgreementOutcome {
        let (words, truncated) = bounded_words_from_text(text, self.config);
        self.observe_partial_words_inner(cursor, words, truncated)
    }

    pub fn observe_partial_words(
        &mut self,
        cursor: TranscriptAgreementCursor,
        words: &[AgreementWord],
    ) -> AgreementOutcome {
        let (words, truncated) = bounded_words(words, self.config);
        self.observe_partial_words_inner(cursor, words, truncated)
    }

    pub fn observe_final_text(
        &mut self,
        cursor: TranscriptAgreementCursor,
        text: &str,
    ) -> AgreementOutcome {
        let (words, truncated) = bounded_words_from_text(text, self.config);
        self.observe_final_words_inner(cursor, words, truncated)
    }

    pub fn observe_final_words(
        &mut self,
        cursor: TranscriptAgreementCursor,
        words: &[AgreementWord],
    ) -> AgreementOutcome {
        let (words, truncated) = bounded_words(words, self.config);
        self.observe_final_words_inner(cursor, words, truncated)
    }

    pub fn endpoint_decision(&self, silence_ms: u64) -> EndpointDecision {
        let committed_text = join_words(&self.committed);
        let punctuated = committed_text.trim_end().ends_with(['?', '.', '!']);
        let fast_path = punctuated && self.stable_ticks >= self.config.fast_endpoint_stable_ticks;
        let required_silence_ms = if fast_path {
            self.config.fast_endpoint_silence_ms
        } else {
            self.config.slow_endpoint_silence_ms
        };
        EndpointDecision {
            should_finalize: !self.committed.is_empty() && silence_ms >= required_silence_ms,
            required_silence_ms,
            fast_path,
        }
    }

    fn observe_partial_words_inner(
        &mut self,
        cursor: TranscriptAgreementCursor,
        words: Vec<AgreementWord>,
        truncated: bool,
    ) -> AgreementOutcome {
        if cursor != self.cursor {
            return AgreementOutcome::RejectedStale {
                active: self.cursor,
                received: cursor,
            };
        }

        let agreement_end =
            common_agreement_len(&self.previous, &words, self.config.timestamp_tolerance_ms);
        let commit_start = self.committed.len();
        let mut newly_committed = Vec::new();
        if agreement_end > commit_start && committed_prefix_matches(&self.committed, &words) {
            newly_committed.extend_from_slice(&words[commit_start..agreement_end]);
            self.committed.extend(newly_committed.iter().cloned());
        }

        if newly_committed.is_empty() {
            self.stable_ticks = self.stable_ticks.saturating_add(1);
        } else {
            self.stable_ticks = 0;
        }
        self.revision = self.revision.saturating_add(1);
        self.previous = words;

        let tentative_start = self.committed.len().min(self.previous.len());
        AgreementOutcome::Applied(TranscriptAgreementUpdate {
            generation_id: cursor.generation_id,
            segment_id: cursor.segment_id,
            revision: self.revision,
            phase: if newly_committed.is_empty() {
                TranscriptAgreementPhase::Tentative
            } else {
                TranscriptAgreementPhase::Committed
            },
            committed_text: join_words(&self.committed),
            tentative_text: join_words(&self.previous[tentative_start..]),
            newly_committed_text: join_words(&newly_committed),
            stable_ticks: self.stable_ticks,
            final_corrected_committed_prefix: false,
            truncated,
        })
    }

    fn observe_final_words_inner(
        &mut self,
        cursor: TranscriptAgreementCursor,
        words: Vec<AgreementWord>,
        truncated: bool,
    ) -> AgreementOutcome {
        if cursor != self.cursor {
            return AgreementOutcome::RejectedStale {
                active: self.cursor,
                received: cursor,
            };
        }

        let final_corrected_committed_prefix = !committed_prefix_matches(&self.committed, &words);
        let newly_committed = if final_corrected_committed_prefix {
            words.clone()
        } else {
            words[self.committed.len().min(words.len())..].to_vec()
        };
        self.revision = self.revision.saturating_add(1);
        let update = TranscriptAgreementUpdate {
            generation_id: cursor.generation_id,
            segment_id: cursor.segment_id,
            revision: self.revision,
            phase: TranscriptAgreementPhase::Final,
            committed_text: join_words(&words),
            tentative_text: String::new(),
            newly_committed_text: join_words(&newly_committed),
            stable_ticks: self.stable_ticks,
            final_corrected_committed_prefix,
            truncated,
        };

        self.cursor.segment_id = self.allocate_segment_id();
        self.clear_segment();
        AgreementOutcome::Applied(update)
    }

    fn allocate_segment_id(&mut self) -> u64 {
        let allocated = self.next_segment_id.max(1);
        self.next_segment_id = next_nonzero(allocated);
        allocated
    }

    fn clear_segment(&mut self) {
        self.revision = 0;
        self.previous.clear();
        self.committed.clear();
        self.stable_ticks = 0;
    }
}

fn bounded_words_from_text(text: &str, config: LocalAgreementConfig) -> (Vec<AgreementWord>, bool) {
    let mut words = Vec::with_capacity(config.max_words.min(32));
    let mut used_bytes = 0usize;
    let mut truncated = false;
    for token in text.split_whitespace() {
        let separator_bytes = usize::from(!words.is_empty());
        let next_bytes = used_bytes
            .saturating_add(separator_bytes)
            .saturating_add(token.len());
        if words.len() >= config.max_words || next_bytes > config.max_text_bytes {
            truncated = true;
            break;
        }
        used_bytes = next_bytes;
        words.push(AgreementWord::untimed(token));
    }
    (words, truncated)
}

fn bounded_words(
    words: &[AgreementWord],
    config: LocalAgreementConfig,
) -> (Vec<AgreementWord>, bool) {
    let mut bounded = Vec::with_capacity(words.len().min(config.max_words));
    let mut used_bytes = 0usize;
    let mut truncated = false;
    for word in words {
        let separator_bytes = usize::from(!bounded.is_empty());
        let next_bytes = used_bytes
            .saturating_add(separator_bytes)
            .saturating_add(word.text.len());
        if bounded.len() >= config.max_words || next_bytes > config.max_text_bytes {
            truncated = true;
            break;
        }
        used_bytes = next_bytes;
        bounded.push(word.clone());
    }
    (bounded, truncated)
}

fn common_agreement_len(
    previous: &[AgreementWord],
    current: &[AgreementWord],
    timestamp_tolerance_ms: u64,
) -> usize {
    previous
        .iter()
        .zip(current)
        .take_while(|(left, right)| words_agree(left, right, timestamp_tolerance_ms))
        .count()
}

fn words_agree(left: &AgreementWord, right: &AgreementWord, timestamp_tolerance_ms: u64) -> bool {
    if left.text != right.text {
        return false;
    }
    match (left.start_ms, right.start_ms) {
        (Some(left), Some(right)) => left.abs_diff(right) <= timestamp_tolerance_ms,
        _ => true,
    }
}

fn committed_prefix_matches(committed: &[AgreementWord], words: &[AgreementWord]) -> bool {
    committed.len() <= words.len()
        && committed
            .iter()
            .zip(words)
            .all(|(committed, current)| committed.text == current.text)
}

fn join_words(words: &[AgreementWord]) -> String {
    words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn next_nonzero(value: u64) -> u64 {
    let next = value.wrapping_add(1);
    if next == 0 {
        1
    } else {
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(outcome: AgreementOutcome) -> TranscriptAgreementUpdate {
        match outcome {
            AgreementOutcome::Applied(update) => update,
            AgreementOutcome::RejectedStale { active, received } => {
                panic!("unexpected stale cursor: active={active:?} received={received:?}")
            }
        }
    }

    #[test]
    fn commits_only_the_prefix_shared_by_consecutive_hypotheses() {
        let mut tracker = LocalAgreementTracker::default();
        let cursor = tracker.cursor();

        let first = applied(tracker.observe_partial_text(cursor, "hello wor"));
        assert_eq!(first.phase, TranscriptAgreementPhase::Tentative);
        assert_eq!(first.committed_text, "");
        assert_eq!(first.tentative_text, "hello wor");

        let second = applied(tracker.observe_partial_text(cursor, "hello world"));
        assert_eq!(second.phase, TranscriptAgreementPhase::Committed);
        assert_eq!(second.committed_text, "hello");
        assert_eq!(second.newly_committed_text, "hello");
        assert_eq!(second.tentative_text, "world");

        let third = applied(tracker.observe_partial_text(cursor, "hello world again"));
        assert_eq!(third.committed_text, "hello world");
        assert_eq!(third.tentative_text, "again");
    }

    #[test]
    fn timestamp_tolerance_blocks_unstable_timed_words() {
        let mut tracker = LocalAgreementTracker::default();
        let cursor = tracker.cursor();
        let first = [AgreementWord::timed("hello", 100, 250)];
        let close = [AgreementWord::timed("hello", 390, 540)];
        let far = [AgreementWord::timed("hello", 800, 950)];

        applied(tracker.observe_partial_words(cursor, &first));
        let within_tolerance = applied(tracker.observe_partial_words(cursor, &close));
        assert_eq!(within_tolerance.phase, TranscriptAgreementPhase::Committed);

        let final_update = applied(tracker.observe_final_words(cursor, &far));
        assert!(!final_update.final_corrected_committed_prefix);

        let next_cursor = tracker.cursor();
        applied(tracker.observe_partial_words(next_cursor, &first));
        let outside_tolerance = applied(tracker.observe_partial_words(next_cursor, &far));
        assert_eq!(outside_tolerance.phase, TranscriptAgreementPhase::Tentative);
        assert!(outside_tolerance.committed_text.is_empty());
    }

    #[test]
    fn reset_and_finalization_reject_stale_results() {
        let mut tracker = LocalAgreementTracker::default();
        let stale_before_reset = tracker.cursor();
        let after_reset = tracker.reset();
        assert_ne!(stale_before_reset.generation_id, after_reset.generation_id);
        assert!(matches!(
            tracker.observe_partial_text(stale_before_reset, "late decode"),
            AgreementOutcome::RejectedStale { .. }
        ));

        applied(tracker.observe_partial_text(after_reset, "current"));
        let final_update = applied(tracker.observe_final_text(after_reset, "current"));
        assert_eq!(final_update.phase, TranscriptAgreementPhase::Final);
        assert_ne!(tracker.cursor().segment_id, after_reset.segment_id);
        assert!(matches!(
            tracker.observe_partial_text(after_reset, "late segment"),
            AgreementOutcome::RejectedStale { .. }
        ));
    }

    #[test]
    fn final_correction_is_explicit_and_authoritative() {
        let mut tracker = LocalAgreementTracker::default();
        let cursor = tracker.cursor();
        applied(tracker.observe_partial_text(cursor, "item potency"));
        applied(tracker.observe_partial_text(cursor, "item potency"));
        let final_update = applied(tracker.observe_final_text(cursor, "idempotency"));
        assert_eq!(final_update.phase, TranscriptAgreementPhase::Final);
        assert_eq!(final_update.committed_text, "idempotency");
        assert!(final_update.final_corrected_committed_prefix);
    }

    #[test]
    fn endpoint_policy_uses_punctuation_only_after_stable_ticks() {
        let mut tracker = LocalAgreementTracker::default();
        let cursor = tracker.cursor();
        applied(tracker.observe_partial_text(cursor, "Are we ready?"));
        applied(tracker.observe_partial_text(cursor, "Are we ready?"));
        assert!(!tracker.endpoint_decision(500).should_finalize);
        applied(tracker.observe_partial_text(cursor, "Are we ready?"));
        applied(tracker.observe_partial_text(cursor, "Are we ready?"));
        let fast = tracker.endpoint_decision(300);
        assert!(fast.fast_path);
        assert!(fast.should_finalize);

        let next = tracker.reset();
        applied(tracker.observe_partial_text(next, "we should continue"));
        applied(tracker.observe_partial_text(next, "we should continue"));
        applied(tracker.observe_partial_text(next, "we should continue"));
        assert!(!tracker.endpoint_decision(999).should_finalize);
        assert!(tracker.endpoint_decision(1_000).should_finalize);
    }

    #[test]
    fn metadata_is_bounded_without_truncating_the_legacy_event() {
        let config = LocalAgreementConfig {
            max_words: 3,
            max_text_bytes: 12,
            ..LocalAgreementConfig::default()
        };
        let mut tracker = LocalAgreementTracker::new(config);
        let cursor = tracker.cursor();
        let update =
            applied(tracker.observe_partial_text(cursor, "one two three four five six seven"));
        assert!(update.truncated);
        assert_eq!(update.tentative_text, "one two");
        assert!(update.tentative_text.len() <= config.max_text_bytes);
    }
}
