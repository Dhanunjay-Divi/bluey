//! Cross-platform, evidence-based meeting detection.
//!
//! Native helpers report metadata-only observations (audio ownership, process
//! identity, foreground/window context). This module corroborates those signals
//! before surfacing a candidate. In particular, a browser process is never
//! considered a meeting by itself: it must have active audio plus provider/page
//! evidence. The detector does not capture audio or screen content and it never
//! starts recording on its own.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cue_core::overlay::{MeetingBannerAction, MeetingCandidate, MeetingEvidence};
use tokio::sync::watch;

const SECOND_MS: i64 = 1_000;
const MINUTE_MS: i64 = 60 * SECOND_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeetingDetectorConfig {
    /// A candidate must remain corroborated this long before it is emitted.
    pub entry_debounce_ms: i64,
    /// Brief audio/process gaps do not immediately end an active candidate.
    pub exit_grace_ms: i64,
    /// Flapping candidates remain suppressed after they disappear.
    pub reentry_cooldown_ms: i64,
    /// User dismissal is stronger than process-level flap suppression.
    pub dismiss_cooldown_ms: i64,
    /// User-facing snooze duration.
    pub snooze_ms: i64,
    /// Minimum confidence required for a dedicated meeting application.
    pub dedicated_threshold: u8,
    /// Browsers need a slightly stronger, multi-signal decision.
    pub browser_threshold: u8,
}

