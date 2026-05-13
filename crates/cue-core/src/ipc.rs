use serde::{Deserialize, Serialize};

use crate::{
    ActionItem, AiRuntimeStatus, AnswerRequest, AnswerResponse, AnswerStreamEvent,
    AudioPipelineStatus, CloudSyncStatus, ContextArtifact, CueCard, DaemonState, MeetingRecap,
    MemoryHit, OverlayPosition, Speaker,
};

pub const DEFAULT_DAEMON_ADDR: &str = "127.0.0.1:57321";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    Ping,
    Status,
    Shutdown,
    OverlayShow,
    OverlayHide,
    OverlayToggle,
    OverlayClear,
    OverlayBoot {
        title: String,
        lines: Vec<String>,
    },
    OverlaySetOpacity {
        opacity: f32,
    },
    OverlaySetPosition {
        position: OverlayPosition,
    },
    PushCard {
        card: CueCard,
    },
    MeetingStart {
        title: Option<String>,
    },
    MeetingEnd,
    TranscriptAdd {
        speaker: Speaker,
        text: String,
        is_final: bool,
    },
    Ask {
        question: String,
    },
    Answer {
        request: AnswerRequest,
    },
    ContextAdd {
        path: String,
        title: Option<String>,
        note: Option<String>,
    },
    ContextList,
    ActivePageCapture,
    ScreenCaptureStart {
        interval_secs: Option<u64>,
    },
    ScreenCaptureStop,
    InstructionsSet {
        text: String,
    },
    InstructionsGet,
    InstructionsClear,
    MemorySearch {
        query: String,
        limit: usize,
    },
    AudioStatus,
    AudioStart {
        enable_system: bool,
        enable_microphone: bool,
    },
    AudioStop,
    AiStatus,
    CloudStatus,
    CloudSyncNow,
    Recap,
    ActionItems,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    Ok,
    Pong,
    Status {
        state: DaemonState,
    },
    Text {
        text: String,
    },
    Recap {
        recap: MeetingRecap,
    },
    ActionItems {
        items: Vec<ActionItem>,
    },
    ContextItems {
        items: Vec<ContextArtifact>,
    },
    MemoryHits {
        hits: Vec<MemoryHit>,
    },
    AudioStatus {
        status: AudioPipelineStatus,
    },
    AiStatus {
        status: AiRuntimeStatus,
    },
    Answer {
        response: AnswerResponse,
        events: Vec<AnswerStreamEvent>,
    },
    CloudStatus {
        status: CloudSyncStatus,
    },
    Error {
        message: String,
    },
}
