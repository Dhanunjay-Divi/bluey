use serde::{Deserialize, Serialize};

use crate::agent_ui::{AgentConnectorInfo, AgentSessionSummary, AgentSummary, SetupStatus};
use crate::{overlay_ipc::ListeningState, AudioSourceKind, CueCard, CueCardArtifact};

/// serde default for opt-in-by-default booleans (e.g. capture both audio sources
/// unless the overlay explicitly disables one).
fn default_true() -> bool {
    true
}

/// serde skip helper: omit a `bool` field from the wire when it is `false`.
/// Used so `SetMeetingState.read_only` serializes identically to before the
/// field existed for the active-rehydrate emitter (which sends `false`).
fn is_false(value: &bool) -> bool {
    !*value
}

/// A fixed set of macOS System Settings privacy panes the overlay may ask the
/// daemon to open. An enum (not a free URL) keeps the daemon's `open` call to a
/// known allowlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsPane {
    /// Privacy & Security → Screen Recording (system-audio capture).
    ScreenRecording,
    /// Privacy & Security → Microphone.
    Microphone,
}

impl SettingsPane {
    /// The macOS deep-link URL for this pane.
    pub fn url(self) -> &'static str {
        match self {
            SettingsPane::ScreenRecording => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            SettingsPane::Microphone => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl Default for OverlayPosition {
    fn default() -> Self {
        Self::Center
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverlayContextItem {
    pub id: uuid::Uuid,
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub path: Option<String>,
    /// A small inline `data:` thumbnail for image/diagram artifacts, so the
    /// overlay can render a ChatGPT-style preview chip WITHOUT the Tauri asset
    /// protocol (the overlay has no filesystem/asset permission; the command bus
    /// carrying a data URI is the whole transport). `None` for non-image kinds,
    /// and then omitted from the wire so text/doc chips carry no null noise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// The id of the last finalized transcript segment present when this artifact
    /// was attached — the overlay renders it inline after the LINE containing
    /// that segment, so it stays where it was added. A segment id (not an index)
    /// because the UI groups segments into fewer lines. `None` → render at tail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_segment_id: Option<String>,
    /// A text excerpt of the artifact for the click-to-preview lightbox (the
    /// first chunk of a code/text/document file). `None` for image kinds (the
    /// `thumbnail` is their preview) and for artifacts with no extractable text.
    /// Bounded by the daemon before it reaches the wire so a huge file can't
    /// bloat the command bus — the preview shows an excerpt, not the whole file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverlaySessionItem {
    pub id: uuid::Uuid,
    pub title: String,
    pub subtitle: String,
    #[serde(default)]
    pub is_active: bool,
    /// The project/workspace this session belongs to, when known (used for the
    /// redesigned panel's project filter chip). `None` when unassociated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Best-effort last-updated marker (epoch seconds or RFC3339 string, per
    /// source). Drives the date-group bucketing (Today / Yesterday / …). Empty
    /// string when unknown (older payloads).
    #[serde(default)]
    pub updated_at: String,
    /// Turn/exchange count, when cheaply countable (shown as "N turns").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_count: Option<usize>,
    /// True when the user has pinned this session to the top of the list.
    #[serde(default)]
    pub pinned: bool,
}

/// One finalized spoken line in the active meeting's transcript, as sent to the
/// overlay so it can rehydrate after a collapse-remount or a full process
/// restart ([`OverlayCommand::SetMeetingState`]). This is a MINIMAL, stable wire
/// surface — it deliberately does NOT carry the diarization / audio-clock
/// internals of [`crate::meeting::TranscriptSegment`], which the UI must not
/// depend on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingTranscriptLine {
    /// The source segment's id (a `Uuid` rendered as a string). Stable across
    /// the snapshot and the live push path, so the UI can dedup a snapshot line
    /// against a live one.
    pub id: String,
    /// Coarse capture channel: `"mic"` (local user) or `"system"` (remote side).
    /// Mapped from the segment's [`crate::meeting::Speaker`], the reliable
    /// mic-vs-system signal — never the display label.
    pub source: String,
    /// A human speaker label when useful (e.g. a diarized individual), else
    /// `None`. `v1` leaves this `None`; the caption uses `source`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// The numeric diarized speaker id behind `speaker`, when known. Carried in
    /// the SNAPSHOT (not just the live TranscriptSpeaker push) so a rehydrated /
    /// continued / reopened meeting's lines are still editable AND so a rename
    /// echo (which relabels every line of a speaker_id) actually finds them.
    /// Without this, snapshot lines had no id → the editor was disabled and
    /// renames didn't reflect. `None` when diarization hasn't resolved one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_id: Option<i64>,
    /// The raw (untrimmed) spoken text of this segment.
    pub text: String,
    /// Always `true` — only finalized segments are persisted and emitted. Named
    /// `is_final` on the Rust struct; the wire key is `"final"` to match the
    /// UI's `TranscriptLine.final`.
    #[serde(rename = "final")]
    pub is_final: bool,
}

/// One prior Q&A exchange in the active meeting's conversation, as sent to the
/// overlay for rehydration ([`OverlayCommand::SetMeetingState`]). A MINIMAL wire
/// surface over [`crate::meeting::ConversationTurn`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingConversationTurn {
    /// The source turn's id (a `Uuid` rendered as a string).
    pub id: String,
    /// The question the user asked.
    pub question: String,
    /// The answer Bluey produced.
    pub answer: String,
    /// The grounding hint the turn was answered from, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// One decision from the active meeting's ledger, pushed to the overlay so the
/// Open Floor Plan can render the "Key Decisions" block at the top of the
/// document. A MINIMAL wire surface over [`crate::meeting::Decision`] — just the
/// human text and a stable id; the source-segment / timestamp internals stay
/// daemon-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingDecision {
    /// The decision's id (a `Uuid` rendered as a string) — stable for keying.
    pub id: String,
    /// The verified decision text (verbatim from the AI ledger).
    pub text: String,
}

/// One row in the MEETINGS lens ("my past meetings"): a cheap summary of a
/// persisted [`crate::meeting::MeetingRecord`], sent in answer to an
/// [`OverlayEvent::MeetingsRequested`] via [`OverlayCommand::SetMeetings`]. The
/// full transcript + Q&A is fetched lazily only when a row is opened (via
/// [`OverlayEvent::MeetingOpenRequested`]), so this list stays light.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSummary {
    /// [`crate::meeting::MeetingRecord::id`] (a `Uuid`) rendered as a string.
    pub id: String,
    pub title: String,
    /// Epoch-ms string, exactly as stored on the record.
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    /// `transcript.len()` — ALL segments, a cheap count for the list; the open
    /// VIEW re-filters to finalized lines.
    pub transcript_count: usize,
    /// `conversation.len()` — the prior Q&A turn count.
    pub turn_count: usize,
    /// First non-empty transcript segment text, trimmed to <= 120 chars; `None`
    /// when the meeting has no transcript.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// `true` when this row is the currently-active (live) meeting.
    #[serde(default)]
    pub is_active: bool,
    /// The agent session id this meeting was chained to, when any. Drives the
    /// "resume agent thread" affordance in the viewer; the frontend derives
    /// `hasAgentSession = agentSessionId != null`. `None` for meetings that were
    /// never asked through an attached agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    /// The agent KIND ("claude_code", "cursor", …) that owns
    /// [`agent_session_id`]. The viewer resumes the thread on THIS agent, not
    /// whatever is currently attached — resuming a Claude id onto an attached
    /// Cursor would mis-target. `None` for legacy links recorded before the kind
    /// was stored (the UI falls back to the attached agent for those).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_kind: Option<String>,
}

/// The lifecycle state of a tool-call step in the live answer status feed.
/// Mirrors the agent's real ACP tool-call status — never fabricated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerStatusState {
    Pending,
    Running,
    Done,
    Failed,
}

