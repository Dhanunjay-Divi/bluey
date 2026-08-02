use serde::{Deserialize, Serialize};

pub const MAX_ASSISTANT_ROLE_CHARS: usize = 200;
pub const MAX_ASSISTANT_COMPANY_CHARS: usize = 200;
pub const MAX_ASSISTANT_INSTRUCTIONS_CHARS: usize = 4_000;
pub const MAX_PRIORITY_QUESTIONS: usize = 12;
pub const MAX_PRIORITY_QUESTION_CHARS: usize = 500;
pub const MAX_SOURCE_REFERENCE_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantMode {
    #[default]
    General,
    Interview,
    BehavioralInterview,
    Coding,
    SystemDesign,
    Meeting,
    Writing,
}

impl AssistantMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Interview => "Interview",
            Self::BehavioralInterview => "Behavioral Interview",
            Self::Coding => "Code",
            Self::SystemDesign => "System Design",
            Self::Meeting => "Meeting",
            Self::Writing => "Writing",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantSourceReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_version_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_fingerprint: Option<String>,
}

impl AssistantSourceReference {
    fn normalize(mut self) -> Result<Self, String> {
        self.application_id = normalize_reference(self.application_id);
        self.receipt_id = normalize_reference(self.receipt_id);
        self.resume_version_id = normalize_reference(self.resume_version_id);
        self.receipt_fingerprint =
            normalize_reference(self.receipt_fingerprint).map(|value| value.to_ascii_lowercase());
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("application_id", self.application_id.as_deref()),
            ("receipt_id", self.receipt_id.as_deref()),
            ("resume_version_id", self.resume_version_id.as_deref()),
        ] {
            if let Some(value) = value {
                validate_reference(name, value)?;
            }
        }
        if let Some(value) = self.receipt_fingerprint.as_deref() {
            let is_sha256 = value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
            if !is_sha256 {
                return Err("receipt_fingerprint must be a 64-character SHA-256 value".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantProfile {
    #[serde(default = "assistant_profile_schema_version")]
    pub schema_version: u8,
    #[serde(default)]
    pub mode: AssistantMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub priority_questions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<AssistantSourceReference>,
}

impl Default for AssistantProfile {
    fn default() -> Self {
        Self {
            schema_version: assistant_profile_schema_version(),
            mode: AssistantMode::General,
            target_role: None,
            company: None,
            custom_instructions: None,
            priority_questions: Vec::new(),
            source: None,
        }
    }
}

impl AssistantProfile {
    pub fn normalize(mut self) -> Result<Self, String> {
        if self.schema_version != assistant_profile_schema_version() {
            return Err(format!(
                "unsupported assistant profile schema {}",
                self.schema_version
            ));
        }

        self.target_role = normalize_single_line(self.target_role);
        self.company = normalize_single_line(self.company);
        self.custom_instructions = normalize_optional(self.custom_instructions);
        self.priority_questions = self
            .priority_questions
            .into_iter()
            .filter_map(|question| normalize_single_line(Some(question)))
            .collect();

        validate_chars(
            "target_role",
            self.target_role.as_deref(),
            MAX_ASSISTANT_ROLE_CHARS,
        )?;
        validate_chars(
            "company",
            self.company.as_deref(),
            MAX_ASSISTANT_COMPANY_CHARS,
        )?;
        validate_chars(
            "custom_instructions",
            self.custom_instructions.as_deref(),
            MAX_ASSISTANT_INSTRUCTIONS_CHARS,
        )?;
        if self.priority_questions.len() > MAX_PRIORITY_QUESTIONS {
            return Err(format!(
                "priority_questions may contain at most {MAX_PRIORITY_QUESTIONS} items"
            ));
        }
        for question in &self.priority_questions {
            validate_chars(
                "priority_question",
                Some(question),
                MAX_PRIORITY_QUESTION_CHARS,
            )?;
        }
        if let Some(source) = self.source.take() {
            self.source = Some(source.normalize()?);
        }
        Ok(self)
    }

    pub fn context_summary(&self) -> Option<String> {
        let mut lines = vec![format!("Coaching mode: {}", self.mode.label())];
        if let Some(role) = self.target_role.as_deref() {
            lines.push(format!("Target role: {role}"));
        }
        if let Some(company) = self.company.as_deref() {
            lines.push(format!("Target company: {company}"));
        }
        if !self.priority_questions.is_empty() {
            lines.push("User-prioritized questions:".to_string());
            lines.extend(
                self.priority_questions
                    .iter()
                    .enumerate()
                    .map(|(index, question)| format!("{}. {question}", index + 1)),
            );
        }
        (lines.len() > 1 || self.mode != AssistantMode::General).then(|| lines.join("\n"))
    }
}

fn assistant_profile_schema_version() -> u8 {
    1
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let cleaned = value
            .chars()
            .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
            .collect::<String>();
        let cleaned = cleaned.trim().to_string();
        (!cleaned.is_empty()).then_some(cleaned)
    })
}

fn normalize_single_line(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let cleaned = value
            .chars()
            .filter(|character| !character.is_control() || character.is_whitespace())
            .collect::<String>();
        let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
        (!cleaned.is_empty()).then_some(cleaned)
    })
}

