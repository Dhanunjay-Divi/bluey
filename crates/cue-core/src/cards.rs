use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::clock;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Question,
    Answer,
    ActionItem,
    Decision,
    Context,
    Transcript,
    Warning,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueCard {
    pub id: Uuid,
    pub kind: CardKind,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub source: Option<String>,
}

impl CueCard {
    pub fn new(kind: CardKind, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            title: title.into(),
            body: body.into(),
            created_at: clock::now_epoch_ms_string(),
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}