/// One row in the live status feed shown while the agent works an answer: either
/// the agent's reasoning, or a tool/connector call with its run state. These are
/// real ACP events the driven agent emits (thoughts + tool calls) — surfaced
/// live like Claude's status feed, never invented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnswerStatusStep {
    /// The agent's reasoning/thinking text (ACP `AgentThoughtChunk`).
    Reasoning { text: String },
    /// A tool/connector invocation (ACP `ToolCall`/`ToolCallUpdate`). `id` is the
    /// ACP tool-call id so repeated updates collapse onto one row.
    Tool {
        id: String,
        title: String,
        state: AnswerStatusState,
    },
}

/// One candidate name the speaker-rename input offers (a calendar attendee).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpeakerCandidate {
    /// Display name — the calendar `displayName` when present, else a
    /// prettified email local-part (e.g. `jane.doe@x.com` → "Jane Doe").
    pub name: String,
    /// The attendee's email (a secondary label / disambiguator; may be empty).
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayCommand {
    Ping,
    Show,
    Hide,
    Toggle,
    Clear,
    Boot {
        title: String,
        lines: Vec<String>,
    },
    SetOpacity {
        opacity: f32,
    },
    SetPosition {
        position: OverlayPosition,
    },
    SetBalance {
        label: String,
    },
    SetContextItems {
        items: Vec<OverlayContextItem>,
        /// Conversation turn count for the active meeting, shown as the overlay's
        /// "In context: N turns" indicator. `0` when no active meeting / no turns.
        #[serde(default)]
        turns: usize,
    },
    SetSessions {
        sessions: Vec<OverlaySessionItem>,
    },
    /// Paginated/searched session page — the redesigned panel's at-scale path.
    /// Unlike [`OverlayCommand::SetSessions`] (a one-shot, capped initial paint),
    /// this answers an [`OverlayEvent::SessionsRequested`] and carries the slice
    /// the UI asked for plus the totals it needs to render "show N more" and a
    /// result count. `sessions` are already sorted by the daemon (pinned first,
    /// then most-recent) and each item carries its `pinned`/`project`/`updated_at`
    /// so the UI can group by date and filter by project without another round
    /// trip. `query`/`offset` are echoed so a late/out-of-order reply can be
    /// matched to (or discarded against) the UI's current request.
    SetSessionsPage {
        sessions: Vec<OverlaySessionItem>,
        /// Total sessions matching the current `query` (before paging) — the UI
        /// uses this for "show N more" and the "Search 318 sessions…" count.
        total: usize,
        /// The offset this page starts at (echo of the request).
        offset: usize,
        /// True when `offset + sessions.len() < total` (more pages remain).
        has_more: bool,
        /// Echo of the search string this page answers (empty = unfiltered), so
        /// the UI can ignore a reply that no longer matches what's typed.
        #[serde(default)]
        query: String,
    },
    ListeningStateChanged {
        state: ListeningState,
        #[serde(default)]
        system: bool,
        #[serde(default)]
        microphone: bool,
        /// The source whose OS permission is denied. This can accompany
        /// `listening` when the other source remains live, and is absent for
        /// compatibility with older aggregate-only senders.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        permission_denied_source: Option<AudioSourceKind>,
    },
    PushCard {
        card: CueCard,
    },
    /// Show the branded MEETING-PREP BANNER — a compact overlay state (our own
    /// card, NOT a native notification) shown ~lead time before a calendar
    /// meeting starts. The UI renders it top-right (title + time range + a
    /// "Warm up the meeting" button); tapping the button EXPANDS the same window
    /// into the full meeting UI and emits [`OverlayEvent::MeetingPrepResponded`]
    /// `{ approved: true }`; dismissing emits `{ approved: false }`. `event_id`
    /// is echoed back so the daemon warms the right meeting.
    ShowMeetingBanner {
        event_id: String,
        title: String,
        /// Occurrence start / end (epoch seconds) for the time-range label. `0`
        /// end = unknown (show start only).
        start_epoch_secs: u64,
        end_epoch_secs: u64,
        /// Roster size + how many accepted, for a "4 invited (3 accepted)" line.
        participant_count: u32,
        accepted_count: u32,
        /// True when the meeting has a video join URL (shows an "online" hint).
        online: bool,
    },
    UpdateCard {
        id: uuid::Uuid,
        body: String,
        #[serde(default)]
        done: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost_label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artifact: Option<CueCardArtifact>,
        /// True when `body` is an ERROR (provider/agent failure, policy block),
        /// not an answer — so the overlay renders a distinct, retryable error
        /// state instead of styling a failure message as the answer.
        /// `#[serde(default)]` keeps old clients back-compatible.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
        /// True when the answer proposes a concrete change/action the agent could
        /// take next (a code fix, an edit, a follow-up) — the overlay shows the
        /// "Fix this" affordance ONLY then, not on purely informational answers.
        /// Derived from the agent's trailing `[[fix]]`/`[[info]]` tag with a
        /// daemon-side heuristic fallback. `#[serde(default)]` = back-compatible.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fixable: bool,
    },
    /// The live status feed for an in-flight answer: the agent's real reasoning
    /// and tool/connector calls, keyed to the answer card `id`. Emitted as the
    /// agent works (before/while the answer streams) so the UI shows what the
    /// agent is actually doing (e.g. "Reading App.tsx", "Querying Jira"), like
    /// Claude's status feed. `steps` is the full ordered list each time (the UI
    /// replaces, not appends); `done` marks the work complete so the UI can
    /// collapse the feed once the answer is in.
    SetAnswerStatus {
        id: uuid::Uuid,
        steps: Vec<AnswerStatusStep>,
        #[serde(default)]
        done: bool,
    },
    /// Push the discovered-agent list to the UI (agent-bridge Slice 5a).
    SetAgents {
        agents: Vec<AgentSummary>,
    },
    /// Push the first-run setup status so ONBOARDING can show, and gate on, the
    /// real state of every prerequisite — instead of the user discovering
    /// mid-meeting that a model never downloaded or the agent is signed out.
    ///
    /// Pushed on request ([`OverlayEvent::SetupStatusRequested`]) and again
    /// whenever a step's state changes (model download progress, an install or
    /// login finishing), so the onboarding screen is live rather than a snapshot.
    SetSetupStatus {
        status: SetupStatus,
    },
    /// Push one agent's prior sessions to the UI (gated on consent upstream).
    SetAgentSessions {
        kind: String,
        sessions: Vec<AgentSessionSummary>,
    },
    /// Push one agent's inherited MCP connectors (shape + readiness only).
    SetAgentConnectors {
        kind: String,
        connectors: Vec<AgentConnectorInfo>,
    },
    /// Push one agent's available models to the UI's model picker. `models[0]`
    /// is always the `"auto"` sentinel (no override); the picker is hidden when
    /// the list has <= 1 entry. `kind` is the snake_case agent label the UI
    /// requested, echoed back so a reply for the wrong agent is dropped.
    SetAgentModels {
        kind: String,
        models: Vec<String>,
    },
    /// Upgrade the speaker label of an already-pushed transcript line.
    /// Diarization resolves "who said this" seconds AFTER the line was first
    /// pushed (the live tier labels on its own tick cadence), so the original
    /// push carries no speaker and this patches it in place. `id` is the segment
    /// id carried on the original push_card; `speaker` is the display label
    /// (e.g. "Speaker 2").
    TranscriptSpeaker {
        id: String,
        speaker: String,
        /// The numeric diarized speaker index this label belongs to, so the UI
        /// can offer inline rename (→ `RenameSpeakerRequested { speaker_id }`).
        /// `None` for a rename echo where the UI already knows the target.
        #[serde(default)]
        speaker_id: Option<i64>,
    },
    /// The active meeting's calendar attendees, so the speaker-rename input can
    /// offer them as tap-to-pick candidates (an invitee is far likelier to be a
    /// speaker than a random name). Empty when no meeting / no roster.
    SetMeetingCandidates {
        candidates: Vec<SpeakerCandidate>,
    },
    /// Snapshot of the active meeting for rehydration (Fix B). `transcript` is
    /// the finalized spoken lines; `conversation` is the prior Q&A turns. Both
    /// empty when no meeting is active (so the UI's request promise still
    /// resolves). NOT a live stream — the UI seeds once on mount, then the live
    /// push_card / update_card path carries deltas on top.
    SetMeetingState {
        transcript: Vec<MeetingTranscriptLine>,
        conversation: Vec<MeetingConversationTurn>,
        /// The active meeting's verified Key Decisions ledger, for the Open Floor
        /// Plan's top-of-document block. Empty (and omitted from the wire) when
        /// there are none, so this stays byte-compatible with pre-decisions
        /// snapshots and the glass UI (which ignores the field).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        decisions: Vec<MeetingDecision>,
        /// The meeting's attached context (screenshots/files), carried in the
        /// SNAPSHOT so reopening/continuing a meeting reloads them inline at
        /// their anchor — not just via the live `SetContextItems` push (which a
        /// fresh mount never receives for an already-attached artifact). Empty
        /// (and omitted) when there are none, so this stays byte-compatible with
        /// pre-context snapshots and the glass UI.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        context: Vec<OverlayContextItem>,
        /// When this snapshot is a PAST-meeting VIEW (answering
        /// [`OverlayEvent::MeetingOpenRequested`]), the opened meeting's id, so
        /// the UI can match this reply to its open request and disambiguate it
        /// from a live-rehydrate reply. `None` for the active-rehydrate emitter
        /// (answering [`OverlayEvent::MeetingStateRequested`]) — with
        /// `skip_serializing_if` that emitter's wire form is BYTE-IDENTICAL to
        /// before this field existed (back-compat).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meeting_id: Option<String>,
        /// `true` when the snapshot is read-only (viewing a past meeting while a
        /// live one must not be clobbered). `false` (the default) for the active
        /// rehydrate; skipped on the wire when `false` so that emitter's form is
        /// byte-identical to before this field existed.
        #[serde(default, skip_serializing_if = "is_false")]
        read_only: bool,
    },
    /// The MEETINGS lens list ("my past meetings"), answering an
    /// [`OverlayEvent::MeetingsRequested`]. Newest-first; empty-shell meetings
    /// are filtered out by the daemon (same rule as History).
    SetMeetings {
        meetings: Vec<MeetingSummary>,
    },
    /// Push a review-gated Fix proposal for the user to approve or reject
    /// (Fix-button slice F3). The overlay renders the three sections plus the
    /// optional diff and shows Approve/Reject. `proposal_id` is the id the
    /// overlay must echo back in [`OverlayEvent::FixApprovalResponded`] — the
    /// daemon only applies a fix whose id matches a still-pending proposal, so a
    /// stale or unknown id can never trigger an apply. `apply_supported` is
    /// `false` for agents that cannot be driven to apply (e.g. no CLI); the UI
    /// disables Approve in that case.
    PushFixProposal {
        proposal_id: uuid::Uuid,
        diagnosis: String,
        reasoning: String,
        fix: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
        apply_supported: bool,
    },
    /// Push a BYOT (bring-your-own-token) billing-disclosure modal the user
    /// MUST acknowledge before a cloud agent is marked attached. Emitted by
    /// the daemon the first time the user attaches an agent whose registry
    /// row's `billing_model` is BYOT/`api_credits` and whose `vendor_short`
    /// is NOT yet in `accepted_byot_vendors` in settings.
    ///
    /// The UI renders `vendor` (display name), `billing_model` (e.g.
    /// `"api_credits"`), and the row's full `consent_warning` (`disclosure`),
    /// plus a Console URL the user can open to manage / revoke their key.
    /// Acknowledgement is the `OverlayEvent::BillingDisclosureResponded`
    /// event carrying the same `vendor_short`; the daemon refuses to attach
    /// the agent until that event arrives, so the modal is unbypassable.
    ///
    /// Data-driven by design: every field comes off the cloud-registry row
    /// (`crate::cloud::registry::CloudAgentEntry`). Adding a new BYOT vendor
    /// = adding a row, not changing the disclosure code path.
    PushBillingDisclosure {
        /// Lowercase vendor short id (matches `vendor_short` in the cloud
        /// registry row — e.g. `"anthropic"`, `"codex_cloud"`). The UI
        /// echoes this back verbatim in the response event so the daemon
        /// can match the consent to the right vendor.
        vendor_short: String,
        /// Human-facing display name (the registry row's `display_name`).
        vendor_display_name: String,
        /// Billing model label off [`crate::cloud::registry::BillingModel`]
        /// (snake_case wire form — e.g. `"api_credits"`, `"subscription"`,
        /// `"byot"`).
        billing_model: String,
        /// Verbatim disclosure copy from the registry row's
        /// `consent_warning`. The UI MUST render this in full — it's the
        /// legal disclosure (BYOT billing, ZDR ineligibility, …).
        disclosure: String,
        /// Pending agent attach to resume once the user accepts. The daemon
        /// keeps the user's original `kind` + `session_id` here so accepting
        /// the disclosure picks up the in-flight attach without a second
        /// user gesture.
        pending_kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pending_session_id: Option<String>,
    },
    /// Offer to install a missing agent CLI. The attached (or requested) agent
    /// has an [`crate`-external `provision::InstallPlan`] recipe but its binary
    /// isn't on PATH, so live answers can't run. The overlay renders an
    /// Install / Cancel card; the exact command is shown for transparency.
    /// Acknowledgement is [`OverlayEvent::AgentInstallResponded`]. Bluey NEVER
    /// signs the user in — install only; auth stays a manual step.
    PushAgentInstall {
        /// Agent kind (snake_case AgentKind wire form) the user echoes back.
        kind: String,
        /// Human display name for the agent (e.g. "GitHub Copilot").
        display_name: String,
        /// The exact command that will run, verbatim (e.g.
        /// `npm install -g @github/copilot`). Shown in full for transparency.
        command: String,
        /// The prerequisite the install needs (e.g. "npm"), if any — the UI can
        /// warn when it's absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prerequisite: Option<String>,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayEvent {
    Ready {
        platform: String,
        capture_excluded: bool,
    },
    Pong,
    Shown,
    Hidden,
    AskRequested {
        question: String,
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        mode: Option<String>,
    },
    AttachRequested,
    AttachFilesRequested {
        paths: Vec<String>,
    },
    RemoveContextRequested {
        id: uuid::Uuid,
    },
    /// UI added a free-text NOTE in the meeting (the Open Floor note composer).
    /// Stored as a Text context artifact anchored at the current transcript
    /// position, inlined into the agent prompt, and persisted to searchable
    /// memory so it is recallable across meetings.
    NoteAdded {
        text: String,
    },
    /// UI toggled a live meeting-intelligence setting (the agent-bar controls).
    /// `key` is one of "summary" / "decisions" / "auto_answer". Takes effect on
    /// the next summary/ledger boundary — no restart. The daemon persists it so
    /// the gate reads it fresh on each fire.
    SettingToggled {
        key: String,
        enabled: bool,
    },
    /// UI renamed a speaker in the active meeting (the "Reassign Speaker" flow).
    /// Persists to the diarization store so the name shows on the transcript AND
    /// enrolls the speaker's voiceprint for cross-meeting recognition. `speaker_id`
    /// is the per-meeting diarized speaker index; `name` is the user's chosen
    /// display name (empty clears it back to a fallback label).
    RenameSpeakerRequested {
        speaker_id: i64,
        name: String,
    },
    /// UI reassigned a RANGE of transcript segments to a speaker — the
    /// "select a span → assign to speaker" correction. `segment_ids` are the
    /// transcript segment ids to relabel; `speaker_id` is the target diarized
    /// speaker (an existing one, or a fresh index for a NEW speaker); `name` is
    /// an optional display name to set for that speaker in one step. During a
    /// live meeting the daemon also re-enrolls the speaker's voiceprint from the
    /// reassigned span's retained audio so future auto-detection improves.
    ReassignSpanRequested {
        segment_ids: Vec<String>,
        speaker_id: i64,
        #[serde(default)]
        name: Option<String>,
    },
    /// UI split ONE transcript segment at a character offset into two, assigning
    /// each part to a (possibly different) speaker — the "break here" correction.
    /// `segment_id` is the segment to split; `char_offset` is where in its text;
    /// `first_speaker_id`/`second_speaker_id` are the speakers for the two halves.
    SplitSegmentRequested {
        segment_id: String,
        char_offset: usize,
        first_speaker_id: i64,
        second_speaker_id: i64,
    },
    /// UI reassigned a precise CHARACTER RANGE of a transcript LINE to a speaker —
    /// the robust "select any text → assign" correction. `member_ids` are the
    /// line's raw segments in order; the daemon concatenates their text, maps
    /// `[char_start, char_end)` onto it, reassigns fully-covered segments whole,
    /// and splits the two boundary segments at the exact char so only the selected
    /// portion moves. `name` optionally sets the speaker's display name in one
    /// step. Replaces the fragile grouped-line-offset SplitSegmentRequested path.
    ReassignRangeRequested {
        member_ids: Vec<String>,
        char_start: usize,
        char_end: usize,
        speaker_id: i64,
        #[serde(default)]
        name: Option<String>,
    },
    /// UI asked for the current discovered-agent list (agent-bridge Slice 5a).
    AgentListRequested,
    /// UI attached an agent; `session_id` is an optional session to resume and
    /// `model` is an optional per-run vendor model id to apply via the registry
    /// row's `model_flag` (a no-op for agents with no model flag).
    AgentAttachRequested {
        kind: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        model: Option<String>,
    },
    /// UI detached the currently attached agent.
    AgentDetachRequested,
    /// UI asked for one agent's prior sessions (gated on consent in the daemon).
    /// `offset`/`limit`/`search` support the redesigned at-scale agent-session
    /// list; all default (0/0/"") to the prior "first page, unfiltered" behavior
    /// so existing callers and payloads are unaffected.
    AgentSessionsRequested {
        kind: String,
        #[serde(default)]
        offset: usize,
        #[serde(default)]
        limit: usize,
        #[serde(default)]
        search: String,
    },
    /// UI asked for one agent's inherited MCP connectors.
    AgentConnectorsRequested {
        kind: String,
    },
    /// UI asked for one agent's available models (for the model picker). Not
    /// consent-gated — a model list is public, unlike session history. `kind` is
    /// the snake_case agent label; the daemon replies with
    /// [`OverlayCommand::SetAgentModels`].
    AgentModelsRequested {
        kind: String,
    },
    /// UI asked for the active meeting's transcript + Q&A so it can rehydrate
    /// after a collapse-remount or a full overlay restart. Read-only; the daemon
    /// replies with [`OverlayCommand::SetMeetingState`]. Empty vecs when there is
    /// no active meeting so the UI's request promise still resolves.
    MeetingStateRequested,
    /// UI (the MEETINGS lens) asked for the list of past meetings. Read-only;
    /// the daemon replies with [`OverlayCommand::SetMeetings`]. `offset`/`limit`
    /// are reserved for paging (v1 ignores them and returns all) so the wire is
    /// forward-compatible — mirrors [`OverlayEvent::AgentSessionsRequested`].
    MeetingsRequested {
        #[serde(default)]
        offset: usize,
        #[serde(default)]
        limit: usize,
    },
    /// UI asked to OPEN (view) one past meeting by id. Read-only: the daemon
    /// replies with a [`OverlayCommand::SetMeetingState`] snapshot carrying
    /// `meeting_id = Some(id)` and NEVER mutates the active meeting. See the
    /// daemon handler for the live-vs-past read-only guard.
    MeetingOpenRequested {
        id: uuid::Uuid,
    },
    /// UI asked to CONTINUE (activate) a past meeting so the Ask screen resumes
    /// in it. Unlike MeetingOpenRequested (pure read-only VIEW), this MUTATES the
    /// active meeting — but only when SAFE. The daemon BLOCKS when audio is live
    /// and the target differs from the active meeting (a live recording is never
    /// lost). See the daemon handler for the safety-critical control flow.
    MeetingContinueRequested {
        id: uuid::Uuid,
    },
    /// UI clicked **New meeting** — archive the active meeting (if any) and start
    /// a fresh empty one, exactly like `bluey meeting end` + `bluey meeting
    /// start` (the same path `bluey on` uses). The daemon replies with a fresh
    /// [`OverlayCommand::SetMeetingState`] so the transcript/Q&A/decisions all
    /// clear. Safe to call with no active meeting (just starts a new one).
    MeetingNewRequested,
    /// UI asked to re-authenticate one hosted-OAuth connector. For now this
    /// only logs and re-emits guidance; the real OAuth flow is future work.
    ConnectorReauthRequested {
        kind: String,
        name: String,
    },
    /// UI clicked **Fix** on an agent answer (Fix-button slice F3). `question`
    /// is the problem to fix (the answer/diagnosis text); `card_id` optionally
    /// references the source card the Fix was launched from. The daemon drives
    /// the attached agent in propose-only mode and replies with a
    /// [`OverlayCommand::PushFixProposal`]; nothing is applied at this step.
    FixRequested {
        #[serde(default)]
        card_id: Option<uuid::Uuid>,
        question: String,
    },
    /// UI approved or rejected a pending Fix proposal. Carries the
    /// `proposal_id` from the [`OverlayCommand::PushFixProposal`] it is
    /// answering, so the daemon can id-match it against the still-pending
    /// proposal (a stale, replayed, or unknown id is rejected and never
    /// applied). Only `approved = true` against a live id drives an apply.
    FixApprovalResponded {
        proposal_id: uuid::Uuid,
        approved: bool,
    },
    /// The user answered a calendar meeting-prep offer (pushed as a
    /// [`OverlayCommand::PushCard`] `CardKind::Question` when a meeting is about
    /// to start). `approved = true` → the daemon warms the backend and builds the
    /// pre-context (agenda + roster) for that meeting; `false` dismisses it and
    /// the meeting is marked handled so it won't re-offer. `event_id` plus
    /// `start_epoch_secs` identify the exact calendar occurrence echoed from the
    /// offer, so a moved/recurring event cannot consume another pending occurrence.
    /// Approval-gated by design: we never auto-start pre-context.
    MeetingPrepResponded {
        event_id: String,
        start_epoch_secs: u64,
        approved: bool,
    },
    /// UI responded to a BYOT billing disclosure modal pushed by
    /// [`OverlayCommand::PushBillingDisclosure`]. `accepted = true` means the
    /// user agreed; the daemon adds `vendor_short` to
    /// `accepted_byot_vendors` in settings and resumes the pending attach
    /// (using the `pending_kind` / `pending_session_id` echoed back here).
    /// `accepted = false` means the user declined; the daemon discards the
    /// pending attach and pushes no overlay change.
    BillingDisclosureResponded {
        /// The same `vendor_short` the push carried. The daemon uses this to
        /// (a) confirm the response matches a pending disclosure, and
        /// (b) record acknowledgement in settings.
        vendor_short: String,
        accepted: bool,
        /// Echoed from the original push so the daemon can resume the same
        /// in-flight attach.
        pending_kind: String,
        #[serde(default)]
        pending_session_id: Option<String>,
    },
    /// The user answered a [`OverlayCommand::PushAgentInstall`] offer. `approved
    /// = true` runs the vetted install recipe (via `provision_with_recovery`) for
    /// `kind`; the daemon reports the outcome as a card. `false` dismisses it.
    /// The install NEVER signs the user in — that stays a manual step.
    AgentInstallResponded {
        /// The `kind` echoed from the push, so the daemon rebuilds the exact plan.
        kind: String,
        approved: bool,
    },
    /// The user asked Bluey to install a coding agent from the UI (onboarding's
    /// "no agent found" state). The daemon picks the best installable agent —
    /// or the named `kind` when given — and replies with the SAME
    /// [`OverlayCommand::PushAgentInstall`] offer the drive path uses, so the
    /// consent card and vetted-recipe execution stay one code path.
    ///
    /// Exists because onboarding previously showed an EMPTY agent list with no
    /// way forward: Bluey cannot answer without an agent, so "install one" has
    /// to be reachable in-product, not only from a terminal.
    AgentInstallRequested {
        /// `None` → daemon chooses the best installable candidate.
        #[serde(default)]
        kind: Option<String>,
    },
    /// The user asked to sign in to an agent whose CLI is installed but signed
    /// out (`capability == "needs_reauth"`). The daemon LAUNCHES that agent's
    /// own login flow (e.g. `cursor-agent login`) so the user completes it in
    /// their browser/device-code flow. Bluey never handles the credentials.
    AgentLoginRequested {
        kind: String,
    },
    /// Onboarding asks for the current [`SetupStatus`]. The daemon replies with
    /// [`OverlayCommand::SetSetupStatus`] and keeps pushing it as steps change.
    SetupStatusRequested,
    InstructionsRequested,
    InstructionsUpdated {
        text: String,
    },
    SessionOpenRequested {
        id: uuid::Uuid,
    },
    SessionRenameRequested {
        id: uuid::Uuid,
        title: String,
    },
    SessionDeleteRequested {
        id: uuid::Uuid,
    },
    SessionContinueRequested,
    SessionNewRequested,
    /// UI requested a page of sessions for the at-scale list (redesign). The
    /// daemon answers with [`OverlayCommand::SetSessionsPage`]: filter by
    /// `search` (title/project, case-insensitive; empty = all), sort pinned-first
    /// then most-recent, and return the `offset..offset+limit` window plus the
    /// total. This replaces the implicit cap-at-8 of [`OverlayCommand::SetSessions`].
    SessionsRequested {
        #[serde(default)]
        offset: usize,
        /// Page size. The daemon clamps to a sane max; `0` means "daemon default".
        #[serde(default)]
        limit: usize,
        /// Case-insensitive filter over title + project. Empty = unfiltered.
        #[serde(default)]
        search: String,
    },
    /// UI pinned a session to the top of the list. The daemon persists the pin
    /// and re-sends the affected page so the move is reflected.
    SessionPinRequested {
        id: uuid::Uuid,
    },
    /// UI unpinned a previously pinned session.
    SessionUnpinRequested {
        id: uuid::Uuid,
    },
    ActivePageCaptureRequested,
    AnalyzeScreenRequested,
    RecapRequested,
    ContextListRequested,
    CaptureStartRequested,
    CaptureStopRequested,
    /// Open a specific macOS System Settings privacy pane so the user can grant
    /// the permission audio capture needs. `pane` is a fixed enum (not a free
    /// URL) so the daemon maps it to a known `x-apple.systempreferences:` target
    /// — no arbitrary-URL/command-injection surface.
    OpenSettingsRequested {
        pane: SettingsPane,
    },
    /// Start system-audio capture via the interactive macOS content-sharing
    /// picker: the daemon spawns the helper in `--pick` mode, which presents the
    /// system picker so the user chooses which app to capture, then streams that
    /// app's audio into the live transcript pipeline.
    PickSystemAudioRequested,
    /// Start audio capture. Optional per-source flags let the overlay pick which
    /// sources to capture (the Audio tab's mic / system toggles). Both default to
    /// `true` (#[serde(default)] yields `false`, so we use explicit option-style
    /// defaults via `default_true`) preserving the prior dual-capture behavior
    /// for callers that send no flags.
    RecordingStartRequested {
        #[serde(default = "default_true")]
        enable_microphone: bool,
        #[serde(default = "default_true")]
        enable_system: bool,
    },
    RecordingStopRequested,
    CloseRequested,
    CardRendered {
        id: uuid::Uuid,
    },
    /// Toggle the **session-history consent** flag from the overlay UI (the
    /// meeting overlay's privacy switch). First-class so the daemon persists it
    /// the same way the IPC `SetAgentSessionHistory` path does — rather than
    /// riding the generic [`Lifecycle`](OverlayEvent::Lifecycle) event with no
    /// handler. `enabled=false` immediately stops the daemon reading prior
    /// sessions.
    SessionHistoryConsentRequested {
        enabled: bool,
    },
    /// Cancel the in-flight answer the user just asked for (the overlay's
    /// stop/cancel affordance). The UI also drops its own answer-chunk listener
    /// locally; this tells the daemon to reset its overlay UI state so a fresh
    /// ask starts clean.
    AskCancelRequested,
    Error {
        message: String,
    },
    Lifecycle {
        stage: String,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        detail: Option<String>,
    },
    Exited,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_balance_serializes_as_overlay_command() {
        let json = serde_json::to_string(&OverlayCommand::SetBalance {
            label: "$12.34".to_string(),
        })
        .expect("serialize overlay balance command");

        assert_eq!(json, r#"{"type":"set_balance","label":"$12.34"}"#);
    }

    #[test]
    fn set_context_items_serializes_as_overlay_command() {
        let id = uuid::Uuid::nil();
        let json = serde_json::to_string(&OverlayCommand::SetContextItems {
            items: vec![OverlayContextItem {
                id,
                title: "GenAI Engineer JD.pdf".to_string(),
                kind: "document".to_string(),
                path: Some("/tmp/GenAI Engineer JD.pdf".to_string()),
                thumbnail: None,
                anchor_segment_id: None,
                text_preview: None,
            }],
            turns: 3,
        })
        .expect("serialize overlay context command");

        assert_eq!(
            json,
            r#"{"type":"set_context_items","items":[{"id":"00000000-0000-0000-0000-000000000000","title":"GenAI Engineer JD.pdf","kind":"document","path":"/tmp/GenAI Engineer JD.pdf"}],"turns":3}"#
        );
    }

    #[test]
    fn set_sessions_serializes_as_overlay_command() {
        let id = uuid::Uuid::nil();
        let json = serde_json::to_string(&OverlayCommand::SetSessions {
            sessions: vec![OverlaySessionItem {
                id,
                title: "System design prep".to_string(),
                subtitle: "3 transcripts · 2 files".to_string(),
                is_active: true,
                project: None,
                updated_at: String::new(),
                turn_count: None,
                pinned: false,
            }],
        })
        .expect("serialize overlay sessions command");

        // `project`/`turn_count` are `skip_serializing_if = None`, so an item
        // with neither only adds the always-present `updated_at` + `pinned`.
        assert_eq!(
            json,
            r#"{"type":"set_sessions","sessions":[{"id":"00000000-0000-0000-0000-000000000000","title":"System design prep","subtitle":"3 transcripts · 2 files","is_active":true,"updated_at":"","pinned":false}]}"#
        );
    }

    #[test]
    fn legacy_session_item_decodes_with_defaulted_new_fields() {
        // An old daemon (pre-redesign) sends only the original four fields. The
        // extended struct must still decode, defaulting the new ones, so a
        // version skew never drops the session list.
        let legacy = r#"{"id":"00000000-0000-0000-0000-000000000000","title":"t","subtitle":"s","is_active":false}"#;
        let item: OverlaySessionItem = serde_json::from_str(legacy).expect("decode legacy item");
        assert_eq!(item.project, None);
        assert_eq!(item.updated_at, "");
        assert_eq!(item.turn_count, None);
        assert!(!item.pinned);
    }

    #[test]
    fn set_sessions_page_round_trips_with_paging_fields() {
        let cmd = OverlayCommand::SetSessionsPage {
            sessions: vec![OverlaySessionItem {
                id: uuid::Uuid::nil(),
                title: "Overlay redesign".to_string(),
                subtitle: "9 turns".to_string(),
                is_active: true,
                project: Some("Bluey".to_string()),
                updated_at: "1718000000".to_string(),
                turn_count: Some(9),
                pinned: true,
            }],
            total: 318,
            offset: 0,
            has_more: true,
            query: "redesign".to_string(),
        };
        let json = serde_json::to_string(&cmd).expect("serialize page");
        assert!(json.contains(r#""type":"set_sessions_page""#));
        assert!(json.contains(r#""total":318"#));
        assert!(json.contains(r#""has_more":true"#));
        let decoded: OverlayCommand = serde_json::from_str(&json).expect("decode page");
        assert_eq!(
            serde_json::to_string(&decoded).expect("re-serialize"),
            json,
            "SetSessionsPage should round-trip"
        );
    }

    #[test]
    fn session_paging_and_pin_events_round_trip() {
        let cases = [
            OverlayEvent::SessionsRequested {
                offset: 20,
                limit: 20,
                search: "auth".to_string(),
            },
            OverlayEvent::SessionPinRequested {
                id: uuid::Uuid::nil(),
            },
            OverlayEvent::SessionUnpinRequested {
                id: uuid::Uuid::nil(),
            },
        ];
        for event in cases {
            let json = serde_json::to_string(&event).expect("serialize event");
            let decoded: OverlayEvent = serde_json::from_str(&json).expect("decode event");
            assert_eq!(
                serde_json::to_string(&decoded).expect("re-serialize"),
                json,
                "session paging/pin event should round-trip"
            );
        }
    }

    #[test]
    fn agent_sessions_requested_defaults_paging_when_absent() {
        // The prior wire form carried only `kind`; it must still decode with the
        // new paging fields defaulted (offset 0, limit 0, empty search).
        let legacy = r#"{"type":"agent_sessions_requested","kind":"claude_code"}"#;
        let decoded: OverlayEvent = serde_json::from_str(legacy).expect("decode legacy");
        match decoded {
            OverlayEvent::AgentSessionsRequested {
                kind,
                offset,
                limit,
                search,
            } => {
                assert_eq!(kind, "claude_code");
                assert_eq!(offset, 0);
                assert_eq!(limit, 0);
                assert_eq!(search, "");
            }
            other => panic!("expected agent_sessions_requested, got {other:?}"),
        }
    }

    #[test]
    fn listening_state_serializes_as_overlay_command() {
        let json = serde_json::to_string(&OverlayCommand::ListeningStateChanged {
            state: ListeningState::Listening,
            system: true,
            microphone: false,
            permission_denied_source: None,
        })
        .expect("serialize overlay listening state command");

        assert_eq!(
            json,
            r#"{"type":"listening_state_changed","state":"listening","system":true,"microphone":false}"#
        );
    }

    #[test]
    fn permission_denied_state_serializes_source() {
        let json = serde_json::to_value(OverlayCommand::ListeningStateChanged {
            state: ListeningState::PermissionDenied,
            system: false,
            microphone: false,
            permission_denied_source: Some(AudioSourceKind::Microphone),
        })
        .expect("serialize permission source");

        assert_eq!(json["permission_denied_source"], "microphone");
    }

    #[test]
    fn listening_state_can_report_a_denied_secondary_source() {
        let json = serde_json::to_value(OverlayCommand::ListeningStateChanged {
            state: ListeningState::Listening,
            system: true,
            microphone: false,
            permission_denied_source: Some(AudioSourceKind::Microphone),
        })
        .expect("serialize secondary-source permission denial");

        assert_eq!(json["state"], "listening");
        assert_eq!(json["system"], true);
        assert_eq!(json["microphone"], false);
        assert_eq!(json["permission_denied_source"], "microphone");
    }

    #[test]
    fn meeting_prep_response_serializes_occurrence_identity() {
        let json = serde_json::to_string(&OverlayEvent::MeetingPrepResponded {
            event_id: "calendar-event".to_string(),
            start_epoch_secs: 1_784_000_000,
            approved: true,
        })
        .expect("serialize meeting prep response");

        assert_eq!(
            json,
            r#"{"type":"meeting_prep_responded","event_id":"calendar-event","start_epoch_secs":1784000000,"approved":true}"#
        );
        let decoded: OverlayEvent =
            serde_json::from_str(&json).expect("decode meeting prep response");
        assert!(matches!(
            decoded,
            OverlayEvent::MeetingPrepResponded {
                event_id,
                start_epoch_secs: 1_784_000_000,
                approved: true,
            } if event_id == "calendar-event"
        ));
    }

    #[test]
    fn set_agents_serializes_with_type_tag() {
        let json = serde_json::to_string(&OverlayCommand::SetAgents {
            agents: vec![crate::agent_ui::AgentSummary {
                kind: "claude_code".to_string(),
                display_name: "Claude Code".to_string(),
                capability: "drive".to_string(),
                connector_count: 2,
                ready_connector_count: 1,
                session_count: Some(4),
                attached: true,
            }],
        })
        .expect("serialize set_agents command");

        assert!(json.starts_with(r#"{"type":"set_agents","agents":[{"#));
        assert!(json.contains(r#""kind":"claude_code""#));
    }

    #[test]
    fn set_agent_sessions_roundtrips() {
        let command = OverlayCommand::SetAgentSessions {
            kind: "cursor".to_string(),
            sessions: vec![crate::agent_ui::AgentSessionSummary {
                id: "s1".to_string(),
                title: Some("Refactor".to_string()),
                updated_at: "1717000000".to_string(),
                project: None,
            }],
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(json.contains(r#""type":"set_agent_sessions""#));
        assert!(json.contains(r#""kind":"cursor""#));
    }

    #[test]
    fn set_agent_connectors_roundtrips() {
        let command = OverlayCommand::SetAgentConnectors {
            kind: "codex".to_string(),
            connectors: vec![crate::agent_ui::AgentConnectorInfo {
                name: "fs".to_string(),
                auth_tier: "none".to_string(),
                ready: true,
            }],
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(json.contains(r#""type":"set_agent_connectors""#));
        assert!(json.contains(r#""name":"fs""#));
    }

    #[test]
    fn set_agent_models_roundtrips() {
        let command = OverlayCommand::SetAgentModels {
            kind: "cursor".to_string(),
            models: vec!["auto".to_string(), "composer-2.5".to_string()],
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(json.contains(r#""type":"set_agent_models""#));
        assert!(json.contains(r#""kind":"cursor""#));
        assert!(json.contains(r#""models":["auto","composer-2.5"]"#));
        let decoded: OverlayCommand = serde_json::from_str(&json).expect("decode");
        assert_eq!(serde_json::to_string(&decoded).expect("re-serialize"), json);
    }

    #[test]
    fn agent_models_requested_decodes_kind() {
        let json = r#"{"type":"agent_models_requested","kind":"antigravity"}"#;
        let decoded: OverlayEvent = serde_json::from_str(json).expect("decode");
        match decoded {
            OverlayEvent::AgentModelsRequested { kind } => assert_eq!(kind, "antigravity"),
            other => panic!("expected agent_models_requested, got {other:?}"),
        }
    }

    #[test]
    fn agent_attach_event_deserializes_with_optional_session() {
        // Old payload without session_id or model: both default to None.
        let json = r#"{"type":"agent_attach_requested","kind":"claude_code"}"#;
        let event: OverlayEvent = serde_json::from_str(json).expect("deserialize attach");
        match event {
            OverlayEvent::AgentAttachRequested {
                kind,
                session_id,
                model,
            } => {
                assert_eq!(kind, "claude_code");
                assert_eq!(session_id, None);
                assert_eq!(model, None);
            }
            other => panic!("expected agent_attach_requested, got {other:?}"),
        }

        let with_session = r#"{"type":"agent_attach_requested","kind":"cursor","session_id":"s9"}"#;
        let event: OverlayEvent = serde_json::from_str(with_session).expect("deserialize");
        match event {
            OverlayEvent::AgentAttachRequested {
                kind,
                session_id,
                model,
            } => {
                assert_eq!(kind, "cursor");
                assert_eq!(session_id.as_deref(), Some("s9"));
                assert_eq!(model, None);
            }
            other => panic!("expected agent_attach_requested, got {other:?}"),
        }
    }

    #[test]
    fn agent_attach_event_carries_optional_model() {
        // A payload with an explicit model must carry it through.
        let with_model =
            r#"{"type":"agent_attach_requested","kind":"cursor","model":"composer-2.5"}"#;
        let event: OverlayEvent = serde_json::from_str(with_model).expect("deserialize");
        match event {
            OverlayEvent::AgentAttachRequested {
                kind,
                session_id,
                model,
            } => {
                assert_eq!(kind, "cursor");
                assert_eq!(session_id, None);
                assert_eq!(model.as_deref(), Some("composer-2.5"));
            }
            other => panic!("expected agent_attach_requested, got {other:?}"),
        }
    }

    #[test]
    fn agent_request_events_serialize_with_type_tag() {
        let list = serde_json::to_string(&OverlayEvent::AgentListRequested).expect("serialize");
        assert_eq!(list, r#"{"type":"agent_list_requested"}"#);

        let detach = serde_json::to_string(&OverlayEvent::AgentDetachRequested).expect("serialize");
        assert_eq!(detach, r#"{"type":"agent_detach_requested"}"#);

        let sessions = serde_json::to_string(&OverlayEvent::AgentSessionsRequested {
            kind: "gemini".to_string(),
            offset: 0,
            limit: 0,
            search: String::new(),
        })
        .expect("serialize");
        assert_eq!(
            sessions,
            r#"{"type":"agent_sessions_requested","kind":"gemini","offset":0,"limit":0,"search":""}"#
        );

        let reauth = serde_json::to_string(&OverlayEvent::ConnectorReauthRequested {
            kind: "cursor".to_string(),
            name: "remote".to_string(),
        })
        .expect("serialize");
        assert_eq!(
            reauth,
            r#"{"type":"connector_reauth_requested","kind":"cursor","name":"remote"}"#
        );
    }

    #[test]
    fn push_fix_proposal_serializes_with_type_tag_and_skips_absent_diff() {
        let id = uuid::Uuid::nil();
        let command = OverlayCommand::PushFixProposal {
            proposal_id: id,
            diagnosis: "PORT is read before the env var is set".to_string(),
            reasoning: "Reading it lazily fixes the ordering".to_string(),
            fix: "cargo fmt".to_string(),
            diff: None,
            apply_supported: true,
        };
        let json = serde_json::to_string(&command).expect("serialize push_fix_proposal");
        assert!(json.contains(r#""type":"push_fix_proposal""#));
        assert!(json.contains(r#""proposal_id":"00000000-0000-0000-0000-000000000000""#));
        assert!(json.contains(r#""apply_supported":true"#));
        // Absent diff is omitted from the wire form.
        assert!(!json.contains("diff"));
    }

    #[test]
    fn push_fix_proposal_includes_diff_when_present() {
        let command = OverlayCommand::PushFixProposal {
            proposal_id: uuid::Uuid::nil(),
            diagnosis: "d".to_string(),
            reasoning: "r".to_string(),
            fix: "f".to_string(),
            diff: Some("--- a\n+++ b".to_string()),
            apply_supported: false,
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(json.contains(r#""diff":"--- a\n+++ b""#));
        assert!(json.contains(r#""apply_supported":false"#));
    }

    #[test]
    fn fix_requested_event_deserializes_with_optional_card_id() {
        // card_id omitted -> None.
        let json = r#"{"type":"fix_requested","question":"the build fails"}"#;
        let event: OverlayEvent = serde_json::from_str(json).expect("deserialize fix_requested");
        match event {
            OverlayEvent::FixRequested { card_id, question } => {
                assert_eq!(card_id, None);
                assert_eq!(question, "the build fails");
            }
            other => panic!("expected fix_requested, got {other:?}"),
        }

        // card_id present -> Some.
        let with_card = r#"{"type":"fix_requested","card_id":"00000000-0000-0000-0000-000000000000","question":"x"}"#;
        let event: OverlayEvent = serde_json::from_str(with_card).expect("deserialize");
        match event {
            OverlayEvent::FixRequested { card_id, question } => {
                assert_eq!(card_id, Some(uuid::Uuid::nil()));
                assert_eq!(question, "x");
            }
            other => panic!("expected fix_requested, got {other:?}"),
        }
    }

    #[test]
    fn fix_approval_responded_event_roundtrips() {
        let json = r#"{"type":"fix_approval_responded","proposal_id":"00000000-0000-0000-0000-000000000000","approved":true}"#;
        let event: OverlayEvent = serde_json::from_str(json).expect("deserialize approval");
        match event {
            OverlayEvent::FixApprovalResponded {
                proposal_id,
                approved,
            } => {
                assert_eq!(proposal_id, uuid::Uuid::nil());
                assert!(approved);
            }
            other => panic!("expected fix_approval_responded, got {other:?}"),
        }

        // Re-serialize the rejection form and confirm the tag + fields.
        let reject = serde_json::to_string(&OverlayEvent::FixApprovalResponded {
            proposal_id: uuid::Uuid::nil(),
            approved: false,
        })
        .expect("serialize");
        assert!(reject.contains(r#""type":"fix_approval_responded""#));
        assert!(reject.contains(r#""approved":false"#));
    }

    #[test]
    fn set_meeting_state_roundtrips() {
        let command = OverlayCommand::SetMeetingState {
            transcript: vec![
                MeetingTranscriptLine {
                    id: "00000000-0000-0000-0000-000000000001".to_string(),
                    source: "system".to_string(),
                    speaker: None,
                    speaker_id: None,
                    text: "hello there".to_string(),
                    is_final: true,
                },
                MeetingTranscriptLine {
                    id: "00000000-0000-0000-0000-000000000002".to_string(),
                    source: "mic".to_string(),
                    speaker: None,
                    speaker_id: None,
                    text: "hi back".to_string(),
                    is_final: true,
                },
            ],
            conversation: vec![MeetingConversationTurn {
                id: "00000000-0000-0000-0000-000000000003".to_string(),
                question: "what next?".to_string(),
                answer: "ship it".to_string(),
                source: Some("overlay ask".to_string()),
            }],
            decisions: vec![MeetingDecision {
                id: "00000000-0000-0000-0000-000000000004".to_string(),
                text: "Ship the beta on Friday.".to_string(),
            }],
            context: vec![OverlayContextItem {
                id: uuid::Uuid::nil(),
                title: "screenshot.png".to_string(),
                kind: "image".to_string(),
                path: None,
                thumbnail: None,
                anchor_segment_id: Some("00000000-0000-0000-0000-000000000001".to_string()),
                text_preview: None,
            }],
            meeting_id: None,
            read_only: false,
        };
        let json = serde_json::to_string(&command).expect("serialize set_meeting_state");
        assert!(json.contains(r#""type":"set_meeting_state""#));
        // `is_final` renders on the wire as `"final"` to match the UI shape.
        assert!(json.contains(r#""final":true"#));
        assert!(!json.contains("is_final"));
        assert!(json.contains(r#""source":"system""#));
        assert!(json.contains(r#""source":"mic""#));
        let decoded: OverlayCommand =
            serde_json::from_str(&json).expect("decode set_meeting_state");
        assert_eq!(
            serde_json::to_string(&decoded).expect("re-serialize"),
            json,
            "SetMeetingState should round-trip"
        );
    }

    #[test]
    fn meeting_state_requested_decodes() {
        let json = r#"{"type":"meeting_state_requested"}"#;
        let decoded: OverlayEvent = serde_json::from_str(json).expect("decode");
        match decoded {
            OverlayEvent::MeetingStateRequested => {}
            other => panic!("expected meeting_state_requested, got {other:?}"),
        }
        // Round-trips back to the same tag with no fields.
        assert_eq!(
            serde_json::to_string(&OverlayEvent::MeetingStateRequested).expect("serialize"),
            json
        );
    }

    #[test]
    fn overlay_lifecycle_event_serializes() {
        let json = serde_json::to_string(&OverlayEvent::Lifecycle {
            stage: "started".to_string(),
            status: Some("ok".to_string()),
            detail: Some("capture_excluded=true".to_string()),
        })
        .expect("serialize lifecycle event");

        assert_eq!(
            json,
            r#"{"type":"lifecycle","stage":"started","status":"ok","detail":"capture_excluded=true"}"#
        );
    }

    #[test]
    fn session_delete_event_serializes() {
        let json = serde_json::to_string(&OverlayEvent::SessionDeleteRequested {
            id: uuid::Uuid::nil(),
        })
        .expect("serialize session delete event");

        assert_eq!(
            json,
            r#"{"type":"session_delete_requested","id":"00000000-0000-0000-0000-000000000000"}"#
        );
    }

    #[test]
    fn set_meeting_state_active_rehydrate_wire_is_byte_identical() {
        // The active-rehydrate emitter sets meeting_id: None + read_only: false.
        // With skip_serializing_if + default, its JSON must be UNCHANGED from
        // before the two fields existed — no `meeting_id`, no `read_only` keys.
        // This is the back-compat invariant the live-rehydrate picker relies on.
        let command = OverlayCommand::SetMeetingState {
            transcript: vec![MeetingTranscriptLine {
                id: "00000000-0000-0000-0000-000000000001".to_string(),
                source: "system".to_string(),
                speaker: None,
                speaker_id: None,
                text: "hello".to_string(),
                is_final: true,
            }],
            conversation: vec![],
            decisions: vec![],
            context: vec![],
            meeting_id: None,
            read_only: false,
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(
            !json.contains("meeting_id"),
            "active rehydrate must omit meeting_id"
        );
        assert!(
            !json.contains("read_only"),
            "active rehydrate must omit read_only"
        );
        assert_eq!(
            json,
            r#"{"type":"set_meeting_state","transcript":[{"id":"00000000-0000-0000-0000-000000000001","source":"system","text":"hello","final":true}],"conversation":[]}"#
        );
    }

    #[test]
    fn set_meeting_state_past_view_carries_meeting_id_and_read_only() {
        let command = OverlayCommand::SetMeetingState {
            transcript: vec![],
            conversation: vec![],
            decisions: vec![],
            context: vec![],
            meeting_id: Some("00000000-0000-0000-0000-0000000000aa".to_string()),
            read_only: true,
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(json.contains(r#""meeting_id":"00000000-0000-0000-0000-0000000000aa""#));
        assert!(json.contains(r#""read_only":true"#));
        let decoded: OverlayCommand = serde_json::from_str(&json).expect("decode");
        assert_eq!(serde_json::to_string(&decoded).expect("re-serialize"), json);
    }

    #[test]
    fn legacy_set_meeting_state_decodes_without_new_fields() {
        // An old daemon sends no meeting_id / read_only; the extended command
        // must still decode, defaulting them (None / false).
        let legacy = r#"{"type":"set_meeting_state","transcript":[],"conversation":[]}"#;
        let decoded: OverlayCommand = serde_json::from_str(legacy).expect("decode legacy");
        match decoded {
            OverlayCommand::SetMeetingState {
                meeting_id,
                read_only,
                ..
            } => {
                assert_eq!(meeting_id, None);
                assert!(!read_only);
            }
            other => panic!("expected set_meeting_state, got {other:?}"),
        }
    }

    #[test]
    fn set_meetings_roundtrips_and_skips_absent_optionals() {
        let command = OverlayCommand::SetMeetings {
            meetings: vec![MeetingSummary {
                id: "00000000-0000-0000-0000-000000000001".to_string(),
                title: "Standup".to_string(),
                started_at: "1718000000000".to_string(),
                ended_at: None,
                transcript_count: 12,
                turn_count: 3,
                preview: None,
                is_active: true,
                agent_session_id: None,
                agent_kind: None,
            }],
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(json.contains(r#""type":"set_meetings""#));
        assert!(json.contains(r#""transcript_count":12"#));
        assert!(json.contains(r#""is_active":true"#));
        // Absent optionals are omitted.
        assert!(!json.contains("ended_at"));
        assert!(!json.contains("preview"));
        assert!(!json.contains("agent_session_id"));
        assert!(!json.contains("agent_kind"));
        let decoded: OverlayCommand = serde_json::from_str(&json).expect("decode");
        assert_eq!(serde_json::to_string(&decoded).expect("re-serialize"), json);
    }

    #[test]
    fn meeting_summary_carries_agent_session_and_preview_when_present() {
        let summary = MeetingSummary {
            id: "00000000-0000-0000-0000-000000000002".to_string(),
            title: "Design review".to_string(),
            started_at: "1718000000000".to_string(),
            ended_at: Some("1718000900000".to_string()),
            transcript_count: 40,
            turn_count: 5,
            preview: Some("So the plan for the migration is".to_string()),
            is_active: false,
            agent_session_id: Some("sess-abc".to_string()),
            agent_kind: Some("claude_code".to_string()),
        };
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(json.contains(r#""ended_at":"1718000900000""#));
        assert!(json.contains(r#""preview":"So the plan for the migration is""#));
        assert!(json.contains(r#""agent_session_id":"sess-abc""#));
        assert!(json.contains(r#""agent_kind":"claude_code""#));
        let decoded: MeetingSummary = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.agent_session_id.as_deref(), Some("sess-abc"));
        assert_eq!(decoded.agent_kind.as_deref(), Some("claude_code"));
    }

    #[test]
    fn meetings_requested_defaults_paging_when_absent() {
        let legacy = r#"{"type":"meetings_requested"}"#;
        let decoded: OverlayEvent = serde_json::from_str(legacy).expect("decode legacy");
        match decoded {
            OverlayEvent::MeetingsRequested { offset, limit } => {
                assert_eq!(offset, 0);
                assert_eq!(limit, 0);
            }
            other => panic!("expected meetings_requested, got {other:?}"),
        }
    }

    #[test]
    fn meeting_open_requested_roundtrips() {
        let event = OverlayEvent::MeetingOpenRequested {
            id: uuid::Uuid::nil(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"meeting_open_requested","id":"00000000-0000-0000-0000-000000000000"}"#
        );
        let decoded: OverlayEvent = serde_json::from_str(&json).expect("decode");
        assert_eq!(serde_json::to_string(&decoded).expect("re-serialize"), json);
    }

    #[test]
    fn meeting_continue_requested_round_trips() {
        let id = uuid::Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
        let event = OverlayEvent::MeetingContinueRequested { id };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains("meeting_continue_requested"),
            "tag present: {json}"
        );
        assert!(json.contains(&id.to_string()), "id present: {json}");
        let decoded: OverlayEvent = serde_json::from_str(&json).expect("decode");
        match decoded {
            OverlayEvent::MeetingContinueRequested { id: decoded_id } => {
                assert_eq!(decoded_id, id);
            }
            other => panic!("expected meeting_continue_requested, got {other:?}"),
        }
    }
}