impl Default for MeetingDetectorConfig {
    fn default() -> Self {
        Self {
            entry_debounce_ms: 1_500,
            exit_grace_ms: 12_000,
            reentry_cooldown_ms: 30_000,
            dismiss_cooldown_ms: 5 * MINUTE_MS,
            snooze_ms: 15 * MINUTE_MS,
            dedicated_threshold: 64,
            browser_threshold: 56,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeetingTransition {
    None,
    Activated(MeetingCandidate),
    Updated(MeetingCandidate),
    Cleared {
        candidate_id: String,
        reason: &'static str,
    },
}

#[derive(Debug, Clone)]
struct CandidateTrack {
    first_seen_ms: i64,
    last_seen_ms: i64,
    eligible_since_ms: Option<i64>,
    last_eligible_ms: Option<i64>,
    candidate: MeetingCandidate,
}

#[derive(Debug)]
pub struct MeetingDetector {
    config: MeetingDetectorConfig,
    tracks: HashMap<String, CandidateTrack>,
    active_key: Option<String>,
    suppressed_until: HashMap<String, i64>,
    ignored_apps: HashSet<String>,
}

impl Default for MeetingDetector {
    fn default() -> Self {
        Self::new(MeetingDetectorConfig::default())
    }
}

impl MeetingDetector {
    pub fn new(config: MeetingDetectorConfig) -> Self {
        Self {
            config,
            tracks: HashMap::new(),
            active_key: None,
            suppressed_until: HashMap::new(),
            ignored_apps: HashSet::new(),
        }
    }

    /// Incorporate one native observation and return a material state change.
    pub fn observe(&mut self, mut evidence: MeetingEvidence) -> MeetingTransition {
        sanitize_evidence(&mut evidence);
        if evidence.app_id.is_empty() || evidence.app_name.is_empty() {
            return MeetingTransition::None;
        }

        let key = candidate_key(&evidence.app_id);
        let observed_at = evidence.observed_at_unix_ms.max(0);
        self.expire_suppression(observed_at);

        if self.ignored_apps.contains(&key)
            || self
                .suppressed_until
                .get(&key)
                .is_some_and(|until| observed_at < *until)
        {
            return MeetingTransition::None;
        }

        let assessment = assess_evidence(&evidence, self.config);
        let candidate = MeetingCandidate {
            candidate_id: key.clone(),
            app_name: evidence.app_name.clone(),
            app_id: evidence.app_id.clone(),
            provider: assessment.provider.clone(),
            confidence: assessment.confidence,
            provenance: assessment.provenance,
            reason: assessment.reason,
            browser: evidence.browser,
            first_seen_unix_ms: observed_at,
            last_seen_unix_ms: observed_at,
        };

        let active_confidence = self
            .active_key
            .as_ref()
            .and_then(|active_key| self.tracks.get(active_key))
            .map(|active| active.candidate.confidence)
            .unwrap_or(0);
        let track = self
            .tracks
            .entry(key.clone())
            .or_insert_with(|| CandidateTrack {
                first_seen_ms: observed_at,
                last_seen_ms: observed_at,
                eligible_since_ms: None,
                last_eligible_ms: None,
                candidate: candidate.clone(),
            });
        let previous_seen_ms = track.last_seen_ms;
        track.last_seen_ms = track.last_seen_ms.max(observed_at);

        if !assessment.eligible {
            // Preserve the last corroborated candidate while its exit grace is
            // active. A weak sample must not downgrade the visible explanation.
            if self.active_key.as_deref() != Some(key.as_str()) {
                track.eligible_since_ms = None;
            }
            return MeetingTransition::None;
        }

        track.last_eligible_ms = Some(observed_at);
        if observed_at.saturating_sub(previous_seen_ms)
            > self.config.entry_debounce_ms.saturating_mul(2)
        {
            track.eligible_since_ms = None;
        }
        let eligible_since = *track.eligible_since_ms.get_or_insert(observed_at);
        let previous_candidate = track.candidate.clone();
        let first_seen = track.first_seen_ms.min(observed_at);
        let next_candidate = MeetingCandidate {
            first_seen_unix_ms: first_seen,
            ..candidate
        };
        let materially_stronger = next_candidate.confidence > previous_candidate.confidence
            || (previous_candidate.provider.is_none() && next_candidate.provider.is_some());
        if self.active_key.as_deref() != Some(key.as_str()) || materially_stronger {
            track.candidate = next_candidate;
        }

        if self.active_key.as_deref() == Some(key.as_str()) {
            if materially_stronger
                && candidate_signature(&track.candidate) != candidate_signature(&previous_candidate)
            {
                return MeetingTransition::Updated(track.candidate.clone());
            }
            return MeetingTransition::None;
        }

        if observed_at.saturating_sub(eligible_since) < self.config.entry_debounce_ms {
            return MeetingTransition::None;
        }

        // If two apps qualify at once, prefer the stronger evidence. A lower
        // confidence helper/process observation cannot replace an active call.
        if self.active_key.is_some() && active_confidence >= track.candidate.confidence {
            return MeetingTransition::None;
        }

        self.active_key = Some(key);
        MeetingTransition::Activated(track.candidate.clone())
    }

    /// Advance absence/cooldown state even when native code has no new sample.
    pub fn tick(&mut self, now_unix_ms: i64) -> MeetingTransition {
        self.expire_suppression(now_unix_ms);
        let Some(active_key) = self.active_key.clone() else {
            return MeetingTransition::None;
        };
        let Some(track) = self.tracks.get(&active_key) else {
            self.active_key = None;
            return MeetingTransition::None;
        };
        let last_eligible = track.last_eligible_ms.unwrap_or(track.last_seen_ms);
        if now_unix_ms.saturating_sub(last_eligible) <= self.config.exit_grace_ms {
            return MeetingTransition::None;
        }

        self.active_key = None;
        self.suppressed_until.insert(
            active_key.clone(),
            now_unix_ms.saturating_add(self.config.reentry_cooldown_ms),
        );
        MeetingTransition::Cleared {
            candidate_id: active_key,
            reason: "evidence_lost",
        }
    }

    /// Apply an action from the non-activating banner.
    pub fn apply_action(
        &mut self,
        candidate_id: &str,
        action: MeetingBannerAction,
        now_unix_ms: i64,
    ) -> MeetingTransition {
        let key = candidate_key(candidate_id);
        match action {
            MeetingBannerAction::Start => {
                // The daemon independently authorizes/starts capture. Suppress
                // repeat banners while that transition settles.
                self.suppressed_until.insert(
                    key.clone(),
                    now_unix_ms.saturating_add(self.config.reentry_cooldown_ms),
                );
            }
            MeetingBannerAction::Dismiss | MeetingBannerAction::Settings => {
                self.suppressed_until.insert(
                    key.clone(),
                    now_unix_ms.saturating_add(self.config.dismiss_cooldown_ms),
                );
            }
            MeetingBannerAction::Expired => {
                // Timeout is not a user rejection. Prevent an immediate banner
                // loop, but allow a still-active call to surface again after
                // the normal short process-flap cooldown.
                self.suppressed_until.insert(
                    key.clone(),
                    now_unix_ms.saturating_add(self.config.reentry_cooldown_ms),
                );
            }
            MeetingBannerAction::Snooze => {
                self.suppressed_until.insert(
                    key.clone(),
                    now_unix_ms.saturating_add(self.config.snooze_ms),
                );
            }
            MeetingBannerAction::Ignore => {
                self.ignored_apps.insert(key.clone());
                self.suppressed_until.remove(&key);
            }
        }

        if self.active_key.as_deref() == Some(key.as_str()) {
            self.active_key = None;
            return MeetingTransition::Cleared {
                candidate_id: key,
                reason: action_reason(action),
            };
        }
        MeetingTransition::None
    }

    pub fn current(&self) -> Option<&MeetingCandidate> {
        self.active_key
            .as_ref()
            .and_then(|key| self.tracks.get(key))
            .map(|track| &track.candidate)
    }

    pub fn is_ignored(&self, app_id: &str) -> bool {
        self.ignored_apps.contains(&candidate_key(app_id))
    }

    pub fn clear_ignored(&mut self, app_id: &str) {
        self.ignored_apps.remove(&candidate_key(app_id));
    }

    pub fn replace_ignored_apps(
        &mut self,
        app_ids: impl IntoIterator<Item = String>,
    ) -> MeetingTransition {
        self.ignored_apps = app_ids
            .into_iter()
            .map(|app_id| candidate_key(&app_id))
            .filter(|app_id| !app_id.is_empty())
            .collect();
        let Some(active_key) = self.active_key.clone() else {
            return MeetingTransition::None;
        };
        if !self.ignored_apps.contains(&active_key) {
            return MeetingTransition::None;
        }
        self.active_key = None;
        MeetingTransition::Cleared {
            candidate_id: active_key,
            reason: "ignored_settings_reloaded",
        }
    }

    fn expire_suppression(&mut self, now_unix_ms: i64) {
        self.suppressed_until
            .retain(|_, until| *until > now_unix_ms);
    }
}

#[derive(Debug, Clone)]
struct Assessment {
    eligible: bool,
    confidence: u8,
    provider: Option<String>,
    provenance: Vec<String>,
    reason: String,
}

fn assess_evidence(evidence: &MeetingEvidence, config: MeetingDetectorConfig) -> Assessment {
    let provider = evidence
        .provider
        .as_deref()
        .and_then(normalize_provider)
        .map(str::to_string)
        .or_else(|| {
            evidence
                .page_url
                .as_deref()
                .and_then(provider_from_context)
                .map(str::to_string)
        })
        .or_else(|| {
            evidence
                .window_title
                .as_deref()
                .and_then(provider_from_context)
                .map(str::to_string)
        });

    let title_has_meeting_context = evidence
        .window_title
        .as_deref()
        .is_some_and(|value| provider_from_active_call_title(value).is_some());
    let url_has_meeting_context = evidence
        .page_url
        .as_deref()
        .is_some_and(|value| provider_from_context(value).is_some());

    let mut score: u16 = 0;
    let mut provenance = Vec::with_capacity(8);
    if evidence.dedicated_meeting_app {
        score += 48;
        provenance.push("dedicated_meeting_app".to_string());
    }
    if evidence.browser {
        provenance.push("browser_host".to_string());
    }
    if evidence.audio_input_active {
        score += 32;
        provenance.push("audio_input".to_string());
    }
    if evidence.audio_output_active {
        score += 16;
        provenance.push("audio_output".to_string());
    }
    if evidence.app_foreground {
        score += 6;
        provenance.push("foreground_app".to_string());
    }
    if provider.is_some() {
        score += 10;
        provenance.push("provider_metadata".to_string());
    }
    if title_has_meeting_context {
        score += 18;
        provenance.push("meeting_window_title".to_string());
    }
    if url_has_meeting_context {
        score += 28;
        provenance.push("meeting_page_url".to_string());
    }
    provenance.push(format!("source:{}", evidence.source));

    let audio_active = evidence.audio_input_active || evidence.audio_output_active;
    let browser_context =
        provider.is_some() && (title_has_meeting_context || url_has_meeting_context);
    let provider_call_context =
        provider.is_some() && (title_has_meeting_context || url_has_meeting_context);
    let threshold = if evidence.browser {
        config.browser_threshold
    } else {
        config.dedicated_threshold
    };
    let eligible = if evidence.browser {
        // Chrome/Edge/Firefox can use audio for voice search, AI assistants,
        // music, or recording. Provider/page evidence is mandatory.
        let audio_is_corroborated = evidence.audio_input_active
            || (evidence.audio_output_active
                && (evidence.app_foreground || url_has_meeting_context));
        audio_active && audio_is_corroborated && browser_context && score >= u16::from(threshold)
    } else {
        // Dedicated apps can own an output stream while sitting idle, playing
        // a notification, or showing a lobby. Microphone ownership is strong
        // active-call evidence. Output-only evidence must additionally name a
        // recognized provider in the current call window/page.
        let audio_is_corroborated =
            evidence.audio_input_active || (evidence.audio_output_active && provider_call_context);
        evidence.dedicated_meeting_app
            && audio_active
            && audio_is_corroborated
            && score >= u16::from(threshold)
    };

    let confidence = score.min(100) as u8;
    let provider_label = provider
        .as_deref()
        .map(provider_display_name)
        .unwrap_or("a call");
    let reason = if evidence.browser {
        if url_has_meeting_context {
            format!(
                "{} has an active {} page with call audio.",
                evidence.app_name, provider_label
            )
        } else if title_has_meeting_context {
            format!(
                "{} has an active {} window with call audio.",
                evidence.app_name, provider_label
            )
        } else {
            format!(
                "{} is using audio, but Bluey has not confirmed a meeting page.",
                evidence.app_name
            )
        }
    } else if evidence.audio_input_active {
        format!("{} is actively using the microphone.", evidence.app_name)
    } else {
        format!("{} has an active call audio session.", evidence.app_name)
    };

    Assessment {
        eligible,
        confidence,
        provider,
        provenance,
        reason,
    }
}

fn sanitize_evidence(evidence: &mut MeetingEvidence) {
    evidence.app_name = compact_field(&evidence.app_name, 160);
    evidence.app_id = compact_field(&evidence.app_id, 256);
    evidence.source = compact_field(&evidence.source, 96);
    evidence.provider = evidence
        .provider
        .take()
        .map(|value| compact_field(&value, 64))
        .filter(|value| !value.is_empty());
    evidence.window_title = evidence
        .window_title
        .take()
        .map(|value| compact_field(&value, 512))
        .filter(|value| !value.is_empty());
    evidence.page_url = evidence
        .page_url
        .take()
        .map(|value| compact_field(&value, 1024))
        .filter(|value| !value.is_empty());
}

fn compact_field(value: &str, max_chars: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn candidate_signature(
    candidate: &MeetingCandidate,
) -> (String, Option<String>, u8, Vec<String>, String) {
    (
        candidate.app_name.clone(),
        candidate.provider.clone(),
        candidate.confidence,
        candidate.provenance.clone(),
        candidate.reason.clone(),
    )
}

fn candidate_key(app_id: &str) -> String {
    app_id.trim().to_ascii_lowercase()
}

fn action_reason(action: MeetingBannerAction) -> &'static str {
    match action {
        MeetingBannerAction::Start => "recording_requested",
        MeetingBannerAction::Dismiss => "dismissed",
        MeetingBannerAction::Expired => "expired",
        MeetingBannerAction::Snooze => "snoozed",
        MeetingBannerAction::Ignore => "ignored",
        MeetingBannerAction::Settings => "settings_opened",
    }
}

fn normalize_provider(value: &str) -> Option<&'static str> {
    provider_from_context(value).or_else(|| match value.trim().to_ascii_lowercase().as_str() {
        "google_meet" | "google meet" | "hangoutsmeet" => Some("google_meet"),
        "zoom" | "zoom_meeting" => Some("zoom"),
        "teams" | "microsoft_teams" | "teams_for_business" => Some("microsoft_teams"),
        "webex" | "webex_meeting" => Some("webex"),
        "slack" | "slack_huddle" => Some("slack_huddle"),
        "whereby" => Some("whereby"),
        "gotomeeting" | "go_to_meeting" => Some("gotomeeting"),
        _ => None,
    })
}

fn provider_from_context(value: &str) -> Option<&'static str> {
    let value = value.to_ascii_lowercase();
    if value.contains("meet.google.com") || value.contains("google meet") {
        Some("google_meet")
    } else if value.contains("zoom.us") || value.contains("zoom meeting") {
        Some("zoom")
    } else if value.contains("teams.microsoft")
        || value.contains("microsoft teams")
        || value.contains("teams meeting")
    {
        Some("microsoft_teams")
    } else if value.contains("webex") {
        Some("webex")
    } else if value.contains("slack huddle") || value.contains("huddle | slack") {
        Some("slack_huddle")
    } else if value.contains("whereby.com") || value.contains("whereby meeting") {
        Some("whereby")
    } else if value.contains("gotomeeting") || value.contains("go to meeting") {
        Some("gotomeeting")
    } else {
        None
    }
}

fn provider_from_active_call_title(value: &str) -> Option<&'static str> {
    let value = value.trim().to_ascii_lowercase();
    if value.contains("meet.google.com")
        || value.contains(" - google meet")
        || value.contains("google meet - ")
        || value.contains("google meet call")
    {
        Some("google_meet")
    } else if value.contains("zoom.us")
        || value.contains("zoom meeting")
        || value.contains("zoom webinar")
    {
        Some("zoom")
    } else if value.contains("teams meeting")
        || value.contains("meeting | microsoft teams")
        || value.contains("meeting - microsoft teams")
        || value.contains("microsoft teams meeting")
    {
        Some("microsoft_teams")
    } else if value.contains("webex meeting")
        || value.contains("webex webinar")
        || value.contains("meeting | webex")
        || value.contains("meeting - webex")
    {
        Some("webex")
    } else if value.contains("slack huddle") || value.contains("huddle | slack") {
        Some("slack_huddle")
    } else if value.contains("whereby meeting")
        || value.contains("meeting | whereby")
        || value.contains("meeting - whereby")
    {
        Some("whereby")
    } else if value.contains("gotomeeting session")
        || value.contains("go to meeting session")
        || value.contains("meeting | gotomeeting")
    {
        Some("gotomeeting")
    } else {
        None
    }
}