fn normalize_reference(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let cleaned = value.trim().to_string();
        (!cleaned.is_empty()).then_some(cleaned)
    })
}

fn validate_chars(name: &str, value: Option<&str>, limit: usize) -> Result<(), String> {
    if value.is_some_and(|value| value.chars().count() > limit) {
        return Err(format!("{name} may contain at most {limit} characters"));
    }
    Ok(())
}

fn validate_reference(name: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().count() > MAX_SOURCE_REFERENCE_CHARS {
        return Err(format!(
            "{name} must contain between 1 and {MAX_SOURCE_REFERENCE_CHARS} characters"
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(format!("{name} contains unsupported characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_empty_general_mode() {
        let profile = AssistantProfile::default();
        assert_eq!(profile.mode, AssistantMode::General);
        assert!(profile.context_summary().is_none());
    }

    #[test]
    fn profile_normalization_trims_and_removes_empty_questions() {
        let profile = AssistantProfile {
            mode: AssistantMode::Interview,
            target_role: Some("  Platform\n Engineer  ".to_string()),
            company: Some("Acme\0".to_string()),
            custom_instructions: Some("  Keep answers concise.  ".to_string()),
            priority_questions: vec!["  Reliability story?  ".to_string(), "  ".to_string()],
            ..AssistantProfile::default()
        }
        .normalize()
        .expect("normalize profile");

        assert_eq!(profile.target_role.as_deref(), Some("Platform Engineer"));
        assert_eq!(profile.company.as_deref(), Some("Acme"));
        assert_eq!(profile.priority_questions, vec!["Reliability story?"]);
        assert!(profile
            .context_summary()
            .expect("summary")
            .contains("Target role: Platform Engineer"));
    }

    #[test]
    fn profile_rejects_oversized_or_unsafe_source_data() {
        let oversized = AssistantProfile {
            target_role: Some("r".repeat(MAX_ASSISTANT_ROLE_CHARS + 1)),
            ..AssistantProfile::default()
        };
        assert!(oversized.normalize().is_err());

        let unsafe_source = AssistantProfile {
            source: Some(AssistantSourceReference {
                application_id: Some("application/escape".to_string()),
                ..AssistantSourceReference::default()
            }),
            ..AssistantProfile::default()
        };
        assert!(unsafe_source.normalize().is_err());
    }

    #[test]
    fn provider_context_never_exposes_provenance_identifiers() {
        let profile = AssistantProfile {
            mode: AssistantMode::Interview,
            target_role: Some("Engineer".to_string()),
            source: Some(AssistantSourceReference {
                application_id: Some("application-secret".to_string()),
                receipt_id: Some("receipt-secret".to_string()),
                resume_version_id: Some("resume-secret".to_string()),
                receipt_fingerprint: Some("a".repeat(64)),
            }),
            ..AssistantProfile::default()
        };
        let summary = profile.context_summary().expect("profile context");
        assert!(summary.contains("Target role: Engineer"));
        assert!(!summary.contains("application-secret"));
        assert!(!summary.contains("receipt-secret"));
        assert!(!summary.contains("resume-secret"));
    }
}
