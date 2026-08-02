use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    clock, AssistantProfile, AssistantSourceReference, CardArtifactType, ContextKind,
    ContextProcessingStatus,
};

pub const WORKSPACE_SCHEMA_VERSION: u8 = 1;
pub const MAX_WORKSPACE_TITLE_CHARS: usize = 120;
pub const MAX_WORKSPACE_INSTRUCTIONS_CHARS: usize = 4_000;
pub const MAX_WORKSPACE_REFERENCE_TITLE_CHARS: usize = 200;
pub const MAX_WORKSPACE_ACTIVITY_REFERENCES: usize = 128;
pub const MAX_WORKSPACE_CONTEXT_REFERENCES: usize = 256;
pub const MAX_WORKSPACE_ARTIFACT_REFERENCES: usize = 128;
const MAX_WORKSPACE_OWNER_CHARS: usize = 200;
const MAX_WORKSPACE_REFERENCE_CHARS: usize = 200;
const MAX_WORKSPACE_TIMESTAMP_CHARS: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRecord {
    #[serde(default = "workspace_schema_version")]
    pub schema_version: u8,
    pub id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_account_id: Option<String>,
    pub title: String,
    /// Optimistic-concurrency version. Mutations must present the exact current
    /// revision so a stale view cannot silently overwrite newer workspace data.
    pub revision: u64,
    pub profile: AssistantProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Derived references only. Session transcripts, context bodies and
    /// artifact bodies remain in MeetingRecord and are never copied here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activity: Vec<WorkspaceActivityReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<WorkspaceContextReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<WorkspaceArtifactReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_job: Option<WorkspaceLinkedJobMetadata>,
    #[serde(default)]
    pub deletion_state: WorkspaceDeletionState,
    pub created_at: String,
    pub updated_at: String,
}

impl WorkspaceRecord {
    pub fn new(
        owner_account_id: Option<String>,
        title: String,
        profile: AssistantProfile,
        instructions: Option<String>,
    ) -> Result<Self, String> {
        let now = clock::now_epoch_ms_string();
        let mut workspace = Self {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            owner_account_id,
            title,
            revision: 1,
            profile,
            instructions,
            activity: Vec::new(),
            context: Vec::new(),
            artifacts: Vec::new(),
            linked_job: None,
            deletion_state: WorkspaceDeletionState::Active,
            created_at: now.clone(),
            updated_at: now,
        };
        workspace.normalize_editable_fields()?;
        workspace.validate()?;
        Ok(workspace)
    }