fn provider_display_name(provider: &str) -> &str {
    match provider {
        "google_meet" => "Google Meet",
        "zoom" => "Zoom",
        "microsoft_teams" => "Microsoft Teams",
        "webex" => "Webex",
        "slack_huddle" => "Slack Huddle",
        "whereby" => "Whereby",
        "gotomeeting" => "GoTo Meeting",
        _ => "meeting",
    }
}

struct MeetingWatchInner {
    sender: watch::Sender<Option<MeetingCandidate>>,
    detector: Mutex<MeetingDetector>,
}

#[derive(Clone)]
pub struct MeetingWatch {
    inner: Arc<MeetingWatchInner>,
}

impl Default for MeetingWatch {
    fn default() -> Self {
        Self::with_config(MeetingDetectorConfig::default())
    }
}

impl MeetingWatch {
    pub fn with_config(config: MeetingDetectorConfig) -> Self {
        let (sender, _receiver) = watch::channel(None);
        Self {
            inner: Arc::new(MeetingWatchInner {
                sender,
                detector: Mutex::new(MeetingDetector::new(config)),
            }),
        }
    }

    pub fn current(&self) -> Option<MeetingCandidate> {
        self.inner.sender.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<MeetingCandidate>> {
        self.inner.sender.subscribe()
    }

    pub fn observe(&self, evidence: MeetingEvidence) -> MeetingTransition {
        let transition = self
            .inner
            .detector
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observe(evidence);
        self.publish_transition(&transition);
        transition
    }

    pub fn tick(&self, now_unix_ms: i64) -> MeetingTransition {
        let transition = self
            .inner
            .detector
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .tick(now_unix_ms);
        self.publish_transition(&transition);
        transition
    }

    pub fn apply_action(
        &self,
        candidate_id: &str,
        action: MeetingBannerAction,
        now_unix_ms: i64,
    ) -> MeetingTransition {
        let transition = self
            .inner
            .detector
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .apply_action(candidate_id, action, now_unix_ms);
        self.publish_transition(&transition);
        transition
    }

    pub fn is_ignored(&self, app_id: &str) -> bool {
        self.inner
            .detector
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_ignored(app_id)
    }

    pub fn clear_ignored(&self, app_id: &str) {
        self.inner
            .detector
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear_ignored(app_id);
    }

    pub fn replace_ignored_apps(&self, app_ids: Vec<String>) -> MeetingTransition {
        let transition = self
            .inner
            .detector
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .replace_ignored_apps(app_ids);
        self.publish_transition(&transition);
        transition
    }

    fn publish_transition(&self, transition: &MeetingTransition) {
        match transition {
            MeetingTransition::Activated(candidate) | MeetingTransition::Updated(candidate) => {
                self.inner.sender.send_replace(Some(candidate.clone()));
            }
            MeetingTransition::Cleared { .. } => {
                self.inner.sender.send_replace(None);
            }
            MeetingTransition::None => {}
        }
    }
}

/// Keeps absence/cooldown transitions moving. Native overlay evidence is fed
/// through `MeetingWatch::observe` by the daemon overlay event handler.
pub fn spawn_loop(watcher: MeetingWatch) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            watcher.tick(chrono::Utc::now().timestamp_millis());
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(
        at: i64,
        app_name: &str,
        app_id: &str,
        browser: bool,
        dedicated: bool,
    ) -> MeetingEvidence {
        MeetingEvidence {
            source: "test".to_string(),
            app_name: app_name.to_string(),
            app_id: app_id.to_string(),
            process_id: Some(42),
            provider: None,
            window_title: None,
            page_url: None,
            audio_input_active: false,
            audio_output_active: false,
            app_foreground: true,
            browser,
            dedicated_meeting_app: dedicated,
            observed_at_unix_ms: at,
        }
    }

