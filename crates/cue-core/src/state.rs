use serde::{Deserialize, Serialize};

use crate::clock;
use crate::OverlayPosition;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingState {
    Idle,
    Listening,
    InMeeting {
        id: String,
        title: Option<String>,
        started_at: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    pub pid: u32,
    pub started_at: String,
    pub meeting: MeetingState,
    pub transcript_segments: usize,
    pub context_items: usize,
    pub answer_instructions_set: bool,
    pub action_items: usize,
    pub decisions: usize,
    pub overlay_visible: bool,
    pub overlay_position: OverlayPosition,
    pub overlay_opacity: f32,
    pub overlay_capture_excluded: Option<bool>,
    pub screen_capture_active: bool,
    pub screen_capture_interval_secs: Option<u64>,
    #[serde(default)]
    pub screen_capture_generation: u64,
}

impl DaemonState {
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            started_at: clock::now_epoch_ms_string(),
            meeting: MeetingState::Idle,
            transcript_segments: 0,
            context_items: 0,
            answer_instructions_set: false,
            action_items: 0,
            decisions: 0,
            overlay_visible: false,
            overlay_position: OverlayPosition::Center,
            overlay_opacity: 0.92,
            overlay_capture_excluded: None,
            screen_capture_active: false,
            screen_capture_interval_secs: None,
            screen_capture_generation: 0,
        }
    }
}