    pub fn normalize_editable_fields(&mut self) -> Result<(), String> {
        self.title = normalize_single_line(&self.title);
        self.instructions = normalize_multiline(self.instructions.take());
        self.profile = self.profile.clone().normalize()?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != WORKSPACE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported workspace schema {}",
                self.schema_version
            ));
        }
        if self.revision == 0 {
            return Err("workspace revision must be positive".to_string());
        }
        validate_required_chars("workspace title", &self.title, MAX_WORKSPACE_TITLE_CHARS)?;
        if self
            .owner_account_id
            .as_deref()
            .is_some_and(|owner| !valid_bounded_text(owner, MAX_WORKSPACE_OWNER_CHARS, false))
        {
            return Err("invalid workspace owner".to_string());
        }
        if self.instructions.as_deref().is_some_and(|instructions| {
            !valid_bounded_text(instructions, MAX_WORKSPACE_INSTRUCTIONS_CHARS, true)
        }) {
            return Err(format!(
                "workspace instructions may contain at most {MAX_WORKSPACE_INSTRUCTIONS_CHARS} characters"
            ));
        }
        let normalized_profile = self.profile.clone().normalize()?;
        if normalized_profile != self.profile {
            return Err("workspace profile must be normalized".to_string());
        }
        validate_timestamp("workspace created_at", &self.created_at)?;
        validate_timestamp("workspace updated_at", &self.updated_at)?;
        if self.activity.len() > MAX_WORKSPACE_ACTIVITY_REFERENCES {
            return Err("workspace has too many activity references".to_string());
        }
        if self.context.len() > MAX_WORKSPACE_CONTEXT_REFERENCES {
            return Err("workspace has too many context references".to_string());
        }
        if self.artifacts.len() > MAX_WORKSPACE_ARTIFACT_REFERENCES {
            return Err("workspace has too many artifact references".to_string());
        }
        for reference in &self.activity {
            reference.validate()?;
        }
        for reference in &self.context {
            reference.validate()?;
        }
        for reference in &self.artifacts {
            reference.validate()?;
        }
        if let Some(linked_job) = self.linked_job.as_ref() {
            linked_job.validate()?;
            if self.profile.source.as_ref() != Some(&linked_job.source) {
                return Err(
                    "workspace linked job source must match the coaching profile source"
                        .to_string(),
                );
            }
        }
        if let WorkspaceDeletionState::Deleted { deleted_at } = &self.deletion_state {
            validate_timestamp("workspace deleted_at", deleted_at)?;
        }
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        matches!(self.deletion_state, WorkspaceDeletionState::Active)
    }

    pub fn touch(&mut self) -> Result<(), String> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| "workspace revision exhausted".to_string())?;
        self.updated_at = clock::now_epoch_ms_string();
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WorkspaceDeletionState {
    #[default]
    Active,
    Deleted {
        deleted_at: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceActivityReference {
    pub meeting_id: Uuid,
    pub title: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

impl WorkspaceActivityReference {
    fn validate(&self) -> Result<(), String> {
        validate_required_chars(
            "workspace activity title",
            &self.title,
            MAX_WORKSPACE_REFERENCE_TITLE_CHARS,
        )?;
        validate_timestamp("workspace activity started_at", &self.started_at)?;
        if let Some(ended_at) = self.ended_at.as_deref() {
            validate_timestamp("workspace activity ended_at", ended_at)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceContextReference {
    pub meeting_id: Uuid,
    pub context_id: Uuid,
    pub title: String,
    pub kind: ContextKind,
    pub processing_status: ContextProcessingStatus,
    pub created_at: String,
}

impl WorkspaceContextReference {
    fn validate(&self) -> Result<(), String> {
        validate_required_chars(
            "workspace context title",
            &self.title,
            MAX_WORKSPACE_REFERENCE_TITLE_CHARS,
        )?;
        validate_timestamp("workspace context created_at", &self.created_at)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceArtifactReference {
    pub meeting_id: Uuid,
    pub conversation_turn_id: Uuid,
    pub artifact_type: CardArtifactType,
    pub title: String,
    pub created_at: String,
}

impl WorkspaceArtifactReference {
    fn validate(&self) -> Result<(), String> {
        validate_required_chars(
            "workspace artifact title",
            &self.title,
            MAX_WORKSPACE_REFERENCE_TITLE_CHARS,
        )?;
        validate_timestamp("workspace artifact created_at", &self.created_at)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceLinkedJobMetadata {
    pub import_id: String,
    pub context_sha256: String,
    pub source: AssistantSourceReference,
    pub linked_at: String,
}

impl WorkspaceLinkedJobMetadata {
    fn validate(&self) -> Result<(), String> {
        validate_required_chars(
            "workspace linked job import_id",
            &self.import_id,
            MAX_WORKSPACE_REFERENCE_CHARS,
        )?;
        if self.context_sha256.len() != 64
            || !self
                .context_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("workspace linked job context_sha256 must be SHA-256".to_string());
        }
        self.source.validate()?;
        validate_timestamp("workspace linked job linked_at", &self.linked_at)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCreateRequest {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<AssistantProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceUpdateRequest {
    pub workspace_id: Uuid,
    pub expected_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<AssistantProfile>,
    #[serde(default)]
    pub instructions: WorkspaceInstructionsPatch,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum WorkspaceInstructionsPatch {
    #[default]
    Unchanged,
    Clear,
    Set {
        text: String,
    },
}

fn workspace_schema_version() -> u8 {
    WORKSPACE_SCHEMA_VERSION
}

fn normalize_single_line(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_multiline(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value
            .chars()
            .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
            .collect::<String>();
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn validate_required_chars(name: &str, value: &str, max: usize) -> Result<(), String> {
    if !valid_bounded_text(value, max, false) {
        return Err(format!(
            "{name} must contain between 1 and {max} characters"
        ));
    }
    Ok(())
}

fn valid_bounded_text(value: &str, max: usize, multiline: bool) -> bool {
    let count = value.chars().count();
    count > 0
        && count <= max
        && value.chars().all(|character| {
            !character.is_control()
                || (multiline && matches!(character, '\n' | '\t'))
                || (!multiline && character.is_whitespace())
        })
}

fn validate_timestamp(name: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_WORKSPACE_TIMESTAMP_CHARS
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("invalid {name}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_normalizes_and_bounds_user_editable_fields() {
        let workspace = WorkspaceRecord::new(
            Some("account-1".to_string()),
            "  Systems   interview  ".to_string(),
            AssistantProfile::default(),
            Some("  Prefer concise answers.  ".to_string()),
        )
        .expect("valid workspace");

        assert_eq!(workspace.title, "Systems interview");
        assert_eq!(
            workspace.instructions.as_deref(),
            Some("Prefer concise answers.")
        );
        assert_eq!(workspace.revision, 1);
        assert!(workspace.is_active());

        assert!(WorkspaceRecord::new(
            None,
            "x".repeat(MAX_WORKSPACE_TITLE_CHARS + 1),
            AssistantProfile::default(),
            None,
        )
        .is_err());
        assert!(WorkspaceRecord::new(
            None,
            "Valid".to_string(),
            AssistantProfile::default(),
            Some("x".repeat(MAX_WORKSPACE_INSTRUCTIONS_CHARS + 1)),
        )
        .is_err());
    }

    #[test]
    fn workspace_record_does_not_contain_session_payload_bytes() {
        let workspace = WorkspaceRecord::new(
            None,
            "Default".to_string(),
            AssistantProfile::default(),
            None,
        )
        .expect("workspace");
        let json = serde_json::to_value(workspace).expect("serialize workspace");

        assert!(json.get("transcript").is_none());
        assert!(json.get("conversation").is_none());
        assert!(json.get("context").is_none());
        assert!(json.get("artifacts").is_none());
    }

    #[test]
    fn linked_job_source_must_exactly_match_the_profile_source() {
        let source = AssistantSourceReference {
            application_id: Some("application-1".to_string()),
            receipt_id: Some("receipt-1".to_string()),
            resume_version_id: Some("resume-1".to_string()),
            receipt_fingerprint: Some("a".repeat(64)),
        };
        let profile = AssistantProfile {
            source: Some(source.clone()),
            ..AssistantProfile::default()
        };
        let mut workspace = WorkspaceRecord::new(
            Some("account-1".to_string()),
            "Interview".to_string(),
            profile,
            None,
        )
        .expect("workspace");
        workspace.linked_job = Some(WorkspaceLinkedJobMetadata {
            import_id: "import-1".to_string(),
            context_sha256: "b".repeat(64),
            source: source.clone(),
            linked_at: clock::now_epoch_ms_string(),
        });
        workspace.validate().expect("matching source");

        workspace.linked_job.as_mut().unwrap().source.application_id =
            Some("application-2".to_string());
        assert!(workspace.validate().is_err());
    }
}