    #[test]
    fn chrome_audio_without_meeting_context_never_activates() {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, "Google Chrome", "com.google.Chrome", true, false);
        sample.audio_input_active = true;
        sample.audio_output_active = true;

        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 2_600;
        assert_eq!(detector.observe(sample), MeetingTransition::None);
        assert!(detector.current().is_none());
    }

    #[test]
    fn corroborated_browser_call_activates_after_debounce() {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, "Google Chrome", "com.google.Chrome", true, false);
        sample.audio_input_active = true;
        sample.audio_output_active = true;
        sample.window_title = Some("Daily sync - Google Meet".to_string());

        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 2_600;
        let MeetingTransition::Activated(candidate) = detector.observe(sample) else {
            panic!("corroborated browser candidate should activate");
        };
        assert_eq!(candidate.provider.as_deref(), Some("google_meet"));
        assert!(candidate.confidence >= 56);
        assert!(candidate
            .provenance
            .contains(&"meeting_window_title".to_string()));
    }

    #[test]
    fn dedicated_app_still_requires_active_audio() {
        let mut detector = MeetingDetector::default();
        let sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        assert_eq!(detector.observe(sample), MeetingTransition::None);
        assert!(detector.current().is_none());
    }

    #[test]
    fn dedicated_app_output_only_without_provider_call_context_never_activates() {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_output_active = true;

        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 10_000;
        assert_eq!(detector.observe(sample), MeetingTransition::None);
        assert!(detector.current().is_none());
    }

    #[test]
    fn dedicated_app_output_only_rejects_provider_metadata_or_generic_window_alone() {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_output_active = true;
        sample.provider = Some("zoom".to_string());
        sample.window_title = Some("Zoom Workplace".to_string());

        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 2_600;
        assert_eq!(detector.observe(sample), MeetingTransition::None);
        assert!(detector.current().is_none());
    }

    #[test]
    fn dedicated_app_output_only_accepts_provider_specific_call_window() {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_output_active = true;
        sample.window_title = Some("Weekly planning - Zoom Meeting".to_string());

        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 2_600;
        let MeetingTransition::Activated(candidate) = detector.observe(sample) else {
            panic!("provider-specific active-call window should corroborate output audio");
        };
        assert_eq!(candidate.provider.as_deref(), Some("zoom"));
        assert!(candidate
            .provenance
            .contains(&"meeting_window_title".to_string()));
    }

    fn assert_generic_output_only_title_is_rejected(
        app_name: &str,
        app_id: &str,
        provider: &str,
        title: &str,
    ) {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, app_name, app_id, false, true);
        sample.audio_output_active = true;
        sample.provider = Some(provider.to_string());
        sample.window_title = Some(title.to_string());

        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 2_600;
        assert_eq!(detector.observe(sample), MeetingTransition::None);
        assert!(detector.current().is_none());
    }

    #[test]
    fn teams_output_only_generic_app_title_never_activates() {
        assert_generic_output_only_title_is_rejected(
            "Microsoft Teams",
            "com.microsoft.teams2",
            "microsoft_teams",
            "Microsoft Teams",
        );
    }

    #[test]
    fn webex_output_only_generic_app_title_never_activates() {
        assert_generic_output_only_title_is_rejected(
            "Webex",
            "com.cisco.webexmeetingsapp",
            "webex",
            "Cisco Webex",
        );
    }

    #[test]
    fn slack_output_only_generic_app_title_never_activates() {
        assert_generic_output_only_title_is_rejected(
            "Slack",
            "com.tinyspeck.slackmacgap",
            "slack_huddle",
            "Slack",
        );
    }

    #[test]
    fn dedicated_app_with_microphone_activates() {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_input_active = true;
        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 2_600;
        assert!(matches!(
            detector.observe(sample),
            MeetingTransition::Activated(_)
        ));
    }

    #[test]
    fn exit_grace_prevents_flapping_then_clears() {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_input_active = true;
        detector.observe(sample.clone());
        sample.observed_at_unix_ms = 2_600;
        detector.observe(sample);

        assert_eq!(detector.tick(14_500), MeetingTransition::None);
        assert!(matches!(
            detector.tick(14_601),
            MeetingTransition::Cleared {
                reason: "evidence_lost",
                ..
            }
        ));
    }

    #[test]
    fn dismiss_and_snooze_suppress_reentry() {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_input_active = true;
        detector.observe(sample.clone());
        sample.observed_at_unix_ms = 2_600;
        detector.observe(sample.clone());
        assert!(matches!(
            detector.apply_action("us.zoom.xos", MeetingBannerAction::Snooze, 3_000),
            MeetingTransition::Cleared {
                reason: "snoozed",
                ..
            }
        ));

        sample.observed_at_unix_ms = 100_000;
        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 15 * MINUTE_MS + 3_001;
        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms += 1_600;
        assert!(matches!(
            detector.observe(sample),
            MeetingTransition::Activated(_)
        ));
    }

    #[test]
    fn expiration_uses_short_reentry_cooldown_not_manual_dismiss_suppression() {
        let config = MeetingDetectorConfig {
            entry_debounce_ms: 100,
            reentry_cooldown_ms: 2_000,
            dismiss_cooldown_ms: 5 * MINUTE_MS,
            ..MeetingDetectorConfig::default()
        };
        let mut detector = MeetingDetector::new(config);
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_input_active = true;
        detector.observe(sample.clone());
        sample.observed_at_unix_ms = 1_101;
        assert!(matches!(
            detector.observe(sample.clone()),
            MeetingTransition::Activated(_)
        ));

        assert!(matches!(
            detector.apply_action("us.zoom.xos", MeetingBannerAction::Expired, 1_200),
            MeetingTransition::Cleared {
                reason: "expired",
                ..
            }
        ));
        sample.observed_at_unix_ms = 3_199;
        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 3_201;
        assert_eq!(detector.observe(sample.clone()), MeetingTransition::None);
        sample.observed_at_unix_ms = 3_302;
        assert!(matches!(
            detector.observe(sample.clone()),
            MeetingTransition::Activated(_)
        ));

        detector.apply_action("us.zoom.xos", MeetingBannerAction::Dismiss, 3_400);
        sample.observed_at_unix_ms = 6_000;
        assert_eq!(detector.observe(sample), MeetingTransition::None);
        assert!(detector.current().is_none());
    }

    #[test]
    fn ignore_is_app_scoped_and_reversible() {
        let mut detector = MeetingDetector::default();
        detector.apply_action("com.google.Chrome", MeetingBannerAction::Ignore, 1_000);
        assert!(detector.is_ignored("COM.GOOGLE.CHROME"));
        detector.clear_ignored("com.google.Chrome");
        assert!(!detector.is_ignored("com.google.Chrome"));
    }

    #[test]
    fn persisted_ignore_settings_replace_runtime_state_and_clear_active_banner() {
        let mut detector = MeetingDetector::default();
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_input_active = true;
        detector.observe(sample.clone());
        sample.observed_at_unix_ms = 2_600;
        detector.observe(sample);

        assert!(matches!(
            detector.replace_ignored_apps(vec!["US.ZOOM.XOS".to_string()]),
            MeetingTransition::Cleared {
                reason: "ignored_settings_reloaded",
                ..
            }
        ));
        assert!(detector.is_ignored("us.zoom.xos"));

        assert_eq!(
            detector.replace_ignored_apps(Vec::new()),
            MeetingTransition::None
        );
        assert!(!detector.is_ignored("us.zoom.xos"));
    }

    #[tokio::test]
    async fn watch_publishes_and_clears_candidates() {
        let watch = MeetingWatch::default();
        let mut receiver = watch.subscribe();
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_input_active = true;
        watch.observe(sample.clone());
        sample.observed_at_unix_ms = 2_600;
        watch.observe(sample);
        receiver.changed().await.unwrap();
        assert_eq!(
            receiver
                .borrow()
                .as_ref()
                .map(|candidate| candidate.app_id.as_str()),
            Some("us.zoom.xos")
        );

        watch.tick(14_601);
        receiver.changed().await.unwrap();
        assert!(receiver.borrow().is_none());
    }

    #[test]
    fn watch_current_updates_even_without_a_subscriber() {
        let watch = MeetingWatch::default();
        let mut sample = evidence(1_000, "Zoom", "us.zoom.xos", false, true);
        sample.audio_input_active = true;
        watch.observe(sample.clone());
        sample.observed_at_unix_ms = 2_600;
        watch.observe(sample);

        assert_eq!(
            watch
                .current()
                .as_ref()
                .map(|candidate| candidate.app_id.as_str()),
            Some("us.zoom.xos")
        );
    }
}
