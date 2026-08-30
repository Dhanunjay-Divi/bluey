//! Tenant-scoped persistence for Bluey Jobs.
//!
//! The Jobs domain deliberately stores customer-facing documents as versioned
//! JSON payloads while retaining relational keys for tenant isolation,
//! idempotency, metering, and job-specific resume guarantees.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use base64::Engine;
use chrono::{Datelike, TimeZone, Utc};
use hmac::{Hmac, Mac};
use rand::RngCore;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use subtle::ConstantTimeEq;
use thiserror::Error;
use unicode_general_category::{get_general_category, GeneralCategory};
use unicode_normalization::UnicodeNormalization;

use super::jobs_tailoring::tailor_resume;
use super::DbPool;

pub const PACKET_OVERAGE_CENTS: i64 = 50;
pub const ADDITIONAL_INBOX_CENTS: i64 = 400;
const ENCRYPTED_PAYLOAD_PREFIX: &str = "bluey-jobs:v1:";
const DISCOVERY_MIN_INTERVAL_MS: i64 = 5 * 60 * 1_000;
const DISCOVERY_MAX_INTERVAL_MS: i64 = 24 * 60 * 60 * 1_000;
const CURATED_DISCOVERY_PROVIDER: &str = "curated_feed";
const CURATED_DISCOVERY_SOURCE_KEY: &str = "bluey-curated-v1";
const CURATED_DISCOVERY_COMPANY: &str = "Curated career feeds";
const CURATED_DISCOVERY_CATALOG_IDS: [&str; 4] = [
    "feed-simplify-new-grad",
    "feed-prepai-internships",
    "feed-prepai-new-grad",
    "feed-zapply-new-grad",
];
/// A guardrail against a single account repeatedly enrolling near-identical
/// public boards. Boards are intentionally one-to-one with a Career Track.
pub const DISCOVERY_MAX_SOURCES_PER_TRACK: usize = 8;
pub const DISCOVERY_MAX_SOURCES_PER_ACCOUNT: usize = 24;
const DISCOVERY_LEASE_MS: i64 = 2 * 60 * 1_000;
const GLOBAL_DISCOVERY_LEASE_MS: i64 = 10 * 60 * 1_000;
const GLOBAL_DISCOVERY_MAX_BATCH_ROWS: usize = 1_000;
const GLOBAL_DISCOVERY_MAX_MATERIALIZED_PER_ACCOUNT: usize = 500;
const EXECUTION_LEASE_TTL_MS: i64 = 60 * 1_000;
const LOCAL_RESUME_ACTION_TTL_MS: i64 = 15 * 60 * 1_000;
pub const SUBMISSION_RECONCILIATION_GRACE_MS: i64 = 24 * 60 * 60 * 1_000;
pub const SERVER_SUBMISSION_AUTHORITY_KEY: &str = "_bluey_server_submission_authority_v1";
pub const FINAL_SUBMIT_PROOF_KEY: &str = "_bluey_final_submit_proof_v1";
type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinalSubmitProof {
    pub schema_version: i64,
    pub adapter: String,
    pub adapter_version: String,
    pub control: String,
    pub job: FinalSubmitJobProof,
    pub target: FinalSubmitTargetProof,
    pub files: Vec<FinalSubmitFileProof>,
    pub fields: Vec<FinalSubmitFieldProof>,
    pub part_order: Vec<FinalSubmitPartOrderProof>,
    pub documents: Vec<FinalSubmitDocumentProof>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certification: Option<AtsFinalSubmitCertificationProof>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_surface: Option<AtsFinalSubmitObservedSurfaceProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsFinalSubmitCertificationProof {
    pub schema_version: i64,
    pub provider: String,
    pub adapter_version: String,
    pub manifest_sha256: String,
    pub activation_sha256: String,
    pub activation_generation: i64,
    pub target_key_sha256: String,
    pub layout_set_sha256: String,
    pub adapter_bundle_sha256: String,
    pub runner_target_sha256s: Vec<String>,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsFinalSubmitObservedSurfaceProof {
    pub schema_version: i64,
    pub variant_key: String,
    pub layout_contract_version: i64,
    pub surface_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinalSubmitJobProof {
    pub approved_canonical_url: String,
    pub page_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinalSubmitTargetProof {
    pub action_url: String,
    pub method: String,
    pub enctype: String,
    pub form_target: String,
    pub provider_job_key: String,
    pub form_identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinalSubmitFileProof {
    pub field_name: String,
    pub name: String,
    pub byte_length: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinalSubmitFieldProof {
    pub field_name: String,
    pub value_byte_length: i64,
    pub value_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinalSubmitPartOrderProof {
    pub kind: String,
    pub index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinalSubmitDocumentProof {
    pub kind: String,
    #[serde(default)]
    pub version_id: Option<String>,
    pub sha256: String,
}

fn default_discovery_interval_ms() -> i64 {
    4 * 60 * 60 * 1_000
}

fn discovery_next_run_at(source: &DiscoverySource, completed_at_ms: i64, failures: i64) -> i64 {
    // Stable per-source jitter avoids a synchronized account-wide polling
    // burst. Failures back off (up to 8x) before the same deterministic jitter
    // is applied; a new source still starts immediately on first enrollment.
    let multiplier = 1_i64 << failures.saturating_sub(1).clamp(0, 3) as u32;
    let interval = source
        .run_interval_ms
        .saturating_mul(multiplier)
        .min(DISCOVERY_MAX_INTERVAL_MS);
    let digest = Sha256::digest(source.id.as_bytes());
    let jitter_seed = u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 prefix"));
    let jitter = (jitter_seed % (interval.max(1) / 10) as u64) as i64;
    completed_at_ms
        .saturating_add(interval)
        .saturating_add(jitter)
}

const DISCOVERY_ACCOUNT_LOCK_SQL: &str =
    "SELECT pg_advisory_xact_lock(hashtextextended('jobs-discovery-account:' || $1, 0))";
const DISCOVERY_ACCOUNT_SHARED_LOCK_SQL: &str =
    "SELECT pg_advisory_xact_lock_shared(hashtextextended('jobs-discovery-account:' || $1, 0))";

fn lock_discovery_account_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<()> {
    tx.query_one(DISCOVERY_ACCOUNT_LOCK_SQL, &[&account_id])?;
    Ok(())
}

pub(crate) fn lock_discovery_account_shared_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<()> {
    tx.query_one(DISCOVERY_ACCOUNT_SHARED_LOCK_SQL, &[&account_id])?;
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmploymentEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
    #[serde(default)]
    pub current: bool,
    #[serde(default)]
    pub highlights: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EducationEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub school: String,
    #[serde(default)]
    pub degree: String,
    #[serde(default)]
    pub field: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
    #[serde(default)]
    pub location: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub technologies: Vec<String>,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResumeSourceAsset {
    pub id: String,
    pub file_name: String,
    pub media_type: String,
    pub file_type: String,
    pub storage_key: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub page_count: Option<i64>,
    pub template_status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerProfile {
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub current_location: String,
    #[serde(default)]
    pub street_address: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub linkedin_url: String,
    #[serde(default)]
    pub portfolio_url: String,
    #[serde(default)]
    pub work_authorization: String,
    #[serde(default)]
    pub sponsorship_required: Option<bool>,
    #[serde(default)]
    pub salary_expectation: String,
    #[serde(default)]
    pub notice_period: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub certifications: Vec<String>,
    #[serde(default)]
    pub employment: Vec<EmploymentEntry>,
    #[serde(default)]
    pub education: Vec<EducationEntry>,
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
    #[serde(default)]
    pub reusable_answers: Value,
    #[serde(default)]
    pub source_resume_name: String,
    #[serde(default)]
    pub source_resume_text: String,
    #[serde(default)]
    pub source_resume_asset_id: String,
    #[serde(default)]
    pub source_resume_sha256: String,
    #[serde(default)]
    pub source_resume_media_type: String,
    #[serde(default)]
    pub source_resume_template_status: String,
    #[serde(default = "default_resume_mode")]
    pub resume_mode: String,
    #[serde(default)]
    pub review_new_claims: bool,
    #[serde(default = "default_submission_mode")]
    pub default_submission_mode: String,
    #[serde(default = "default_auto_submit_threshold")]
    pub auto_submit_threshold: i64,
    #[serde(default = "default_daily_limit")]
    pub daily_limit: i64,
    #[serde(default)]
    pub onboarding_step: i64,
    #[serde(default)]
    pub onboarding_complete: bool,
    #[serde(default)]
    pub updated_at_ms: i64,
}

impl Default for CareerProfile {
    fn default() -> Self {
        Self {
            full_name: String::new(),
            email: String::new(),
            phone: String::new(),
            headline: String::new(),
            current_location: String::new(),
            street_address: String::new(),
            summary: String::new(),
            linkedin_url: String::new(),
            portfolio_url: String::new(),
            work_authorization: String::new(),
            sponsorship_required: None,
            salary_expectation: String::new(),
            notice_period: String::new(),
            skills: Vec::new(),
            certifications: Vec::new(),
            employment: Vec::new(),
            education: Vec::new(),
            projects: Vec::new(),
            reusable_answers: json!({}),
            source_resume_name: String::new(),
            source_resume_text: String::new(),
            source_resume_asset_id: String::new(),
            source_resume_sha256: String::new(),
            source_resume_media_type: String::new(),
            source_resume_template_status: String::new(),
            resume_mode: default_resume_mode(),
            review_new_claims: false,
            default_submission_mode: default_submission_mode(),
            auto_submit_threshold: default_auto_submit_threshold(),
            daily_limit: default_daily_limit(),
            onboarding_step: 0,
            onboarding_complete: false,
            updated_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerFact {
    #[serde(default)]
    pub id: String,
    pub category: String,
    pub label: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default = "default_fact_source")]
    pub source: String,
    #[serde(default = "default_verification_status")]
    pub verification_status: String,
    #[serde(default)]
    pub confirmed_at_ms: Option<i64>,
    #[serde(default)]
    pub confirmed_by: Option<String>,
    #[serde(default = "default_schema_version")]
    pub schema_version: i64,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPreferences {
    #[serde(default)]
    pub desired_roles: Vec<String>,
    #[serde(default)]
    pub desired_locations: Vec<String>,
    #[serde(default = "default_location_policy")]
    pub location_policy: String,
    #[serde(default = "default_remote_preference")]
    pub remote_preference: String,
    #[serde(default)]
    pub employment_types: Vec<String>,
    #[serde(default)]
    pub engagement_types: Vec<String>,
    #[serde(default)]
    pub minimum_compensation: Option<i64>,
    #[serde(default = "default_sponsorship_policy")]
    pub sponsorship: String,
    #[serde(default)]
    pub excluded_companies: Vec<String>,
    #[serde(default)]
    pub excluded_titles: Vec<String>,
    #[serde(default = "default_daily_limit")]
    pub daily_limit: i64,
    #[serde(default)]
    pub apply_once_per_company: bool,
    #[serde(default = "default_max_posting_age_days")]
    pub max_posting_age_days: i64,
    #[serde(default)]
    pub time_zone_offset_minutes: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

impl Default for JobPreferences {
    fn default() -> Self {
        Self {
            desired_roles: Vec::new(),
            desired_locations: Vec::new(),
            location_policy: default_location_policy(),
            remote_preference: default_remote_preference(),
            employment_types: vec!["full_time".to_string()],
            engagement_types: Vec::new(),
            minimum_compensation: None,
            sponsorship: default_sponsorship_policy(),
            excluded_companies: Vec::new(),
            excluded_titles: Vec::new(),
            daily_limit: default_daily_limit(),
            apply_once_per_company: true,
            max_posting_age_days: default_max_posting_age_days(),
            time_zone_offset_minutes: 0,
            updated_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerTrackPolicyAuthority {
    #[serde(default)]
    pub taxonomy_version: String,
    #[serde(default)]
    pub taxonomy_sha256: String,
    #[serde(default)]
    pub taxonomy_activation_epoch: i64,
    #[serde(default)]
    pub canonicalizer_schema_version: i64,
    #[serde(default)]
    pub canonicalizer_sha256: String,
    #[serde(default)]
    pub account_input_generation: i64,
    #[serde(default)]
    pub account_input_transition_sha256: String,
    #[serde(default)]
    pub account_input_semantic_sha256: String,
    #[serde(default)]
    pub track_input_generation: i64,
    #[serde(default)]
    pub track_input_transition_sha256: String,
    #[serde(default)]
    pub track_semantic_sha256: String,
    #[serde(default)]
    pub canonical_role_id: String,
    #[serde(default)]
    pub canonical_role_family_id: String,
    #[serde(default)]
    pub canonical_location_ids: Vec<String>,
    #[serde(default)]
    pub source_resume_asset_id: String,
    #[serde(default)]
    pub source_resume_sha256: String,
    #[serde(default)]
    pub application_identity_id: String,
    #[serde(default)]
    pub application_identity_sha256: String,
    #[serde(default)]
    pub job_preferences_sha256: String,
    #[serde(default)]
    pub policy_revision_id: String,
    #[serde(default)]
    pub policy_revision_no: i64,
    #[serde(default)]
    pub canonical_policy_sha256: String,
    #[serde(default)]
    pub policy_head_generation: i64,
    #[serde(default)]
    pub policy_head_transition_sha256: String,
    #[serde(default)]
    pub policy_review_receipt_id: String,
    #[serde(default)]
    pub policy_review_receipt_sha256: String,
    #[serde(default = "default_track_policy_review_state")]
    pub review_state: String,
    #[serde(default)]
    pub review_reason_codes: Vec<String>,
}

fn default_track_policy_review_state() -> String {
    "legacy_unreviewed".to_string()
}

impl Default for CareerTrackPolicyAuthority {
    fn default() -> Self {
        Self {
            taxonomy_version: String::new(),
            taxonomy_sha256: String::new(),
            taxonomy_activation_epoch: 0,
            canonicalizer_schema_version: 0,
            canonicalizer_sha256: String::new(),
            account_input_generation: 0,
            account_input_transition_sha256: String::new(),
            account_input_semantic_sha256: String::new(),
            track_input_generation: 0,
            track_input_transition_sha256: String::new(),
            track_semantic_sha256: String::new(),
            canonical_role_id: String::new(),
            canonical_role_family_id: String::new(),
            canonical_location_ids: Vec::new(),
            source_resume_asset_id: String::new(),
            source_resume_sha256: String::new(),
            application_identity_id: String::new(),
            application_identity_sha256: String::new(),
            job_preferences_sha256: String::new(),
            policy_revision_id: String::new(),
            policy_revision_no: 0,
            canonical_policy_sha256: String::new(),
            policy_head_generation: 0,
            policy_head_transition_sha256: String::new(),
            policy_review_receipt_id: String::new(),
            policy_review_receipt_sha256: String::new(),
            review_state: default_track_policy_review_state(),
            review_reason_codes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerTrackPolicy {
    #[serde(default)]
    pub role_family: String,
    #[serde(default)]
    pub relevant_employment_ids: Vec<String>,
    #[serde(default)]
    pub employment_types: Vec<String>,
    #[serde(default)]
    pub engagement_types: Vec<String>,
    #[serde(default)]
    pub work_authorizations: Vec<String>,
    #[serde(default)]
    pub authority: CareerTrackPolicyAuthority,
}

impl Default for CareerTrackPolicy {
    fn default() -> Self {
        Self {
            role_family: String::new(),
            relevant_employment_ids: Vec::new(),
            employment_types: vec!["full_time".to_string()],
            engagement_types: Vec::new(),
            work_authorizations: Vec::new(),
            authority: CareerTrackPolicyAuthority::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerTrack {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub locations: Vec<String>,
    #[serde(default)]
    pub remote_preference: String,
    #[serde(default)]
    pub application_identity_id: Option<String>,
    #[serde(default)]
    pub policy: CareerTrackPolicy,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default)]
    pub match_count: i64,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoSubmitAuthorization {
    pub id: String,
    pub career_track_id: String,
    pub application_identity_id: String,
    pub source_resume_asset_id: String,
    pub revision_no: i64,
    pub authorized_at_ms: i64,
    #[serde(default)]
    pub revoked_at_ms: Option<i64>,
    #[serde(default)]
    pub status: String,
    #[serde(skip_serializing, default)]
    pub authority_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveryScamSignal {
    pub code: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobDiscoveryEvidence {
    pub provenance: String,
    pub canonical_status: String,
    #[serde(default)]
    pub canonical_job_id: Option<String>,
    pub employer_verification_status: String,
    #[serde(default)]
    pub employer_id: Option<String>,
    #[serde(default)]
    pub canonical_employer_domain: Option<String>,
    #[serde(default)]
    pub application_domain: Option<String>,
    pub scam_risk_status: String,
    #[serde(default)]
    pub scam_signals: Vec<DiscoveryScamSignal>,
    pub original_source_status: String,
    #[serde(default)]
    pub original_source_checked_at_ms: Option<i64>,
    #[serde(default)]
    pub original_source_snapshot_expires_at_ms: Option<i64>,
    #[serde(default)]
    pub original_source_evidence_hash: Option<String>,
    #[serde(default)]
    pub original_source_mismatched_fields: Vec<String>,
    #[serde(default = "default_true")]
    pub requires_original_revalidation: bool,
}

impl Default for JobDiscoveryEvidence {
    fn default() -> Self {
        Self {
            provenance: "unknown".to_string(),
            canonical_status: "unknown".to_string(),
            canonical_job_id: None,
            employer_verification_status: "unknown".to_string(),
            employer_id: None,
            canonical_employer_domain: None,
            application_domain: None,
            scam_risk_status: "unknown".to_string(),
            scam_signals: Vec::new(),
            original_source_status: "unknown".to_string(),
            original_source_checked_at_ms: None,
            original_source_snapshot_expires_at_ms: None,
            original_source_evidence_hash: None,
            original_source_mismatched_fields: Vec::new(),
            requires_original_revalidation: true,
        }
    }
}

impl JobDiscoveryEvidence {
    pub fn external_feed_lead(canonical_job_id: String) -> Self {
        Self {
            provenance: "external_feed".to_string(),
            canonical_status: "canonical".to_string(),
            canonical_job_id: Some(canonical_job_id),
            ..Self::default()
        }
    }

    /// Test-only legacy materialization. Production execution authority must
    /// never be minted from mutable posting JSON.
    #[cfg(test)]
    pub(crate) fn verified_original_source(
        canonical_job_id: String,
        employer_id: String,
        application_domain: Option<String>,
        checked_at_ms: i64,
        evidence_hash: String,
    ) -> Self {
        let mut evidence = Self::provider_verified_original_source(
            canonical_job_id,
            employer_id,
            application_domain,
            checked_at_ms,
            evidence_hash,
        );
        evidence.provenance = "test_fixture_original_source".to_string();
        evidence.employer_verification_status = "verified".to_string();
        evidence.scam_risk_status = "clear".to_string();
        evidence
    }

    /// Record a fresh snapshot from an allowlisted hosted ATS without
    /// overstating independent employer or scam verification. This evidence
    /// is sufficient for a review-first packet, but never for unattended
    /// queueing. Execution-grade source authority is relational and
    /// release-bound; it is not stored in caller-mutable posting JSON.
    pub fn provider_verified_original_source(
        canonical_job_id: String,
        employer_id: String,
        application_domain: Option<String>,
        checked_at_ms: i64,
        evidence_hash: String,
    ) -> Self {
        Self {
            provenance: "original_source".to_string(),
            canonical_status: "canonical".to_string(),
            canonical_job_id: Some(canonical_job_id),
            employer_verification_status: "ats_tenant_verified".to_string(),
            employer_id: Some(employer_id),
            canonical_employer_domain: None,
            application_domain,
            scam_risk_status: "source_screened".to_string(),
            scam_signals: Vec::new(),
            original_source_status: "verified_open".to_string(),
            original_source_checked_at_ms: Some(checked_at_ms),
            original_source_snapshot_expires_at_ms: Some(
                checked_at_ms.saturating_add(24 * 60 * 60 * 1_000),
            ),
            original_source_evidence_hash: Some(evidence_hash),
            original_source_mismatched_fields: Vec::new(),
            requires_original_revalidation: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPosting {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub canonical_key: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub external_id: String,
    pub company: String,
    pub title: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub workplace: String,
    #[serde(default)]
    pub canonical_url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub compensation: String,
    #[serde(default)]
    pub employment_type: String,
    #[serde(default)]
    pub track_id: String,
    #[serde(default)]
    pub match_score: i64,
    #[serde(default)]
    pub matched_reasons: Vec<String>,
    #[serde(default)]
    pub missing_requirements: Vec<String>,
    #[serde(default)]
    pub posted_at_ms: Option<i64>,
    #[serde(default)]
    pub last_verified_at_ms: Option<i64>,
    #[serde(default = "default_active_availability")]
    pub availability_status: String,
    #[serde(default = "default_match_status")]
    pub status: String,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub discovery_evidence: JobDiscoveryEvidence,
    #[serde(default)]
    pub eligibility: Option<JobEligibilityDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverySource {
    pub id: String,
    pub account_id: String,
    #[serde(default)]
    pub track_id: String,
    pub provider: String,
    pub source_key: String,
    pub config: Value,
    pub status: String,
    pub health: String,
    pub consecutive_failures: i64,
    pub run_interval_ms: i64,
    pub next_run_at_ms: i64,
    pub last_success_at_ms: Option<i64>,
    pub last_failure_at_ms: Option<i64>,
    pub last_error_code: Option<String>,
    pub lease_expires_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoverySourceSummaryConfig {
    pub company: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoverySourceSummary {
    pub id: String,
    pub provider: String,
    pub config: DiscoverySourceSummaryConfig,
    pub status: String,
    pub health: String,
    pub last_success_at_ms: Option<i64>,
}

impl From<&DiscoverySource> for DiscoverySourceSummary {
    fn from(source: &DiscoverySource) -> Self {
        Self {
            id: source.id.clone(),
            provider: source.provider.clone(),
            config: DiscoverySourceSummaryConfig {
                company: source
                    .config
                    .get("company")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            },
            status: source.status.clone(),
            health: source.health.clone(),
            last_success_at_ms: source.last_success_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverySourceInput {
    #[serde(default)]
    pub track_id: String,
    pub provider: String,
    pub source_key: String,
    #[serde(default)]
    pub company: String,
    #[serde(default = "default_discovery_interval_ms")]
    pub run_interval_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverySourceLease {
    pub source: DiscoverySource,
    pub lease_token: String,
    pub replay_key: String,
    pub scheduled_for_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredJobInput {
    pub external_id: String,
    pub canonical_url: String,
    pub title: String,
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub source_catalog_id: String,
    #[serde(default)]
    pub requires_original_revalidation: bool,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub workplace: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub compensation: String,
    #[serde(default)]
    pub employment_type: String,
    #[serde(default)]
    pub engagement_type: String,
    #[serde(default)]
    pub posted_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveryRunResult {
    pub run_id: String,
    pub replay_key: String,
    pub status: String,
    pub discovered_count: i64,
    pub upserted_count: i64,
    pub closed_count: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalDiscoverySourceInput {
    pub provider: String,
    pub source_key: String,
    pub source_family: String,
    pub artifact_url: String,
    pub artifact_sha256: String,
    pub expected_rows: i64,
    pub snapshot_at_ms: i64,
    #[serde(default = "default_discovery_interval_ms")]
    pub run_interval_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalDiscoverySource {
    pub id: String,
    pub provider: String,
    pub source_key: String,
    pub config: Value,
    pub status: String,
    pub health: String,
    pub consecutive_failures: i64,
    pub run_interval_ms: i64,
    pub next_run_at_ms: i64,
    pub last_success_at_ms: Option<i64>,
    pub last_failure_at_ms: Option<i64>,
    pub last_error_code: Option<String>,
    pub lease_expires_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalDiscoverySourceLease {
    pub source: GlobalDiscoverySource,
    pub lease_token: String,
    pub replay_key: String,
    pub scheduled_for_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalIngestionBatchInput {
    pub lease_token: String,
    pub replay_key: String,
    pub scheduled_for_ms: i64,
    pub batch_index: i64,
    pub artifact_sha256: String,
    pub jobs: Vec<DiscoveredJobInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalIngestionBatchResult {
    pub run_id: String,
    pub batch_index: i64,
    pub row_count: i64,
    pub received_rows: i64,
    pub received_batches: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalIngestionCompleteInput {
    pub lease_token: String,
    pub replay_key: String,
    pub scheduled_for_ms: i64,
    pub artifact_sha256: String,
    pub expected_rows: i64,
    #[serde(default)]
    pub accepted_rows: Option<i64>,
    #[serde(default)]
    pub rejected_rows: i64,
    #[serde(default)]
    pub rejection_reasons: BTreeMap<String, i64>,
    pub expected_batches: i64,
    #[serde(default)]
    pub complete_snapshot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalIngestionFailureInput {
    pub lease_token: String,
    pub replay_key: String,
    pub scheduled_for_ms: i64,
    pub artifact_sha256: String,
    pub error_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalIngestionRunResult {
    pub run_id: String,
    pub replay_key: String,
    pub status: String,
    pub received_rows: i64,
    pub received_batches: i64,
    pub rejected_rows: i64,
    pub rejection_reasons: BTreeMap<String, i64>,
    pub expired_count: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalMaterializationResult {
    pub considered_count: i64,
    pub materialized_count: i64,
    pub refreshed_count: i64,
    pub skipped_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobDiscoveryAuthority {
    pub source_id: String,
    pub provider: String,
    pub source_status: String,
    pub source_health: String,
    pub membership_status: String,
    pub last_seen_at_ms: i64,
    pub last_seen_run_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EligibilityReason {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExperienceRequirement {
    #[serde(default)]
    pub required_min_months: Option<i64>,
    #[serde(default)]
    pub required_max_months: Option<i64>,
    #[serde(default)]
    pub preferred_min_months: Option<i64>,
    #[serde(default)]
    pub preferred_max_months: Option<i64>,
    #[serde(default)]
    pub title_floor_months: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleExperienceEvidence {
    #[serde(default)]
    pub role_family: String,
    #[serde(default)]
    pub relevant_employment_ids: Vec<String>,
    #[serde(default)]
    pub total_months: i64,
    #[serde(default)]
    pub target_min_months: i64,
    #[serde(default)]
    pub target_max_months: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AtsCertificationSummary {
    pub provider_label: String,
    #[serde(default)]
    pub adapter_version: Option<String>,
    #[serde(default)]
    pub certified_runner_kinds: Vec<String>,
    pub status: String,
    #[serde(default)]
    pub last_verified_at_ms: Option<i64>,
    #[serde(default)]
    pub expires_at_ms: Option<i64>,
    pub reason: String,
    pub next_action: String,
    pub canary_available: bool,
}

impl Default for AtsCertificationSummary {
    fn default() -> Self {
        Self {
            provider_label: "Application site".to_string(),
            adapter_version: None,
            certified_runner_kinds: Vec::new(),
            status: "review_only".to_string(),
            last_verified_at_ms: None,
            expires_at_ms: None,
            reason: "No active ATS certification is available for this exact application target."
                .to_string(),
            next_action: "Review the packet and complete the provider-specific approval."
                .to_string(),
            canary_available: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobEligibilityDecision {
    pub capability: String,
    pub can_prepare: bool,
    pub can_auto_submit: bool,
    pub can_queue_local: bool,
    pub can_queue_cloud: bool,
    #[serde(default)]
    pub hard_failures: Vec<EligibilityReason>,
    #[serde(default)]
    pub review_reasons: Vec<EligibilityReason>,
    #[serde(default)]
    pub passed_checks: Vec<String>,
    #[serde(default)]
    pub base_profile_fit: i64,
    #[serde(default)]
    pub tailored_packet_coverage: Option<i64>,
    #[serde(default)]
    pub experience_requirement: ExperienceRequirement,
    #[serde(default)]
    pub experience_evidence: RoleExperienceEvidence,
    #[serde(default)]
    pub career_track_id: String,
    #[serde(default)]
    pub application_identity_id: Option<String>,
    #[serde(default)]
    pub evidence_revision_id: Option<String>,
    #[serde(default)]
    pub ats_certification: AtsCertificationSummary,
    pub evaluated_at_ms: i64,
}

impl Default for JobEligibilityDecision {
    fn default() -> Self {
        Self {
            capability: "unknown_review".to_string(),
            can_prepare: true,
            can_auto_submit: false,
            can_queue_local: false,
            can_queue_cloud: false,
            hard_failures: Vec::new(),
            review_reasons: Vec::new(),
            passed_checks: Vec::new(),
            base_profile_fit: 0,
            tailored_packet_coverage: None,
            experience_requirement: ExperienceRequirement::default(),
            experience_evidence: RoleExperienceEvidence::default(),
            career_track_id: String::new(),
            application_identity_id: None,
            evidence_revision_id: None,
            ats_certification: AtsCertificationSummary::default(),
            evaluated_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeVersion {
    pub id: String,
    pub job_id: String,
    pub version_no: i64,
    pub mode: String,
    pub content: Value,
    pub diff: Value,
    #[serde(default)]
    pub claim_ids: Vec<String>,
    pub checksum: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileEvidenceRevision {
    pub id: String,
    pub career_track_id: String,
    #[serde(default)]
    pub revision_no: i64,
    pub content_hash: String,
    pub snapshot: Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeClaimEvidence {
    pub id: String,
    pub resume_version_id: String,
    pub claim_id: String,
    pub evidence_revision_id: String,
    #[serde(default)]
    pub source_ids: Vec<String>,
    pub claim: Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobApplication {
    #[serde(default)]
    pub id: String,
    pub job_id: String,
    #[serde(default)]
    pub resume_version_id: Option<String>,
    #[serde(default = "default_application_state")]
    pub state: String,
    #[serde(default = "default_submission_mode")]
    pub submission_mode: String,
    #[serde(default)]
    pub match_score: i64,
    #[serde(default)]
    pub answers: Vec<Value>,
    #[serde(default)]
    pub cover_letter: String,
    #[serde(default)]
    pub receipt: Value,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub submitted_at_ms: Option<i64>,
}

/// An application packet assembled in memory while its resume generation is
/// pending. The candidate is deliberately not persisted: callers continue to
/// see the last committed packet until `finalize_prepared_application` commits
/// both the resume version and this application with an optimistic fence.
#[derive(Debug, Clone)]
pub struct PreparedApplicationDraft {
    pub application: JobApplication,
    pub baseline_resume: ResumeVersion,
    /// Exact candidate snapshot used to build `baseline_resume` and its truth
    /// fingerprint. Callers must use this snapshot for any async generation.
    pub profile: CareerProfile,
    /// Confirmed facts frozen with the candidate snapshot.
    pub facts: Vec<CareerFact>,
    /// Active Career Track selected for this exact job.
    pub track: CareerTrack,
    /// Verified application identity bound to `track`.
    pub identity: ApplicationIdentity,
    /// Immutable candidate/Track/identity snapshot referenced by every claim.
    pub evidence_revision: ProfileEvidenceRevision,
    /// Exact posting snapshot used for tailoring, generation, and the frozen
    /// application receipt. Finalization rejects a concurrent posting refresh.
    pub posting: JobPosting,
    expected_application: Option<ExpectedApplicationRevision>,
}

#[derive(Debug, Clone)]
struct ExpectedApplicationRevision {
    id: String,
    state: String,
    updated_at_ms: i64,
    payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSession {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub runner: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub current_company: String,
    #[serde(default)]
    pub current_step: String,
    #[serde(default)]
    pub application_id: Option<String>,
    #[serde(default)]
    pub takeover_url: Option<String>,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intervention {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub application_id: Option<String>,
    pub kind: String,
    #[serde(default = "default_open_status")]
    pub status: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub resolution_kind: String,
    #[serde(default)]
    pub resume_after_resolution: bool,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub provider_message_id: String,
    #[serde(default)]
    pub expires_at_ms: Option<i64>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub resolved_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerMemory {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub key: String,
    pub question: String,
    pub value: String,
    pub scope: String,
    #[serde(default)]
    pub scope_id: Option<String>,
    #[serde(default = "default_true")]
    pub confirmed: bool,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub last_used_at_ms: Option<i64>,
    #[serde(default)]
    pub use_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateEvent {
    #[serde(default)]
    pub id: String,
    pub event_type: String,
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub application_id: Option<String>,
    pub action: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationEvidence {
    #[serde(default)]
    pub id: String,
    pub application_id: String,
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub storage_key: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub resume_version_id: Option<String>,
    #[serde(default)]
    pub occurred_at_ms: i64,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub enum SubmissionFinalizeResult {
    Committed(JobApplication),
    Replayed(JobApplication),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobsIntegration {
    #[serde(default)]
    pub id: String,
    pub provider: String,
    #[serde(default = "default_disconnected_status")]
    pub status: String,
    #[serde(default)]
    pub account_label: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationIdentity {
    #[serde(default)]
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_pending_status")]
    pub verification_status: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxConnection {
    #[serde(default)]
    pub id: String,
    pub provider: String,
    #[serde(default = "default_pending_status")]
    pub status: String,
    #[serde(default)]
    pub account_label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobsOAuthState {
    pub provider: String,
    pub code_verifier: String,
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub authorization_purpose: String,
    #[serde(default)]
    pub requested_scopes: Vec<String>,
    #[serde(default)]
    pub requested_capabilities: Vec<String>,
    #[serde(default)]
    pub expected_grant_revision: i64,
    #[serde(default)]
    pub return_path: String,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobsProviderCredential {
    pub connection_id: String,
    pub provider: String,
    pub provider_subject: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub grant_revision: i64,
    #[serde(default)]
    pub grant_sha256: String,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobsProviderSyncState {
    pub connection_id: String,
    pub provider: String,
    #[serde(default)]
    pub cursor: Value,
    #[serde(default)]
    pub next_sync_at_ms: i64,
    #[serde(default)]
    pub last_synced_at_ms: Option<i64>,
    #[serde(default)]
    pub last_error: String,
    #[serde(default)]
    pub lease_owner: Option<String>,
    #[serde(default)]
    pub lease_expires_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobsProviderMessage {
    #[serde(default)]
    pub id: String,
    pub connection_id: String,
    pub provider: String,
    pub external_id: String,
    #[serde(default)]
    pub sender: String,
    #[serde(default)]
    pub recipients: Vec<String>,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub body_text: String,
    pub received_at_ms: i64,
    #[serde(default)]
    pub application_id: Option<String>,
    #[serde(default)]
    pub processing_status: String,
    #[serde(default)]
    pub classification: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub processed_at_ms: Option<i64>,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobsCommunicationAction {
    #[serde(default)]
    pub id: String,
    pub application_id: String,
    pub connection_id: String,
    #[serde(default)]
    pub source_message_id: Option<String>,
    pub kind: String,
    pub provider: String,
    pub idempotency_key: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub payload_sha256: String,
    #[serde(default)]
    pub authority_sha256: String,
    #[serde(default = "default_pending_status")]
    pub status: String,
    #[serde(default)]
    pub provider_object_id: String,
    #[serde(default)]
    pub lease_owner: Option<String>,
    #[serde(default)]
    pub lease_kind: Option<String>,
    #[serde(default)]
    pub lease_expires_at_ms: Option<i64>,
    #[serde(default)]
    pub active_attempt_id: Option<String>,
    #[serde(default)]
    pub next_attempt_at_ms: i64,
    #[serde(default)]
    pub attempt_count: i64,
    #[serde(default)]
    pub reconciliation_count: i64,
    #[serde(default = "default_action_revision")]
    pub action_revision: i64,
    #[serde(default)]
    pub approval_revision: i64,
    #[serde(default)]
    pub approved_authority_sha256: String,
    #[serde(default)]
    pub approved_grant_revision: i64,
    #[serde(default)]
    pub approved_grant_sha256: String,
    #[serde(default)]
    pub approved_at_ms: Option<i64>,
    #[serde(default)]
    pub dispatched_at_ms: Option<i64>,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

fn default_action_revision() -> i64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobsCommunicationActionLease {
    pub account_id: String,
    pub action: JobsCommunicationAction,
    pub attempt_id: String,
    pub lease_kind: String,
    pub provider_operation_key: String,
    pub authority_sha256: String,
    pub approval_revision: i64,
    pub grant_revision: i64,
    pub grant_sha256: String,
    pub lease_token: String,
    pub fence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobsCommunicationLeaseAccess {
    pub account_id: String,
    pub action_id: String,
    pub attempt_id: String,
    pub owner_id: String,
    pub lease_token: String,
    pub fence: i64,
    pub authority_sha256: String,
    pub approval_revision: i64,
    pub grant_revision: i64,
    pub grant_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobsCommunicationActionFinish {
    #[serde(flatten)]
    pub lease: JobsCommunicationLeaseAccess,
    pub outcome: String,
    #[serde(default)]
    pub provider_object_id: String,
    #[serde(default)]
    pub evidence: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobsCommunicationActionReconciliation {
    #[serde(flatten)]
    pub lease: JobsCommunicationLeaseAccess,
    pub resolution: String,
    #[serde(default)]
    pub provider_object_id: String,
    #[serde(default)]
    pub evidence: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobsCommunicationAttemptEvidence {
    pub id: String,
    pub action_id: String,
    pub provider: String,
    pub provider_operation_key: String,
    pub dispatch_no: i64,
    pub approval_revision: i64,
    pub authority_sha256: String,
    pub grant_revision: i64,
    pub grant_sha256: String,
    pub state: String,
    #[serde(default)]
    pub provider_object_id: String,
    #[serde(default)]
    pub evidence: Value,
    pub evidence_sha256: String,
    pub request_started_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobsCommunicationReconciliationEvidence {
    pub id: String,
    pub action_id: String,
    pub attempt_id: String,
    pub fence: i64,
    pub resolution: String,
    #[serde(default)]
    pub provider_object_id: String,
    #[serde(default)]
    pub evidence: Value,
    pub evidence_sha256: String,
    pub recorded_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobsCommunicationActionExport {
    pub id: String,
    pub application_id: String,
    pub connection_id: String,
    pub source_message_id: Option<String>,
    pub kind: String,
    pub provider: String,
    pub payload: Value,
    pub status: String,
    pub action_revision: i64,
    pub approval_revision: i64,
    pub approved_at_ms: Option<i64>,
    pub dispatched_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobsProviderMessageExport {
    pub id: String,
    pub connection_id: String,
    pub provider: String,
    pub sender: String,
    pub recipients: Vec<String>,
    pub subject: String,
    pub body_text: String,
    pub received_at_ms: i64,
    pub application_id: Option<String>,
    pub processing_status: String,
    pub classification: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobsCommunicationEvidenceExport {
    pub action_id: String,
    pub event_kind: String,
    pub recorded_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobsCommunicationReconciliationExport {
    pub action_id: String,
    pub resolution: String,
    pub recorded_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobsEntitlement {
    pub plan: String,
    pub track_limit: i64,
    pub monthly_packet_limit: i64,
    pub used_packets: i64,
    pub period_start_ms: i64,
    pub period_end_ms: i64,
    pub local_browser: bool,
    pub cloud_browser: bool,
    pub overage_cents: i64,
    pub monthly_price_cents: i64,
    pub application_identity_limit: i64,
    pub connected_inbox_limit: i64,
    pub additional_inbox_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalBrowserReleaseArtifact {
    pub platform: String,
    pub architecture: String,
    pub package_kind: String,
    pub role: String,
    pub file_name: String,
    pub url: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub descriptor_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LocalBrowserReleaseAvailability {
    Available {
        reason: String,
        channel: String,
        release_id: String,
        artifact_origin: String,
        manifest_sha256: String,
        release_sequence: i64,
        build_id: String,
        app_version: String,
        protocol_version: i64,
        released_at_ms: i64,
        artifacts: Vec<LocalBrowserReleaseArtifact>,
    },
    Disabled {
        reason: String,
    },
    Unassigned {
        reason: String,
    },
    Unavailable {
        reason: String,
    },
}

impl LocalBrowserReleaseAvailability {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunnerChannelAvailability {
    pub status: String,
    pub available: bool,
    pub plan_included: bool,
    pub distribution_enabled: bool,
    pub reason: String,
    pub next_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<LocalBrowserReleaseAvailability>,
}

impl Default for RunnerChannelAvailability {
    fn default() -> Self {
        Self {
            status: "invited_beta".to_string(),
            available: false,
            plan_included: false,
            distribution_enabled: false,
            reason: "This runner is not available for this account.".to_string(),
            next_action: "Use Review first.".to_string(),
            release: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunnerAvailability {
    pub local: RunnerChannelAvailability,
    pub cloud: RunnerChannelAvailability,
    pub auto_submit_available: bool,
    pub auto_submit_reason: String,
}

impl Default for RunnerAvailability {
    fn default() -> Self {
        Self {
            local: RunnerChannelAvailability::default(),
            cloud: RunnerChannelAvailability::default(),
            auto_submit_available: false,
            auto_submit_reason:
                "Auto-submit is unavailable because no Bluey runner is available for this account."
                    .to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub id: String,
    pub run_id: String,
    pub event_type: String,
    pub event: Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct LocalRunTicket {
    pub id: String,
    pub account_id: String,
    pub application_id: String,
    pub ticket_hash: String,
    pub ticket_secret: String,
    pub payload: Value,
    pub status: String,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalRunAuthorityPhase {
    Claim,
    Submit,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalRunResumeAction {
    pub run_id: String,
    pub intervention_id: String,
    pub action: String,
    pub expires_at_ms: i64,
    #[serde(skip_serializing)]
    pub account_id: String,
    #[serde(skip_serializing)]
    pub application_id: String,
    #[serde(skip_serializing)]
    pub first_consumption: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExecutionLeaseGrant {
    pub run_id: String,
    pub lease_token: String,
    pub fence: i64,
    pub lease_expires_at_ms: i64,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RunnerVolumeExecutionLeaseGrant {
    #[serde(flatten)]
    pub lease: ExecutionLeaseGrant,
    pub purge_subject: String,
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub volume_key_fingerprint: String,
    pub runtime_grant_id: String,
    pub runtime_sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExecutionLeaseRecord {
    pub run_id: String,
    pub fence: i64,
    pub lease_expires_at_ms: i64,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BrowserProfileSnapshotRecord {
    pub browser_profile_id: String,
    pub generation: i64,
    pub sha256: String,
    pub size_bytes: i64,
    pub envelope_version: i64,
    pub writer_run_id: String,
    pub writer_fence: i64,
    pub updated_at_ms: i64,
    #[serde(skip_serializing)]
    pub object_key: String,
}

#[derive(Debug, Error)]
pub enum ExecutionLeaseError {
    #[error("Invalid execution lease request.")]
    InvalidRequest,
    #[error("Execution lease target not found.")]
    NotFound,
    #[error("Execution lease is not available.")]
    Conflict,
    #[error("execution lease storage failed")]
    Storage(#[source] anyhow::Error),
}

impl From<anyhow::Error> for ExecutionLeaseError {
    fn from(error: anyhow::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<rusqlite::Error> for ExecutionLeaseError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.into())
    }
}

impl From<postgres::Error> for ExecutionLeaseError {
    fn from(error: postgres::Error) -> Self {
        Self::Storage(error.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobsWorkspace {
    pub profile: CareerProfile,
    pub preferences: JobPreferences,
    pub facts: Vec<CareerFact>,
    pub tracks: Vec<CareerTrack>,
    pub matches: Vec<JobPosting>,
    pub applications: Vec<JobApplication>,
    pub application_evidence: Vec<ApplicationEvidence>,
    pub browser_sessions: Vec<BrowserSession>,
    pub interventions: Vec<Intervention>,
    pub answer_memory: Vec<AnswerMemory>,
    pub candidate_events: Vec<CandidateEvent>,
    pub integrations: Vec<JobsIntegration>,
    pub application_identities: Vec<ApplicationIdentity>,
    #[serde(default)]
    pub auto_submit_authorizations: Vec<AutoSubmitAuthorization>,
    pub mailbox_connections: Vec<MailboxConnection>,
    pub discovery_sources: Vec<DiscoverySourceSummary>,
    pub entitlement: JobsEntitlement,
    #[serde(default)]
    pub runner_availability: RunnerAvailability,
}

/// User-exportable Jobs data. Ephemeral local-run tickets and their secrets are
/// deliberately excluded; the durable application packet and receipt are
/// already represented by the workspace and resume versions.
#[derive(Debug, Clone, Serialize)]
pub struct JobsAccountExport {
    pub workspace: JobsWorkspace,
    pub canonical_track_policy_ledger: CanonicalTrackPolicyLedgerExport,
    pub resume_versions: Vec<ResumeVersion>,
    pub attempt_reservations: Vec<AttemptReservation>,
    pub run_events: Vec<RunEvent>,
    pub provider_messages: Vec<JobsProviderMessageExport>,
    pub communication_actions: Vec<JobsCommunicationActionExport>,
    pub communication_evidence: Vec<JobsCommunicationEvidenceExport>,
    pub communication_reconciliations: Vec<JobsCommunicationReconciliationExport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PacketCommitResult {
    pub newly_metered: bool,
    pub included: bool,
    pub amount_cents: i64,
    pub used_packets: i64,
    pub monthly_packet_limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttemptReservation {
    pub id: String,
    pub application_id: String,
    pub company_key: String,
    pub period_key: String,
    pub runner: String,
    pub status: String,
    pub reserved_at_ms: i64,
    pub updated_at_ms: i64,
}

fn default_resume_mode() -> String {
    "factual".to_string()
}

fn default_submission_mode() -> String {
    "review_first".to_string()
}

fn default_auto_submit_threshold() -> i64 {
    80
}

fn default_daily_limit() -> i64 {
    10
}

fn default_max_posting_age_days() -> i64 {
    14
}

fn default_location_policy() -> String {
    "ask".to_string()
}

fn default_remote_preference() -> String {
    "hybrid_ok".to_string()
}

fn default_sponsorship_policy() -> String {
    "ask".to_string()
}

fn default_fact_source() -> String {
    "user_entry".to_string()
}

fn default_verification_status() -> String {
    "unverified".to_string()
}

fn default_schema_version() -> i64 {
    1
}

fn default_true() -> bool {
    true
}

fn default_match_status() -> String {
    "matched".to_string()
}

fn default_active_availability() -> String {
    "active".to_string()
}

fn default_application_state() -> String {
    "matched".to_string()
}

fn default_open_status() -> String {
    "open".to_string()
}

fn default_disconnected_status() -> String {
    "disconnected".to_string()
}

fn default_pending_status() -> String {
    "pending".to_string()
}

pub(crate) fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, Copy)]
struct JobsPlanPolicy {
    monthly_price_cents: i64,
    track_limit: i64,
    packet_limit: i64,
    application_identity_limit: i64,
    connected_inbox_limit: i64,
    local_browser: bool,
    cloud_browser: bool,
}

fn plan_policy(plan: &str) -> JobsPlanPolicy {
    match plan {
        "pro" => JobsPlanPolicy {
            monthly_price_cents: 2_900,
            track_limit: 3,
            packet_limit: 50,
            application_identity_limit: 10,
            connected_inbox_limit: 2,
            local_browser: true,
            cloud_browser: false,
        },
        "cloud" => JobsPlanPolicy {
            monthly_price_cents: 4_900,
            track_limit: 5,
            packet_limit: 100,
            application_identity_limit: 25,
            connected_inbox_limit: 5,
            local_browser: true,
            cloud_browser: true,
        },
        _ => JobsPlanPolicy {
            monthly_price_cents: 0,
            track_limit: 1,
            packet_limit: 5,
            application_identity_limit: 2,
            connected_inbox_limit: 1,
            local_browser: false,
            cloud_browser: false,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn entitlement_with_policy(
    plan: String,
    track_limit: i64,
    monthly_packet_limit: i64,
    used_packets: i64,
    period_start_ms: i64,
    period_end_ms: i64,
    local_browser: bool,
    cloud_browser: bool,
) -> JobsEntitlement {
    let policy = plan_policy(&plan);
    JobsEntitlement {
        plan,
        track_limit,
        monthly_packet_limit,
        used_packets,
        period_start_ms,
        period_end_ms,
        local_browser,
        cloud_browser,
        overage_cents: PACKET_OVERAGE_CENTS,
        monthly_price_cents: policy.monthly_price_cents,
        application_identity_limit: policy.application_identity_limit,
        connected_inbox_limit: policy.connected_inbox_limit,
        additional_inbox_cents: ADDITIONAL_INBOX_CENTS,
    }
}

pub fn normalize_application_email(email: &str) -> Result<String> {
    let normalized = email.trim().to_ascii_lowercase();
    let mut pieces = normalized.split('@');
    let local = pieces.next().unwrap_or_default();
    let domain = pieces.next().unwrap_or_default();
    if local.is_empty()
        || domain.is_empty()
        || pieces.next().is_some()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
    {
        anyhow::bail!("enter a complete email address")
    }
    Ok(normalized)
}

fn private_lookup_hash(scope: &str, value: &str) -> Result<String> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&jobs_data_key()?)
        .map_err(|_| anyhow::anyhow!("invalid Bluey Jobs data key"))?;
    mac.update(scope.as_bytes());
    mac.update(&[0]);
    mac.update(value.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn parse_json<T: DeserializeOwned>(raw: String, label: &str) -> Result<T> {
    let plain = decrypt_payload(&raw).with_context(|| format!("decrypt {label}"))?;
    serde_json::from_str(&plain).with_context(|| format!("parse {label}"))
}

fn parse_application_json(
    raw: String,
    authoritative_id: &str,
    authoritative_job_id: &str,
    label: &str,
) -> Result<JobApplication> {
    let mut application: JobApplication = parse_json(raw, label)?;
    application.id = authoritative_id.to_string();
    application.job_id = authoritative_job_id.to_string();
    Ok(application)
}

fn to_json<T: Serialize>(value: &T, label: &str) -> Result<String> {
    let plain = serde_json::to_string(value).with_context(|| format!("serialize {label}"))?;
    encrypt_payload(&plain).with_context(|| format!("encrypt {label}"))
}

pub fn validate_data_encryption_config() -> Result<()> {
    jobs_data_key().map(|_| ())
}

fn jobs_data_key() -> Result<[u8; 32]> {
    if let Ok(raw) = std::env::var("BLUEY_JOBS_DATA_KEY") {
        let value = raw.trim();
        let decoded = if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            hex::decode(value).context("decode hex BLUEY_JOBS_DATA_KEY")?
        } else {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(value)
                .or_else(|_| base64::engine::general_purpose::STANDARD.decode(value))
                .context("decode base64 BLUEY_JOBS_DATA_KEY")?
        };
        return decoded
            .try_into()
            .map_err(|_| anyhow::anyhow!("BLUEY_JOBS_DATA_KEY must decode to 32 bytes"));
    }
    if cfg!(debug_assertions) {
        return Ok(Sha256::digest(b"bluey-jobs-development-only-key").into());
    }
    anyhow::bail!("BLUEY_JOBS_DATA_KEY is required when Bluey Jobs is enabled")
}

fn encrypt_payload(plain: &str) -> Result<String> {
    let cipher = Aes256Gcm::new_from_slice(&jobs_data_key()?)
        .map_err(|_| anyhow::anyhow!("invalid Bluey Jobs data key"))?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plain.as_bytes())
        .map_err(|_| anyhow::anyhow!("Bluey Jobs payload encryption failed"))?;
    let mut envelope = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    envelope.extend_from_slice(&nonce_bytes);
    envelope.extend_from_slice(&ciphertext);
    Ok(format!(
        "{ENCRYPTED_PAYLOAD_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope)
    ))
}

fn decrypt_payload(raw: &str) -> Result<String> {
    let Some(encoded) = raw.strip_prefix(ENCRYPTED_PAYLOAD_PREFIX) else {
        return Ok(raw.to_string());
    };
    let envelope = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .context("decode Bluey Jobs payload")?;
    if envelope.len() <= 12 {
        anyhow::bail!("Bluey Jobs payload is truncated");
    }
    let (nonce, ciphertext) = envelope.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(&jobs_data_key()?)
        .map_err(|_| anyhow::anyhow!("invalid Bluey Jobs data key"))?;
    let plain = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow::anyhow!("Bluey Jobs payload authentication failed"))?;
    String::from_utf8(plain).context("Bluey Jobs payload is not UTF-8")
}

#[cfg(test)]
pub(crate) fn decrypt_payload_for_test(raw: &str) -> Result<String> {
    decrypt_payload(raw)
}

fn parse_json_lossy<T: DeserializeOwned>(raw: &str) -> Option<T> {
    decrypt_payload(raw)
        .ok()
        .and_then(|plain| serde_json::from_str(&plain).ok())
}

include!("jobs/candidate_policy.rs");
include!("jobs/taxonomy_policy.rs");
include!("jobs/resume_truth.rs");
include!("jobs/evidence.rs");
include!("jobs/profile_postings.rs");
include!("jobs/discovery.rs");
include!("jobs/global_discovery.rs");
include!("jobs/global_discovery_completion.rs");
include!("jobs/global_materialization.rs");
include!("jobs/global_archive.rs");
include!("jobs/resume_assets.rs");
include!("jobs/auto_submit.rs");
include!("jobs/operational_holds.rs");
include!("jobs/eligibility.rs");
include!("jobs/applications.rs");
include!("jobs/customer_data.rs");
include!("jobs/submission_reconciliation.rs");
include!("jobs/mailbox_sync.rs");
include!("jobs/communication_actions.rs");
include!("jobs/workflow_commands.rs");
include!("jobs/workflow_cleanup.rs");
include!("jobs/execution_authority.rs");
include!("jobs/local_runner.rs");
include!("jobs/execution_leases.rs");
include!("jobs/runner_volume_purge.rs");
include!("jobs/browser_release_authority.rs");
include!("jobs/browser_release_trust.rs");
include!("jobs/browser_release_registry.rs");
include!("jobs/ats_certification_authority.rs");
include!("jobs/managed_cloud_release_authority.rs");
include!("jobs/original_source_verification.rs");
include!("jobs/job_integrity_authority.rs");
include!("jobs/job_integrity_composition.rs");
include!("jobs/browser_profile_snapshots.rs");
include!("jobs/workspace.rs");

#[cfg(any(test, feature = "integration-test-support"))]
#[cfg_attr(feature = "integration-test-support", allow(dead_code))]
#[path = "jobs/production_positive_authority_fixture.rs"]
pub(crate) mod production_positive_authority_fixture;

#[cfg(feature = "integration-test-support")]
#[doc(hidden)]
pub struct IntegrationTestProductionPositiveJobAuthoritiesRequest<'a> {
    pub pool: &'a DbPool,
    pub account_id: &'a str,
    pub posting: &'a JobPosting,
    pub profile: &'a CareerProfile,
    pub preferences: &'a JobPreferences,
    pub canonical_employer_domain: &'a str,
    pub suffix: &'a str,
    pub runner_kind: &'a str,
}

#[cfg(feature = "integration-test-support")]
#[doc(hidden)]
pub fn install_integration_test_production_positive_job_authorities(
    request: IntegrationTestProductionPositiveJobAuthoritiesRequest<'_>,
) -> JobPosting {
    let (saved, managed) =
        production_positive_authority_fixture::save_production_positive_verified_import(
            request.pool,
            request.account_id,
            request.posting,
            request.profile,
            request.preferences,
        );
    let installed =
        production_positive_authority_fixture::install_production_positive_job_authorities_for_runner(
            request.pool,
            request.account_id,
            &saved,
            &managed,
            request.canonical_employer_domain,
            request.suffix,
            request.runner_kind,
        );
    assert!(installed.source.feature_active);
    assert!(installed.source.integrity_binding.is_some());
    assert_eq!(installed.ats.status.status, "active");
    assert!(installed.ats.active_binding.is_some());
    assert_eq!(
        installed.composed.job_integrity.status,
        JobIntegrityResolutionStatus::Verified
    );
    installed.posting
}

#[cfg(test)]
#[path = "jobs/postgres_local_authority_tests.rs"]
mod postgres_local_authority_tests;

include!("jobs/tests.rs");
