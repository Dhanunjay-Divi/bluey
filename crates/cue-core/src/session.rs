use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub title: String,
    pub status: SessionStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_active_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub token_count: u64,
    pub compressed_summary: Option<String>,
    pub active_skill: Option<Skill>,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Paused,
    Archived,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Archived => "archived",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Skill {
    Dsa,
    SystemDesign,
    Programming,
    Behavioral,
    Sales,
    Negotiation,
    Presentation,
    Devops,
    DataScience,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub id: Uuid,
    pub session_id: Uuid,
    pub turn_index: u32,
    pub user_message: String,
    pub model_response: String,
    pub lane: Lane,
    pub provider: String,
    pub model: String,
    pub created_at: i64,
    pub duration_ms: Option<u32>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Snap,
    SnapEdit,
    Solve,
    Think,
}

impl Lane {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Snap => "snap",
            Self::SnapEdit => "snap_edit",
            Self::Solve => "solve",
            Self::Think => "think",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "snap" => Some(Self::Snap),
            "snap_edit" => Some(Self::SnapEdit),
            "solve" => Some(Self::Solve),
            "think" => Some(Self::Think),
            _ => None,
        }
    }
}

/// Input struct for creating a new turn (no id/turn_index yet).
#[derive(Debug, Clone)]
pub struct NewTurn {
    pub user_message: String,
    pub model_response: String,
    pub lane: Lane,
    pub provider: String,
    pub model: String,
    pub created_at: i64,
    pub duration_ms: Option<u32>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cost_cents: Option<i64>,
}
