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
type HmacSha256 = Hmac<Sha256>;

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

fn lock_discovery_account_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<()> {
    tx.query_one(DISCOVERY_ACCOUNT_LOCK_SQL, &[&account_id])?;
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
    #[serde(default)]
    pub remote_preference: String,
    #[serde(default)]
    pub employment_types: Vec<String>,
    #[serde(default)]
    pub engagement_types: Vec<String>,
    #[serde(default)]
    pub minimum_compensation: Option<i64>,
    #[serde(default)]
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
            remote_preference: "hybrid_ok".to_string(),
            employment_types: vec!["full_time".to_string()],
            engagement_types: Vec::new(),
            minimum_compensation: None,
            sponsorship: "ask".to_string(),
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
}

impl Default for CareerTrackPolicy {
    fn default() -> Self {
        Self {
            role_family: String::new(),
            relevant_employment_ids: Vec::new(),
            employment_types: vec!["full_time".to_string()],
            engagement_types: Vec::new(),
            work_authorizations: Vec::new(),
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
pub struct RunnerChannelAvailability {
    pub status: String,
    pub available: bool,
    pub plan_included: bool,
    pub distribution_enabled: bool,
    pub reason: String,
    pub next_action: String,
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
pub struct ExecutionLeaseRecord {
    pub run_id: String,
    pub fence: i64,
    pub lease_expires_at_ms: i64,
    pub phase: String,
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
    pub resume_versions: Vec<ResumeVersion>,
    pub attempt_reservations: Vec<AttemptReservation>,
    pub run_events: Vec<RunEvent>,
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

fn parse_json_lossy<T: DeserializeOwned>(raw: &str) -> Option<T> {
    decrypt_payload(raw)
        .ok()
        .and_then(|plain| serde_json::from_str(&plain).ok())
}

include!("jobs/candidate_policy.rs");
include!("jobs/resume_truth.rs");
include!("jobs/evidence.rs");
include!("jobs/profile_postings.rs");
include!("jobs/discovery.rs");
include!("jobs/global_discovery.rs");
include!("jobs/global_discovery_completion.rs");
include!("jobs/global_materialization.rs");
include!("jobs/resume_assets.rs");
include!("jobs/eligibility.rs");
include!("jobs/applications.rs");
include!("jobs/customer_data.rs");
include!("jobs/execution_authority.rs");
include!("jobs/local_runner.rs");
include!("jobs/execution_leases.rs");
include!("jobs/workspace.rs");

#[cfg(test)]
#[path = "jobs/postgres_local_authority_tests.rs"]
mod postgres_local_authority_tests;

include!("jobs/tests.rs");
