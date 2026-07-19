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
/// A guardrail against a single account repeatedly enrolling near-identical
/// public boards. Boards are intentionally one-to-one with a Career Track.
pub const DISCOVERY_MAX_SOURCES_PER_TRACK: usize = 8;
pub const DISCOVERY_MAX_SOURCES_PER_ACCOUNT: usize = 24;
const DISCOVERY_LEASE_MS: i64 = 2 * 60 * 1_000;
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
    pub location: String,
    #[serde(default)]
    pub workplace: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub compensation: String,
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

pub fn canonical_job_key(posting: &JobPosting) -> String {
    let normalized = format!(
        "{}|{}|{}|{}",
        posting.company.trim().to_lowercase(),
        posting.title.trim().to_lowercase(),
        posting.location.trim().to_lowercase(),
        canonical_url_for_job_key(&posting.canonical_url)
    );
    hex::encode(Sha256::digest(normalized.as_bytes()))
}

/// Keep public-job identity stable when an ATS link differs only by common
/// attribution parameters. Preserve other query parameters because some ATSs
/// use them to select a genuine job variant.
fn canonical_url_for_job_key(raw: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(raw.trim()) else {
        return raw.trim().trim_end_matches('/').to_lowercase();
    };
    let mut retained_query = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_query_key(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    retained_query.sort();
    url.set_query(None);
    if !retained_query.is_empty() {
        let mut query = url.query_pairs_mut();
        for (key, value) in retained_query {
            query.append_pair(&key, &value);
        }
    }
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(if path.is_empty() { "/" } else { &path });
    url.to_string().trim_end_matches('/').to_lowercase()
}

fn is_tracking_query_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "gh_src" | "lever-source" | "source" | "ref" | "referrer"
    ) || key.to_ascii_lowercase().starts_with("utm_")
}

pub fn default_profile(email: &str) -> CareerProfile {
    CareerProfile {
        email: email.to_string(),
        ..CareerProfile::default()
    }
}

pub fn get_profile(pool: &DbPool, account_id: &str, email: &str) -> Result<CareerProfile> {
    let mut profile = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "Jobs profile"))
                .transpose()
                .map(|value| value.unwrap_or_else(|| default_profile(email)))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "SELECT profile_json FROM jobs_profiles WHERE account_id = $1",
                &[&account_id],
            )?;
            row.map(|value| parse_json(value.get(0), "Jobs profile"))
                .transpose()
                .map(|value| value.unwrap_or_else(|| default_profile(email)))
        }
    })?;
    profile.auto_submit_threshold = default_auto_submit_threshold();
    profile.daily_limit = default_daily_limit();
    Ok(profile)
}

pub fn save_profile(
    pool: &DbPool,
    account_id: &str,
    profile: &CareerProfile,
) -> Result<CareerProfile> {
    let mut value = profile.clone();
    value.onboarding_step = value.onboarding_step.clamp(0, 6);
    value.auto_submit_threshold = default_auto_submit_threshold();
    value.daily_limit = default_daily_limit();
    value.updated_at_ms = now_ms();
    let payload = to_json(&value, "Jobs profile")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO jobs_profiles (
                    account_id, profile_json, onboarding_step, onboarding_complete,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(account_id) DO UPDATE SET
                    profile_json = excluded.profile_json,
                    onboarding_step = excluded.onboarding_step,
                    onboarding_complete = excluded.onboarding_complete,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    account_id,
                    payload,
                    value.onboarding_step,
                    i64::from(value.onboarding_complete),
                    value.updated_at_ms
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let onboarding_complete = i32::from(value.onboarding_complete);
            conn.execute(
                "INSERT INTO jobs_profiles (
                    account_id, profile_json, onboarding_step, onboarding_complete,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $5)
                 ON CONFLICT(account_id) DO UPDATE SET
                    profile_json = EXCLUDED.profile_json,
                    onboarding_step = EXCLUDED.onboarding_step,
                    onboarding_complete = EXCLUDED.onboarding_complete,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[
                    &account_id,
                    &payload,
                    &value.onboarding_step,
                    &onboarding_complete,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn list_facts(pool: &DbPool, account_id: &str) -> Result<Vec<CareerFact>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, category, label, value_json, source, verification_status,
                        confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
                   FROM jobs_facts WHERE account_id = ?1
                  ORDER BY category, updated_at_ms DESC",
            )?;
            let facts = stmt
                .query_map(params![account_id], fact_from_sqlite_row)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("list Jobs facts")?;
            Ok(facts)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.query(
                "SELECT id, category, label, value_json, source, verification_status,
                        confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
                   FROM jobs_facts WHERE account_id = $1
                  ORDER BY category, updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(fact_from_pg_row)
            .collect()
        }
    })
}

fn fact_from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CareerFact> {
    let raw: String = row.get(3)?;
    Ok(CareerFact {
        id: row.get(0)?,
        category: row.get(1)?,
        label: row.get(2)?,
        value: parse_json_lossy(&raw).unwrap_or(Value::Null),
        source: row.get(4)?,
        verification_status: row.get(5)?,
        confirmed_at_ms: row.get(6)?,
        confirmed_by: row.get(7)?,
        schema_version: row.get(8)?,
        created_at_ms: row.get(9)?,
        updated_at_ms: row.get(10)?,
    })
}

fn fact_from_pg_row(row: postgres::Row) -> Result<CareerFact> {
    let raw: String = row.get(3);
    Ok(CareerFact {
        id: row.get(0),
        category: row.get(1),
        label: row.get(2),
        value: parse_json(raw, "Jobs fact value")?,
        source: row.get(4),
        verification_status: row.get(5),
        confirmed_at_ms: row.get(6),
        confirmed_by: row.get(7),
        schema_version: row.get(8),
        created_at_ms: row.get(9),
        updated_at_ms: row.get(10),
    })
}

pub fn upsert_fact(pool: &DbPool, account_id: &str, fact: &CareerFact) -> Result<CareerFact> {
    let mut value = fact.clone();
    if value.id.trim().is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    if value.verification_status == "confirmed" && value.confirmed_at_ms.is_none() {
        value.confirmed_at_ms = Some(now);
        value.confirmed_by = Some("user".to_string());
    }
    let payload = to_json(&value.value, "Jobs fact value")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO jobs_facts (
                    id, account_id, category, label, value_json, source,
                    verification_status, confirmed_at_ms, confirmed_by,
                    schema_version, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET
                    category = excluded.category,
                    label = excluded.label,
                    value_json = excluded.value_json,
                    source = excluded.source,
                    verification_status = excluded.verification_status,
                    confirmed_at_ms = excluded.confirmed_at_ms,
                    confirmed_by = excluded.confirmed_by,
                    schema_version = excluded.schema_version,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_facts.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.category,
                    value.label,
                    payload,
                    value.source,
                    value.verification_status,
                    value.confirmed_at_ms,
                    value.confirmed_by,
                    value.schema_version,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "INSERT INTO jobs_facts (
                    id, account_id, category, label, value_json, source,
                    verification_status, confirmed_at_ms, confirmed_by,
                    schema_version, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT(id) DO UPDATE SET
                    category = EXCLUDED.category,
                    label = EXCLUDED.label,
                    value_json = EXCLUDED.value_json,
                    source = EXCLUDED.source,
                    verification_status = EXCLUDED.verification_status,
                    confirmed_at_ms = EXCLUDED.confirmed_at_ms,
                    confirmed_by = EXCLUDED.confirmed_by,
                    schema_version = EXCLUDED.schema_version,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_facts.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.category,
                    &value.label,
                    &payload,
                    &value.source,
                    &value.verification_status,
                    &value.confirmed_at_ms,
                    &value.confirmed_by,
                    &value.schema_version,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn upsert_user_fact(
    pool: &DbPool,
    account_id: &str,
    fact_id: Option<&str>,
    category: &str,
    label: &str,
    fact_value: Value,
) -> Result<CareerFact> {
    let requested_id = fact_id.map(str::trim).filter(|id| !id.is_empty());
    let now = now_ms();
    let payload = to_json(&fact_value, "Jobs fact value")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let existing = requested_id
                .map(|id| {
                    tx.query_row(
                        "SELECT id, category, label, value_json, source, verification_status,
                                confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
                           FROM jobs_facts WHERE account_id = ?1 AND id = ?2",
                        params![account_id, id],
                        fact_from_sqlite_row,
                    )
                    .optional()
                })
                .transpose()?
                .flatten();
            if requested_id.is_some() && existing.is_none() {
                anyhow::bail!("career fact not found")
            }
            if existing
                .as_ref()
                .is_some_and(|fact| fact.source != "user_entry")
            {
                anyhow::bail!("imported career facts require a dedicated confirmation flow")
            }
            let id = existing
                .as_ref()
                .map(|fact| fact.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let created_at_ms = existing
                .as_ref()
                .map(|fact| fact.created_at_ms)
                .unwrap_or(now);
            tx.execute(
                "INSERT INTO jobs_facts (
                    id, account_id, category, label, value_json, source,
                    verification_status, confirmed_at_ms, confirmed_by,
                    schema_version, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'user_entry',
                           'confirmed', ?6, 'user', 1, ?7, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    category = excluded.category,
                    label = excluded.label,
                    value_json = excluded.value_json,
                    verification_status = 'confirmed',
                    confirmed_at_ms = excluded.confirmed_at_ms,
                    confirmed_by = 'user',
                    schema_version = 1,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_facts.account_id = excluded.account_id
                   AND jobs_facts.source = 'user_entry'",
                params![id, account_id, category, label, payload, now, created_at_ms],
            )?;
            tx.commit()?;
            Ok(CareerFact {
                id,
                category: category.to_string(),
                label: label.to_string(),
                value: fact_value,
                source: "user_entry".to_string(),
                verification_status: "confirmed".to_string(),
                confirmed_at_ms: Some(now),
                confirmed_by: Some("user".to_string()),
                schema_version: 1,
                created_at_ms,
                updated_at_ms: now,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let existing = if let Some(id) = requested_id {
                tx.query_opt(
                        "SELECT id, category, label, value_json, source, verification_status,
                                confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
                           FROM jobs_facts WHERE account_id = $1 AND id = $2 FOR UPDATE",
                        &[&account_id, &id],
                    )?
                    .map(fact_from_pg_row)
                    .transpose()?
            } else {
                None
            };
            if requested_id.is_some() && existing.is_none() {
                anyhow::bail!("career fact not found")
            }
            if existing
                .as_ref()
                .is_some_and(|fact| fact.source != "user_entry")
            {
                anyhow::bail!("imported career facts require a dedicated confirmation flow")
            }
            let id = existing
                .as_ref()
                .map(|fact| fact.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let created_at_ms = existing
                .as_ref()
                .map(|fact| fact.created_at_ms)
                .unwrap_or(now);
            tx.execute(
                "INSERT INTO jobs_facts (
                    id, account_id, category, label, value_json, source,
                    verification_status, confirmed_at_ms, confirmed_by,
                    schema_version, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, 'user_entry',
                           'confirmed', $6, 'user', 1, $7, $6)
                 ON CONFLICT(id) DO UPDATE SET
                    category = EXCLUDED.category,
                    label = EXCLUDED.label,
                    value_json = EXCLUDED.value_json,
                    verification_status = 'confirmed',
                    confirmed_at_ms = EXCLUDED.confirmed_at_ms,
                    confirmed_by = 'user',
                    schema_version = 1,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_facts.account_id = EXCLUDED.account_id
                   AND jobs_facts.source = 'user_entry'",
                &[
                    &id,
                    &account_id,
                    &category,
                    &label,
                    &payload,
                    &now,
                    &created_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(CareerFact {
                id,
                category: category.to_string(),
                label: label.to_string(),
                value: fact_value,
                source: "user_entry".to_string(),
                verification_status: "confirmed".to_string(),
                confirmed_at_ms: Some(now),
                confirmed_by: Some("user".to_string()),
                schema_version: 1,
                created_at_ms,
                updated_at_ms: now,
            })
        }
    })
}

pub fn delete_fact(pool: &DbPool, account_id: &str, fact_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "DELETE FROM jobs_facts WHERE account_id = ?1 AND id = ?2",
            params![account_id, fact_id],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "DELETE FROM jobs_facts WHERE account_id = $1 AND id = $2",
            &[&account_id, &fact_id],
        )? > 0),
    })
}

pub fn get_preferences(pool: &DbPool, account_id: &str) -> Result<JobPreferences> {
    let value = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json::<JobPreferences>(value, "Jobs preferences"))
                .transpose()
                .map(|value| value.unwrap_or_default())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "SELECT preferences_json FROM jobs_preferences WHERE account_id = $1",
                &[&account_id],
            )?;
            row.map(|value| parse_json::<JobPreferences>(value.get(0), "Jobs preferences"))
                .transpose()
                .map(|value| value.unwrap_or_default())
        }
    })?;
    Ok(enforce_job_preference_safety(value))
}

fn enforce_job_preference_safety(mut value: JobPreferences) -> JobPreferences {
    // An application email is an alias for one candidate, not a second
    // identity that can bypass employer-level submission safeguards.
    value.apply_once_per_company = true;
    value.daily_limit = default_daily_limit();
    value.max_posting_age_days = default_max_posting_age_days();
    value
}

pub fn save_preferences(
    pool: &DbPool,
    account_id: &str,
    preferences: &JobPreferences,
) -> Result<JobPreferences> {
    let mut value = preferences.clone();
    value.daily_limit = default_daily_limit();
    value.max_posting_age_days = default_max_posting_age_days();
    value.apply_once_per_company = true;
    value.updated_at_ms = now_ms();
    let payload = to_json(&value, "Jobs preferences")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_preferences(account_id, preferences_json, updated_at_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(account_id) DO UPDATE SET
                    preferences_json = excluded.preferences_json,
                    updated_at_ms = excluded.updated_at_ms",
                params![account_id, payload, value.updated_at_ms],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_preferences(account_id, preferences_json, updated_at_ms)
                 VALUES ($1, $2, $3)
                 ON CONFLICT(account_id) DO UPDATE SET
                    preferences_json = EXCLUDED.preferences_json,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[&account_id, &payload, &value.updated_at_ms],
            )?;
            Ok(value)
        }
    })
}

pub fn list_tracks(pool: &DbPool, account_id: &str) -> Result<Vec<CareerTrack>> {
    crate::db::run_blocking_db(|| {
        match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT track_json FROM jobs_tracks WHERE account_id = ?1 ORDER BY active DESC, updated_at_ms DESC",
            )?;
            let raws = stmt
                .query_map(params![account_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raws.into_iter()
                .map(|raw| parse_json(raw, "Career Track"))
                .collect()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT track_json FROM jobs_tracks WHERE account_id = $1 ORDER BY active DESC, updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| parse_json(row.get(0), "Career Track"))
            .collect(),
    }
    })
}

pub fn upsert_track(pool: &DbPool, account_id: &str, track: &CareerTrack) -> Result<CareerTrack> {
    let mut value = track.clone();
    if let Some(identity_id) = value.application_identity_id.as_deref() {
        let identity = get_application_identity(pool, account_id, identity_id)?
            .ok_or_else(|| anyhow::anyhow!("application email not found"))?;
        if identity.verification_status != "verified" {
            anyhow::bail!("verify the application email before using it on a Career Track")
        }
    }
    if value.id.trim().is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    let payload = to_json(&value, "Career Track")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_tracks(id, account_id, track_json, active, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    track_json = excluded.track_json,
                    active = excluded.active,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_tracks.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    payload,
                    i64::from(value.active),
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let active = i32::from(value.active);
            pool.get_pg()?.execute(
                "INSERT INTO jobs_tracks(id, account_id, track_json, active, created_at_ms, updated_at_ms)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(id) DO UPDATE SET
                    track_json = EXCLUDED.track_json,
                    active = EXCLUDED.active,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_tracks.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &payload,
                    &active,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn delete_track(pool: &DbPool, account_id: &str, track_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let exists = tx
                .query_row(
                    "SELECT 1 FROM jobs_tracks WHERE account_id = ?1 AND id = ?2",
                    params![account_id, track_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if exists {
                let source_count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = ?1 AND track_id = ?2",
                    params![account_id, track_id],
                    |row| row.get(0),
                )?;
                let mut stmt =
                    tx.prepare("SELECT posting_json FROM jobs_postings WHERE account_id = ?1")?;
                let bound_posting = stmt
                    .query_map(params![account_id], |row| row.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
                    .collect::<Result<Vec<_>>>()?
                    .iter()
                    .any(|posting| posting.track_id == track_id);
                drop(stmt);
                if source_count > 0 || bound_posting {
                    anyhow::bail!("Career Track still has Jobs matches or discovery sources")
                }
                tx.execute(
                    "DELETE FROM jobs_tracks WHERE account_id = ?1 AND id = ?2",
                    params![account_id, track_id],
                )?;
            }
            tx.commit()?;
            Ok(exists)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            let exists = tx
                .query_opt(
                    "SELECT 1 FROM jobs_tracks WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &track_id],
                )?
                .is_some();
            if exists {
                let source_count: i64 = tx.query_one(
                    "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = $1 AND track_id = $2",
                    &[&account_id, &track_id],
                )?.get(0);
                let bound_posting = tx
                    .query(
                        "SELECT posting_json FROM jobs_postings WHERE account_id = $1",
                        &[&account_id],
                    )?
                    .into_iter()
                    .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
                    .collect::<Result<Vec<_>>>()?
                    .iter()
                    .any(|posting| posting.track_id == track_id);
                if source_count > 0 || bound_posting {
                    anyhow::bail!("Career Track still has Jobs matches or discovery sources")
                }
                tx.execute(
                    "DELETE FROM jobs_tracks WHERE account_id = $1 AND id = $2",
                    &[&account_id, &track_id],
                )?;
            }
            tx.commit()?;
            Ok(exists)
        }
    })
}

pub fn list_postings(pool: &DbPool, account_id: &str) -> Result<Vec<JobPosting>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT posting_json FROM jobs_postings
                  WHERE account_id = ?1
                  ORDER BY match_score DESC, updated_at_ms DESC",
            )?;
            let raws = stmt
                .query_map(params![account_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raws.into_iter()
                .map(|raw| parse_json(raw, "job posting"))
                .collect()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT posting_json FROM jobs_postings
                  WHERE account_id = $1
                  ORDER BY match_score DESC, updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| parse_json(row.get(0), "job posting"))
            .collect(),
    })
}

pub fn get_posting(pool: &DbPool, account_id: &str, job_id: &str) -> Result<Option<JobPosting>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
                    params![account_id, job_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "job posting"))
                .transpose()
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.query_opt(
                "SELECT posting_json FROM jobs_postings WHERE account_id = $1 AND id = $2",
                &[&account_id, &job_id],
            )?
            .map(|row| parse_json(row.get(0), "job posting"))
            .transpose()
        }
    })
}

pub fn upsert_posting(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> Result<JobPosting> {
    let mut value = posting.clone();
    if value.id.trim().is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    value.source = if value.source.trim().is_empty() {
        "pasted_link".to_string()
    } else {
        value.source.trim().to_lowercase()
    };
    value.status = if value.status.trim().is_empty() {
        default_match_status()
    } else {
        value.status.trim().to_lowercase()
    };
    value.availability_status = if value.availability_status.trim().is_empty() {
        default_active_availability()
    } else {
        value.availability_status.trim().to_lowercase()
    };
    if !matches!(
        value.availability_status.as_str(),
        "active" | "expired" | "unknown"
    ) {
        anyhow::bail!("invalid job availability status")
    }
    value.canonical_key = canonical_job_key(&value);
    if value.match_score == 0 {
        let (score, reasons, missing) = score_posting(&value, profile, preferences);
        value.match_score = score;
        value.matched_reasons = reasons;
        value.missing_requirements = missing;
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    if value.availability_status == "active" && value.last_verified_at_ms.is_none() {
        value.last_verified_at_ms = Some(now);
    }
    value.updated_at_ms = now;
    let applications = list_applications(pool, account_id)?;
    let reservations = list_attempt_reservations(pool, account_id)?;
    let existing_application_id = applications
        .iter()
        .find(|application| application.job_id == value.id)
        .map(|application| application.id.as_str());
    value.eligibility = Some(build_job_eligibility(
        &value,
        profile,
        preferences,
        &reservations,
        true,
        existing_application_id,
    ));
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let candidate = prepare_snapshot_posting(
                &value,
                None,
                profile,
                preferences,
                &applications,
                &reservations,
                now,
            )?;
            let candidate_payload = to_json(&candidate, "job posting")?;
            tx.execute(
                "INSERT INTO jobs_postings (
                    id, account_id, canonical_key, posting_json, source, canonical_url,
                    company, title, location, match_score, status, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(account_id, canonical_key) DO NOTHING",
                params![
                    candidate.id,
                    account_id,
                    candidate.canonical_key,
                    candidate_payload,
                    candidate.source,
                    candidate.canonical_url,
                    candidate.company,
                    candidate.title,
                    candidate.location,
                    candidate.match_score,
                    candidate.status,
                    candidate.created_at_ms,
                    candidate.updated_at_ms,
                ],
            )?;
            let raw: String = tx.query_row(
                "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND canonical_key = ?2",
                params![account_id, candidate.canonical_key],
                |row| row.get(0),
            )?;
            let actual: JobPosting = parse_json(raw, "job posting")?;
            let saved = prepare_snapshot_posting(
                &value,
                Some(actual),
                profile,
                preferences,
                &applications,
                &reservations,
                now,
            )?;
            let payload = to_json(&saved, "job posting")?;
            tx.execute(
                "UPDATE jobs_postings SET posting_json = ?3, source = ?4, canonical_url = ?5,
                    company = ?6, title = ?7, location = ?8, match_score = ?9, status = ?10,
                    updated_at_ms = ?11 WHERE account_id = ?1 AND canonical_key = ?2",
                params![
                    account_id,
                    saved.canonical_key,
                    payload,
                    saved.source,
                    saved.canonical_url,
                    saved.company,
                    saved.title,
                    saved.location,
                    saved.match_score,
                    saved.status,
                    saved.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(saved)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            let candidate = prepare_snapshot_posting(
                &value,
                None,
                profile,
                preferences,
                &applications,
                &reservations,
                now,
            )?;
            let candidate_payload = to_json(&candidate, "job posting")?;
            tx.execute(
                "INSERT INTO jobs_postings (
                    id, account_id, canonical_key, posting_json, source, canonical_url,
                    company, title, location, match_score, status, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT(account_id, canonical_key) DO NOTHING",
                &[
                    &candidate.id,
                    &account_id,
                    &candidate.canonical_key,
                    &candidate_payload,
                    &candidate.source,
                    &candidate.canonical_url,
                    &candidate.company,
                    &candidate.title,
                    &candidate.location,
                    &candidate.match_score,
                    &candidate.status,
                    &candidate.created_at_ms,
                    &candidate.updated_at_ms,
                ],
            )?;
            let actual: JobPosting = parse_json(
                tx.query_one(
                    "SELECT posting_json FROM jobs_postings
                      WHERE account_id = $1 AND canonical_key = $2 FOR UPDATE",
                    &[&account_id, &candidate.canonical_key],
                )?
                .get(0),
                "job posting",
            )?;
            let saved = prepare_snapshot_posting(
                &value,
                Some(actual),
                profile,
                preferences,
                &applications,
                &reservations,
                now,
            )?;
            let payload = to_json(&saved, "job posting")?;
            tx.execute(
                "UPDATE jobs_postings SET posting_json = $3, source = $4, canonical_url = $5,
                    company = $6, title = $7, location = $8, match_score = $9, status = $10,
                    updated_at_ms = $11 WHERE account_id = $1 AND canonical_key = $2",
                &[
                    &account_id,
                    &saved.canonical_key,
                    &payload,
                    &saved.source,
                    &saved.canonical_url,
                    &saved.company,
                    &saved.title,
                    &saved.location,
                    &saved.match_score,
                    &saved.status,
                    &saved.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(saved)
        }
    })
}

/// Build the posting document used by the atomic discovery publisher. Database
/// reads happen inside that publisher's transaction; this pure step keeps the
/// same canonical-key, scoring, eligibility, and cross-track rules as normal
/// job saves without opening a nested connection.
fn prepare_snapshot_posting(
    posting: &JobPosting,
    existing: Option<JobPosting>,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    applications: &[JobApplication],
    reservations: &[AttemptReservation],
    observed_at_ms: i64,
) -> Result<JobPosting> {
    let mut value = posting.clone();
    if value.id.trim().is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    value.source = if value.source.trim().is_empty() {
        "pasted_link".to_string()
    } else {
        value.source.trim().to_lowercase()
    };
    value.status = if value.status.trim().is_empty() {
        default_match_status()
    } else {
        value.status.trim().to_lowercase()
    };
    value.availability_status = if value.availability_status.trim().is_empty() {
        default_active_availability()
    } else {
        value.availability_status.trim().to_lowercase()
    };
    if !matches!(
        value.availability_status.as_str(),
        "active" | "expired" | "unknown"
    ) {
        anyhow::bail!("invalid job availability status")
    }
    value.canonical_key = canonical_job_key(&value);
    if let Some(existing) = existing {
        if existing.track_id != value.track_id {
            anyhow::bail!("job is already bound to another Career Track")
        }
        value.id = existing.id;
        value.created_at_ms = existing.created_at_ms;
        if value.posted_at_ms.is_none() {
            value.posted_at_ms = existing.posted_at_ms;
        }
    }
    if value.match_score == 0 {
        let (score, reasons, missing) = score_posting(&value, profile, preferences);
        value.match_score = score;
        value.matched_reasons = reasons;
        value.missing_requirements = missing;
    }
    if value.created_at_ms == 0 {
        value.created_at_ms = observed_at_ms;
    }
    if value.availability_status == "active" && value.last_verified_at_ms.is_none() {
        value.last_verified_at_ms = Some(observed_at_ms);
    }
    value.updated_at_ms = observed_at_ms;
    let existing_application_id = applications
        .iter()
        .find(|application| application.job_id == value.id)
        .map(|application| application.id.as_str());
    value.eligibility = Some(build_job_eligibility(
        &value,
        profile,
        preferences,
        reservations,
        true,
        existing_application_id,
    ));
    Ok(value)
}

pub fn list_discovery_sources(pool: &DbPool, account_id: &str) -> Result<Vec<DiscoverySource>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE account_id = ?1
                  ORDER BY created_at_ms ASC",
            )?;
            let rows = stmt.query_map(params![account_id], discovery_source_from_sqlite_row)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("list Jobs discovery sources")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE account_id = $1
                  ORDER BY created_at_ms ASC",
                &[&account_id],
            )?
            .into_iter()
            .map(discovery_source_from_pg_row)
            .collect(),
    })
}

/// Return the authoritative job currently bound to one verified
/// source/external-ID pair. Workspace backfill uses this to repair sources
/// created by the pre-membership importer without rewriting healthy matches
/// on every read.
pub fn verified_import_discovery_membership_job_id(
    pool: &DbPool,
    account_id: &str,
    provider: &str,
    source_key: &str,
    external_id: &str,
) -> Result<Option<String>> {
    let provider = provider.trim().to_ascii_lowercase();
    let source_key = source_key.trim();
    let external_id = external_id.trim();
    if provider.is_empty() || source_key.is_empty() || external_id.is_empty() {
        return Ok(None);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT m.job_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE s.account_id = ?1 AND s.provider = ?2 AND s.source_key = ?3
                    AND m.account_id = ?1 AND m.external_id = ?4",
                params![account_id, provider, source_key, external_id],
                |row| row.get(0),
            )
            .optional()
            .context("get verified-import discovery membership"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT m.job_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE s.account_id = $1 AND s.provider = $2 AND s.source_key = $3
                    AND m.account_id = $1 AND m.external_id = $4",
                &[&account_id, &provider, &source_key, &external_id],
            )
            .map(|row| row.map(|row| row.get(0)))
            .context("get verified-import discovery membership"),
    })
}

pub fn get_discovery_source(pool: &DbPool, source_id: &str) -> Result<Option<DiscoverySource>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE id = ?1",
                params![source_id],
                discovery_source_from_sqlite_row,
            )
            .optional()
            .context("get Jobs discovery source"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE id = $1",
                &[&source_id],
            )?
            .map(discovery_source_from_pg_row)
            .transpose(),
    })
}

pub fn upsert_discovery_source(
    pool: &DbPool,
    account_id: &str,
    input: &DiscoverySourceInput,
) -> Result<DiscoverySource> {
    // Keep this validation before the write: callers that save an imported
    // match must be able to reject an invalid track/board binding rather than
    // make persistence look successful and merely log a failed enrollment.
    validate_discovery_source_input(pool, account_id, input)?;
    let provider = input.provider.trim().to_ascii_lowercase();
    if !matches!(
        provider.as_str(),
        "greenhouse" | "lever" | "ashby" | "smartrecruiters" | "workday"
    ) {
        anyhow::bail!("unsupported Jobs discovery provider")
    }
    let requested_source_key = input.source_key.trim();
    if requested_source_key.is_empty()
        || requested_source_key.len() > 160
        || !requested_source_key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'~'))
    {
        anyhow::bail!("discovery source key contains unsupported characters")
    }
    if provider != "workday" && requested_source_key.contains('~') {
        anyhow::bail!("discovery source key contains unsupported characters")
    }
    let company = input.company.trim();
    if company.is_empty() || company.chars().count() > 200 {
        anyhow::bail!("discovery source company is required")
    }
    let track_id = input.track_id.trim();
    if !track_id.is_empty()
        && !list_tracks(pool, account_id)?
            .iter()
            .any(|track| track.id == track_id)
    {
        anyhow::bail!("discovery source Career Track was not found")
    }
    let interval = input
        .run_interval_ms
        .clamp(DISCOVERY_MIN_INTERVAL_MS, DISCOVERY_MAX_INTERVAL_MS);
    let (source_key, config) = match provider.as_str() {
        "greenhouse" => (
            requested_source_key.to_string(),
            json!({
                "kind": "greenhouse",
                "boardToken": requested_source_key,
                "company": company,
            }),
        ),
        "lever" => (
            requested_source_key.to_string(),
            json!({
                "kind": "lever",
                "site": requested_source_key,
                "company": company,
            }),
        ),
        "ashby" => (
            requested_source_key.to_string(),
            json!({
                "kind": "ashby",
                "boardName": requested_source_key,
                "company": company,
            }),
        ),
        "smartrecruiters" => (
            requested_source_key.to_string(),
            json!({
                "kind": "smartrecruiters",
                "companyIdentifier": requested_source_key,
                "company": company,
            }),
        ),
        "workday" => {
            let identifiers = requested_source_key.split('~').collect::<Vec<_>>();
            if identifiers.len() != 3 || identifiers.iter().any(|value| value.is_empty()) {
                anyhow::bail!("Workday source key must use tenant~instance~site")
            }
            (
                requested_source_key.to_string(),
                json!({
                    "kind": "workday",
                    "tenant": identifiers[0],
                    "instance": identifiers[1],
                    "site": identifiers[2],
                    "locale": "en-US",
                    "company": company,
                }),
            )
        }
        _ => unreachable!(),
    };
    let digest = hex::encode(Sha256::digest(format!(
        "{account_id}\0{provider}\0{source_key}\0{track_id}"
    )));
    let id = format!("source-{}", &digest[..32]);
    let now = now_ms();
    let payload = to_json(&config, "Jobs discovery source")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            enforce_discovery_source_authority_sqlite(
                &tx,
                account_id,
                &provider,
                &source_key,
                track_id,
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_sources (
                    id, account_id, track_id, provider, source_key, source_json,
                    status, health, consecutive_failures, run_interval_ms,
                    next_run_at_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', 'waiting', 0, ?7, ?8, ?8, ?8)
                 ON CONFLICT(account_id, provider, source_key, track_id) DO UPDATE SET
                    source_json = excluded.source_json,
                    run_interval_ms = excluded.run_interval_ms,
                    status = 'active',
                    health = CASE WHEN jobs_discovery_sources.health = 'paused'
                                  THEN CASE WHEN jobs_discovery_sources.last_success_at_ms IS NULL
                                            THEN 'waiting' ELSE 'degraded' END
                                  ELSE jobs_discovery_sources.health END,
                    next_run_at_ms = MIN(jobs_discovery_sources.next_run_at_ms, excluded.next_run_at_ms),
                    updated_at_ms = excluded.updated_at_ms",
                params![id, account_id, track_id, provider, source_key, payload, interval, now],
            )?;
            let source = tx
                .query_row(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources
                  WHERE account_id = ?1 AND provider = ?2 AND source_key = ?3 AND track_id = ?4",
                    params![account_id, provider, source_key, track_id],
                    discovery_source_from_sqlite_row,
                )
                .context("upsert Jobs discovery source")?;
            tx.commit()?;
            Ok(source)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            // PostgreSQL does not take a predicate lock for an empty source
            // list, so serialize enrollment decisions per account. This keeps
            // the quota and one-board/one-track contract authoritative even
            // when two imports arrive at the same time.
            lock_discovery_account_postgres(&mut tx, account_id)?;
            enforce_discovery_source_authority_postgres(
                &mut tx,
                account_id,
                &provider,
                &source_key,
                track_id,
            )?;
            let row = tx.query_one(
                "INSERT INTO jobs_discovery_sources (
                    id, account_id, track_id, provider, source_key, source_json,
                    status, health, consecutive_failures, run_interval_ms,
                    next_run_at_ms, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'active', 'waiting', 0, $7, $8, $8, $8)
                 ON CONFLICT(account_id, provider, source_key, track_id) DO UPDATE SET
                    source_json = EXCLUDED.source_json,
                    run_interval_ms = EXCLUDED.run_interval_ms,
                    status = 'active',
                    health = CASE WHEN jobs_discovery_sources.health = 'paused'
                                  THEN CASE WHEN jobs_discovery_sources.last_success_at_ms IS NULL
                                            THEN 'waiting' ELSE 'degraded' END
                                  ELSE jobs_discovery_sources.health END,
                    next_run_at_ms = LEAST(jobs_discovery_sources.next_run_at_ms, EXCLUDED.next_run_at_ms),
                    updated_at_ms = EXCLUDED.updated_at_ms
                 RETURNING id, account_id, track_id, provider, source_key, source_json,
                           status, health, consecutive_failures, run_interval_ms,
                           next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                           last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms",
                &[&id, &account_id, &track_id, &provider, &source_key, &payload, &interval, &now],
            )?;
            let source = discovery_source_from_pg_row(row)?;
            tx.commit()?;
            Ok(source)
        }
    })
}

fn enforce_discovery_source_authority_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    provider: &str,
    source_key: &str,
    track_id: &str,
) -> Result<()> {
    if !track_id.is_empty()
        && tx
            .query_row(
                "SELECT 1 FROM jobs_tracks WHERE account_id = ?1 AND id = ?2",
                params![account_id, track_id],
                |_| Ok(()),
            )
            .optional()?
            .is_none()
    {
        anyhow::bail!("discovery source Career Track was not found")
    }
    let bound_track: Option<String> = tx
        .query_row(
            "SELECT track_id FROM jobs_discovery_sources
              WHERE account_id = ?1 AND provider = ?2 AND source_key = ?3",
            params![account_id, provider, source_key],
            |row| row.get(0),
        )
        .optional()?;
    if bound_track
        .as_deref()
        .is_some_and(|bound| bound != track_id)
    {
        anyhow::bail!("discovery board is already bound to another Career Track")
    }
    if bound_track.is_none() {
        let account_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )?;
        if account_count as usize >= DISCOVERY_MAX_SOURCES_PER_ACCOUNT {
            anyhow::bail!("discovery source limit reached for this account")
        }
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = ?1 AND track_id = ?2",
            params![account_id, track_id],
            |row| row.get(0),
        )?;
        if count as usize >= DISCOVERY_MAX_SOURCES_PER_TRACK {
            anyhow::bail!("discovery source limit reached for this Career Track")
        }
    }
    Ok(())
}

fn enforce_discovery_source_authority_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    provider: &str,
    source_key: &str,
    track_id: &str,
) -> Result<()> {
    if !track_id.is_empty()
        && tx
            .query_opt(
                "SELECT 1 FROM jobs_tracks WHERE account_id = $1 AND id = $2",
                &[&account_id, &track_id],
            )?
            .is_none()
    {
        anyhow::bail!("discovery source Career Track was not found")
    }
    let bound_track = tx
        .query_opt(
            "SELECT track_id FROM jobs_discovery_sources
              WHERE account_id = $1 AND provider = $2 AND source_key = $3",
            &[&account_id, &provider, &source_key],
        )?
        .map(|row| row.get::<_, String>(0));
    if bound_track
        .as_deref()
        .is_some_and(|bound| bound != track_id)
    {
        anyhow::bail!("discovery board is already bound to another Career Track")
    }
    if bound_track.is_none() {
        let account_count: i64 = tx
            .query_one(
                "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = $1",
                &[&account_id],
            )?
            .get(0);
        if account_count as usize >= DISCOVERY_MAX_SOURCES_PER_ACCOUNT {
            anyhow::bail!("discovery source limit reached for this account")
        }
        let count: i64 = tx
            .query_one(
                "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = $1 AND track_id = $2",
                &[&account_id, &track_id],
            )?
            .get(0);
        if count as usize >= DISCOVERY_MAX_SOURCES_PER_TRACK {
            anyhow::bail!("discovery source limit reached for this Career Track")
        }
    }
    Ok(())
}

/// Validate an automatic public-ATS board binding before any job data is
/// persisted. A board deliberately belongs to one Career Track per account;
/// multi-track discovery will require an explicit join model rather than
/// silently overwriting a posting's track.
pub fn validate_discovery_source_input(
    pool: &DbPool,
    account_id: &str,
    input: &DiscoverySourceInput,
) -> Result<()> {
    let provider = input.provider.trim().to_ascii_lowercase();
    if !matches!(
        provider.as_str(),
        "greenhouse" | "lever" | "ashby" | "smartrecruiters" | "workday"
    ) {
        anyhow::bail!("unsupported Jobs discovery provider")
    }
    let source_key = input.source_key.trim();
    if source_key.is_empty()
        || source_key.len() > 160
        || !source_key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'~'))
    {
        anyhow::bail!("discovery source key contains unsupported characters")
    }
    if provider != "workday" && source_key.contains('~') {
        anyhow::bail!("discovery source key contains unsupported characters")
    }
    if provider == "workday" {
        let identifiers = source_key.split('~').collect::<Vec<_>>();
        if identifiers.len() != 3 || identifiers.iter().any(|value| value.is_empty()) {
            anyhow::bail!("Workday source key must use tenant~instance~site")
        }
    }
    let company = input.company.trim();
    if company.is_empty() || company.chars().count() > 200 {
        anyhow::bail!("discovery source company is required")
    }
    let track_id = input.track_id.trim();
    if !track_id.is_empty()
        && !list_tracks(pool, account_id)?
            .iter()
            .any(|track| track.id == track_id)
    {
        anyhow::bail!("discovery source Career Track was not found")
    }

    let sources = list_discovery_sources(pool, account_id)?;
    if sources.iter().any(|source| {
        source.provider == provider
            && source.source_key == source_key
            && source.track_id != track_id
    }) {
        anyhow::bail!("discovery board is already bound to another Career Track")
    }
    let exists = sources.iter().any(|source| {
        source.provider == provider
            && source.source_key == source_key
            && source.track_id == track_id
    });
    if !exists {
        if sources.len() >= DISCOVERY_MAX_SOURCES_PER_ACCOUNT {
            anyhow::bail!("discovery source limit reached for this account")
        }
        if sources
            .iter()
            .filter(|source| source.track_id == track_id)
            .count()
            >= DISCOVERY_MAX_SOURCES_PER_TRACK
        {
            anyhow::bail!("discovery source limit reached for this Career Track")
        }
    }
    Ok(())
}

fn normalized_discovery_source_values(
    account_id: &str,
    input: &DiscoverySourceInput,
) -> Result<(String, String, String, i64, String, String)> {
    let provider = input.provider.trim().to_ascii_lowercase();
    let source_key = input.source_key.trim().to_string();
    let track_id = input.track_id.trim().to_string();
    let company = input.company.trim();
    let interval = input
        .run_interval_ms
        .clamp(DISCOVERY_MIN_INTERVAL_MS, DISCOVERY_MAX_INTERVAL_MS);
    let config = match provider.as_str() {
        "greenhouse" => {
            json!({ "kind": "greenhouse", "boardToken": source_key, "company": company })
        }
        "lever" => json!({ "kind": "lever", "site": source_key, "company": company }),
        "ashby" => json!({ "kind": "ashby", "boardName": source_key, "company": company }),
        "smartrecruiters" => {
            json!({ "kind": "smartrecruiters", "companyIdentifier": source_key, "company": company })
        }
        "workday" => {
            let identifiers = source_key.split('~').collect::<Vec<_>>();
            if identifiers.len() != 3 || identifiers.iter().any(|value| value.is_empty()) {
                anyhow::bail!("Workday source key must use tenant~instance~site")
            }
            json!({
                "kind": "workday", "tenant": identifiers[0], "instance": identifiers[1],
                "site": identifiers[2], "locale": "en-US", "company": company,
            })
        }
        _ => anyhow::bail!("unsupported Jobs discovery provider"),
    };
    let digest = hex::encode(Sha256::digest(format!(
        "{account_id}\0{provider}\0{source_key}\0{track_id}"
    )));
    Ok((
        provider,
        source_key,
        track_id,
        interval,
        to_json(&config, "Jobs discovery source")?,
        format!("source-{}", &digest[..32]),
    ))
}

fn verified_import_membership_content_hash(
    provider: &str,
    source_key: &str,
    posting: &JobPosting,
) -> String {
    let payload = json!({
        "provider": provider,
        "source_key": source_key,
        "external_id": posting.external_id.trim(),
        "canonical_url": posting.canonical_url.trim(),
        "company": posting.company.trim(),
        "title": posting.title.trim(),
        "location": posting.location.trim(),
        "workplace": posting.workplace.trim(),
        "description": posting.description.trim(),
        "compensation": posting.compensation.trim(),
        "posted_at_ms": posting.posted_at_ms,
    });
    hex::encode(Sha256::digest(
        serde_json::to_vec(&payload).unwrap_or_default(),
    ))
}

fn verified_import_membership_run_id(
    account_id: &str,
    source_id: &str,
    external_id: &str,
) -> String {
    let digest = hex::encode(Sha256::digest(format!(
        "{account_id}\0{source_id}\0{external_id}"
    )));
    format!("verified-import-{}", &digest[..32])
}

fn resolve_verified_import_identity_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    source_id: &str,
    external_id: &str,
    canonical_key: &str,
) -> Result<Option<JobPosting>> {
    let membership_job_id: Option<String> = tx
        .query_row(
            "SELECT job_id FROM jobs_discovery_memberships
              WHERE source_id = ?1 AND external_id = ?2",
            params![source_id, external_id],
            |row| row.get(0),
        )
        .optional()?;
    let membership_posting = match membership_job_id {
        Some(job_id) => tx
            .query_row(
                "SELECT posting_json FROM jobs_postings
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, job_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("discovery membership refers to a missing job"))
            .map(Some)?,
        None => None,
    };
    let canonical_posting = tx
        .query_row(
            "SELECT posting_json FROM jobs_postings
              WHERE account_id = ?1 AND canonical_key = ?2",
            params![account_id, canonical_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
        .transpose()?;
    if membership_posting
        .as_ref()
        .zip(canonical_posting.as_ref())
        .is_some_and(|(membership, canonical)| membership.id != canonical.id)
    {
        anyhow::bail!("discovery membership and canonical job disagree")
    }
    Ok(membership_posting.or(canonical_posting))
}

fn resolve_verified_import_identity_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    source_id: &str,
    external_id: &str,
    canonical_key: &str,
) -> Result<Option<JobPosting>> {
    let membership_job_id = tx
        .query_opt(
            "SELECT job_id FROM jobs_discovery_memberships
              WHERE source_id = $1 AND external_id = $2 FOR UPDATE",
            &[&source_id, &external_id],
        )?
        .map(|row| row.get::<_, String>(0));
    let membership_posting = match membership_job_id {
        Some(job_id) => tx
            .query_opt(
                "SELECT posting_json FROM jobs_postings
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &job_id],
            )?
            .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("discovery membership refers to a missing job"))
            .map(Some)?,
        None => None,
    };
    let canonical_posting = tx
        .query_opt(
            "SELECT posting_json FROM jobs_postings
              WHERE account_id = $1 AND canonical_key = $2 FOR UPDATE",
            &[&account_id, &canonical_key],
        )?
        .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
        .transpose()?;
    if membership_posting
        .as_ref()
        .zip(canonical_posting.as_ref())
        .is_some_and(|(membership, canonical)| membership.id != canonical.id)
    {
        anyhow::bail!("discovery membership and canonical job disagree")
    }
    Ok(membership_posting.or(canonical_posting))
}

/// Atomically accept a verified public import and its required board binding.
/// Manual and unsupported links intentionally continue through `upsert_posting`
/// alone. For a verified import, neither a source nor a match becomes visible
/// unless both writes succeed under the same account/source authority lock.
pub fn save_verified_import_posting_with_source(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    source: &DiscoverySourceInput,
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> Result<JobPosting> {
    validate_discovery_source_input(pool, account_id, source)?;
    let (provider, source_key, track_id, interval, source_payload, source_id) =
        normalized_discovery_source_values(account_id, source)?;
    if posting.track_id.trim() != track_id {
        anyhow::bail!("verified import discovery source does not match the job Career Track")
    }
    let external_id = posting.external_id.trim();
    if external_id.is_empty() || external_id.chars().count() > 240 {
        anyhow::bail!("verified import has no valid external job ID")
    }
    let (_, posting_source_key) =
        canonical_public_discovery_url(&provider, &posting.canonical_url)?;
    if posting_source_key != source_key {
        anyhow::bail!("verified import URL does not belong to its discovery source")
    }
    let applications = list_applications(pool, account_id)?;
    let reservations = list_attempt_reservations(pool, account_id)?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            enforce_discovery_source_authority_sqlite(
                &tx,
                account_id,
                &provider,
                &source_key,
                &track_id,
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_sources (
                    id, account_id, track_id, provider, source_key, source_json,
                    status, health, consecutive_failures, run_interval_ms,
                    next_run_at_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', 'waiting', 0, ?7, ?8, ?8, ?8)
                 ON CONFLICT(account_id, provider, source_key, track_id) DO UPDATE SET
                    source_json = excluded.source_json, run_interval_ms = excluded.run_interval_ms,
                    status = 'active',
                    health = CASE WHEN jobs_discovery_sources.health = 'paused'
                                  THEN CASE WHEN jobs_discovery_sources.last_success_at_ms IS NULL
                                            THEN 'waiting' ELSE 'degraded' END
                                  ELSE jobs_discovery_sources.health END,
                    next_run_at_ms = MIN(jobs_discovery_sources.next_run_at_ms, excluded.next_run_at_ms),
                    updated_at_ms = excluded.updated_at_ms",
                params![source_id, account_id, track_id, provider, source_key, source_payload, interval, now],
            )?;
            let actual_source_id: String = tx.query_row(
                "SELECT id FROM jobs_discovery_sources
                  WHERE account_id = ?1 AND provider = ?2 AND source_key = ?3",
                params![account_id, provider, source_key],
                |row| row.get(0),
            )?;
            let existing = resolve_verified_import_identity_sqlite(
                &tx,
                account_id,
                &actual_source_id,
                external_id,
                &canonical_job_key(posting),
            )?;
            let saved = prepare_snapshot_posting(
                posting,
                existing.clone(),
                profile,
                preferences,
                &applications,
                &reservations,
                now,
            )?;
            let payload = to_json(&saved, "job posting")?;
            if existing.is_some() {
                let updated = tx.execute(
                    "UPDATE jobs_postings SET canonical_key = ?3, posting_json = ?4,
                        source = ?5, canonical_url = ?6, company = ?7, title = ?8,
                        location = ?9, match_score = ?10, status = ?11, updated_at_ms = ?12
                      WHERE account_id = ?1 AND id = ?2",
                    params![
                        account_id,
                        saved.id,
                        saved.canonical_key,
                        payload,
                        saved.source,
                        saved.canonical_url,
                        saved.company,
                        saved.title,
                        saved.location,
                        saved.match_score,
                        saved.status,
                        saved.updated_at_ms,
                    ],
                )?;
                if updated != 1 {
                    anyhow::bail!("verified import job identity is stale")
                }
            } else {
                tx.execute(
                    "INSERT INTO jobs_postings (
                        id, account_id, canonical_key, posting_json, source, canonical_url,
                        company, title, location, match_score, status, created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        saved.id,
                        account_id,
                        saved.canonical_key,
                        payload,
                        saved.source,
                        saved.canonical_url,
                        saved.company,
                        saved.title,
                        saved.location,
                        saved.match_score,
                        saved.status,
                        saved.created_at_ms,
                        saved.updated_at_ms,
                    ],
                )?;
            }
            let content_hash =
                verified_import_membership_content_hash(&provider, &source_key, &saved);
            let import_run_id =
                verified_import_membership_run_id(account_id, &actual_source_id, external_id);
            tx.execute(
                "INSERT INTO jobs_discovery_memberships (
                    source_id, account_id, canonical_key, external_id, job_id,
                    content_hash, first_seen_at_ms, last_seen_at_ms,
                    last_seen_run_id, availability_status
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, 'pending')
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                    canonical_key = excluded.canonical_key, job_id = excluded.job_id",
                params![
                    actual_source_id,
                    account_id,
                    saved.canonical_key,
                    external_id,
                    saved.id,
                    content_hash,
                    now,
                    import_run_id,
                ],
            )?;
            tx.commit()?;
            Ok(saved)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            enforce_discovery_source_authority_postgres(
                &mut tx,
                account_id,
                &provider,
                &source_key,
                &track_id,
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_sources (
                    id, account_id, track_id, provider, source_key, source_json,
                    status, health, consecutive_failures, run_interval_ms,
                    next_run_at_ms, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'active', 'waiting', 0, $7, $8, $8, $8)
                 ON CONFLICT(account_id, provider, source_key, track_id) DO UPDATE SET
                    source_json = EXCLUDED.source_json, run_interval_ms = EXCLUDED.run_interval_ms,
                    status = 'active',
                    health = CASE WHEN jobs_discovery_sources.health = 'paused'
                                  THEN CASE WHEN jobs_discovery_sources.last_success_at_ms IS NULL
                                            THEN 'waiting' ELSE 'degraded' END
                                  ELSE jobs_discovery_sources.health END,
                    next_run_at_ms = LEAST(jobs_discovery_sources.next_run_at_ms, EXCLUDED.next_run_at_ms),
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[&source_id, &account_id, &track_id, &provider, &source_key, &source_payload, &interval, &now],
            )?;
            let actual_source_id: String = tx
                .query_one(
                    "SELECT id FROM jobs_discovery_sources
                      WHERE account_id = $1 AND provider = $2 AND source_key = $3
                      FOR UPDATE",
                    &[&account_id, &provider, &source_key],
                )?
                .get(0);
            let existing = resolve_verified_import_identity_postgres(
                &mut tx,
                account_id,
                &actual_source_id,
                external_id,
                &canonical_job_key(posting),
            )?;
            let saved = prepare_snapshot_posting(
                posting,
                existing.clone(),
                profile,
                preferences,
                &applications,
                &reservations,
                now,
            )?;
            let payload = to_json(&saved, "job posting")?;
            if existing.is_some() {
                let updated = tx.execute(
                    "UPDATE jobs_postings SET canonical_key = $3, posting_json = $4,
                        source = $5, canonical_url = $6, company = $7, title = $8,
                        location = $9, match_score = $10, status = $11, updated_at_ms = $12
                      WHERE account_id = $1 AND id = $2",
                    &[
                        &account_id,
                        &saved.id,
                        &saved.canonical_key,
                        &payload,
                        &saved.source,
                        &saved.canonical_url,
                        &saved.company,
                        &saved.title,
                        &saved.location,
                        &saved.match_score,
                        &saved.status,
                        &saved.updated_at_ms,
                    ],
                )?;
                if updated != 1 {
                    anyhow::bail!("verified import job identity is stale")
                }
            } else {
                tx.execute(
                    "INSERT INTO jobs_postings (
                        id, account_id, canonical_key, posting_json, source, canonical_url,
                        company, title, location, match_score, status, created_at_ms, updated_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                    &[
                        &saved.id,
                        &account_id,
                        &saved.canonical_key,
                        &payload,
                        &saved.source,
                        &saved.canonical_url,
                        &saved.company,
                        &saved.title,
                        &saved.location,
                        &saved.match_score,
                        &saved.status,
                        &saved.created_at_ms,
                        &saved.updated_at_ms,
                    ],
                )?;
            }
            let content_hash =
                verified_import_membership_content_hash(&provider, &source_key, &saved);
            let import_run_id =
                verified_import_membership_run_id(account_id, &actual_source_id, external_id);
            tx.execute(
                "INSERT INTO jobs_discovery_memberships (
                    source_id, account_id, canonical_key, external_id, job_id,
                    content_hash, first_seen_at_ms, last_seen_at_ms,
                    last_seen_run_id, availability_status
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8, 'pending')
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                    canonical_key = EXCLUDED.canonical_key, job_id = EXCLUDED.job_id",
                &[
                    &actual_source_id,
                    &account_id,
                    &saved.canonical_key,
                    &external_id,
                    &saved.id,
                    &content_hash,
                    &now,
                    &import_run_id,
                ],
            )?;
            tx.commit()?;
            Ok(saved)
        }
    })
}

pub fn set_discovery_source_status(
    pool: &DbPool,
    account_id: &str,
    source_id: &str,
    status: &str,
) -> Result<Option<DiscoverySource>> {
    if !matches!(status, "active" | "paused") {
        anyhow::bail!("invalid discovery source status")
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET status = ?3,
                        health = CASE WHEN ?3 = 'paused' THEN 'paused'
                                      WHEN last_success_at_ms IS NULL THEN 'waiting'
                                      ELSE 'degraded' END,
                        next_run_at_ms = CASE WHEN ?3 = 'active' THEN ?4 ELSE next_run_at_ms END,
                        lease_owner = NULL, lease_token = NULL, lease_expires_at_ms = NULL,
                        updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, source_id, status, now],
            )?;
            if changed == 0 {
                tx.commit()?;
                return Ok(None);
            }
            let source = tx
                .query_row(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE account_id = ?1 AND id = ?2",
                    params![account_id, source_id],
                    discovery_source_from_sqlite_row,
                )
                .optional()
                .context("update Jobs discovery source")?;
            tx.commit()?;
            Ok(source)
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "UPDATE jobs_discovery_sources
                    SET status = $3,
                        health = CASE WHEN $3 = 'paused' THEN 'paused'
                                      WHEN last_success_at_ms IS NULL THEN 'waiting'
                                      ELSE 'degraded' END,
                        next_run_at_ms = CASE WHEN $3 = 'active' THEN $4 ELSE next_run_at_ms END,
                        lease_owner = NULL, lease_token = NULL, lease_expires_at_ms = NULL,
                        updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2
              RETURNING id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms",
                &[&account_id, &source_id, &status, &now],
            )?
            .map(discovery_source_from_pg_row)
            .transpose(),
    })
}

fn discovery_source_from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DiscoverySource> {
    let config: String = row.get(5)?;
    Ok(DiscoverySource {
        id: row.get(0)?,
        account_id: row.get(1)?,
        track_id: row.get(2)?,
        provider: row.get(3)?,
        source_key: row.get(4)?,
        config: parse_json_lossy(&config).unwrap_or_else(|| json!({})),
        status: row.get(6)?,
        health: row.get(7)?,
        consecutive_failures: row.get(8)?,
        run_interval_ms: row.get(9)?,
        next_run_at_ms: row.get(10)?,
        last_success_at_ms: row.get(11)?,
        last_failure_at_ms: row.get(12)?,
        last_error_code: row.get(13)?,
        lease_expires_at_ms: row.get(14)?,
        created_at_ms: row.get(15)?,
        updated_at_ms: row.get(16)?,
    })
}

fn discovery_source_from_pg_row(row: postgres::Row) -> Result<DiscoverySource> {
    let config: String = row.get(5);
    Ok(DiscoverySource {
        id: row.get(0),
        account_id: row.get(1),
        track_id: row.get(2),
        provider: row.get(3),
        source_key: row.get(4),
        config: parse_json(config, "Jobs discovery source")?,
        status: row.get(6),
        health: row.get(7),
        consecutive_failures: row.get(8),
        run_interval_ms: row.get(9),
        next_run_at_ms: row.get(10),
        last_success_at_ms: row.get(11),
        last_failure_at_ms: row.get(12),
        last_error_code: row.get(13),
        lease_expires_at_ms: row.get(14),
        created_at_ms: row.get(15),
        updated_at_ms: row.get(16),
    })
}

pub fn lease_due_discovery_source(
    pool: &DbPool,
    worker_id: &str,
) -> Result<Option<DiscoverySourceLease>> {
    let worker_id = worker_id.trim();
    if worker_id.len() < 3
        || worker_id.len() > 160
        || !worker_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.'))
    {
        anyhow::bail!("invalid discovery worker ID")
    }
    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);
    let lease_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let lease_token_hash = discovery_lease_token_hash(&lease_token);
    let now = now_ms();
    let lease_expires = now + DISCOVERY_LEASE_MS;

    recover_legacy_stale_discovery_commits(pool, now)?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let source = tx
                .query_row(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                            status, health, consecutive_failures, run_interval_ms,
                            next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                            last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_discovery_sources
                      WHERE status = 'active' AND health <> 'paused'
                        AND next_run_at_ms <= ?1
                        AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= ?1)
                        AND NOT EXISTS (
                            SELECT 1 FROM jobs_discovery_runs r
                             WHERE r.source_id = jobs_discovery_sources.id
                               AND r.status = 'committing'
                        )
                      ORDER BY next_run_at_ms ASC, id ASC LIMIT 1",
                    params![now],
                    discovery_source_from_sqlite_row,
                )
                .optional()?;
            let Some(source) = source else {
                tx.commit()?;
                return Ok(None);
            };
            let scheduled_for_ms = source.next_run_at_ms;
            let replay_key = discovery_replay_key(&source.id, scheduled_for_ms);
            let run_id = discovery_run_id(&source.id, &replay_key);
            tx.execute(
                "UPDATE jobs_discovery_sources
                    SET lease_owner = ?2, lease_token = ?3, lease_expires_at_ms = ?4,
                        updated_at_ms = ?1
                  WHERE id = ?5",
                params![now, worker_id, lease_token_hash, lease_expires, source.id],
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_runs (
                    id, account_id, source_id, replay_key, status, started_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, 'running', ?5)
                 ON CONFLICT(source_id, replay_key) DO NOTHING",
                params![run_id, source.account_id, source.id, replay_key, now],
            )?;
            tx.commit()?;
            Ok(Some(DiscoverySourceLease {
                source: DiscoverySource {
                    lease_expires_at_ms: Some(lease_expires),
                    updated_at_ms: now,
                    ..source
                },
                lease_token,
                replay_key,
                scheduled_for_ms,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources
                  WHERE status = 'active' AND health <> 'paused'
                    AND next_run_at_ms <= $1
                    AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= $1)
                    AND NOT EXISTS (
                        SELECT 1 FROM jobs_discovery_runs r
                         WHERE r.source_id = jobs_discovery_sources.id
                           AND r.status = 'committing'
                    )
                  ORDER BY next_run_at_ms ASC, id ASC
                  FOR UPDATE SKIP LOCKED LIMIT 1",
                &[&now],
            )?;
            let Some(row) = row else {
                tx.commit()?;
                return Ok(None);
            };
            let source = discovery_source_from_pg_row(row)?;
            let scheduled_for_ms = source.next_run_at_ms;
            let replay_key = discovery_replay_key(&source.id, scheduled_for_ms);
            let run_id = discovery_run_id(&source.id, &replay_key);
            tx.execute(
                "UPDATE jobs_discovery_sources
                    SET lease_owner = $2, lease_token = $3, lease_expires_at_ms = $4,
                        updated_at_ms = $1
                  WHERE id = $5",
                &[
                    &now,
                    &worker_id,
                    &lease_token_hash,
                    &lease_expires,
                    &source.id,
                ],
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_runs (
                    id, account_id, source_id, replay_key, status, started_at_ms
                 ) VALUES ($1, $2, $3, $4, 'running', $5)
                 ON CONFLICT(source_id, replay_key) DO NOTHING",
                &[&run_id, &source.account_id, &source.id, &replay_key, &now],
            )?;
            tx.commit()?;
            Ok(Some(DiscoverySourceLease {
                source: DiscoverySource {
                    lease_expires_at_ms: Some(lease_expires),
                    updated_at_ms: now,
                    ..source
                },
                lease_token,
                replay_key,
                scheduled_for_ms,
            }))
        }
    })
}

fn discovery_lease_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn discovery_replay_key(source_id: &str, scheduled_for_ms: i64) -> String {
    let digest = hex::encode(Sha256::digest(format!("{source_id}\0{scheduled_for_ms}")));
    format!("discovery-{}", &digest[..40])
}

fn discovery_run_id(source_id: &str, replay_key: &str) -> String {
    let digest = hex::encode(Sha256::digest(format!("{source_id}\0{replay_key}")));
    format!("discovery-run-{}", &digest[..32])
}

/// Compatibility-only cleanup for rows written by the pre-atomic publisher.
/// New publication never enters `committing`; this releases an old stranded
/// row after its lease expires so it cannot block a source forever.
fn recover_legacy_stale_discovery_commits(pool: &DbPool, now: i64) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let recovered = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'failed', error_code = 'commit_timeout', completed_at_ms = ?1
                  WHERE status = 'committing'
                    AND EXISTS (
                        SELECT 1 FROM jobs_discovery_sources s
                         WHERE s.id = jobs_discovery_runs.source_id
                           AND (s.lease_expires_at_ms IS NULL OR s.lease_expires_at_ms <= ?1)
                    )",
                params![now],
            )?;
            if recovered > 0 {
                tx.execute(
                    "UPDATE jobs_discovery_sources
                        SET health = CASE WHEN consecutive_failures + 1 >= 3 THEN 'paused' ELSE 'degraded' END,
                            consecutive_failures = consecutive_failures + 1,
                            last_failure_at_ms = ?1, last_error_code = 'commit_timeout',
                            next_run_at_ms = ?1, lease_owner = NULL, lease_token = NULL,
                            lease_expires_at_ms = NULL, updated_at_ms = ?1
                      WHERE EXISTS (
                          SELECT 1 FROM jobs_discovery_runs r
                           WHERE r.source_id = jobs_discovery_sources.id
                             AND r.status = 'failed' AND r.error_code = 'commit_timeout'
                             AND r.completed_at_ms = ?1
                      )",
                    params![now],
                )?;
            }
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let recovered = tx.execute(
                "UPDATE jobs_discovery_runs r
                    SET status = 'failed', error_code = 'commit_timeout', completed_at_ms = $1
                   FROM jobs_discovery_sources s
                  WHERE r.source_id = s.id AND r.status = 'committing'
                    AND (s.lease_expires_at_ms IS NULL OR s.lease_expires_at_ms <= $1)",
                &[&now],
            )?;
            if recovered > 0 {
                tx.execute(
                    "UPDATE jobs_discovery_sources s
                        SET health = CASE WHEN s.consecutive_failures + 1 >= 3 THEN 'paused' ELSE 'degraded' END,
                            consecutive_failures = s.consecutive_failures + 1,
                            last_failure_at_ms = $1, last_error_code = 'commit_timeout',
                            next_run_at_ms = $1, lease_owner = NULL, lease_token = NULL,
                            lease_expires_at_ms = NULL, updated_at_ms = $1
                      WHERE EXISTS (
                          SELECT 1 FROM jobs_discovery_runs r
                           WHERE r.source_id = s.id AND r.status = 'failed'
                             AND r.error_code = 'commit_timeout' AND r.completed_at_ms = $1
                      )",
                    &[&now],
                )?;
            }
            tx.commit()?;
            Ok(())
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn publish_discovery_snapshot(
    pool: &DbPool,
    source: &DiscoverySource,
    lease_token: &str,
    replay_key: &str,
    scheduled_for_ms: i64,
    normalized: &BTreeMap<String, (JobPosting, String)>,
    snapshot_hash: &str,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    applications: &[JobApplication],
    reservations: &[AttemptReservation],
    observed_at_ms: i64,
) -> Result<DiscoveryRunResult> {
    let token_hash = discovery_lease_token_hash(lease_token);
    let run_id = discovery_run_id(&source.id, replay_key);
    let next_run_at = discovery_next_run_at(source, observed_at_ms, 0);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let tx_now = now_ms();
            let fresh = tx
                .query_row(
                    "SELECT provider, source_key, track_id, source_json, status, updated_at_ms,
                            next_run_at_ms, lease_expires_at_ms, lease_token
                       FROM jobs_discovery_sources WHERE id = ?1 AND account_id = ?2",
                    params![source.id, source.account_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, Option<i64>>(7)?,
                            row.get::<_, Option<String>>(8)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                provider,
                source_key,
                track_id,
                raw_config,
                status,
                updated_at_ms,
                next_run,
                lease_expires,
                stored_token,
            )) = fresh
            else {
                anyhow::bail!("discovery lease is stale")
            };
            if provider != source.provider
                || source_key != source.source_key
                || track_id != source.track_id
                || parse_json::<Value>(raw_config, "Jobs discovery source")? != source.config
                || status != "active"
                || updated_at_ms != source.updated_at_ms
                || next_run != scheduled_for_ms
                || lease_expires.is_none_or(|expiry| expiry <= tx_now)
                || stored_token.as_deref() != Some(token_hash.as_str())
            {
                anyhow::bail!("discovery lease is stale")
            }
            let run = tx
                .query_row(
                    "SELECT id, status, discovered_count, upserted_count, closed_count, snapshot_hash
                       FROM jobs_discovery_runs WHERE source_id = ?1 AND replay_key = ?2",
                    params![source.id, replay_key],
                    |row| Ok((
                        row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?, row.get::<_, i64>(4)?, row.get::<_, Option<String>>(5)?,
                    )),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("discovery lease is stale"))?;
            if matches!(run.1.as_str(), "completed" | "failed") {
                if run.1 == "completed" && run.5.as_deref() != Some(snapshot_hash) {
                    anyhow::bail!("discovery replay payload does not match the completed snapshot")
                }
                return Ok(DiscoveryRunResult {
                    run_id: run.0,
                    replay_key: replay_key.to_string(),
                    status: run.1,
                    discovered_count: run.2,
                    upserted_count: run.3,
                    closed_count: run.4,
                    replayed: true,
                });
            }
            if run.1 != "running" || run.5.as_deref().is_some_and(|hash| hash != snapshot_hash) {
                anyhow::bail!("discovery lease is stale")
            }
            tx.execute(
                "UPDATE jobs_discovery_runs SET snapshot_hash = ?3
                  WHERE source_id = ?1 AND replay_key = ?2 AND status = 'running'",
                params![source.id, replay_key, snapshot_hash],
            )?;

            let mut seen = Vec::with_capacity(normalized.len());
            for (external_id, (posting, content_hash)) in normalized {
                let membership_job_id: Option<String> = tx
                    .query_row(
                        "SELECT job_id FROM jobs_discovery_memberships
                          WHERE source_id = ?1 AND external_id = ?2",
                        params![source.id, external_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let membership_posting = match membership_job_id {
                    Some(job_id) => tx
                        .query_row(
                            "SELECT posting_json FROM jobs_postings
                              WHERE account_id = ?1 AND id = ?2",
                            params![source.account_id, job_id],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?
                        .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
                        .transpose()?
                        .ok_or_else(|| {
                            anyhow::anyhow!("discovery membership refers to a missing job")
                        })
                        .map(Some)?,
                    None => None,
                };
                let canonical_posting = tx
                    .query_row(
                        "SELECT posting_json FROM jobs_postings
                          WHERE account_id = ?1 AND canonical_key = ?2",
                        params![source.account_id, posting.canonical_key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
                    .transpose()?;
                if membership_posting
                    .as_ref()
                    .zip(canonical_posting.as_ref())
                    .is_some_and(|(membership, canonical)| membership.id != canonical.id)
                {
                    anyhow::bail!("discovery membership and canonical job disagree")
                }
                let existing = membership_posting.or(canonical_posting);
                let saved = prepare_snapshot_posting(
                    posting,
                    existing.as_ref().cloned(),
                    profile,
                    preferences,
                    applications,
                    reservations,
                    observed_at_ms,
                )?;
                let payload = to_json(&saved, "job posting")?;
                if existing.is_some() {
                    tx.execute(
                        "UPDATE jobs_postings SET canonical_key = ?3, posting_json = ?4,
                            source = ?5, canonical_url = ?6, company = ?7, title = ?8,
                            location = ?9, match_score = ?10, status = ?11, updated_at_ms = ?12
                          WHERE account_id = ?1 AND id = ?2",
                        params![
                            source.account_id,
                            saved.id,
                            saved.canonical_key,
                            payload,
                            saved.source,
                            saved.canonical_url,
                            saved.company,
                            saved.title,
                            saved.location,
                            saved.match_score,
                            saved.status,
                            saved.updated_at_ms,
                        ],
                    )?;
                } else {
                    tx.execute(
                        "INSERT INTO jobs_postings (
                            id, account_id, canonical_key, posting_json, source, canonical_url,
                            company, title, location, match_score, status, created_at_ms, updated_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            saved.id, source.account_id, saved.canonical_key, payload,
                            saved.source, saved.canonical_url, saved.company, saved.title,
                            saved.location, saved.match_score, saved.status, saved.created_at_ms,
                            saved.updated_at_ms,
                        ],
                    )?;
                }
                tx.execute(
                    "INSERT INTO jobs_discovery_memberships (
                        source_id, account_id, canonical_key, external_id, job_id,
                        content_hash, first_seen_at_ms, last_seen_at_ms,
                        last_seen_run_id, availability_status
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, 'active')
                     ON CONFLICT(source_id, external_id) DO UPDATE SET
                        canonical_key = excluded.canonical_key, job_id = excluded.job_id,
                        content_hash = excluded.content_hash, last_seen_at_ms = excluded.last_seen_at_ms,
                        last_seen_run_id = excluded.last_seen_run_id, availability_status = 'active',
                        missing_count = 0, missing_since_at_ms = NULL",
                    params![
                        source.id, source.account_id, saved.canonical_key, external_id, saved.id,
                        content_hash, observed_at_ms, run_id,
                    ],
                )?;
                seen.push(external_id.clone());
            }
            let closed_count = close_missing_snapshot_memberships_sqlite(
                &tx,
                source,
                &run_id,
                observed_at_ms,
                &seen,
                profile,
                preferences,
                applications,
                reservations,
            )?;
            let source_changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET health = 'healthy', consecutive_failures = 0,
                        last_success_at_ms = ?4, last_error_code = NULL,
                        next_run_at_ms = ?5, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = ?4
                  WHERE id = ?1 AND account_id = ?2 AND lease_token = ?3",
                params![
                    source.id,
                    source.account_id,
                    token_hash,
                    observed_at_ms,
                    next_run_at
                ],
            )?;
            if source_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            let completed = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'completed', discovered_count = ?3, upserted_count = ?4,
                        closed_count = ?5, error_code = NULL, snapshot_hash = ?7,
                        completed_at_ms = ?6
                  WHERE source_id = ?1 AND replay_key = ?2 AND status = 'running'
                    AND snapshot_hash = ?7",
                params![
                    source.id,
                    replay_key,
                    normalized.len() as i64,
                    seen.len() as i64,
                    closed_count,
                    observed_at_ms,
                    snapshot_hash,
                ],
            )?;
            if completed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            tx.commit()?;
            Ok(DiscoveryRunResult {
                run_id,
                replay_key: replay_key.to_string(),
                status: "completed".to_string(),
                discovered_count: normalized.len() as i64,
                upserted_count: seen.len() as i64,
                closed_count,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, &source.account_id)?;
            let tx_now = now_ms();
            let fresh = tx.query_opt(
                "SELECT provider, source_key, track_id, source_json, status, updated_at_ms,
                        next_run_at_ms, lease_expires_at_ms, lease_token
                   FROM jobs_discovery_sources
                  WHERE id = $1 AND account_id = $2 FOR UPDATE",
                &[&source.id, &source.account_id],
            )?;
            let Some(fresh) = fresh else {
                anyhow::bail!("discovery lease is stale")
            };
            let provider: String = fresh.get(0);
            let source_key: String = fresh.get(1);
            let track_id: String = fresh.get(2);
            let raw_config: String = fresh.get(3);
            let status: String = fresh.get(4);
            let updated_at_ms: i64 = fresh.get(5);
            let next_run: i64 = fresh.get(6);
            let lease_expires: Option<i64> = fresh.get(7);
            let stored_token: Option<String> = fresh.get(8);
            if provider != source.provider
                || source_key != source.source_key
                || track_id != source.track_id
                || parse_json::<Value>(raw_config, "Jobs discovery source")? != source.config
                || status != "active"
                || updated_at_ms != source.updated_at_ms
                || next_run != scheduled_for_ms
                || lease_expires.is_none_or(|expiry| expiry <= tx_now)
                || stored_token.as_deref() != Some(token_hash.as_str())
            {
                anyhow::bail!("discovery lease is stale")
            }
            let run = tx.query_opt(
                "SELECT id, status, discovered_count, upserted_count, closed_count, snapshot_hash
                   FROM jobs_discovery_runs
                  WHERE source_id = $1 AND replay_key = $2 FOR UPDATE",
                &[&source.id, &replay_key],
            )?.ok_or_else(|| anyhow::anyhow!("discovery lease is stale"))?;
            let run_id: String = run.get(0);
            let run_status: String = run.get(1);
            let discovered_count: i64 = run.get(2);
            let upserted_count: i64 = run.get(3);
            let closed_count: i64 = run.get(4);
            let stored_hash: Option<String> = run.get(5);
            if matches!(run_status.as_str(), "completed" | "failed") {
                if run_status == "completed" && stored_hash.as_deref() != Some(snapshot_hash) {
                    anyhow::bail!("discovery replay payload does not match the completed snapshot")
                }
                return Ok(DiscoveryRunResult {
                    run_id,
                    replay_key: replay_key.to_string(),
                    status: run_status,
                    discovered_count,
                    upserted_count,
                    closed_count,
                    replayed: true,
                });
            }
            if run_status != "running"
                || stored_hash
                    .as_deref()
                    .is_some_and(|hash| hash != snapshot_hash)
            {
                anyhow::bail!("discovery lease is stale")
            }
            tx.execute(
                "UPDATE jobs_discovery_runs SET snapshot_hash = $3
                  WHERE source_id = $1 AND replay_key = $2 AND status = 'running'",
                &[&source.id, &replay_key, &snapshot_hash],
            )?;

            let mut seen = Vec::with_capacity(normalized.len());
            for (external_id, (posting, content_hash)) in normalized {
                let membership_job_id = tx
                    .query_opt(
                        "SELECT job_id FROM jobs_discovery_memberships
                          WHERE source_id = $1 AND external_id = $2 FOR UPDATE",
                        &[&source.id, &external_id],
                    )?
                    .map(|row| row.get::<_, String>(0));
                let membership_posting = match membership_job_id {
                    Some(job_id) => tx
                        .query_opt(
                            "SELECT posting_json FROM jobs_postings
                              WHERE account_id = $1 AND id = $2 FOR UPDATE",
                            &[&source.account_id, &job_id],
                        )?
                        .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
                        .transpose()?
                        .ok_or_else(|| {
                            anyhow::anyhow!("discovery membership refers to a missing job")
                        })
                        .map(Some)?,
                    None => None,
                };
                let canonical_posting = tx
                    .query_opt(
                        "SELECT posting_json FROM jobs_postings
                          WHERE account_id = $1 AND canonical_key = $2 FOR UPDATE",
                        &[&source.account_id, &posting.canonical_key],
                    )?
                    .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
                    .transpose()?;
                if membership_posting
                    .as_ref()
                    .zip(canonical_posting.as_ref())
                    .is_some_and(|(membership, canonical)| membership.id != canonical.id)
                {
                    anyhow::bail!("discovery membership and canonical job disagree")
                }
                let existing = membership_posting.or(canonical_posting);
                let saved = prepare_snapshot_posting(
                    posting,
                    existing.as_ref().cloned(),
                    profile,
                    preferences,
                    applications,
                    reservations,
                    observed_at_ms,
                )?;
                let payload = to_json(&saved, "job posting")?;
                if existing.is_some() {
                    tx.execute(
                        "UPDATE jobs_postings SET canonical_key = $3, posting_json = $4,
                            source = $5, canonical_url = $6, company = $7, title = $8,
                            location = $9, match_score = $10, status = $11, updated_at_ms = $12
                          WHERE account_id = $1 AND id = $2",
                        &[
                            &source.account_id,
                            &saved.id,
                            &saved.canonical_key,
                            &payload,
                            &saved.source,
                            &saved.canonical_url,
                            &saved.company,
                            &saved.title,
                            &saved.location,
                            &saved.match_score,
                            &saved.status,
                            &saved.updated_at_ms,
                        ],
                    )?;
                } else {
                    tx.execute(
                        "INSERT INTO jobs_postings (
                            id, account_id, canonical_key, posting_json, source, canonical_url,
                            company, title, location, match_score, status, created_at_ms, updated_at_ms
                         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                        &[
                            &saved.id,
                            &source.account_id,
                            &saved.canonical_key,
                            &payload,
                            &saved.source,
                            &saved.canonical_url,
                            &saved.company,
                            &saved.title,
                            &saved.location,
                            &saved.match_score,
                            &saved.status,
                            &saved.created_at_ms,
                            &saved.updated_at_ms,
                        ],
                    )?;
                }
                tx.execute(
                    "INSERT INTO jobs_discovery_memberships (
                        source_id, account_id, canonical_key, external_id, job_id,
                        content_hash, first_seen_at_ms, last_seen_at_ms,
                        last_seen_run_id, availability_status
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8, 'active')
                     ON CONFLICT(source_id, external_id) DO UPDATE SET
                        canonical_key = EXCLUDED.canonical_key, job_id = EXCLUDED.job_id,
                        content_hash = EXCLUDED.content_hash, last_seen_at_ms = EXCLUDED.last_seen_at_ms,
                        last_seen_run_id = EXCLUDED.last_seen_run_id, availability_status = 'active',
                        missing_count = 0, missing_since_at_ms = NULL",
                    &[
                        &source.id, &source.account_id, &saved.canonical_key, &external_id,
                        &saved.id, &content_hash, &observed_at_ms, &run_id,
                    ],
                )?;
                seen.push(external_id.clone());
            }
            let closed_count = close_missing_snapshot_memberships_postgres(
                &mut tx,
                source,
                &run_id,
                observed_at_ms,
                &seen,
                profile,
                preferences,
                applications,
                reservations,
            )?;
            let source_changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET health = 'healthy', consecutive_failures = 0,
                        last_success_at_ms = $4, last_error_code = NULL,
                        next_run_at_ms = $5, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = $4
                  WHERE id = $1 AND account_id = $2 AND lease_token = $3",
                &[
                    &source.id,
                    &source.account_id,
                    &token_hash,
                    &observed_at_ms,
                    &next_run_at,
                ],
            )?;
            if source_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            let completed = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'completed', discovered_count = $3, upserted_count = $4,
                        closed_count = $5, error_code = NULL, snapshot_hash = $7,
                        completed_at_ms = $6
                  WHERE source_id = $1 AND replay_key = $2 AND status = 'running'
                    AND snapshot_hash = $7",
                &[
                    &source.id,
                    &replay_key,
                    &(normalized.len() as i64),
                    &(seen.len() as i64),
                    &closed_count,
                    &observed_at_ms,
                    &snapshot_hash,
                ],
            )?;
            if completed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            tx.commit()?;
            Ok(DiscoveryRunResult {
                run_id,
                replay_key: replay_key.to_string(),
                status: "completed".to_string(),
                discovered_count: normalized.len() as i64,
                upserted_count: seen.len() as i64,
                closed_count,
                replayed: false,
            })
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn close_missing_snapshot_memberships_sqlite(
    tx: &rusqlite::Transaction<'_>,
    source: &DiscoverySource,
    _run_id: &str,
    observed_at_ms: i64,
    seen: &[String],
    profile: &CareerProfile,
    preferences: &JobPreferences,
    applications: &[JobApplication],
    reservations: &[AttemptReservation],
) -> Result<i64> {
    const MISSING_GRACE_MS: i64 = 30 * 60 * 1_000;
    let mut stmt = tx.prepare(
        "SELECT external_id, job_id, missing_count, missing_since_at_ms
           FROM jobs_discovery_memberships
          WHERE source_id = ?1 AND availability_status = 'active'",
    )?;
    let active = stmt
        .query_map(params![source.id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);
    let seen = seen.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut closed = 0;
    for (external_id, job_id, missing_count, missing_since_at_ms) in active {
        if seen.contains(external_id.as_str()) {
            continue;
        }
        let missing_since = missing_since_at_ms.unwrap_or(observed_at_ms);
        let next_missing_count = missing_count + 1;
        let should_close = next_missing_count >= 2
            && observed_at_ms.saturating_sub(missing_since) >= MISSING_GRACE_MS;
        let availability = if should_close { "expired" } else { "active" };
        let changed = tx.execute(
            "UPDATE jobs_discovery_memberships
                SET availability_status = ?4, missing_count = ?5, missing_since_at_ms = ?3
              WHERE source_id = ?1 AND external_id = ?2 AND availability_status = 'active'",
            params![
                source.id,
                external_id,
                missing_since,
                availability,
                next_missing_count,
            ],
        )?;
        if changed == 0 || !should_close {
            continue;
        }
        let still_active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs_discovery_memberships
              WHERE account_id = ?1 AND job_id = ?2 AND availability_status = 'active')",
            params![source.account_id, job_id],
            |row| row.get(0),
        )?;
        if still_active {
            continue;
        }
        let raw: Option<String> = tx
            .query_row(
                "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
                params![source.account_id, job_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(raw) = raw {
            let mut posting: JobPosting = parse_json(raw, "job posting")?;
            posting.availability_status = "expired".to_string();
            posting.last_verified_at_ms = Some(observed_at_ms);
            posting.updated_at_ms = observed_at_ms;
            let existing_application_id = applications
                .iter()
                .find(|application| application.job_id == posting.id)
                .map(|application| application.id.as_str());
            posting.eligibility = Some(build_job_eligibility(
                &posting,
                profile,
                preferences,
                reservations,
                true,
                existing_application_id,
            ));
            let payload = to_json(&posting, "job posting")?;
            tx.execute(
                "UPDATE jobs_postings SET posting_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![source.account_id, job_id, payload, observed_at_ms],
            )?;
        }
        closed += 1;
    }
    Ok(closed)
}

#[allow(clippy::too_many_arguments)]
fn close_missing_snapshot_memberships_postgres(
    tx: &mut postgres::Transaction<'_>,
    source: &DiscoverySource,
    _run_id: &str,
    observed_at_ms: i64,
    seen: &[String],
    profile: &CareerProfile,
    preferences: &JobPreferences,
    applications: &[JobApplication],
    reservations: &[AttemptReservation],
) -> Result<i64> {
    const MISSING_GRACE_MS: i64 = 30 * 60 * 1_000;
    let active = tx
        .query(
            "SELECT external_id, job_id, missing_count, missing_since_at_ms
               FROM jobs_discovery_memberships
              WHERE source_id = $1 AND availability_status = 'active' FOR UPDATE",
            &[&source.id],
        )?
        .into_iter()
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, String>(1),
                row.get::<_, i64>(2),
                row.get::<_, Option<i64>>(3),
            )
        })
        .collect::<Vec<_>>();
    let seen = seen.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut closed = 0;
    for (external_id, job_id, missing_count, missing_since_at_ms) in active {
        if seen.contains(external_id.as_str()) {
            continue;
        }
        let missing_since = missing_since_at_ms.unwrap_or(observed_at_ms);
        let next_missing_count = missing_count + 1;
        let should_close = next_missing_count >= 2
            && observed_at_ms.saturating_sub(missing_since) >= MISSING_GRACE_MS;
        let availability = if should_close { "expired" } else { "active" };
        let changed = tx.execute(
            "UPDATE jobs_discovery_memberships
                SET availability_status = $4, missing_count = $5, missing_since_at_ms = $3
              WHERE source_id = $1 AND external_id = $2 AND availability_status = 'active'",
            &[
                &source.id,
                &external_id,
                &missing_since,
                &availability,
                &next_missing_count,
            ],
        )?;
        if changed == 0 || !should_close {
            continue;
        }
        let still_active: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM jobs_discovery_memberships
              WHERE account_id = $1 AND job_id = $2 AND availability_status = 'active')",
                &[&source.account_id, &job_id],
            )?
            .get(0);
        if still_active {
            continue;
        }
        let raw = tx.query_opt(
            "SELECT posting_json FROM jobs_postings WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&source.account_id, &job_id],
        )?.map(|row| row.get::<_, String>(0));
        if let Some(raw) = raw {
            let mut posting: JobPosting = parse_json(raw, "job posting")?;
            posting.availability_status = "expired".to_string();
            posting.last_verified_at_ms = Some(observed_at_ms);
            posting.updated_at_ms = observed_at_ms;
            let existing_application_id = applications
                .iter()
                .find(|application| application.job_id == posting.id)
                .map(|application| application.id.as_str());
            posting.eligibility = Some(build_job_eligibility(
                &posting,
                profile,
                preferences,
                reservations,
                true,
                existing_application_id,
            ));
            let payload = to_json(&posting, "job posting")?;
            tx.execute(
                "UPDATE jobs_postings SET posting_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&source.account_id, &job_id, &payload, &observed_at_ms],
            )?;
        }
        closed += 1;
    }
    Ok(closed)
}

pub fn complete_discovery_run(
    pool: &DbPool,
    source_id: &str,
    lease_token: &str,
    replay_key: &str,
    scheduled_for_ms: i64,
    jobs: &[DiscoveredJobInput],
    complete_snapshot: bool,
) -> Result<DiscoveryRunResult> {
    if !complete_snapshot {
        anyhow::bail!("discovery completion requires a complete provider snapshot")
    }
    if jobs.len() > 10_000 {
        anyhow::bail!("discovery snapshot exceeded the job limit")
    }
    let source = get_discovery_source(pool, source_id)?
        .ok_or_else(|| anyhow::anyhow!("discovery source not found"))?;

    let company = source
        .config
        .get("company")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("discovery source company is missing"))?;
    let profile = get_profile(pool, &source.account_id, "")?;
    let preferences = get_preferences(pool, &source.account_id)?;
    let applications = list_applications(pool, &source.account_id)?;
    let reservations = list_attempt_reservations(pool, &source.account_id)?;
    let fetched_at_ms = now_ms();
    let mut normalized = BTreeMap::<String, (JobPosting, String)>::new();
    for input in jobs {
        validate_discovered_job(&source, input)?;
        let canonical_url = canonicalize_discovered_url(&source, &input.canonical_url)?;
        let content_hash = discovered_job_content_hash(&source, input, company, &canonical_url);
        let mut posting = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: source.provider.clone(),
            external_id: input.external_id.trim().to_string(),
            company: company.to_string(),
            title: input.title.trim().to_string(),
            location: input.location.trim().to_string(),
            workplace: input.workplace.trim().to_string(),
            canonical_url,
            description: input.description.trim().to_string(),
            compensation: input.compensation.trim().to_string(),
            employment_type: String::new(),
            track_id: source.track_id.clone(),
            match_score: 0,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: input
                .posted_at_ms
                .filter(|value| *value <= fetched_at_ms + DAY_MS),
            last_verified_at_ms: Some(fetched_at_ms),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            eligibility: None,
        };
        posting.canonical_key = canonical_job_key(&posting);
        let external_id = posting.external_id.clone();
        match normalized.get(&external_id) {
            Some((existing, existing_hash))
                if existing_hash == &content_hash
                    && existing.canonical_key == posting.canonical_key => {}
            Some(_) => {
                anyhow::bail!("discovery snapshot contains a conflicting external job ID")
            }
            None => {
                normalized.insert(external_id, (posting, content_hash));
            }
        }
    }

    let snapshot_hash = discovery_snapshot_hash(&source, replay_key, &normalized);
    if let Some(existing) =
        completed_discovery_run(pool, source_id, replay_key, Some(&snapshot_hash))?
    {
        return Ok(DiscoveryRunResult {
            replayed: true,
            ..existing
        });
    }
    publish_discovery_snapshot(
        pool,
        &source,
        lease_token,
        replay_key,
        scheduled_for_ms,
        &normalized,
        &snapshot_hash,
        &profile,
        &preferences,
        &applications,
        &reservations,
        fetched_at_ms,
    )
}

pub fn fail_discovery_run(
    pool: &DbPool,
    source_id: &str,
    lease_token: &str,
    replay_key: &str,
    scheduled_for_ms: i64,
    error_code: &str,
) -> Result<DiscoveryRunResult> {
    if !matches!(
        error_code,
        "throttled"
            | "timeout"
            | "unavailable"
            | "invalid_response"
            | "unauthorized"
            | "provider_error"
            | "unsupported_provider"
    ) {
        anyhow::bail!("invalid discovery failure code")
    }
    let now = now_ms();
    let token_hash = discovery_lease_token_hash(lease_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (source, stored_token) = tx
                .query_row(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                            status, health, consecutive_failures, run_interval_ms,
                            next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                            last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms,
                            lease_token
                       FROM jobs_discovery_sources WHERE id = ?1",
                    params![source_id],
                    |row| {
                        Ok((
                            discovery_source_from_sqlite_row(row)?,
                            row.get::<_, Option<String>>(17)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("discovery source not found"))?;
            let run = tx
                .query_row(
                    "SELECT id, status, discovered_count, upserted_count, closed_count
                       FROM jobs_discovery_runs WHERE source_id = ?1 AND replay_key = ?2",
                    params![source.id, replay_key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("discovery lease is stale"))?;
            if matches!(run.1.as_str(), "completed" | "failed") {
                tx.commit()?;
                return Ok(DiscoveryRunResult {
                    run_id: run.0,
                    replay_key: replay_key.to_string(),
                    status: run.1,
                    discovered_count: run.2,
                    upserted_count: run.3,
                    closed_count: run.4,
                    replayed: true,
                });
            }
            if lease_token.len() < 32
                || replay_key != discovery_replay_key(&source.id, scheduled_for_ms)
                || scheduled_for_ms != source.next_run_at_ms
                || source.status != "active"
                || source
                    .lease_expires_at_ms
                    .is_none_or(|expiry| expiry <= now)
                || stored_token.as_deref() != Some(token_hash.as_str())
                || run.1 != "running"
            {
                anyhow::bail!("discovery lease is stale")
            }
            let failures = source.consecutive_failures + 1;
            let health = if failures >= 3 { "paused" } else { "degraded" };
            let next_run_at = discovery_next_run_at(&source, now, failures);
            let source_changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET health = ?4, consecutive_failures = ?5,
                        last_failure_at_ms = ?6, last_error_code = ?7,
                        next_run_at_ms = ?8, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = ?6
                  WHERE id = ?1 AND account_id = ?2 AND lease_token = ?3",
                params![
                    source.id,
                    source.account_id,
                    token_hash,
                    health,
                    failures,
                    now,
                    error_code,
                    next_run_at
                ],
            )?;
            if source_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            let run_changed = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'failed', error_code = ?3, completed_at_ms = ?4
                  WHERE source_id = ?1 AND replay_key = ?2 AND status = 'running'",
                params![source.id, replay_key, error_code, now],
            )?;
            if run_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            tx.commit()?;
            Ok(DiscoveryRunResult {
                run_id: run.0,
                replay_key: replay_key.to_string(),
                status: "failed".to_string(),
                discovered_count: 0,
                upserted_count: 0,
                closed_count: 0,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let account_id: String = tx
                .query_opt(
                    "SELECT account_id FROM jobs_discovery_sources WHERE id = $1",
                    &[&source_id],
                )?
                .map(|row| row.get(0))
                .ok_or_else(|| anyhow::anyhow!("discovery source not found"))?;
            lock_discovery_account_postgres(&mut tx, &account_id)?;
            let row = tx
                .query_opt(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                            status, health, consecutive_failures, run_interval_ms,
                            next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                            last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms,
                            lease_token
                       FROM jobs_discovery_sources WHERE id = $1 FOR UPDATE",
                    &[&source_id],
                )?
                .ok_or_else(|| anyhow::anyhow!("discovery source not found"))?;
            let stored_token: Option<String> = row.get(17);
            let source = discovery_source_from_pg_row(row)?;
            let run = tx
                .query_opt(
                    "SELECT id, status, discovered_count, upserted_count, closed_count
                       FROM jobs_discovery_runs
                      WHERE source_id = $1 AND replay_key = $2 FOR UPDATE",
                    &[&source.id, &replay_key],
                )?
                .map(|row| {
                    (
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, i64>(2),
                        row.get::<_, i64>(3),
                        row.get::<_, i64>(4),
                    )
                })
                .ok_or_else(|| anyhow::anyhow!("discovery lease is stale"))?;
            if matches!(run.1.as_str(), "completed" | "failed") {
                tx.commit()?;
                return Ok(DiscoveryRunResult {
                    run_id: run.0,
                    replay_key: replay_key.to_string(),
                    status: run.1,
                    discovered_count: run.2,
                    upserted_count: run.3,
                    closed_count: run.4,
                    replayed: true,
                });
            }
            if lease_token.len() < 32
                || replay_key != discovery_replay_key(&source.id, scheduled_for_ms)
                || scheduled_for_ms != source.next_run_at_ms
                || source.status != "active"
                || source
                    .lease_expires_at_ms
                    .is_none_or(|expiry| expiry <= now)
                || stored_token.as_deref() != Some(token_hash.as_str())
                || run.1 != "running"
            {
                anyhow::bail!("discovery lease is stale")
            }
            let failures = source.consecutive_failures + 1;
            let health = if failures >= 3 { "paused" } else { "degraded" };
            let next_run_at = discovery_next_run_at(&source, now, failures);
            let source_changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET health = $4, consecutive_failures = $5,
                        last_failure_at_ms = $6, last_error_code = $7,
                        next_run_at_ms = $8, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = $6
                  WHERE id = $1 AND account_id = $2 AND lease_token = $3",
                &[
                    &source.id,
                    &source.account_id,
                    &token_hash,
                    &health,
                    &failures,
                    &now,
                    &error_code,
                    &next_run_at,
                ],
            )?;
            if source_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            let run_changed = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'failed', error_code = $3, completed_at_ms = $4
                  WHERE source_id = $1 AND replay_key = $2 AND status = 'running'",
                &[&source.id, &replay_key, &error_code, &now],
            )?;
            if run_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            tx.commit()?;
            Ok(DiscoveryRunResult {
                run_id: run.0,
                replay_key: replay_key.to_string(),
                status: "failed".to_string(),
                discovered_count: 0,
                upserted_count: 0,
                closed_count: 0,
                replayed: false,
            })
        }
    })
}

fn validate_discovered_job(source: &DiscoverySource, input: &DiscoveredJobInput) -> Result<()> {
    if input.external_id.trim().is_empty()
        || input.external_id.chars().count() > 240
        || input.title.trim().is_empty()
        || input.title.chars().count() > 500
        || input.description.chars().count() > 200_000
    {
        anyhow::bail!("discovery job is invalid")
    }
    canonicalize_discovered_url(source, &input.canonical_url)?;
    Ok(())
}

fn canonicalize_discovered_url(source: &DiscoverySource, raw: &str) -> Result<String> {
    let (canonical_url, source_key) = canonical_public_discovery_url(&source.provider, raw)?;
    if source_key != source.source_key {
        anyhow::bail!("discovery job URL does not belong to the configured source")
    }
    Ok(canonical_url)
}

/// Derive a scheduled public-ATS source only from a job that the server's
/// allowlisted importer has already verified. Unknown or manual imports never
/// become scheduled sources.
pub fn discovery_source_input_from_verified_import(
    imported_source: &str,
    canonical_url: &str,
    company: &str,
    track_id: &str,
) -> Result<Option<DiscoverySourceInput>> {
    let provider = match imported_source {
        "greenhouse_import" => "greenhouse",
        "lever_import" => "lever",
        "ashby_import" => "ashby",
        "smartrecruiters_import" => "smartrecruiters",
        "workday_import" => "workday",
        _ => return Ok(None),
    };
    let (_, source_key) = canonical_public_discovery_url(provider, canonical_url)?;
    let company = company.trim();
    if company.is_empty() || company.chars().count() > 200 {
        anyhow::bail!("verified job import has no valid company")
    }

    Ok(Some(DiscoverySourceInput {
        track_id: track_id.to_string(),
        provider: provider.to_string(),
        source_key,
        company: company.to_string(),
        run_interval_ms: default_discovery_interval_ms(),
    }))
}

/// Parse one of the public ATS links used by both the server importer and the
/// scheduler. Keeping one parser prevents an import from being saved in a form
/// that discovery later refuses to enroll.
pub fn canonical_public_discovery_url(provider: &str, raw: &str) -> Result<(String, String)> {
    let mut url = reqwest::Url::parse(raw.trim()).context("parse discovered job URL")?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("discovery job URL must be public HTTPS without credentials")
    }
    if url.port().is_some() {
        anyhow::bail!("discovery job URL must use the default HTTPS port")
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    let source_key = match provider {
        "greenhouse"
            if matches!(
                host.as_str(),
                "boards.greenhouse.io" | "job-boards.greenhouse.io"
            ) =>
        {
            source_key_before_job_segment(&segments, "jobs", "Greenhouse")?
        }
        "lever" if matches!(host.as_str(), "jobs.lever.co" | "jobs.eu.lever.co") => {
            direct_board_source_key(&segments, "Lever")?
        }
        "ashby" if host == "jobs.ashbyhq.com" => direct_board_source_key(&segments, "Ashby")?,
        "smartrecruiters" if host == "jobs.smartrecruiters.com" => {
            direct_board_source_key(&segments, "SmartRecruiters")?
        }
        "workday" => workday_source_key(&host, &segments)?,
        _ => anyhow::bail!("discovery job URL does not belong to the configured provider"),
    };
    let mut retained_query = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_query_key(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    retained_query.sort();
    url.set_query(None);
    if !retained_query.is_empty() {
        let mut query = url.query_pairs_mut();
        for (key, value) in retained_query {
            query.append_pair(&key, &value);
        }
    }
    url.set_fragment(None);
    Ok((
        url.to_string().trim_end_matches('/').to_string(),
        source_key,
    ))
}

fn direct_board_source_key(segments: &[&str], provider: &str) -> Result<String> {
    let Some((source_key, job_id)) = segments.first().zip(segments.get(1)) else {
        anyhow::bail!("discovery {provider} URL must include a board and job ID")
    };
    checked_discovery_identifier(source_key, "board identifier")?;
    checked_discovery_identifier(job_id, "job identifier")?;
    Ok((*source_key).to_string())
}

fn source_key_before_job_segment(
    segments: &[&str],
    job_segment: &str,
    provider: &str,
) -> Result<String> {
    let mut matches = segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| **segment == job_segment);
    let Some((job_index, _)) = matches.next() else {
        anyhow::bail!("discovery {provider} URL must include a job ID")
    };
    if matches.next().is_some() || job_index == 0 || segments.get(job_index + 1).is_none() {
        anyhow::bail!("discovery {provider} URL is ambiguous")
    }
    let source_key = segments[job_index - 1];
    checked_discovery_identifier(source_key, "board identifier")?;
    checked_discovery_identifier(segments[job_index + 1], "job identifier")?;
    Ok(source_key.to_string())
}

fn workday_source_key(host: &str, segments: &[&str]) -> Result<String> {
    let host_parts = host.split('.').collect::<Vec<_>>();
    if host_parts.len() != 4 || host_parts[2] != "myworkdayjobs" || host_parts[3] != "com" {
        anyhow::bail!("discovery job URL does not belong to the configured provider")
    }
    let tenant = host_parts[0];
    let instance = host_parts[1];
    checked_discovery_identifier(tenant, "Workday tenant")?;
    checked_discovery_identifier(instance, "Workday instance")?;
    if !instance.starts_with("wd") {
        anyhow::bail!("discovery Workday URL has an invalid instance")
    }
    let mut matches = segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| **segment == "job");
    let Some((job_index, _)) = matches.next() else {
        anyhow::bail!("discovery Workday URL must include a job requisition")
    };
    if matches.next().is_some() || job_index == 0 || segments.len() <= job_index + 1 {
        anyhow::bail!("discovery Workday URL is ambiguous")
    }
    let site = segments[job_index - 1];
    checked_discovery_identifier(site, "Workday site")?;
    if segments[job_index + 1..]
        .iter()
        .any(|segment| checked_discovery_identifier(segment, "Workday job path").is_err())
    {
        anyhow::bail!("discovery Workday URL has an invalid job path")
    }
    Ok(format!("{tenant}~{instance}~{site}"))
}

fn checked_discovery_identifier(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        anyhow::bail!("discovery {label} contains unsupported characters")
    }
    Ok(())
}

fn discovered_job_content_hash(
    source: &DiscoverySource,
    input: &DiscoveredJobInput,
    company: &str,
    canonical_url: &str,
) -> String {
    let payload = json!({
        "provider": source.provider,
        "source_key": source.source_key,
        "external_id": input.external_id.trim(),
        "canonical_url": canonical_url,
        "company": company,
        "title": input.title.trim(),
        "location": input.location.trim(),
        "workplace": input.workplace.trim(),
        "description": input.description.trim(),
        "compensation": input.compensation.trim(),
        "posted_at_ms": input.posted_at_ms,
    });
    hex::encode(Sha256::digest(
        serde_json::to_vec(&payload).unwrap_or_default(),
    ))
}

fn discovery_snapshot_hash(
    source: &DiscoverySource,
    replay_key: &str,
    jobs: &BTreeMap<String, (JobPosting, String)>,
) -> String {
    let entries = jobs
        .iter()
        .map(|(external_id, (posting, content_hash))| {
            json!({
                "external_id": external_id,
                "canonical_key": posting.canonical_key,
                "content_hash": content_hash,
            })
        })
        .collect::<Vec<_>>();
    let payload = json!({
        "source_id": source.id,
        "replay_key": replay_key,
        "entries": entries,
    });
    hex::encode(Sha256::digest(
        serde_json::to_vec(&payload).unwrap_or_default(),
    ))
}

fn completed_discovery_run(
    pool: &DbPool,
    source_id: &str,
    replay_key: &str,
    expected_snapshot_hash: Option<&str>,
) -> Result<Option<DiscoveryRunResult>> {
    let completed = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT id, replay_key, status, discovered_count, upserted_count,
                        closed_count, snapshot_hash
                   FROM jobs_discovery_runs
                  WHERE source_id = ?1 AND replay_key = ?2
                    AND status IN ('completed', 'failed')",
                params![source_id, replay_key],
                |row| {
                    Ok((
                        DiscoveryRunResult {
                            run_id: row.get(0)?,
                            replay_key: row.get(1)?,
                            status: row.get(2)?,
                            discovered_count: row.get(3)?,
                            upserted_count: row.get(4)?,
                            closed_count: row.get(5)?,
                            replayed: false,
                        },
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()
            .context("get completed discovery run"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, replay_key, status, discovered_count, upserted_count,
                        closed_count, snapshot_hash
                   FROM jobs_discovery_runs
                  WHERE source_id = $1 AND replay_key = $2
                    AND status IN ('completed', 'failed')",
                &[&source_id, &replay_key],
            )?
            .map(|row| {
                (
                    DiscoveryRunResult {
                        run_id: row.get(0),
                        replay_key: row.get(1),
                        status: row.get(2),
                        discovered_count: row.get(3),
                        upserted_count: row.get(4),
                        closed_count: row.get(5),
                        replayed: false,
                    },
                    row.get::<_, Option<String>>(6),
                )
            })
            .map(Ok)
            .transpose(),
    })?;
    if let (Some(expected), Some((result, stored))) = (expected_snapshot_hash, completed.as_ref()) {
        if result.status == "completed" && stored.as_deref() != Some(expected) {
            anyhow::bail!("discovery replay payload does not match the completed snapshot")
        }
    }
    Ok(completed.map(|(result, _)| result))
}

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const LIVE_VERIFICATION_MAX_AGE_MS: i64 = DAY_MS;

pub fn get_job_discovery_authority(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
) -> Result<Vec<JobDiscoveryAuthority>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT s.id, s.provider, s.status, s.health,
                        m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = ?1 AND m.job_id = ?2
                  ORDER BY m.last_seen_at_ms DESC",
            )?;
            let rows = stmt.query_map(params![account_id, job_id], |row| {
                Ok(JobDiscoveryAuthority {
                    source_id: row.get(0)?,
                    provider: row.get(1)?,
                    source_status: row.get(2)?,
                    source_health: row.get(3)?,
                    membership_status: row.get(4)?,
                    last_seen_at_ms: row.get(5)?,
                    last_seen_run_id: row.get(6)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("get Jobs discovery authority")
        }
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query(
                "SELECT s.id, s.provider, s.status, s.health,
                        m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = $1 AND m.job_id = $2
                  ORDER BY m.last_seen_at_ms DESC",
                &[&account_id, &job_id],
            )?
            .into_iter()
            .map(|row| JobDiscoveryAuthority {
                source_id: row.get(0),
                provider: row.get(1),
                source_status: row.get(2),
                source_health: row.get(3),
                membership_status: row.get(4),
                last_seen_at_ms: row.get(5),
                last_seen_run_id: row.get(6),
            })
            .collect()),
    })
}

fn apply_discovery_authority(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    decision: &mut JobEligibilityDecision,
) -> Result<()> {
    let authorities = get_job_discovery_authority(pool, account_id, &posting.id)?;
    apply_discovery_authorities(&authorities, decision);
    Ok(())
}

fn apply_discovery_authorities(
    authorities: &[JobDiscoveryAuthority],
    decision: &mut JobEligibilityDecision,
) {
    if authorities.is_empty() {
        return;
    }
    let now = now_ms();
    let runnable = authorities.iter().any(|authority| {
        authority.source_status == "active"
            && authority.source_health == "healthy"
            && authority.membership_status == "active"
            && authority.last_seen_at_ms >= now - LIVE_VERIFICATION_MAX_AGE_MS
    });
    if runnable {
        decision
            .passed_checks
            .push("discovery_source_healthy".to_string());
        return;
    }

    decision.can_auto_submit = false;
    decision.can_queue_local = false;
    decision.can_queue_cloud = false;
    let message = if authorities
        .iter()
        .any(|authority| authority.source_health == "paused")
    {
        "This job source is paused after repeated failures. Bluey will not run applications until it recovers."
    } else if authorities
        .iter()
        .any(|authority| authority.source_health == "degraded")
    {
        "This job source is degraded. Bluey will keep the packet in Review until a healthy refresh succeeds."
    } else {
        "Bluey must refresh this job source before an application can enter a runner."
    };
    push_reason(
        &mut decision.review_reasons,
        "discovery_source_unhealthy",
        message,
    );
}

fn enforce_application_finalization_eligibility(
    application: &JobApplication,
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    reservations: &[AttemptReservation],
    authorities: &[JobDiscoveryAuthority],
) -> Result<()> {
    let mut decision = build_job_eligibility(
        posting,
        profile,
        preferences,
        reservations,
        false,
        Some(application.id.as_str()),
    );
    apply_discovery_authorities(authorities, &mut decision);
    if !decision.can_prepare {
        anyhow::bail!("job eligibility changed while the application packet was generated")
    }
    if application.state == "queued" && !decision.can_auto_submit {
        anyhow::bail!("auto-submit eligibility changed while the application packet was generated")
    }
    Ok(())
}

pub fn posting_age_days(posting: &JobPosting, at_ms: i64) -> i64 {
    let published_or_first_seen = posting.posted_at_ms.unwrap_or(posting.created_at_ms);
    at_ms.saturating_sub(published_or_first_seen).max(0) / DAY_MS
}

pub fn evaluate_job_eligibility(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    require_live_verification: bool,
    existing_application_id: Option<&str>,
) -> Result<JobEligibilityDecision> {
    let profile = get_profile(pool, account_id, "")?;
    let preferences = get_preferences(pool, account_id)?;
    let reservations = list_attempt_reservations(pool, account_id)?;
    let mut decision = build_job_eligibility(
        posting,
        &profile,
        &preferences,
        &reservations,
        require_live_verification,
        existing_application_id,
    );
    apply_discovery_authority(pool, account_id, posting, &mut decision)?;
    Ok(decision)
}

fn build_job_eligibility(
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    reservations: &[AttemptReservation],
    require_live_verification: bool,
    existing_application_id: Option<&str>,
) -> JobEligibilityDecision {
    let now = now_ms();
    let capability = submission_capability(posting);
    let mut hard_failures = Vec::new();
    let mut review_reasons = Vec::new();
    let mut passed_checks = Vec::new();

    match posting.availability_status.as_str() {
        "active" => passed_checks.push("job_active".to_string()),
        "unknown" => push_reason(
            &mut review_reasons,
            "availability_unverified",
            "Bluey has not verified that this pasted job is still accepting applications.",
        ),
        _ => push_reason(
            &mut hard_failures,
            "job_closed",
            "This job is no longer accepting applications.",
        ),
    }

    let age_days = posting_age_days(posting, now);
    if age_days > preferences.max_posting_age_days {
        push_reason(
            &mut hard_failures,
            "job_too_old",
            &format!(
                "Posted {age_days} days ago; your limit is {} days.",
                preferences.max_posting_age_days
            ),
        );
    } else {
        passed_checks.push("job_fresh".to_string());
    }

    let live_verification_missing = posting
        .last_verified_at_ms
        .is_none_or(|verified_at| verified_at < now - LIVE_VERIFICATION_MAX_AGE_MS);
    if require_live_verification && live_verification_missing {
        push_reason(
            &mut review_reasons,
            "live_verification_required",
            "Bluey must confirm this job is still open before a runner starts.",
        );
    } else if !live_verification_missing {
        passed_checks.push("job_recently_verified".to_string());
    }

    let company = posting.company.to_lowercase();
    if preferences.excluded_companies.iter().any(|excluded| {
        let excluded = excluded.trim().to_lowercase();
        !excluded.is_empty() && (company.contains(&excluded) || excluded.contains(&company))
    }) {
        push_reason(
            &mut hard_failures,
            "company_excluded",
            "This company is excluded by your Jobs settings.",
        );
    } else {
        passed_checks.push("company_allowed".to_string());
    }

    let title = posting.title.to_lowercase();
    if preferences.excluded_titles.iter().any(|excluded| {
        let excluded = excluded.trim().to_lowercase();
        !excluded.is_empty() && title.contains(&excluded)
    }) {
        push_reason(
            &mut hard_failures,
            "title_excluded",
            "This title is excluded by your Jobs settings.",
        );
    } else {
        passed_checks.push("title_allowed".to_string());
    }

    if let Some(minimum) = preferences.minimum_compensation {
        match compensation_range(&posting.compensation) {
            Some((_, maximum)) if maximum < minimum => push_reason(
                &mut hard_failures,
                "salary_below_floor",
                "The listed compensation is below your minimum compensation setting.",
            ),
            Some(_) => passed_checks.push("salary_floor_passed".to_string()),
            None => push_reason(
                &mut review_reasons,
                "salary_not_listed",
                "Compensation is not clear enough to verify your salary floor.",
            ),
        }
    }

    if let Some(reason) = location_failure(posting, profile, preferences) {
        if preferences.location_policy == "ask" {
            push_reason(&mut review_reasons, "location_needs_confirmation", &reason);
        } else {
            push_reason(&mut hard_failures, "location_mismatch", &reason);
        }
    } else {
        passed_checks.push("location_allowed".to_string());
    }

    if has_employment_type_conflict(posting, preferences) {
        push_reason(
            &mut hard_failures,
            "employment_type_mismatch",
            "This job uses an employment type you did not select.",
        );
    } else {
        passed_checks.push("employment_type_allowed".to_string());
    }

    if let (Some((minimum, maximum)), Some((required, inferred_from_title))) = (
        candidate_experience_range(profile),
        required_experience_years(posting),
    ) {
        if required < minimum || required > maximum {
            let source = if inferred_from_title {
                "Bluey inferred the level from the job title"
            } else {
                "The posting"
            };
            push_reason(
                &mut hard_failures,
                "experience_outside_target_range",
                &format!(
                    "{source} indicates about {required} years of experience; Bluey is targeting roles requesting {minimum}-{maximum} years for your profile."
                ),
            );
        } else {
            passed_checks.push("experience_aligned".to_string());
        }
    }

    if preferences.sponsorship == "required" {
        if clearly_blocks_sponsorship(posting) {
            push_reason(
                &mut hard_failures,
                "sponsorship_unavailable",
                "This job appears to reject sponsorship.",
            );
        } else if clearly_offers_sponsorship(posting) {
            passed_checks.push("sponsorship_available".to_string());
        } else {
            push_reason(
                &mut review_reasons,
                "sponsorship_needs_confirmation",
                "Sponsorship support must be confirmed before Auto-submit.",
            );
        }
    } else if preferences.sponsorship == "ask" {
        if clearly_offers_sponsorship(posting) {
            passed_checks.push("sponsorship_available".to_string());
        } else {
            push_reason(
                &mut review_reasons,
                "sponsorship_answer_required",
                "Confirm the sponsorship answer before Auto-submit.",
            );
        }
    } else {
        passed_checks.push("sponsorship_policy_passed".to_string());
    }

    let active_company_key = normalize_company_key(&posting.company);
    let has_active_company_application = reservations.iter().any(|reservation| {
        existing_application_id.is_none_or(|existing_id| reservation.application_id != existing_id)
            && active_attempt_status(&reservation.status)
            && reservation.company_key == active_company_key
    });
    if has_active_company_application {
        push_reason(
            &mut hard_failures,
            "company_application_exists",
            "Bluey already has an in-progress or submitted application for this company. Another Career Track, resume, or application email does not create a second candidate.",
        );
    } else {
        passed_checks.push("company_application_clear".to_string());
    }

    let period_key = attempt_period_key(now, preferences.time_zone_offset_minutes);
    let daily_count = reservations
        .iter()
        .filter(|reservation| {
            existing_application_id
                .is_none_or(|existing_id| reservation.application_id != existing_id)
                && reservation.period_key == period_key
                && active_attempt_status(&reservation.status)
        })
        .count() as i64;
    if daily_count >= preferences.daily_limit.clamp(1, 50) {
        push_reason(
            &mut hard_failures,
            "daily_limit_reached",
            "Today's application limit has been reached.",
        );
    } else {
        passed_checks.push("daily_limit_available".to_string());
    }

    if !posting.missing_requirements.is_empty() {
        push_reason(
            &mut review_reasons,
            "missing_requirements",
            "This application still has required details to review.",
        );
    }
    if posting.match_score < profile.auto_submit_threshold.clamp(60, 100) {
        push_reason(
            &mut review_reasons,
            "below_auto_submit_threshold",
            "The match score is below your Auto-submit threshold.",
        );
    }

    match capability.as_str() {
        "certified" => passed_checks.push("ats_certified".to_string()),
        "beta_review" => push_reason(
            &mut review_reasons,
            "ats_review_required",
            "This application system is in beta and requires packet review.",
        ),
        "handoff" => push_reason(
            &mut review_reasons,
            "site_handoff_required",
            "Bluey can prepare the packet, but you complete submission on this site.",
        ),
        "unknown_review" => push_reason(
            &mut review_reasons,
            "unknown_ats_review_required",
            "This application system is not certified for runner submission.",
        ),
        _ => push_reason(
            &mut hard_failures,
            "site_blocked",
            "This job link cannot be opened by Bluey Jobs.",
        ),
    }

    let can_prepare = capability != "blocked"
        && hard_failures
            .iter()
            .all(|reason| reason.code == "daily_limit_reached");
    let queue_capable = matches!(capability.as_str(), "certified" | "beta_review");
    let can_queue =
        can_prepare && hard_failures.is_empty() && queue_capable && !live_verification_missing;
    let can_auto_submit = can_queue
        && capability == "certified"
        && review_reasons.is_empty()
        && posting.missing_requirements.is_empty()
        && posting.match_score >= profile.auto_submit_threshold.clamp(60, 100);

    JobEligibilityDecision {
        capability,
        can_prepare,
        can_auto_submit,
        can_queue_local: can_queue,
        can_queue_cloud: can_queue,
        hard_failures,
        review_reasons,
        passed_checks,
        evaluated_at_ms: now,
    }
}

fn push_reason(reasons: &mut Vec<EligibilityReason>, code: &str, message: &str) {
    if reasons.iter().any(|reason| reason.code == code) {
        return;
    }
    reasons.push(EligibilityReason {
        code: code.to_string(),
        message: message.to_string(),
    });
}

fn submission_capability(posting: &JobPosting) -> String {
    let url = posting.canonical_url.trim().to_ascii_lowercase();
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return "blocked".to_string();
    }
    if url.contains("linkedin.com") || url.contains("indeed.com") {
        return "handoff".to_string();
    }
    if known_review_only_ats(&url) {
        return "beta_review".to_string();
    }
    "unknown_review".to_string()
}

fn known_review_only_ats(url: &str) -> bool {
    [
        "greenhouse.io",
        "boards.greenhouse.io",
        "lever.co",
        "jobs.lever.co",
        "ashbyhq.com",
        "jobs.ashbyhq.com",
        "smartrecruiters.com",
        "workday.com",
        "myworkdayjobs.com",
    ]
    .iter()
    .any(|host| url.contains(host))
}

fn location_failure(
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> Option<String> {
    let workplace = format!("{} {}", posting.workplace, posting.location).to_ascii_lowercase();
    let is_remote = workplace.contains("remote");
    let is_hybrid = workplace.contains("hybrid");
    let is_onsite = workplace.contains("on-site")
        || workplace.contains("onsite")
        || workplace.contains("on site");

    if (preferences.location_policy == "remote_only"
        || preferences.remote_preference == "remote_only")
        && !is_remote
    {
        return Some("Your settings allow remote jobs only.".to_string());
    }
    if preferences.remote_preference == "remote_or_hybrid" && is_onsite && !is_hybrid {
        return Some(
            "Your settings allow remote or hybrid jobs, not on-site-only jobs.".to_string(),
        );
    }

    if preferences.desired_locations.is_empty() || is_remote {
        return None;
    }
    let posting_location = normalized_location(&posting.location);
    let mut allowed_locations = preferences.desired_locations.clone();
    if preferences.location_policy == "local" && !profile.current_location.trim().is_empty() {
        allowed_locations.push(profile.current_location.clone());
    }
    let matches_location = allowed_locations.iter().any(|candidate| {
        let candidate = normalized_location(candidate);
        !candidate.is_empty()
            && (posting_location.contains(&candidate) || candidate.contains(&posting_location))
    });
    (!matches_location).then(|| {
        format!(
            "{} is outside your selected job locations.",
            if posting.location.trim().is_empty() {
                "This location"
            } else {
                posting.location.trim()
            }
        )
    })
}

fn normalized_location(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

fn eligibility_error_message(decision: &JobEligibilityDecision) -> String {
    decision
        .hard_failures
        .first()
        .or_else(|| decision.review_reasons.first())
        .map(|reason| reason.message.clone())
        .unwrap_or_else(|| "This application is not eligible for that action.".to_string())
}

fn normalize_company_key(company: &str) -> String {
    const LEGAL_SUFFIXES: &[&str] = &[
        "ag",
        "co",
        "company",
        "corp",
        "corporation",
        "gmbh",
        "inc",
        "incorporated",
        "limited",
        "llc",
        "ltd",
        "plc",
        "pte",
        "pty",
    ];
    let mut tokens: Vec<String> = company
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    let fallback = tokens.concat();
    if tokens.first().is_some_and(|token| token == "the") {
        tokens.remove(0);
    }
    while tokens
        .last()
        .is_some_and(|token| LEGAL_SUFFIXES.contains(&token.as_str()))
    {
        tokens.pop();
    }
    let normalized = tokens.concat();
    if normalized.is_empty() {
        fallback
    } else {
        normalized
    }
}

fn active_attempt_status(status: &str) -> bool {
    matches!(
        status,
        "reserved" | "running" | "side_effect_unknown" | "submitted"
    )
}

fn attempt_period_key(at_ms: i64, offset_minutes: i64) -> String {
    let adjusted = at_ms.saturating_add(offset_minutes.clamp(-840, 840) * 60_000);
    Utc.timestamp_millis_opt(adjusted)
        .single()
        .map(|timestamp| {
            format!(
                "{:04}-{:02}-{:02}",
                timestamp.year(),
                timestamp.month(),
                timestamp.day()
            )
        })
        .unwrap_or_else(|| "1970-01-01".to_string())
}

fn compensation_range(compensation: &str) -> Option<(i64, i64)> {
    let normalized = compensation.replace([',', '$'], "").to_lowercase();
    let mut amounts = Vec::new();
    for token in normalized
        .split(|character: char| !(character.is_ascii_digit() || character == 'k'))
        .filter(|token| !token.is_empty())
    {
        if let Some(raw) = token.strip_suffix('k') {
            if let Ok(value) = raw.parse::<i64>() {
                amounts.push(value * 1_000);
            }
        } else if let Ok(value) = token.parse::<i64>() {
            amounts.push(if value < 1_000 { value * 1_000 } else { value });
        }
    }
    if amounts.is_empty() {
        None
    } else {
        Some((
            *amounts.iter().min().unwrap_or(&0),
            *amounts.iter().max().unwrap_or(&0),
        ))
    }
}

fn has_employment_type_conflict(posting: &JobPosting, preferences: &JobPreferences) -> bool {
    let allowed: Vec<String> = preferences
        .employment_types
        .iter()
        .map(|item| item.trim().to_lowercase())
        .filter(|item| !item.is_empty())
        .collect();
    if allowed.is_empty() {
        return false;
    }
    let detected = normalize_employment_type(&posting.employment_type).or_else(|| {
        let text = format!("{} {}", posting.title, posting.description).to_lowercase();
        if text.contains("internship") || text.contains(" intern ") {
            Some("internship")
        } else if text.contains("contract") || text.contains("contractor") {
            Some("contract")
        } else if text.contains("part-time") || text.contains("part time") {
            Some("part_time")
        } else if text.contains("full-time") || text.contains("full time") {
            Some("full_time")
        } else {
            None
        }
    });
    detected.is_some_and(|kind| !allowed.iter().any(|allowed_kind| allowed_kind == kind))
}

fn normalize_employment_type(value: &str) -> Option<&'static str> {
    let normalized = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    if normalized.contains("intern") {
        Some("internship")
    } else if normalized.contains("contract") || normalized.contains("temporary") {
        Some("contract")
    } else if normalized.contains("part_time") || normalized == "parttime" {
        Some("part_time")
    } else if normalized.contains("full_time") || normalized == "fulltime" {
        Some("full_time")
    } else {
        None
    }
}

fn clearly_blocks_sponsorship(posting: &JobPosting) -> bool {
    let text = format!("{} {}", posting.title, posting.description).to_lowercase();
    [
        "no sponsorship",
        "not sponsor",
        "does not sponsor",
        "do not sponsor",
        "don't sponsor",
        "not able to sponsor",
        "without sponsorship",
        "must be authorized to work",
        "will not sponsor",
        "cannot sponsor",
        "unable to sponsor",
        "not eligible for visa sponsorship",
        "not eligible for sponsorship",
        "ineligible for visa sponsorship",
        "ineligible for sponsorship",
        "no visa sponsorship available",
        "no visa sponsorship is available",
        "no sponsorship available",
        "no sponsorship is available",
        "visa sponsorship is not available",
        "visa sponsorship not available",
        "sponsorship is not available",
        "sponsorship not available",
        "sponsorship unavailable",
        "u.s. citizenship required",
        "us citizenship required",
        "must be a u.s. citizen",
        "must be a us citizen",
        "must be u.s. citizen",
        "must be us citizen",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn clearly_offers_sponsorship(posting: &JobPosting) -> bool {
    if clearly_blocks_sponsorship(posting) {
        return false;
    }
    let text = format!("{} {}", posting.title, posting.description).to_lowercase();
    [
        "eligible for visa sponsorship",
        "eligible for sponsorship",
        "visa sponsorship is available",
        "visa sponsorship available",
        "sponsorship is available",
        "sponsorship available",
        "we provide visa sponsorship",
        "we offer visa sponsorship",
        "will sponsor visas",
        "can sponsor visas",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn candidate_experience_range(profile: &CareerProfile) -> Option<(i64, i64)> {
    let now = Utc::now();
    let current_month = i64::from(now.year()) * 12 + i64::from(now.month0());
    let mut intervals: Vec<(i64, i64)> = profile
        .employment
        .iter()
        .filter_map(|entry| {
            let start = parse_year_month(&entry.start_date)?;
            let end = if entry.current || entry.end_date.trim().is_empty() {
                current_month
            } else {
                parse_year_month(&entry.end_date)?.saturating_add(1)
            };
            (end > start).then_some((start, end))
        })
        .collect();
    if intervals.is_empty() {
        return None;
    }
    intervals.sort_unstable_by_key(|interval| interval.0);
    let mut total_months = 0i64;
    let mut merged = intervals[0];
    for interval in intervals.into_iter().skip(1) {
        if interval.0 <= merged.1 {
            merged.1 = merged.1.max(interval.1);
        } else {
            total_months = total_months.saturating_add(merged.1 - merged.0);
            merged = interval;
        }
    }
    total_months = total_months.saturating_add(merged.1 - merged.0);
    let years = (total_months + 6) / 12;
    Some((years.saturating_sub(1), years.saturating_add(2)))
}

fn parse_year_month(value: &str) -> Option<i64> {
    let mut parts = value.trim().split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    if !(1900..=2200).contains(&year) {
        return None;
    }
    let month = parts
        .next()
        .and_then(|part| part.parse::<i64>().ok())
        .unwrap_or(1);
    if !(1..=12).contains(&month) {
        return None;
    }
    Some(year * 12 + month - 1)
}

fn explicit_required_experience_years(posting: &JobPosting) -> Option<i64> {
    let text = format!("{} {}", posting.title, posting.description).to_ascii_lowercase();
    let normalized: String = text
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '+' | '-') {
                character
            } else {
                ' '
            }
        })
        .collect();
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| matches!(**token, "year" | "years"))
        .filter(|(index, _)| tokens.get(index + 1).is_none_or(|token| *token != "ago"))
        .filter_map(|(index, _)| {
            let start = index.saturating_sub(3);
            tokens[start..index]
                .iter()
                .rev()
                .find_map(|token| parse_year_requirement(token))
        })
        .filter(|years| (0..=20).contains(years))
        .max()
}

fn required_experience_years(posting: &JobPosting) -> Option<(i64, bool)> {
    explicit_required_experience_years(posting)
        .map(|years| (years, false))
        .or_else(|| title_seniority_floor(&posting.title).map(|years| (years, true)))
}

fn title_seniority_floor(title: &str) -> Option<i64> {
    let normalized: String = title
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect();
    let words: Vec<&str> = normalized.split_whitespace().collect();
    if words.contains(&"principal") {
        Some(9)
    } else if words.contains(&"director") {
        Some(8)
    } else if words.contains(&"staff") {
        Some(7)
    } else if words
        .iter()
        .any(|word| matches!(*word, "senior" | "sr" | "lead"))
    {
        Some(5)
    } else {
        None
    }
}

fn parse_year_requirement(token: &str) -> Option<i64> {
    let numeric = token
        .trim_matches('+')
        .split('-')
        .next()
        .unwrap_or_default();
    numeric.parse::<i64>().ok().or(match numeric {
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        "ten" => Some(10),
        _ => None,
    })
}

fn score_posting(
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> (i64, Vec<String>, Vec<String>) {
    let mut score = 45i64;
    let mut reasons = Vec::new();
    let mut missing = Vec::new();
    let title = posting.title.to_lowercase();
    let description = posting.description.to_lowercase();
    let location = posting.location.to_lowercase();

    if preferences
        .desired_roles
        .iter()
        .any(|role| title.contains(&role.to_lowercase()) || role.to_lowercase().contains(&title))
    {
        score += 20;
        reasons.push("Role matches your target".to_string());
    }

    let matching_skills: Vec<String> = profile
        .skills
        .iter()
        .filter(|skill| description.contains(&skill.to_lowercase()))
        .take(5)
        .cloned()
        .collect();
    if !matching_skills.is_empty() {
        score += (matching_skills.len() as i64 * 4).min(20);
        reasons.push(format!("Matches {} profile skills", matching_skills.len()));
    } else if !profile.skills.is_empty() && !description.is_empty() {
        missing.push("No direct skill overlap found yet".to_string());
    }

    if preferences.desired_locations.iter().any(|candidate| {
        let candidate = candidate.to_lowercase();
        location.contains(&candidate) || candidate.contains(&location)
    }) || (preferences.remote_preference.contains("remote")
        && (posting.workplace.eq_ignore_ascii_case("remote") || location.contains("remote")))
    {
        score += 10;
        reasons.push("Location preference fits".to_string());
    }

    if let (Some((minimum, maximum)), Some((required, _))) = (
        candidate_experience_range(profile),
        required_experience_years(posting),
    ) {
        if (minimum..=maximum).contains(&required) {
            score += 10;
            reasons.push(format!(
                "Experience request fits your {minimum}-{maximum} year target range"
            ));
        } else {
            score -= 15;
            missing.push(format!(
                "Role requests {required} years; your target range is {minimum}-{maximum} years"
            ));
        }
    }

    if posting.compensation.is_empty() || preferences.minimum_compensation.is_none() {
        reasons.push("Compensation needs confirmation".to_string());
    }

    (score.clamp(0, 99), reasons, missing)
}

fn candidate_truth_fingerprint(profile: &CareerProfile) -> String {
    // Fingerprint the exact candidate snapshot used to build the packet. The
    // login/application email is intentionally excluded because the verified
    // application identity is fenced separately, and `updated_at_ms` is not a
    // candidate fact. Everything else can affect resume prose, form answers,
    // eligibility, or user-visible contact data and must invalidate stale work.
    let mut snapshot = profile.clone();
    snapshot.email.clear();
    snapshot.updated_at_ms = 0;
    let encoded = serde_json::to_vec(&snapshot)
        .expect("CareerProfile contains no serialization-fallible values");
    hex::encode(Sha256::digest(encoded))
}

fn confirmed_facts_fingerprint(facts: &[CareerFact]) -> String {
    let mut confirmed = facts
        .iter()
        .filter(|fact| fact.verification_status == "confirmed")
        .cloned()
        .collect::<Vec<_>>();
    confirmed.sort_by(|left, right| left.id.cmp(&right.id));
    let encoded = serde_json::to_vec(&confirmed)
        .expect("CareerFact contains no serialization-fallible values");
    hex::encode(Sha256::digest(encoded))
}

fn selected_application_identity<'a>(
    track_identity_id: Option<&str>,
    identities: &'a [ApplicationIdentity],
) -> Option<&'a ApplicationIdentity> {
    track_identity_id
        .and_then(|identity_id| identities.iter().find(|item| item.id == identity_id))
        .or_else(|| identities.iter().find(|item| item.is_default))
        .filter(|item| item.verification_status == "verified")
}

fn posting_snapshot_fingerprint(posting: &JobPosting) -> Result<String> {
    let encoded = serde_json::to_vec(posting).context("encode Jobs posting snapshot")?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

pub fn list_attempt_reservations(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<AttemptReservation>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = ?1 ORDER BY reserved_at_ms DESC",
            )?;
            let rows = stmt
                .query_map(params![account_id], |row| {
                    Ok(AttemptReservation {
                        id: row.get(0)?,
                        application_id: row.get(1)?,
                        company_key: row.get(2)?,
                        period_key: row.get(3)?,
                        runner: row.get(4)?,
                        status: row.get(5)?,
                        reserved_at_ms: row.get(6)?,
                        updated_at_ms: row.get(7)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        }
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = $1 ORDER BY reserved_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| AttemptReservation {
                id: row.get(0),
                application_id: row.get(1),
                company_key: row.get(2),
                period_key: row.get(3),
                runner: row.get(4),
                status: row.get(5),
                reserved_at_ms: row.get(6),
                updated_at_ms: row.get(7),
            })
            .collect()),
    })
}

pub fn reserve_application_attempt(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    runner: &str,
) -> Result<AttemptReservation> {
    let application = get_application(pool, account_id, application_id)?
        .ok_or_else(|| anyhow::anyhow!("application not found"))?;
    let posting = get_posting(pool, account_id, &application.job_id)?
        .ok_or_else(|| anyhow::anyhow!("job not found"))?;
    let preferences = get_preferences(pool, account_id)?;
    let _ = get_entitlement(pool, account_id)?;
    let company_key = normalize_company_key(&posting.company);
    let period_key = attempt_period_key(now_ms(), preferences.time_zone_offset_minutes);
    let daily_limit = preferences.daily_limit.clamp(1, 50);
    let now = now_ms();
    let id = format!("attempt-{application_id}");

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if let Some(existing) = tx
                .query_row(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = ?1 AND application_id = ?2",
                    params![account_id, application_id],
                    |row| {
                        Ok(AttemptReservation {
                            id: row.get(0)?,
                            application_id: row.get(1)?,
                            company_key: row.get(2)?,
                            period_key: row.get(3)?,
                            runner: row.get(4)?,
                            status: row.get(5)?,
                            reserved_at_ms: row.get(6)?,
                            updated_at_ms: row.get(7)?,
                        })
                    },
                )
                .optional()?
            {
                if active_attempt_status(&existing.status) {
                    tx.execute(
                        "UPDATE jobs_attempt_reservations SET runner = ?3, updated_at_ms = ?4
                          WHERE account_id = ?1 AND application_id = ?2",
                        params![account_id, application_id, runner, now],
                    )?;
                    tx.commit()?;
                    return Ok(AttemptReservation {
                        runner: runner.to_string(),
                        updated_at_ms: now,
                        ..existing
                    });
                }
            }
            let discovered: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_discovery_memberships
                  WHERE account_id = ?1 AND job_id = ?2",
                params![account_id, posting.id],
                |row| row.get(0),
            )?;
            if discovered > 0 {
                let healthy: i64 = tx.query_row(
                    "SELECT COUNT(*)
                       FROM jobs_discovery_memberships m
                       JOIN jobs_discovery_sources s ON s.id = m.source_id
                      WHERE m.account_id = ?1 AND m.job_id = ?2
                        AND m.availability_status = 'active'
                        AND m.last_seen_at_ms >= ?3
                        AND s.status = 'active' AND s.health = 'healthy'",
                    params![account_id, posting.id, now - LIVE_VERIFICATION_MAX_AGE_MS],
                    |row| row.get(0),
                )?;
                if healthy == 0 {
                    anyhow::bail!(
                        "the discovery source must be healthy before reserving this application"
                    )
                }
            }
            let used: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_attempt_reservations
                  WHERE account_id = ?1 AND period_key = ?2
                    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                params![account_id, period_key],
                |row| row.get(0),
            )?;
            if used >= daily_limit {
                anyhow::bail!("today's application attempt limit has been reached")
            }
            let company_in_use: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_attempt_reservations
                  WHERE account_id = ?1 AND company_key = ?2 AND application_id <> ?3
                    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                params![account_id, company_key, application_id],
                |row| row.get(0),
            )?;
            if company_in_use > 0 {
                anyhow::bail!(
                    "an in-progress or submitted application already exists for this company"
                )
            }
            tx.execute(
                "INSERT INTO jobs_attempt_reservations (
                    id, account_id, application_id, company_key, period_key, runner, status,
                    reserved_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'reserved', ?7, ?7)
                 ON CONFLICT(account_id, application_id) DO UPDATE SET
                    company_key = excluded.company_key, period_key = excluded.period_key,
                    runner = excluded.runner, status = 'reserved', reserved_at_ms = excluded.reserved_at_ms,
                    updated_at_ms = excluded.updated_at_ms",
                params![id, account_id, application_id, company_key, period_key, runner, now],
            )?;
            tx.commit()?;
            Ok(AttemptReservation {
                id,
                application_id: application_id.to_string(),
                company_key,
                period_key,
                runner: runner.to_string(),
                status: "reserved".to_string(),
                reserved_at_ms: now,
                updated_at_ms: now,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT account_id FROM jobs_entitlements WHERE account_id = $1 FOR UPDATE",
                &[&account_id],
            )?;
            if let Some(row) = tx.query_opt(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = $1 AND application_id = $2",
                &[&account_id, &application_id],
            )? {
                let existing = AttemptReservation {
                    id: row.get(0),
                    application_id: row.get(1),
                    company_key: row.get(2),
                    period_key: row.get(3),
                    runner: row.get(4),
                    status: row.get(5),
                    reserved_at_ms: row.get(6),
                    updated_at_ms: row.get(7),
                };
                if active_attempt_status(&existing.status) {
                    tx.execute(
                        "UPDATE jobs_attempt_reservations SET runner = $3, updated_at_ms = $4
                          WHERE account_id = $1 AND application_id = $2",
                        &[&account_id, &application_id, &runner, &now],
                    )?;
                    tx.commit()?;
                    return Ok(AttemptReservation {
                        runner: runner.to_string(),
                        updated_at_ms: now,
                        ..existing
                    });
                }
            }
            let authorities = tx.query(
                "SELECT s.status, s.health, m.availability_status, m.last_seen_at_ms
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = $1 AND m.job_id = $2
                  FOR UPDATE OF s, m",
                &[&account_id, &posting.id],
            )?;
            if !authorities.is_empty() {
                let healthy = authorities.iter().any(|row| {
                    row.get::<_, String>(0) == "active"
                        && row.get::<_, String>(1) == "healthy"
                        && row.get::<_, String>(2) == "active"
                        && row.get::<_, i64>(3) >= now - LIVE_VERIFICATION_MAX_AGE_MS
                });
                if !healthy {
                    anyhow::bail!(
                        "the discovery source must be healthy before reserving this application"
                    )
                }
            }
            let used: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND period_key = $2
                        AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                    &[&account_id, &period_key],
                )?
                .get(0);
            if used >= daily_limit {
                anyhow::bail!("today's application attempt limit has been reached")
            }
            let company_in_use: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND company_key = $2 AND application_id <> $3
                        AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                    &[&account_id, &company_key, &application_id],
                )?
                .get(0);
            if company_in_use > 0 {
                anyhow::bail!(
                    "an in-progress or submitted application already exists for this company"
                )
            }
            tx.execute(
                "INSERT INTO jobs_attempt_reservations (
                    id, account_id, application_id, company_key, period_key, runner, status,
                    reserved_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'reserved', $7, $7)
                 ON CONFLICT(account_id, application_id) DO UPDATE SET
                    company_key = EXCLUDED.company_key, period_key = EXCLUDED.period_key,
                    runner = EXCLUDED.runner, status = 'reserved', reserved_at_ms = EXCLUDED.reserved_at_ms,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[&id, &account_id, &application_id, &company_key, &period_key, &runner, &now],
            )?;
            tx.commit()?;
            Ok(AttemptReservation {
                id,
                application_id: application_id.to_string(),
                company_key,
                period_key,
                runner: runner.to_string(),
                status: "reserved".to_string(),
                reserved_at_ms: now,
                updated_at_ms: now,
            })
        }
    })
}

pub fn update_attempt_reservation_status(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    status: &str,
) -> Result<bool> {
    if !matches!(
        status,
        "reserved" | "running" | "released" | "side_effect_unknown" | "submitted"
    ) {
        anyhow::bail!("invalid application attempt status")
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if status == "running" {
                let job_id: String = tx.query_row(
                    "SELECT job_id FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| row.get(0),
                )?;
                ensure_discovery_authority_in_sqlite_tx(&tx, account_id, &job_id, now)?;
            }
            let changed = tx.execute(
                "UPDATE jobs_attempt_reservations SET status = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND application_id = ?2",
                params![account_id, application_id, status, now],
            )? > 0;
            tx.commit()?;
            Ok(changed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if status == "running" {
                let job_id: String = tx
                    .query_one(
                        "SELECT job_id FROM jobs_applications
                          WHERE account_id = $1 AND id = $2 FOR UPDATE",
                        &[&account_id, &application_id],
                    )?
                    .get(0);
                ensure_discovery_authority_in_pg_tx(&mut tx, account_id, &job_id, now)?;
            }
            let changed = tx.execute(
                "UPDATE jobs_attempt_reservations SET status = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND application_id = $2",
                &[&account_id, &application_id, &status, &now],
            )? > 0;
            tx.commit()?;
            Ok(changed)
        }
    })
}

fn ensure_discovery_authority_in_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    job_id: &str,
    now: i64,
) -> Result<()> {
    let discovered: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_discovery_memberships
          WHERE account_id = ?1 AND job_id = ?2",
        params![account_id, job_id],
        |row| row.get(0),
    )?;
    if discovered == 0 {
        return Ok(());
    }
    let healthy: i64 = tx.query_row(
        "SELECT COUNT(*)
           FROM jobs_discovery_memberships m
           JOIN jobs_discovery_sources s ON s.id = m.source_id
          WHERE m.account_id = ?1 AND m.job_id = ?2
            AND m.availability_status = 'active'
            AND m.last_seen_at_ms >= ?3
            AND s.status = 'active' AND s.health = 'healthy'",
        params![account_id, job_id, now - LIVE_VERIFICATION_MAX_AGE_MS],
        |row| row.get(0),
    )?;
    if healthy == 0 {
        anyhow::bail!("the discovery source must be healthy before the runner starts")
    }
    Ok(())
}

fn ensure_discovery_authority_in_pg_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    job_id: &str,
    now: i64,
) -> Result<()> {
    let authorities = tx.query(
        "SELECT s.status, s.health, m.availability_status, m.last_seen_at_ms
           FROM jobs_discovery_memberships m
           JOIN jobs_discovery_sources s ON s.id = m.source_id
          WHERE m.account_id = $1 AND m.job_id = $2
          FOR UPDATE OF s, m",
        &[&account_id, &job_id],
    )?;
    if authorities.is_empty() {
        return Ok(());
    }
    let healthy = authorities.iter().any(|row| {
        row.get::<_, String>(0) == "active"
            && row.get::<_, String>(1) == "healthy"
            && row.get::<_, String>(2) == "active"
            && row.get::<_, i64>(3) >= now - LIVE_VERIFICATION_MAX_AGE_MS
    });
    if !healthy {
        anyhow::bail!("the discovery source must be healthy before the runner starts")
    }
    Ok(())
}

pub fn list_applications(pool: &DbPool, account_id: &str) -> Result<Vec<JobApplication>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, job_id, application_json FROM jobs_applications
                  WHERE account_id = ?1 ORDER BY updated_at_ms DESC",
            )?;
            let raws = stmt
                .query_map(params![account_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raws.into_iter()
                .map(|(id, job_id, raw)| {
                    parse_application_json(raw, &id, &job_id, "job application")
                })
                .collect()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, job_id, application_json FROM jobs_applications
                  WHERE account_id = $1 ORDER BY updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| {
                parse_application_json(row.get(2), row.get(0), row.get(1), "job application")
            })
            .collect(),
    })
}

pub fn get_application(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
) -> Result<Option<JobApplication>> {
    crate::db::run_blocking_db(|| {
        match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<(String, String, String)> = conn
                .query_row(
                    "SELECT id, job_id, application_json FROM jobs_applications WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            raw.map(|(id, job_id, value)| {
                parse_application_json(value, &id, &job_id, "job application")
            })
                .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, job_id, application_json FROM jobs_applications WHERE account_id = $1 AND id = $2",
                &[&account_id, &application_id],
            )?
            .map(|row| {
                parse_application_json(row.get(2), row.get(0), row.get(1), "job application")
            })
            .transpose(),
    }
    })
}

pub fn get_resume_version(
    pool: &DbPool,
    account_id: &str,
    resume_version_id: &str,
) -> Result<Option<ResumeVersion>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.query_row(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions WHERE account_id = ?1 AND id = ?2",
                params![account_id, resume_version_id],
                resume_from_sqlite_row,
            )
            .optional()
            .context("get resume version")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions WHERE account_id = $1 AND id = $2",
                &[&account_id, &resume_version_id],
            )?
            .map(resume_from_pg_row)
            .transpose(),
    })
}

pub fn list_resume_versions(pool: &DbPool, account_id: &str) -> Result<Vec<ResumeVersion>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions WHERE account_id = ?1
                  ORDER BY created_at_ms ASC, version_no ASC",
            )?;
            let rows = stmt.query_map(params![account_id], resume_from_sqlite_row)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("list resume versions")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions WHERE account_id = $1
                  ORDER BY created_at_ms ASC, version_no ASC",
                &[&account_id],
            )?
            .into_iter()
            .map(resume_from_pg_row)
            .collect(),
    })
}

fn resume_from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResumeVersion> {
    let content: String = row.get(4)?;
    let diff: String = row.get(5)?;
    let claims: String = row.get(6)?;
    Ok(ResumeVersion {
        id: row.get(0)?,
        job_id: row.get(1)?,
        version_no: row.get(2)?,
        mode: row.get(3)?,
        content: parse_json_lossy(&content).unwrap_or_else(|| json!({})),
        diff: parse_json_lossy(&diff).unwrap_or_else(|| json!({})),
        claim_ids: parse_json_lossy(&claims).unwrap_or_default(),
        checksum: row.get(7)?,
        created_at_ms: row.get(8)?,
    })
}

fn resume_from_pg_row(row: postgres::Row) -> Result<ResumeVersion> {
    let content: String = row.get(4);
    let diff: String = row.get(5);
    let claims: String = row.get(6);
    Ok(ResumeVersion {
        id: row.get(0),
        job_id: row.get(1),
        version_no: row.get(2),
        mode: row.get(3),
        content: parse_json(content, "resume content")?,
        diff: parse_json(diff, "resume diff")?,
        claim_ids: parse_json(claims, "resume claims")?,
        checksum: row.get(7),
        created_at_ms: row.get(8),
    })
}

pub fn prepare_application(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    mode: &str,
    submission_mode: &str,
) -> Result<(JobApplication, ResumeVersion)> {
    let (application, resume, _, _, _) =
        prepare_application_inner(pool, account_id, job_id, mode, submission_mode, true)?;
    Ok((application, resume))
}

pub fn prepare_application_draft(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    mode: &str,
    submission_mode: &str,
) -> Result<PreparedApplicationDraft> {
    let (application, baseline_resume, expected_application, profile, posting) =
        prepare_application_inner(pool, account_id, job_id, mode, submission_mode, false)?;
    Ok(PreparedApplicationDraft {
        application,
        baseline_resume,
        profile,
        posting,
        expected_application,
    })
}

fn prepare_application_inner(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    mode: &str,
    submission_mode: &str,
    finalize_immediately: bool,
) -> Result<(
    JobApplication,
    ResumeVersion,
    Option<ExpectedApplicationRevision>,
    CareerProfile,
    JobPosting,
)> {
    let profile = get_profile(pool, account_id, "")?;
    let posting =
        get_posting(pool, account_id, job_id)?.ok_or_else(|| anyhow::anyhow!("job not found"))?;
    let posting_fingerprint = posting_snapshot_fingerprint(&posting)?;
    let existing = find_application_for_job_with_revision(pool, account_id, job_id)?;
    let existing_application = existing
        .as_ref()
        .map(|(application, _)| application.clone());
    let expected_application = existing.as_ref().map(|(_, revision)| revision.clone());
    let eligibility = evaluate_job_eligibility(
        pool,
        account_id,
        &posting,
        false,
        existing_application
            .as_ref()
            .map(|application| application.id.as_str()),
    )?;
    if !eligibility.can_prepare {
        anyhow::bail!(eligibility_error_message(&eligibility))
    }
    // Resume contact data is user-editable and may differ from the Bluey login.
    // Only the authenticated account email may bootstrap a verified identity.
    let login_email = account_login_email(pool, account_id)?;
    let _ = ensure_primary_application_identity(pool, account_id, &login_email)?;
    let identities = list_application_identities(pool, account_id)?;
    let track_identity_id = list_tracks(pool, account_id)?
        .into_iter()
        .find(|track| track.id == posting.track_id)
        .and_then(|track| track.application_identity_id);
    let application_identity =
        selected_application_identity(track_identity_id.as_deref(), &identities).ok_or_else(
            || anyhow::anyhow!("verify an application email before preparing this packet"),
        )?;
    let facts = list_facts(pool, account_id)?;
    let approved_fact_ids: Vec<String> = facts
        .iter()
        .filter(|fact| fact.verification_status == "confirmed")
        .map(|fact| fact.id.clone())
        .collect();
    let confirmed_facts_fingerprint = confirmed_facts_fingerprint(&facts);
    let tailored_resume = tailor_resume(&profile, &posting, mode);
    let truth_fingerprint = candidate_truth_fingerprint(&profile);
    let content = json!({
        "target": {
            "job_id": posting.id,
            "company": posting.company,
            "title": posting.title,
            "location": posting.location,
        },
        "contact": {
            "name": profile.full_name,
            "email": application_identity.email,
            "phone": profile.phone,
            "location": profile.current_location,
            "linkedin_url": profile.linkedin_url,
            "portfolio_url": profile.portfolio_url,
        },
        "headline": tailored_resume.headline,
        "summary": tailored_resume.summary,
        "skills": tailored_resume.skills,
        "employment": tailored_resume.employment,
        "education": profile.education,
        "projects": tailored_resume.projects,
        "certifications": profile.certifications,
        "source_resume_name": profile.source_resume_name,
        "provenance": {
            "fact_ids": approved_fact_ids,
            "mode": mode,
            "generated_for_job_id": posting.id,
            "application_identity_id": application_identity.id,
            "career_track_id": posting.track_id,
            "candidate_truth_fingerprint": truth_fingerprint,
            "candidate_truth_fingerprint_version": 1,
            "confirmed_facts_fingerprint": confirmed_facts_fingerprint,
            "confirmed_facts_fingerprint_version": 1,
            "job_snapshot_fingerprint": posting_fingerprint,
        },
    });
    let diff = tailored_resume.diff;
    let checksum_source = format!("{}|{}|{}", account_id, job_id, content);
    let checksum = hex::encode(Sha256::digest(checksum_source.as_bytes()));
    let now = now_ms();
    let resume = if finalize_immediately {
        save_resume_version(
            pool,
            account_id,
            job_id,
            mode,
            content,
            diff,
            approved_fact_ids.clone(),
            checksum,
        )?
    } else {
        ResumeVersion {
            id: String::new(),
            job_id: job_id.to_string(),
            version_no: 0,
            mode: mode.to_string(),
            content,
            diff,
            claim_ids: approved_fact_ids.clone(),
            checksum,
            created_at_ms: now,
        }
    };
    let mut application = existing_application.unwrap_or(JobApplication {
        id: uuid::Uuid::new_v4().to_string(),
        job_id: job_id.to_string(),
        resume_version_id: None,
        state: "preparing".to_string(),
        submission_mode: submission_mode.to_string(),
        match_score: posting.match_score,
        answers: Vec::new(),
        cover_letter: String::new(),
        receipt: json!({}),
        run_id: None,
        created_at_ms: now,
        updated_at_ms: now,
        submitted_at_ms: None,
    });
    let remembered_answers = answers_for_posting(pool, account_id, &posting)?;
    for remembered in remembered_answers {
        let key = remembered
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let already_answered = application.answers.iter().any(|answer| {
            ["key", "question", "field", "name"].iter().any(|field| {
                answer
                    .get(*field)
                    .and_then(Value::as_str)
                    .is_some_and(|value| normalize_answer_memory_key(value) == key)
            })
        });
        if !already_answered {
            application.answers.push(remembered);
        }
    }
    application.resume_version_id = (!resume.id.is_empty()).then(|| resume.id.clone());
    let auto_submit_eligible = submission_mode == "auto_submit" && eligibility.can_auto_submit;
    application.state = if !finalize_immediately {
        "preparing".to_string()
    } else if auto_submit_eligible {
        "queued".to_string()
    } else {
        "awaiting_review".to_string()
    };
    application.submission_mode = submission_mode.to_string();
    application.match_score = posting.match_score;
    application.updated_at_ms = now;
    application.receipt = json!({
        "job_snapshot": posting,
        "resume_version_id": if resume.id.is_empty() { Value::Null } else { json!(resume.id) },
        "career_track_id": posting.track_id,
        "candidate_truth_fingerprint": truth_fingerprint,
        "candidate_truth_fingerprint_version": 1,
        "confirmed_facts_fingerprint": confirmed_facts_fingerprint,
        "confirmed_facts_fingerprint_version": 1,
        "job_snapshot_fingerprint": posting_fingerprint,
        "application_identity": {
            "id": application_identity.id,
            "email": application_identity.email,
            "label": application_identity.label,
            "verified": true,
        },
        "prepared_at_ms": now,
        "final_answers": application.answers,
        "eligibility": eligibility,
        "cover_letter_status": if application.cover_letter.trim().is_empty() { "not_included" } else { "included" },
        "metering": {
            "status": if !finalize_immediately { "pending_generation" } else if auto_submit_eligible { "counts_when_queued" } else { "counts_when_approved_or_downloaded" },
            "canonical_job_key": posting.canonical_key,
        },
        "resume_generation": {
            "status": if finalize_immediately { "deterministic" } else { "pending" },
        },
        "confirmation": Value::Null,
    });
    if finalize_immediately {
        save_application(pool, account_id, &application)?;
    }
    Ok((application, resume, expected_application, profile, posting))
}

pub fn finalize_prepared_application(
    pool: &DbPool,
    account_id: &str,
    prepared: &PreparedApplicationDraft,
    content: Value,
    diff: Value,
    generation: Value,
) -> Result<(JobApplication, ResumeVersion)> {
    let mut application = prepared.application.clone();
    let baseline = &prepared.baseline_resume;
    if application.state != "preparing" {
        anyhow::bail!("application is not waiting for resume generation")
    }
    let posting = get_posting(pool, account_id, &application.job_id)?
        .ok_or_else(|| anyhow::anyhow!("job not found"))?;
    let expected_posting_fingerprint = posting_snapshot_fingerprint(&prepared.posting)?;
    if posting_snapshot_fingerprint(&posting)? != expected_posting_fingerprint {
        anyhow::bail!("job posting changed while the application packet was generated")
    }
    if application
        .receipt
        .get("job_snapshot_fingerprint")
        .and_then(Value::as_str)
        != Some(expected_posting_fingerprint.as_str())
        || content
            .pointer("/provenance/job_snapshot_fingerprint")
            .and_then(Value::as_str)
            != Some(expected_posting_fingerprint.as_str())
    {
        anyhow::bail!("generated resume does not match the job posting snapshot")
    }
    if baseline.job_id != posting.id {
        anyhow::bail!("application baseline targets a different job")
    }
    if content.pointer("/target/job_id").and_then(Value::as_str) != Some(posting.id.as_str()) {
        anyhow::bail!("generated resume targets a different job")
    }
    let expected_truth_fingerprint = application
        .receipt
        .get("candidate_truth_fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("application draft has no truth fingerprint"))?
        .to_string();
    if content
        .pointer("/provenance/candidate_truth_fingerprint")
        .and_then(Value::as_str)
        != Some(expected_truth_fingerprint.as_str())
    {
        anyhow::bail!("generated resume does not match the candidate truth snapshot")
    }
    if candidate_truth_fingerprint(&prepared.profile) != expected_truth_fingerprint {
        anyhow::bail!("application draft does not match the candidate truth snapshot")
    }
    let current_profile = get_profile(pool, account_id, "")?;
    if candidate_truth_fingerprint(&current_profile) != expected_truth_fingerprint {
        anyhow::bail!("candidate profile changed while the application packet was generated")
    }
    let expected_facts_fingerprint = application
        .receipt
        .get("confirmed_facts_fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("application draft has no confirmed-facts fingerprint"))?
        .to_string();
    if content
        .pointer("/provenance/confirmed_facts_fingerprint")
        .and_then(Value::as_str)
        != Some(expected_facts_fingerprint.as_str())
    {
        anyhow::bail!("generated resume does not match the confirmed candidate facts")
    }
    let expected_identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("application draft has no verified identity"))?
        .to_string();
    let expected_identity_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("application draft has no verified identity email"))?
        .to_string();
    if content
        .pointer("/provenance/application_identity_id")
        .and_then(Value::as_str)
        != Some(expected_identity_id.as_str())
        || content.pointer("/contact/email").and_then(Value::as_str)
            != Some(expected_identity_email.as_str())
    {
        anyhow::bail!("generated resume does not match the verified application identity")
    }

    let checksum_source = format!("{}|{}|{}", account_id, posting.id, content);
    let checksum = hex::encode(Sha256::digest(checksum_source.as_bytes()));
    let eligibility = evaluate_job_eligibility(
        pool,
        account_id,
        &posting,
        false,
        Some(application.id.as_str()),
    )?;
    let auto_submit_eligible =
        application.submission_mode == "auto_submit" && eligibility.can_auto_submit;
    application.state = if auto_submit_eligible {
        "queued".to_string()
    } else {
        "awaiting_review".to_string()
    };
    application.updated_at_ms = now_ms();
    let receipt = application
        .receipt
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?;
    receipt.insert(
        "eligibility".to_string(),
        serde_json::to_value(eligibility)?,
    );
    receipt.insert("resume_generation".to_string(), generation);
    receipt.insert(
        "prepared_at_ms".to_string(),
        json!(application.updated_at_ms),
    );
    receipt.insert(
        "metering".to_string(),
        json!({
            "status": if auto_submit_eligible { "counts_when_queued" } else { "counts_when_approved_or_downloaded" },
            "canonical_job_key": posting.canonical_key,
        }),
    );
    commit_prepared_application(
        pool,
        account_id,
        &mut application,
        &prepared.expected_application,
        baseline,
        content,
        diff,
        checksum,
        &expected_truth_fingerprint,
        &expected_posting_fingerprint,
        &expected_identity_id,
        &expected_identity_email,
        &expected_facts_fingerprint,
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_prepared_application(
    pool: &DbPool,
    account_id: &str,
    application: &mut JobApplication,
    expected: &Option<ExpectedApplicationRevision>,
    baseline: &ResumeVersion,
    content: Value,
    diff: Value,
    checksum: String,
    expected_truth_fingerprint: &str,
    expected_posting_fingerprint: &str,
    expected_identity_id: &str,
    expected_identity_email: &str,
    expected_facts_fingerprint: &str,
) -> Result<(JobApplication, ResumeVersion)> {
    validate_application_state(&application.state)?;
    let content_json = to_json(&content, "resume content")?;
    let diff_json = to_json(&diff, "resume diff")?;
    let claim_ids = baseline.claim_ids.clone();
    let claim_ids_json = to_json(&claim_ids, "resume claims")?;
    let resume_created_at_ms = now_ms();

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current_profile: CareerProfile = tx
                .query_row(
                    "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|raw| parse_json(raw, "Jobs profile during application finalization"))
                .transpose()?
                .unwrap_or_else(|| default_profile(""));
            if candidate_truth_fingerprint(&current_profile) != expected_truth_fingerprint {
                anyhow::bail!(
                    "candidate profile changed while the application packet was generated"
                )
            }
            let current_posting: JobPosting = tx
                .query_row(
                    "SELECT posting_json FROM jobs_postings
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application.job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|raw| parse_json(raw, "Jobs posting during application finalization"))
                .transpose()?
                .ok_or_else(|| anyhow::anyhow!("job not found during application finalization"))?;
            if posting_snapshot_fingerprint(&current_posting)? != expected_posting_fingerprint {
                anyhow::bail!("job posting changed while the application packet was generated")
            }
            let current_track: Option<CareerTrack> = tx
                .query_row(
                    "SELECT track_json FROM jobs_tracks
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, current_posting.track_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|raw| parse_json(raw, "career track during application finalization"))
                .transpose()?;
            let track_identity_id = current_track.and_then(|track| track.application_identity_id);
            let mut identity_stmt = tx.prepare(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities WHERE account_id = ?1",
            )?;
            let identity_rows = identity_stmt
                .query_map(params![account_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let identities = identity_rows
                .into_iter()
                .map(|(raw, status, is_default)| {
                    parse_application_identity_row(raw, status, is_default)
                })
                .collect::<Result<Vec<_>>>()?;
            let current_identity =
                selected_application_identity(track_identity_id.as_deref(), &identities);
            if current_identity.is_none_or(|identity| {
                identity.id != expected_identity_id || identity.email != expected_identity_email
            }) {
                anyhow::bail!("verified application identity changed during generation")
            }
            let mut fact_stmt = tx.prepare(
                "SELECT id, category, label, value_json, source, verification_status,
                        confirmed_at_ms, confirmed_by, schema_version,
                        created_at_ms, updated_at_ms
                   FROM jobs_facts
                  WHERE account_id = ?1 AND verification_status = 'confirmed'",
            )?;
            let current_facts = fact_stmt
                .query_map(params![account_id], fact_from_sqlite_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let mut current_fact_ids = current_facts
                .iter()
                .map(|fact| fact.id.as_str())
                .collect::<Vec<_>>();
            current_fact_ids.sort_unstable();
            let mut expected_fact_ids = claim_ids.iter().map(String::as_str).collect::<Vec<_>>();
            expected_fact_ids.sort_unstable();
            if current_fact_ids != expected_fact_ids
                || confirmed_facts_fingerprint(&current_facts) != expected_facts_fingerprint
            {
                anyhow::bail!("confirmed candidate facts changed during resume generation")
            }
            let preferences = tx
                .query_row(
                    "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|raw| {
                    parse_json::<JobPreferences>(raw, "Jobs preferences during finalization")
                })
                .transpose()?
                .unwrap_or_default();
            let preferences = enforce_job_preference_safety(preferences);
            let mut reservation_stmt = tx.prepare(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations WHERE account_id = ?1",
            )?;
            let reservations = reservation_stmt
                .query_map(params![account_id], |row| {
                    Ok(AttemptReservation {
                        id: row.get(0)?,
                        application_id: row.get(1)?,
                        company_key: row.get(2)?,
                        period_key: row.get(3)?,
                        runner: row.get(4)?,
                        status: row.get(5)?,
                        reserved_at_ms: row.get(6)?,
                        updated_at_ms: row.get(7)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let mut authority_stmt = tx.prepare(
                "SELECT s.id, s.provider, s.status, s.health,
                        m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = ?1 AND m.job_id = ?2",
            )?;
            let authorities = authority_stmt
                .query_map(params![account_id, application.job_id], |row| {
                    Ok(JobDiscoveryAuthority {
                        source_id: row.get(0)?,
                        provider: row.get(1)?,
                        source_status: row.get(2)?,
                        source_health: row.get(3)?,
                        membership_status: row.get(4)?,
                        last_seen_at_ms: row.get(5)?,
                        last_seen_run_id: row.get(6)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            enforce_application_finalization_eligibility(
                application,
                &current_posting,
                &current_profile,
                &preferences,
                &reservations,
                &authorities,
            )?;
            drop(identity_stmt);
            drop(fact_stmt);
            drop(reservation_stmt);
            drop(authority_stmt);
            let resume = if let Some(existing) = tx
                .query_row(
                    "SELECT id, job_id, version_no, mode, content_json, diff_json,
                            claim_ids_json, checksum, created_at_ms
                       FROM jobs_resume_versions
                      WHERE account_id = ?1 AND job_id = ?2 AND checksum = ?3",
                    params![account_id, application.job_id, checksum],
                    resume_from_sqlite_row,
                )
                .optional()?
            {
                existing
            } else {
                let version_no: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(version_no), 0) + 1 FROM jobs_resume_versions
                      WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, application.job_id],
                    |row| row.get(0),
                )?;
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO jobs_resume_versions (
                        id, account_id, job_id, version_no, mode, content_json,
                        diff_json, claim_ids_json, checksum, created_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        id,
                        account_id,
                        application.job_id,
                        version_no,
                        baseline.mode,
                        content_json,
                        diff_json,
                        claim_ids_json,
                        checksum,
                        resume_created_at_ms,
                    ],
                )?;
                ResumeVersion {
                    id,
                    job_id: application.job_id.clone(),
                    version_no,
                    mode: baseline.mode.clone(),
                    content: content.clone(),
                    diff: diff.clone(),
                    claim_ids: claim_ids.clone(),
                    checksum: checksum.clone(),
                    created_at_ms: resume_created_at_ms,
                }
            };

            application.resume_version_id = Some(resume.id.clone());
            application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                .insert("resume_version_id".to_string(), json!(resume.id));
            let payload = to_json(application, "job application")?;
            let changed = match expected {
                Some(expected) => tx.execute(
                    "UPDATE jobs_applications SET resume_version_id = ?5, state = ?6,
                            application_json = ?7, updated_at_ms = ?8, submitted_at_ms = ?9
                      WHERE account_id = ?1 AND job_id = ?2 AND id = ?3
                        AND state = ?4 AND updated_at_ms = ?10 AND application_json = ?11",
                    params![
                        account_id,
                        application.job_id,
                        expected.id,
                        expected.state,
                        application.resume_version_id,
                        application.state,
                        payload,
                        application.updated_at_ms,
                        application.submitted_at_ms,
                        expected.updated_at_ms,
                        expected.payload,
                    ],
                )?,
                None => tx.execute(
                    "INSERT INTO jobs_applications (
                        id, account_id, job_id, resume_version_id, state,
                        application_json, created_at_ms, updated_at_ms, submitted_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(account_id, job_id) DO NOTHING",
                    params![
                        application.id,
                        account_id,
                        application.job_id,
                        application.resume_version_id,
                        application.state,
                        payload,
                        application.created_at_ms,
                        application.updated_at_ms,
                        application.submitted_at_ms,
                    ],
                )?,
            };
            if changed != 1 {
                anyhow::bail!("application changed while resume generation was in progress");
            }
            tx.commit()?;
            Ok((application.clone(), resume))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::Serializable)
                .start()?;
            let current_profile: CareerProfile = tx
                .query_opt(
                    "SELECT profile_json FROM jobs_profiles WHERE account_id = $1 FOR SHARE",
                    &[&account_id],
                )?
                .map(|row| {
                    parse_json(
                        row.get::<_, String>(0),
                        "Jobs profile during application finalization",
                    )
                })
                .transpose()?
                .unwrap_or_else(|| default_profile(""));
            if candidate_truth_fingerprint(&current_profile) != expected_truth_fingerprint {
                anyhow::bail!(
                    "candidate profile changed while the application packet was generated"
                )
            }
            let current_posting: JobPosting = tx
                .query_opt(
                    "SELECT posting_json FROM jobs_postings
                      WHERE account_id = $1 AND id = $2 FOR SHARE",
                    &[&account_id, &application.job_id],
                )?
                .map(|row| {
                    parse_json(
                        row.get::<_, String>(0),
                        "Jobs posting during application finalization",
                    )
                })
                .transpose()?
                .ok_or_else(|| anyhow::anyhow!("job not found during application finalization"))?;
            if posting_snapshot_fingerprint(&current_posting)? != expected_posting_fingerprint {
                anyhow::bail!("job posting changed while the application packet was generated")
            }
            let current_track: Option<CareerTrack> = tx
                .query_opt(
                    "SELECT track_json FROM jobs_tracks
                      WHERE account_id = $1 AND id = $2 FOR SHARE",
                    &[&account_id, &current_posting.track_id],
                )?
                .map(|row| {
                    parse_json(
                        row.get::<_, String>(0),
                        "career track during application finalization",
                    )
                })
                .transpose()?;
            let track_identity_id = current_track.and_then(|track| track.application_identity_id);
            let identities = tx
                .query(
                    "SELECT identity_json, verification_status, is_default
                       FROM jobs_application_identities
                      WHERE account_id = $1 FOR SHARE",
                    &[&account_id],
                )?
                .into_iter()
                .map(|row| {
                    parse_application_identity_row(
                        row.get(0),
                        row.get(1),
                        row.get::<_, i32>(2) != 0,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let current_identity =
                selected_application_identity(track_identity_id.as_deref(), &identities);
            if current_identity.is_none_or(|identity| {
                identity.id != expected_identity_id || identity.email != expected_identity_email
            }) {
                anyhow::bail!("verified application identity changed during generation")
            }
            let current_facts = tx
                .query(
                    "SELECT id, category, label, value_json, source, verification_status,
                            confirmed_at_ms, confirmed_by, schema_version,
                            created_at_ms, updated_at_ms
                       FROM jobs_facts
                      WHERE account_id = $1 AND verification_status = 'confirmed'
                      FOR SHARE",
                    &[&account_id],
                )?
                .into_iter()
                .map(fact_from_pg_row)
                .collect::<Result<Vec<_>>>()?;
            let mut current_fact_ids = current_facts
                .iter()
                .map(|fact| fact.id.as_str())
                .collect::<Vec<_>>();
            current_fact_ids.sort_unstable();
            let mut expected_fact_ids = claim_ids.iter().map(String::as_str).collect::<Vec<_>>();
            expected_fact_ids.sort_unstable();
            if current_fact_ids != expected_fact_ids
                || confirmed_facts_fingerprint(&current_facts) != expected_facts_fingerprint
            {
                anyhow::bail!("confirmed candidate facts changed during resume generation")
            }
            let preferences = tx
                .query_opt(
                    "SELECT preferences_json FROM jobs_preferences
                      WHERE account_id = $1 FOR SHARE",
                    &[&account_id],
                )?
                .map(|row| {
                    parse_json::<JobPreferences>(
                        row.get::<_, String>(0),
                        "Jobs preferences during finalization",
                    )
                })
                .transpose()?
                .unwrap_or_default();
            let preferences = enforce_job_preference_safety(preferences);
            let reservations = tx
                .query(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = $1 FOR SHARE",
                    &[&account_id],
                )?
                .into_iter()
                .map(|row| AttemptReservation {
                    id: row.get(0),
                    application_id: row.get(1),
                    company_key: row.get(2),
                    period_key: row.get(3),
                    runner: row.get(4),
                    status: row.get(5),
                    reserved_at_ms: row.get(6),
                    updated_at_ms: row.get(7),
                })
                .collect::<Vec<_>>();
            let authorities = tx
                .query(
                    "SELECT s.id, s.provider, s.status, s.health,
                            m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                       FROM jobs_discovery_memberships m
                       JOIN jobs_discovery_sources s ON s.id = m.source_id
                      WHERE m.account_id = $1 AND m.job_id = $2
                      FOR SHARE OF m, s",
                    &[&account_id, &application.job_id],
                )?
                .into_iter()
                .map(|row| JobDiscoveryAuthority {
                    source_id: row.get(0),
                    provider: row.get(1),
                    source_status: row.get(2),
                    source_health: row.get(3),
                    membership_status: row.get(4),
                    last_seen_at_ms: row.get(5),
                    last_seen_run_id: row.get(6),
                })
                .collect::<Vec<_>>();
            enforce_application_finalization_eligibility(
                application,
                &current_posting,
                &current_profile,
                &preferences,
                &reservations,
                &authorities,
            )?;
            let resume = if let Some(row) = tx.query_opt(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions
                  WHERE account_id = $1 AND job_id = $2 AND checksum = $3",
                &[&account_id, &application.job_id, &checksum],
            )? {
                resume_from_pg_row(row)?
            } else {
                let version_no: i64 = tx
                    .query_one(
                        "SELECT COALESCE(MAX(version_no), 0) + 1 FROM jobs_resume_versions
                          WHERE account_id = $1 AND job_id = $2",
                        &[&account_id, &application.job_id],
                    )?
                    .get(0);
                let id = uuid::Uuid::new_v4().to_string();
                let inserted = tx.query_opt(
                    "INSERT INTO jobs_resume_versions (
                        id, account_id, job_id, version_no, mode, content_json,
                        diff_json, claim_ids_json, checksum, created_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                     ON CONFLICT (account_id, job_id, checksum) DO NOTHING
                     RETURNING id, job_id, version_no, mode, content_json, diff_json,
                               claim_ids_json, checksum, created_at_ms",
                    &[
                        &id,
                        &account_id,
                        &application.job_id,
                        &version_no,
                        &baseline.mode,
                        &content_json,
                        &diff_json,
                        &claim_ids_json,
                        &checksum,
                        &resume_created_at_ms,
                    ],
                )?;
                match inserted {
                    Some(row) => resume_from_pg_row(row)?,
                    None => resume_from_pg_row(tx.query_one(
                        "SELECT id, job_id, version_no, mode, content_json, diff_json,
                                claim_ids_json, checksum, created_at_ms
                           FROM jobs_resume_versions
                          WHERE account_id = $1 AND job_id = $2 AND checksum = $3",
                        &[&account_id, &application.job_id, &checksum],
                    )?)?,
                }
            };

            application.resume_version_id = Some(resume.id.clone());
            application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                .insert("resume_version_id".to_string(), json!(resume.id));
            let payload = to_json(application, "job application")?;
            let changed = match expected {
                Some(expected) => tx.execute(
                    "UPDATE jobs_applications SET resume_version_id = $5, state = $6,
                            application_json = $7, updated_at_ms = $8, submitted_at_ms = $9
                      WHERE account_id = $1 AND job_id = $2 AND id = $3
                        AND state = $4 AND updated_at_ms = $10 AND application_json = $11",
                    &[
                        &account_id,
                        &application.job_id,
                        &expected.id,
                        &expected.state,
                        &application.resume_version_id,
                        &application.state,
                        &payload,
                        &application.updated_at_ms,
                        &application.submitted_at_ms,
                        &expected.updated_at_ms,
                        &expected.payload,
                    ],
                )?,
                None => tx.execute(
                    "INSERT INTO jobs_applications (
                        id, account_id, job_id, resume_version_id, state,
                        application_json, created_at_ms, updated_at_ms, submitted_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                     ON CONFLICT(account_id, job_id) DO NOTHING",
                    &[
                        &application.id,
                        &account_id,
                        &application.job_id,
                        &application.resume_version_id,
                        &application.state,
                        &payload,
                        &application.created_at_ms,
                        &application.updated_at_ms,
                        &application.submitted_at_ms,
                    ],
                )?,
            };
            if changed != 1 {
                anyhow::bail!("application changed while resume generation was in progress");
            }
            tx.commit()?;
            Ok((application.clone(), resume))
        }
    })
}

fn answers_for_posting(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
) -> Result<Vec<Value>> {
    let company_scope = normalize_company_scope(&posting.company);
    let mut selected: BTreeMap<String, (u8, AnswerMemory)> = BTreeMap::new();
    for answer in list_answer_memory(pool, account_id)? {
        if !answer.confirmed {
            continue;
        }
        let rank = match answer.scope.as_str() {
            "company" if answer.scope_id.as_deref() == Some(company_scope.as_str()) => 3,
            "track"
                if !posting.track_id.is_empty()
                    && answer.scope_id.as_deref() == Some(posting.track_id.as_str()) =>
            {
                2
            }
            "account" => 1,
            _ => continue,
        };
        match selected.get(&answer.key) {
            Some((existing_rank, _)) if *existing_rank >= rank => {}
            _ => {
                selected.insert(answer.key.clone(), (rank, answer));
            }
        }
    }
    Ok(selected
        .into_values()
        .map(|(_, answer)| {
            json!({
                "key": answer.key,
                "question": answer.question,
                "value": answer.value,
                "source": "answer_memory",
                "memory_id": answer.id,
                "scope": answer.scope,
                "scope_id": answer.scope_id,
            })
        })
        .collect())
}

fn normalize_company_scope(company: &str) -> String {
    let mut value = String::new();
    let mut separator = false;
    for character in company.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !value.is_empty() {
                value.push('-');
            }
            value.push(character);
            separator = false;
        } else {
            separator = true;
        }
    }
    value
}

fn account_login_email(pool: &DbPool, account_id: &str) -> Result<String> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT email FROM accounts WHERE id = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .context("get Bluey account email"),
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query_one("SELECT email FROM accounts WHERE id = $1", &[&account_id])?
            .get(0)),
    })
}

#[allow(clippy::too_many_arguments)]
fn save_resume_version(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    mode: &str,
    content: Value,
    diff: Value,
    claim_ids: Vec<String>,
    checksum: String,
) -> Result<ResumeVersion> {
    let content_json = to_json(&content, "resume content")?;
    let diff_json = to_json(&diff, "resume diff")?;
    let claim_ids_json = to_json(&claim_ids, "resume claims")?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            if let Some(existing) = conn
                .query_row(
                    "SELECT id, job_id, version_no, mode, content_json, diff_json,
                            claim_ids_json, checksum, created_at_ms
                       FROM jobs_resume_versions
                      WHERE account_id = ?1 AND job_id = ?2 AND checksum = ?3",
                    params![account_id, job_id, checksum],
                    resume_from_sqlite_row,
                )
                .optional()?
            {
                return Ok(existing);
            }
            let version_no: i64 = conn.query_row(
                "SELECT COALESCE(MAX(version_no), 0) + 1 FROM jobs_resume_versions
                  WHERE account_id = ?1 AND job_id = ?2",
                params![account_id, job_id],
                |row| row.get(0),
            )?;
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO jobs_resume_versions (
                    id, account_id, job_id, version_no, mode, content_json,
                    diff_json, claim_ids_json, checksum, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    account_id,
                    job_id,
                    version_no,
                    mode,
                    content_json,
                    diff_json,
                    claim_ids_json,
                    checksum,
                    now,
                ],
            )?;
            Ok(ResumeVersion {
                id,
                job_id: job_id.to_string(),
                version_no,
                mode: mode.to_string(),
                content,
                diff,
                claim_ids,
                checksum,
                created_at_ms: now,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            if let Some(row) = conn.query_opt(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions
                  WHERE account_id = $1 AND job_id = $2 AND checksum = $3",
                &[&account_id, &job_id, &checksum],
            )? {
                return resume_from_pg_row(row);
            }
            let version_no: i64 = conn
                .query_one(
                    "SELECT COALESCE(MAX(version_no), 0) + 1 FROM jobs_resume_versions
                      WHERE account_id = $1 AND job_id = $2",
                    &[&account_id, &job_id],
                )?
                .get(0);
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO jobs_resume_versions (
                    id, account_id, job_id, version_no, mode, content_json,
                    diff_json, claim_ids_json, checksum, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                &[
                    &id,
                    &account_id,
                    &job_id,
                    &version_no,
                    &mode,
                    &content_json,
                    &diff_json,
                    &claim_ids_json,
                    &checksum,
                    &now,
                ],
            )?;
            Ok(ResumeVersion {
                id,
                job_id: job_id.to_string(),
                version_no,
                mode: mode.to_string(),
                content,
                diff,
                claim_ids,
                checksum,
                created_at_ms: now,
            })
        }
    })
}

fn find_application_for_job_with_revision(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
) -> Result<Option<(JobApplication, ExpectedApplicationRevision)>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<(String, String, i64, String)> = conn
                .query_row(
                    "SELECT id, state, updated_at_ms, application_json
                       FROM jobs_applications WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, job_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            raw.map(|(id, state, updated_at_ms, value)| {
                let application =
                    parse_application_json(value.clone(), &id, job_id, "job application")?;
                Ok((
                    application,
                    ExpectedApplicationRevision {
                        id,
                        state,
                        updated_at_ms,
                        payload: value,
                    },
                ))
            })
            .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, state, updated_at_ms, application_json
                   FROM jobs_applications WHERE account_id = $1 AND job_id = $2",
                &[&account_id, &job_id],
            )?
            .map(|row| {
                let value: String = row.get(3);
                let id: String = row.get(0);
                let application =
                    parse_application_json(value.clone(), &id, job_id, "job application")?;
                Ok((
                    application,
                    ExpectedApplicationRevision {
                        id,
                        state: row.get(1),
                        updated_at_ms: row.get(2),
                        payload: value,
                    },
                ))
            })
            .transpose(),
    })
}

fn save_application(
    pool: &DbPool,
    account_id: &str,
    application: &JobApplication,
) -> Result<JobApplication> {
    validate_application_state(&application.state)?;
    let payload = to_json(application, "job application")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_applications (
                    id, account_id, job_id, resume_version_id, state,
                    application_json, created_at_ms, updated_at_ms, submitted_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(account_id, job_id) DO UPDATE SET
                    resume_version_id = excluded.resume_version_id,
                    state = excluded.state,
                    application_json = excluded.application_json,
                    updated_at_ms = excluded.updated_at_ms,
                    submitted_at_ms = excluded.submitted_at_ms",
                params![
                    application.id,
                    account_id,
                    application.job_id,
                    application.resume_version_id,
                    application.state,
                    payload,
                    application.created_at_ms,
                    application.updated_at_ms,
                    application.submitted_at_ms,
                ],
            )?;
            Ok(application.clone())
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_applications (
                    id, account_id, job_id, resume_version_id, state,
                    application_json, created_at_ms, updated_at_ms, submitted_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(account_id, job_id) DO UPDATE SET
                    resume_version_id = EXCLUDED.resume_version_id,
                    state = EXCLUDED.state,
                    application_json = EXCLUDED.application_json,
                    updated_at_ms = EXCLUDED.updated_at_ms,
                    submitted_at_ms = EXCLUDED.submitted_at_ms",
                &[
                    &application.id,
                    &account_id,
                    &application.job_id,
                    &application.resume_version_id,
                    &application.state,
                    &payload,
                    &application.created_at_ms,
                    &application.updated_at_ms,
                    &application.submitted_at_ms,
                ],
            )?;
            Ok(application.clone())
        }
    })
}

pub fn update_application(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    state: &str,
    submission_mode: Option<&str>,
) -> Result<Option<JobApplication>> {
    validate_application_state(state)?;
    let Some(mut application) = get_application(pool, account_id, application_id)? else {
        return Ok(None);
    };
    validate_application_transition(&application.state, state)?;
    if matches!(state, "queued" | "running") {
        let posting = get_posting(pool, account_id, &application.job_id)?
            .ok_or_else(|| anyhow::anyhow!("job not found"))?;
        let eligibility =
            evaluate_job_eligibility(pool, account_id, &posting, true, Some(&application.id))?;
        if !eligibility.can_queue_local {
            anyhow::bail!(eligibility_error_message(&eligibility))
        }
        if let Some(receipt) = application.receipt.as_object_mut() {
            receipt.insert(
                "eligibility".to_string(),
                serde_json::to_value(eligibility)?,
            );
        }
    }
    if state == "submitted"
        && !application_submission_evidence_complete(pool, account_id, &application)?
    {
        anyhow::bail!("attach the exact resume used and submission confirmation before marking this application submitted")
    }
    application.state = state.to_string();
    if let Some(mode) = submission_mode {
        if !matches!(mode, "review_first" | "auto_submit") {
            anyhow::bail!("invalid application submission mode");
        }
        application.submission_mode = mode.to_string();
    }
    application.updated_at_ms = now_ms();
    if state == "submitted" {
        application.submitted_at_ms = Some(application.updated_at_ms);
    }
    save_application(pool, account_id, &application).map(Some)
}

pub fn assign_application_run(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<Option<JobApplication>> {
    let Some(mut application) = get_application(pool, account_id, application_id)? else {
        return Ok(None);
    };
    application.run_id = Some(run_id.to_string());
    application.updated_at_ms = now_ms();
    save_application(pool, account_id, &application).map(Some)
}

pub fn replace_application_receipt(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    receipt: Value,
) -> Result<Option<JobApplication>> {
    let Some(mut application) = get_application(pool, account_id, application_id)? else {
        return Ok(None);
    };
    application.receipt = receipt;
    application.updated_at_ms = now_ms();
    save_application(pool, account_id, &application).map(Some)
}

pub fn validate_application_state(state: &str) -> Result<()> {
    const STATES: &[&str] = &[
        "matched",
        "preparing",
        "needs_confirmation",
        "awaiting_review",
        "queued",
        "running",
        "needs_input",
        "side_effect_unknown",
        "submitted",
        "failed",
    ];
    if STATES.contains(&state) {
        Ok(())
    } else {
        anyhow::bail!("invalid application state")
    }
}

fn validate_application_transition(current: &str, next: &str) -> Result<()> {
    if current == next {
        return Ok(());
    }
    let allowed = match current {
        "matched" => matches!(next, "preparing" | "awaiting_review" | "failed"),
        "preparing" => matches!(
            next,
            "needs_confirmation" | "awaiting_review" | "queued" | "failed"
        ),
        "needs_confirmation" => matches!(next, "awaiting_review" | "failed"),
        "awaiting_review" => matches!(next, "queued" | "failed"),
        "queued" => matches!(
            next,
            "awaiting_review" | "running" | "needs_input" | "failed"
        ),
        "running" => matches!(
            next,
            "needs_input" | "side_effect_unknown" | "submitted" | "failed"
        ),
        "needs_input" => matches!(
            next,
            "queued" | "running" | "side_effect_unknown" | "failed"
        ),
        "side_effect_unknown" => matches!(next, "needs_input" | "submitted" | "failed"),
        "failed" => matches!(next, "queued" | "awaiting_review"),
        "submitted" => false,
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        anyhow::bail!("invalid application state transition")
    }
}

fn current_period() -> (i64, i64) {
    let now = Utc::now();
    let start = Utc
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(now);
    let (year, month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let end = Utc
        .with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .unwrap_or(now);
    (start.timestamp_millis(), end.timestamp_millis())
}

pub fn get_entitlement(pool: &DbPool, account_id: &str) -> Result<JobsEntitlement> {
    let (period_start, period_end) = current_period();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO jobs_entitlements (
                    account_id, plan, track_limit, monthly_packet_limit, used_packets,
                    period_start_ms, period_end_ms, local_browser, cloud_browser, updated_at_ms
                 ) VALUES (?1, 'free', 1, 5, 0, ?2, ?3, 0, 0, ?4)
                 ON CONFLICT(account_id) DO NOTHING",
                params![account_id, period_start, period_end, now_ms()],
            )?;
            conn.execute(
                "UPDATE jobs_entitlements SET used_packets = 0, period_start_ms = ?2,
                    period_end_ms = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND period_end_ms <= ?4",
                params![account_id, period_start, period_end, now_ms()],
            )?;
            conn.query_row(
                "SELECT plan, track_limit, monthly_packet_limit, used_packets,
                        period_start_ms, period_end_ms, local_browser, cloud_browser
                   FROM jobs_entitlements WHERE account_id = ?1",
                params![account_id],
                |row| {
                    Ok(entitlement_with_policy(
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get::<_, i64>(6)? != 0,
                        row.get::<_, i64>(7)? != 0,
                    ))
                },
            )
            .context("get Jobs entitlement")
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "INSERT INTO jobs_entitlements (
                    account_id, plan, track_limit, monthly_packet_limit, used_packets,
                    period_start_ms, period_end_ms, local_browser, cloud_browser, updated_at_ms
                 ) VALUES ($1, 'free', 1, 5, 0, $2, $3, 0, 0, $4)
                 ON CONFLICT(account_id) DO NOTHING",
                &[&account_id, &period_start, &period_end, &now_ms()],
            )?;
            conn.execute(
                "UPDATE jobs_entitlements SET used_packets = 0, period_start_ms = $2,
                    period_end_ms = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND period_end_ms <= $4",
                &[&account_id, &period_start, &period_end, &now_ms()],
            )?;
            let row = conn.query_one(
                "SELECT plan, track_limit, monthly_packet_limit, used_packets,
                        period_start_ms, period_end_ms, local_browser, cloud_browser
                   FROM jobs_entitlements WHERE account_id = $1",
                &[&account_id],
            )?;
            Ok(entitlement_with_policy(
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                row.get(5),
                row.get::<_, i32>(6) != 0,
                row.get::<_, i32>(7) != 0,
            ))
        }
    })
}

pub fn set_entitlement_plan(
    pool: &DbPool,
    account_id: &str,
    plan: &str,
) -> Result<JobsEntitlement> {
    let policy = plan_policy(plan);
    let track_limit = policy.track_limit;
    let packet_limit = policy.packet_limit;
    let local_browser = i32::from(policy.local_browser);
    let cloud_browser = i32::from(policy.cloud_browser);
    let _ = get_entitlement(pool, account_id)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "UPDATE jobs_entitlements SET plan = ?2, track_limit = ?3,
                    monthly_packet_limit = ?4, local_browser = ?5, cloud_browser = ?6,
                    updated_at_ms = ?7 WHERE account_id = ?1",
                params![
                    account_id,
                    plan,
                    track_limit,
                    packet_limit,
                    local_browser,
                    cloud_browser,
                    now_ms(),
                ],
            )?;
            get_entitlement(pool, account_id)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "UPDATE jobs_entitlements SET plan = $2, track_limit = $3,
                    monthly_packet_limit = $4, local_browser = $5, cloud_browser = $6,
                    updated_at_ms = $7 WHERE account_id = $1",
                &[
                    &account_id,
                    &plan,
                    &track_limit,
                    &packet_limit,
                    &local_browser,
                    &cloud_browser,
                    &now_ms(),
                ],
            )?;
            get_entitlement(pool, account_id)
        }
    })
}

pub fn commit_packet(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
) -> Result<PacketCommitResult> {
    let application = get_application(pool, account_id, application_id)?
        .ok_or_else(|| anyhow::anyhow!("application not found"))?;
    if application.resume_version_id.is_none() {
        anyhow::bail!("application packet has no job-specific resume");
    }
    let _ = get_entitlement(pool, account_id)?;
    let now = now_ms();
    let metering_key = format!("jobs-packet:{}", application.job_id);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if let Some((included, amount_cents)) = tx
                .query_row(
                    "SELECT included, amount_cents FROM jobs_packet_metering
                      WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, application.job_id],
                    |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)?)),
                )
                .optional()?
            {
                let (used, limit): (i64, i64) = tx.query_row(
                    "SELECT used_packets, monthly_packet_limit FROM jobs_entitlements WHERE account_id = ?1",
                    params![account_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                return Ok(PacketCommitResult {
                    newly_metered: false,
                    included,
                    amount_cents,
                    used_packets: used,
                    monthly_packet_limit: limit,
                });
            }
            let (used, limit): (i64, i64) = tx.query_row(
                "SELECT used_packets, monthly_packet_limit FROM jobs_entitlements WHERE account_id = ?1",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let allowance: Option<String> = tx
                .query_row(
                    "SELECT status
                       FROM jobs_generation_allowance_reservations
                      WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, application.job_id],
                    |row| row.get(0),
                )
                .optional()?;
            if allowance.as_deref() == Some("committed") {
                anyhow::bail!("committed Jobs allowance has no packet metering row")
            }
            let pre_reserved = allowance.as_deref() == Some("reserved");
            let included = pre_reserved || used < limit;
            let amount_cents = if included { 0 } else { PACKET_OVERAGE_CENTS };
            if amount_cents > 0 {
                let balance_before: i64 = tx.query_row(
                    "SELECT balance_cents FROM accounts WHERE id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )?;
                if tx.execute(
                    "UPDATE accounts SET balance_cents = balance_cents - ?1
                      WHERE id = ?2 AND balance_cents >= ?1",
                    params![amount_cents, account_id],
                )? == 0
                {
                    anyhow::bail!("insufficient Bluey balance for Jobs overage");
                }
                crate::db::balance::consume_credit_batches_tx(&tx, account_id, amount_cents)?;
                crate::db::balance::insert_balance_ledger_sqlite_tx(
                    &tx,
                    crate::db::balance::BalanceLedgerEntry {
                        account_id,
                        event_type: "jobs_packet_overage",
                        amount_cents: -amount_cents,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_before - amount_cents,
                        reason: Some("jobs_completed_packet"),
                        provider: None,
                        processor_payment_id: None,
                        source_id: Some(&application.job_id),
                        idempotency_key: Some(&metering_key),
                        request_id: Some(&metering_key),
                        metadata_json: Some("{\"product\":\"bluey_jobs\"}"),
                    },
                )?;
            }
            tx.execute(
                "INSERT INTO jobs_packet_metering (
                    account_id, job_id, application_id, metering_key,
                    included, amount_cents, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    account_id,
                    application.job_id,
                    application.id,
                    metering_key,
                    i64::from(included),
                    amount_cents,
                    now,
                ],
            )?;
            tx.execute(
                "UPDATE jobs_entitlements SET used_packets = used_packets + 1,
                    updated_at_ms = ?2 WHERE account_id = ?1 AND ?3 = 0",
                params![account_id, now, i64::from(pre_reserved)],
            )?;
            if allowance.as_deref() == Some("reserved") {
                tx.execute(
                    "UPDATE jobs_generation_allowance_reservations
                        SET status = 'committed', application_id = ?3, updated_at_ms = ?4
                      WHERE account_id = ?1 AND job_id = ?2 AND status = 'reserved'",
                    params![account_id, application.job_id, application.id, now],
                )?;
            }
            tx.commit()?;
            Ok(PacketCommitResult {
                newly_metered: true,
                included,
                amount_cents,
                used_packets: used + i64::from(!pre_reserved),
                monthly_packet_limit: limit,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&format!(
                    "jobs-allowance:{account_id}:{}",
                    application.job_id
                )],
            )?;
            let entitlement = tx.query_one(
                "SELECT used_packets, monthly_packet_limit FROM jobs_entitlements
                  WHERE account_id = $1 FOR UPDATE",
                &[&account_id],
            )?;
            if let Some(row) = tx.query_opt(
                "SELECT included, amount_cents FROM jobs_packet_metering
                  WHERE account_id = $1 AND job_id = $2",
                &[&account_id, &application.job_id],
            )? {
                return Ok(PacketCommitResult {
                    newly_metered: false,
                    included: row.get::<_, i32>(0) != 0,
                    amount_cents: row.get(1),
                    used_packets: entitlement.get(0),
                    monthly_packet_limit: entitlement.get(1),
                });
            }
            let used: i64 = entitlement.get(0);
            let limit: i64 = entitlement.get(1);
            let allowance = tx.query_opt(
                "SELECT status
                   FROM jobs_generation_allowance_reservations
                  WHERE account_id = $1 AND job_id = $2 FOR UPDATE",
                &[&account_id, &application.job_id],
            )?;
            if allowance
                .as_ref()
                .is_some_and(|row| row.get::<_, String>(0) == "committed")
            {
                anyhow::bail!("committed Jobs allowance has no packet metering row")
            }
            let pre_reserved = allowance
                .as_ref()
                .is_some_and(|row| row.get::<_, String>(0) == "reserved");
            let included = pre_reserved || used < limit;
            let included_db = i32::from(included);
            let amount_cents = if included { 0 } else { PACKET_OVERAGE_CENTS };
            if amount_cents > 0 {
                let balance_before: i64 = tx
                    .query_one(
                        "SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE",
                        &[&account_id],
                    )?
                    .get(0);
                if tx.execute(
                    "UPDATE accounts SET balance_cents = balance_cents - $1
                      WHERE id = $2 AND balance_cents >= $1",
                    &[&amount_cents, &account_id],
                )? == 0
                {
                    anyhow::bail!("insufficient Bluey balance for Jobs overage");
                }
                crate::db::balance::consume_credit_batches_pg_tx(
                    &mut tx,
                    account_id,
                    amount_cents,
                )?;
                crate::db::balance::insert_balance_ledger_pg_tx(
                    &mut tx,
                    crate::db::balance::BalanceLedgerEntry {
                        account_id,
                        event_type: "jobs_packet_overage",
                        amount_cents: -amount_cents,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_before - amount_cents,
                        reason: Some("jobs_completed_packet"),
                        provider: None,
                        processor_payment_id: None,
                        source_id: Some(&application.job_id),
                        idempotency_key: Some(&metering_key),
                        request_id: Some(&metering_key),
                        metadata_json: Some("{\"product\":\"bluey_jobs\"}"),
                    },
                )?;
            }
            tx.execute(
                "INSERT INTO jobs_packet_metering (
                    account_id, job_id, application_id, metering_key,
                    included, amount_cents, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &account_id,
                    &application.job_id,
                    &application.id,
                    &metering_key,
                    &included_db,
                    &amount_cents,
                    &now,
                ],
            )?;
            tx.execute(
                "UPDATE jobs_entitlements SET used_packets = used_packets + 1,
                    updated_at_ms = $2 WHERE account_id = $1 AND $3 = 0",
                &[&account_id, &now, &i32::from(pre_reserved)],
            )?;
            if allowance
                .as_ref()
                .is_some_and(|row| row.get::<_, String>(0) == "reserved")
            {
                tx.execute(
                    "UPDATE jobs_generation_allowance_reservations
                        SET status = 'committed', application_id = $3, updated_at_ms = $4
                      WHERE account_id = $1 AND job_id = $2 AND status = 'reserved'",
                    &[&account_id, &application.job_id, &application.id, &now],
                )?;
            }
            tx.commit()?;
            Ok(PacketCommitResult {
                newly_metered: true,
                included,
                amount_cents,
                used_packets: used + i64::from(!pre_reserved),
                monthly_packet_limit: limit,
            })
        }
    })
}

pub fn list_browser_sessions(pool: &DbPool, account_id: &str) -> Result<Vec<BrowserSession>> {
    list_payloads(
        pool,
        account_id,
        "jobs_browser_sessions",
        "session_json",
        "updated_at_ms DESC",
        "browser session",
    )
}

pub fn upsert_browser_session(
    pool: &DbPool,
    account_id: &str,
    session: &BrowserSession,
) -> Result<BrowserSession> {
    let mut value = session.clone();
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    let payload = to_json(&value, "browser session")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_browser_sessions (
                    id, account_id, runner, status, session_json, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status,
                    session_json = excluded.session_json, updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_browser_sessions.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.runner,
                    value.status,
                    payload,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_browser_sessions (
                    id, account_id, runner, status, session_json, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT(id) DO UPDATE SET status = EXCLUDED.status,
                    session_json = EXCLUDED.session_json, updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_browser_sessions.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.runner,
                    &value.status,
                    &payload,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn list_interventions(pool: &DbPool, account_id: &str) -> Result<Vec<Intervention>> {
    list_payloads(
        pool,
        account_id,
        "jobs_interventions",
        "intervention_json",
        "status = 'open' DESC, created_at_ms DESC",
        "intervention",
    )
}

pub fn save_intervention(
    pool: &DbPool,
    account_id: &str,
    intervention: &Intervention,
) -> Result<Intervention> {
    let mut value = intervention.clone();
    value.kind = value.kind.trim().to_ascii_lowercase();
    value.status = value.status.trim().to_ascii_lowercase();
    value.resolution_kind = value.resolution_kind.trim().to_ascii_lowercase();
    value.provider = value.provider.trim().to_ascii_lowercase();
    if !matches!(
        value.kind.as_str(),
        "captcha"
            | "two_factor"
            | "assessment"
            | "unknown_question"
            | "missing_fact"
            | "sensitive_question"
            | "browser_takeover"
    ) {
        anyhow::bail!("invalid intervention kind")
    }
    if !matches!(
        value.status.as_str(),
        "open" | "approved" | "resolved" | "expired" | "cancelled"
    ) {
        anyhow::bail!("invalid intervention status")
    }
    if !matches!(
        value.resolution_kind.as_str(),
        "" | "browser_takeover" | "email_otp_approval" | "answer"
    ) {
        anyhow::bail!("invalid intervention resolution")
    }
    if let Some(application_id) = value.application_id.as_deref() {
        if get_application(pool, account_id, application_id)?.is_none() {
            anyhow::bail!("application not found")
        }
    }
    if value.resolution_kind == "email_otp_approval"
        && (value.kind != "two_factor"
            || !matches!(value.provider.as_str(), "gmail" | "outlook_email")
            || value.provider_message_id.trim().is_empty()
            || value.expires_at_ms.is_none())
    {
        anyhow::bail!("email verification needs a provider message and expiry")
    }
    if value.metadata.is_null() {
        value.metadata = json!({});
    }
    if contains_authentication_secret(&value.metadata) {
        anyhow::bail!("authentication codes and credentials cannot be stored in interventions")
    }
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    if value.status == "open"
        && value
            .expires_at_ms
            .is_some_and(|expires_at| expires_at <= now)
    {
        value.status = "expired".to_string();
    }
    if matches!(value.status.as_str(), "resolved" | "cancelled" | "expired")
        && value.resolved_at_ms.is_none()
    {
        value.resolved_at_ms = Some(now);
    }
    let payload = to_json(&value, "intervention")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_interventions (
                    id, account_id, application_id, kind, status, intervention_json,
                    created_at_ms, resolved_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status,
                    intervention_json = excluded.intervention_json,
                    resolved_at_ms = excluded.resolved_at_ms
                 WHERE jobs_interventions.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.application_id,
                    value.kind,
                    value.status,
                    payload,
                    value.created_at_ms,
                    value.resolved_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_interventions (
                    id, account_id, application_id, kind, status, intervention_json,
                    created_at_ms, resolved_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(id) DO UPDATE SET status = EXCLUDED.status,
                    intervention_json = EXCLUDED.intervention_json,
                    resolved_at_ms = EXCLUDED.resolved_at_ms
                 WHERE jobs_interventions.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.application_id,
                    &value.kind,
                    &value.status,
                    &payload,
                    &value.created_at_ms,
                    &value.resolved_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn normalize_answer_memory_key(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(character);
            pending_space = false;
        } else {
            pending_space = true;
        }
    }
    normalized
}

pub fn list_answer_memory(pool: &DbPool, account_id: &str) -> Result<Vec<AnswerMemory>> {
    list_payloads(
        pool,
        account_id,
        "jobs_answer_memory",
        "answer_json",
        "updated_at_ms DESC",
        "answer memory",
    )
}

pub fn save_answer_memory(
    pool: &DbPool,
    account_id: &str,
    answer: &AnswerMemory,
) -> Result<AnswerMemory> {
    let mut value = answer.clone();
    value.question = value.question.trim().to_string();
    value.value = value.value.trim().to_string();
    value.scope = value.scope.trim().to_ascii_lowercase();
    value.source = value.source.trim().to_ascii_lowercase();
    value.key = normalize_answer_memory_key(if value.key.trim().is_empty() {
        &value.question
    } else {
        &value.key
    });
    if value.question.is_empty() || value.key.is_empty() {
        anyhow::bail!("enter the application question")
    }
    if value.value.is_empty() {
        anyhow::bail!("enter the answer Bluey should remember")
    }
    if value.question.len() > 2_000 || value.value.len() > 10_000 {
        anyhow::bail!("answer memory is too long")
    }
    if !matches!(value.scope.as_str(), "account" | "track" | "company") {
        anyhow::bail!("invalid answer memory scope")
    }
    let scope_id = value
        .scope_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    if value.scope == "account" {
        value.scope_id = None;
    } else if scope_id.is_empty() {
        anyhow::bail!("choose where this answer should be reused")
    } else {
        value.scope_id = Some(scope_id);
    }
    if value.scope == "track"
        && !list_tracks(pool, account_id)?
            .iter()
            .any(|track| Some(track.id.as_str()) == value.scope_id.as_deref())
    {
        anyhow::bail!("career track not found")
    }
    if value.source.is_empty() {
        value.source = "settings".to_string();
    }
    value.confirmed = true;

    let existing = list_answer_memory(pool, account_id)?
        .into_iter()
        .find(|item| {
            item.scope == value.scope
                && item.scope_id.as_deref().unwrap_or_default()
                    == value.scope_id.as_deref().unwrap_or_default()
                && item.key == value.key
        });
    if let Some(existing) = existing {
        value.id = existing.id;
        if value.created_at_ms == 0 {
            value.created_at_ms = existing.created_at_ms;
        }
        if value.last_used_at_ms.is_none() {
            value.last_used_at_ms = existing.last_used_at_ms;
        }
        value.use_count = value.use_count.max(existing.use_count);
    }
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    value.use_count = value.use_count.max(0);
    let scope_id = value.scope_id.as_deref().unwrap_or_default().to_string();
    let question_hash = private_lookup_hash(
        &format!("jobs-answer-memory:{account_id}:{}:{scope_id}", value.scope),
        &value.key,
    )?;
    let payload = to_json(&value, "answer memory")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_answer_memory (
                    id, account_id, scope, scope_id, question_hash, answer_json,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET scope = excluded.scope,
                    scope_id = excluded.scope_id, question_hash = excluded.question_hash,
                    answer_json = excluded.answer_json, updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_answer_memory.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.scope,
                    scope_id,
                    question_hash,
                    payload,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_answer_memory (
                    id, account_id, scope, scope_id, question_hash, answer_json,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(id) DO UPDATE SET scope = EXCLUDED.scope,
                    scope_id = EXCLUDED.scope_id, question_hash = EXCLUDED.question_hash,
                    answer_json = EXCLUDED.answer_json, updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_answer_memory.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.scope,
                    &scope_id,
                    &question_hash,
                    &payload,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn delete_answer_memory(pool: &DbPool, account_id: &str, answer_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "DELETE FROM jobs_answer_memory WHERE account_id = ?1 AND id = ?2",
            params![account_id, answer_id],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "DELETE FROM jobs_answer_memory WHERE account_id = $1 AND id = $2",
            &[&account_id, &answer_id],
        )? > 0),
    })
}

pub fn list_candidate_events(pool: &DbPool, account_id: &str) -> Result<Vec<CandidateEvent>> {
    list_payloads(
        pool,
        account_id,
        "jobs_candidate_events",
        "event_json",
        "created_at_ms DESC, id DESC",
        "candidate event",
    )
}

pub fn save_candidate_event(
    pool: &DbPool,
    account_id: &str,
    event: &CandidateEvent,
) -> Result<CandidateEvent> {
    let mut value = event.clone();
    value.event_type = value.event_type.trim().to_ascii_lowercase();
    value.action = value.action.trim().to_ascii_lowercase();
    value.note = value.note.trim().to_string();
    value.reasons = value
        .reasons
        .iter()
        .map(|reason| reason.trim().to_ascii_lowercase())
        .filter(|reason| !reason.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if value.note.len() > 1_000 || value.reasons.len() > 8 {
        anyhow::bail!("candidate feedback is too long")
    }

    let application = if let Some(application_id) = value.application_id.as_deref() {
        Some(
            get_application(pool, account_id, application_id)?
                .ok_or_else(|| anyhow::anyhow!("application not found"))?,
        )
    } else {
        None
    };
    if value.job_id.is_none() {
        value.job_id = application.as_ref().map(|item| item.job_id.clone());
    }
    if let Some(application) = application.as_ref() {
        if value.job_id.as_deref() != Some(application.job_id.as_str()) {
            anyhow::bail!("application does not belong to this job")
        }
    }
    if let Some(job_id) = value.job_id.as_deref() {
        if get_posting(pool, account_id, job_id)?.is_none() {
            anyhow::bail!("job not found")
        }
    }

    value.status = match value.event_type.as_str() {
        "match_feedback" => {
            if value.application_id.is_some() || value.job_id.is_none() {
                anyhow::bail!("match feedback must reference one job")
            }
            if !matches!(value.action.as_str(), "pass" | "restore") {
                anyhow::bail!("invalid match feedback action")
            }
            if value.action == "pass"
                && value.reasons.iter().any(|reason| {
                    !matches!(
                        reason.as_str(),
                        "role_mismatch"
                            | "location"
                            | "compensation"
                            | "seniority"
                            | "company"
                            | "sponsorship"
                            | "already_applied"
                            | "not_interested"
                            | "other"
                    )
                })
            {
                anyhow::bail!("invalid match feedback reason")
            }
            "recorded"
        }
        "application_issue" => {
            if value.application_id.is_none() {
                anyhow::bail!("application issue must reference an application")
            }
            if !matches!(
                value.action.as_str(),
                "site_problem"
                    | "wrong_information"
                    | "duplicate_application"
                    | "submission_status"
                    | "billing"
                    | "other"
            ) {
                anyhow::bail!("invalid application issue category")
            }
            "open"
        }
        "application_outcome" => {
            if value.application_id.is_none() {
                anyhow::bail!("application outcome must reference an application")
            }
            if !matches!(
                value.action.as_str(),
                "interview" | "rejected" | "offer" | "withdrawn"
            ) {
                anyhow::bail!("invalid application outcome")
            }
            "confirmed"
        }
        _ => anyhow::bail!("invalid candidate event type"),
    }
    .to_string();

    value.id = uuid::Uuid::new_v4().to_string();
    let now = now_ms();
    value.created_at_ms = now;
    value.updated_at_ms = now;
    let payload = to_json(&value, "candidate event")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_candidate_events (
                    id, account_id, event_type, job_id, application_id, status,
                    event_json, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    value.id,
                    account_id,
                    value.event_type,
                    value.job_id,
                    value.application_id,
                    value.status,
                    payload,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_candidate_events (
                    id, account_id, event_type, job_id, application_id, status,
                    event_json, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &value.id,
                    &account_id,
                    &value.event_type,
                    &value.job_id,
                    &value.application_id,
                    &value.status,
                    &payload,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

fn contains_authentication_secret(value: &Value) -> bool {
    match value {
        Value::Object(items) => items.iter().any(|(key, nested)| {
            matches!(
                key.trim().to_ascii_lowercase().as_str(),
                "code"
                    | "otp"
                    | "one_time_code"
                    | "password"
                    | "access_token"
                    | "refresh_token"
                    | "secret"
                    | "credential"
            ) || contains_authentication_secret(nested)
        }),
        Value::Array(items) => items.iter().any(contains_authentication_secret),
        _ => false,
    }
}

pub fn list_application_evidence(
    pool: &DbPool,
    account_id: &str,
    application_id: Option<&str>,
) -> Result<Vec<ApplicationEvidence>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raws = if let Some(application_id) = application_id {
                let mut stmt = conn.prepare(
                    "SELECT evidence_json FROM jobs_application_evidence
                      WHERE account_id = ?1 AND application_id = ?2
                      ORDER BY occurred_at_ms DESC, created_at_ms DESC",
                )?;
                let values = stmt
                    .query_map(params![account_id, application_id], |row| {
                        row.get::<_, String>(0)
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                values
            } else {
                let mut stmt = conn.prepare(
                    "SELECT evidence_json FROM jobs_application_evidence
                      WHERE account_id = ?1
                      ORDER BY occurred_at_ms DESC, created_at_ms DESC",
                )?;
                let values = stmt
                    .query_map(params![account_id], |row| row.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                values
            };
            raws.into_iter()
                .map(|raw| parse_json(raw, "application evidence"))
                .collect()
        }
        DbPool::Postgres(_) => {
            let rows = if let Some(application_id) = application_id {
                pool.get_pg()?.query(
                    "SELECT evidence_json FROM jobs_application_evidence
                      WHERE account_id = $1 AND application_id = $2
                      ORDER BY occurred_at_ms DESC, created_at_ms DESC",
                    &[&account_id, &application_id],
                )?
            } else {
                pool.get_pg()?.query(
                    "SELECT evidence_json FROM jobs_application_evidence
                      WHERE account_id = $1
                      ORDER BY occurred_at_ms DESC, created_at_ms DESC",
                    &[&account_id],
                )?
            };
            rows.into_iter()
                .map(|row| parse_json(row.get(0), "application evidence"))
                .collect()
        }
    })
}

pub fn save_application_evidence(
    pool: &DbPool,
    account_id: &str,
    evidence: &ApplicationEvidence,
) -> Result<ApplicationEvidence> {
    let application = get_application(pool, account_id, &evidence.application_id)?
        .ok_or_else(|| anyhow::anyhow!("application not found"))?;
    let mut value = evidence.clone();
    if !matches!(
        value.kind.as_str(),
        "resume"
            | "cover_letter"
            | "attachment"
            | "submission_confirmation"
            | "status_email"
            | "interview_event"
    ) {
        anyhow::bail!("invalid application evidence kind")
    }
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    if value.occurred_at_ms == 0 {
        value.occurred_at_ms = now_ms();
    }
    if value.created_at_ms == 0 {
        value.created_at_ms = now_ms();
    }
    if value.metadata.is_null() {
        value.metadata = json!({});
    }

    let idempotency_source = match value.kind.as_str() {
        "resume" => {
            let resume_version_id = value
                .resume_version_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("resume evidence needs a resume version"))?;
            if application.resume_version_id.as_deref() != Some(resume_version_id) {
                anyhow::bail!("resume evidence does not match this application's resume")
            }
            let resume = get_resume_version(pool, account_id, resume_version_id)?
                .ok_or_else(|| anyhow::anyhow!("resume version not found"))?;
            if resume.job_id != application.job_id {
                anyhow::bail!("resume evidence belongs to another job")
            }
            validate_document_evidence(&value)?;
            format!("{}:{}", resume_version_id, value.sha256)
        }
        "cover_letter" | "attachment" => {
            validate_document_evidence(&value)?;
            format!("{}:{}", value.storage_key, value.sha256)
        }
        "status_email" | "interview_event" => {
            if value.provider.trim().is_empty() {
                anyhow::bail!("provider evidence needs a provider")
            }
            let external_id = value
                .metadata
                .get("external_id")
                .and_then(Value::as_str)
                .filter(|item| !item.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("provider evidence needs an external event id"))?;
            format!("{}:{}", value.provider, external_id)
        }
        "submission_confirmation" => {
            let confirmation = value
                .metadata
                .get("confirmation")
                .and_then(Value::as_str)
                .unwrap_or(value.label.as_str())
                .trim();
            if confirmation.is_empty() {
                anyhow::bail!("submission confirmation cannot be empty")
            }
            value
                .metadata
                .get("external_id")
                .and_then(Value::as_str)
                .unwrap_or(confirmation)
                .to_string()
        }
        _ => unreachable!(),
    };
    let provider_event_hash = private_lookup_hash(
        &format!("application-evidence:{}:{}", value.kind, value.provider),
        &format!("{}:{idempotency_source}", value.application_id),
    )?;
    let payload = to_json(&value, "application evidence")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO jobs_application_evidence (
                    id, account_id, application_id, kind, provider_event_hash,
                    evidence_json, occurred_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(account_id, provider_event_hash) DO NOTHING",
                params![
                    value.id,
                    account_id,
                    value.application_id,
                    value.kind,
                    provider_event_hash,
                    payload,
                    value.occurred_at_ms,
                    value.created_at_ms,
                ],
            )?;
            let raw: String = conn.query_row(
                "SELECT evidence_json FROM jobs_application_evidence
                  WHERE account_id = ?1 AND provider_event_hash = ?2",
                params![account_id, provider_event_hash],
                |row| row.get(0),
            )?;
            parse_json(raw, "application evidence")
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "INSERT INTO jobs_application_evidence (
                    id, account_id, application_id, kind, provider_event_hash,
                    evidence_json, occurred_at_ms, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(account_id, provider_event_hash) DO NOTHING",
                &[
                    &value.id,
                    &account_id,
                    &value.application_id,
                    &value.kind,
                    &provider_event_hash,
                    &payload,
                    &value.occurred_at_ms,
                    &value.created_at_ms,
                ],
            )?;
            let row = conn.query_one(
                "SELECT evidence_json FROM jobs_application_evidence
                  WHERE account_id = $1 AND provider_event_hash = $2",
                &[&account_id, &provider_event_hash],
            )?;
            parse_json(row.get(0), "application evidence")
        }
    })
}

fn validate_document_evidence(evidence: &ApplicationEvidence) -> Result<()> {
    if evidence.file_name.trim().is_empty() || evidence.storage_key.trim().is_empty() {
        anyhow::bail!("document evidence needs the attached file name and storage key")
    }
    if evidence.sha256.len() != 64 || !evidence.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        anyhow::bail!("document evidence needs a SHA-256 checksum")
    }
    Ok(())
}

fn application_submission_evidence_complete(
    pool: &DbPool,
    account_id: &str,
    application: &JobApplication,
) -> Result<bool> {
    let Some(resume_version_id) = application.resume_version_id.as_deref() else {
        return Ok(false);
    };
    let evidence = list_application_evidence(pool, account_id, Some(&application.id))?;
    let matching_resume = evidence.iter().any(|item| {
        item.kind == "resume" && item.resume_version_id.as_deref() == Some(resume_version_id)
    });
    let confirmation = evidence
        .iter()
        .any(|item| item.kind == "submission_confirmation");
    Ok(matching_resume && confirmation)
}

struct PreparedSubmissionEvidence {
    value: ApplicationEvidence,
    provider_event_hash: String,
    payload: String,
}

fn prepare_submission_evidence(
    application_id: &str,
    request_fingerprint: &str,
    evidence: &[ApplicationEvidence],
    now: i64,
) -> Result<Vec<PreparedSubmissionEvidence>> {
    if evidence.is_empty() || evidence.len() > 9 {
        anyhow::bail!("invalid final submission evidence")
    }
    let mut resume_count = 0usize;
    let mut confirmation_count = 0usize;
    let mut prepared = Vec::with_capacity(evidence.len());
    for (index, item) in evidence.iter().enumerate() {
        let mut value = item.clone();
        if value.application_id != application_id
            || !matches!(
                value.kind.as_str(),
                "resume" | "cover_letter" | "attachment" | "submission_confirmation"
            )
        {
            anyhow::bail!("invalid final submission evidence")
        }
        resume_count += usize::from(value.kind == "resume");
        confirmation_count += usize::from(value.kind == "submission_confirmation");
        validate_document_evidence(&value)?;
        if value.kind == "submission_confirmation"
            && value
                .metadata
                .get("confirmation")
                .and_then(Value::as_str)
                .is_none_or(|confirmation| confirmation.trim().is_empty())
        {
            anyhow::bail!("submission confirmation cannot be empty")
        }
        if value.id.is_empty() {
            value.id = uuid::Uuid::new_v4().to_string();
        }
        if value.occurred_at_ms == 0 {
            value.occurred_at_ms = now;
        }
        if value.created_at_ms == 0 {
            value.created_at_ms = now;
        }
        if value.metadata.is_null() {
            value.metadata = json!({});
        }
        let provider_event_hash = private_lookup_hash(
            "jobs-final-submission-evidence",
            &format!(
                "{application_id}:{request_fingerprint}:{index}:{}",
                value.kind
            ),
        )?;
        let payload = to_json(&value, "application evidence")?;
        prepared.push(PreparedSubmissionEvidence {
            value,
            provider_event_hash,
            payload,
        });
    }
    if resume_count != 1 || confirmation_count != 1 {
        anyhow::bail!("final submission needs one resume and one confirmation")
    }
    Ok(prepared)
}

#[allow(clippy::too_many_arguments)]
pub fn finalize_submission(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    receipt: Value,
    request_fingerprint: &str,
    evidence: &[ApplicationEvidence],
    terminal_session: &BrowserSession,
    local_ticket_hash: Option<&str>,
) -> Result<SubmissionFinalizeResult> {
    if !matches!(runner, "cloud" | "local")
        || request_fingerprint.len() != 64
        || !request_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        anyhow::bail!("invalid final submission request")
    }
    if receipt
        .get("_bluey_server_submission_fingerprint_v1")
        .and_then(Value::as_str)
        != Some(request_fingerprint)
    {
        anyhow::bail!("invalid final submission fingerprint")
    }
    if (runner == "local") != local_ticket_hash.is_some() {
        anyhow::bail!("invalid final submission runner binding")
    }
    let now = now_ms();
    let prepared_evidence =
        prepare_submission_evidence(application_id, request_fingerprint, evidence, now)?;
    let mut terminal_session = terminal_session.clone();
    if terminal_session.application_id.as_deref() != Some(application_id)
        || terminal_session.id != run_id
        || terminal_session.runner != runner
    {
        anyhow::bail!("browser session does not match final submission")
    }
    terminal_session.status = "complete".to_string();
    terminal_session.current_step = "Application submitted".to_string();
    terminal_session.takeover_url = None;
    terminal_session.updated_at_ms = now;
    let terminal_session_payload = to_json(&terminal_session, "browser session")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let raw: Option<(String, String)> = tx
                .query_row(
                    "SELECT job_id, application_json FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((job_id, raw)) = raw else {
                anyhow::bail!("application not found")
            };
            let mut application =
                parse_application_json(raw, application_id, &job_id, "Jobs application")?;
            if application.state == "submitted" {
                if application
                    .receipt
                    .get("_bluey_server_submission_fingerprint_v1")
                    .and_then(Value::as_str)
                    == Some(request_fingerprint)
                {
                    tx.commit()?;
                    return Ok(SubmissionFinalizeResult::Replayed(application));
                }
                anyhow::bail!("application already has a different final receipt")
            }
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("application browser run does not match final receipt")
            }
            validate_application_transition(&application.state, "submitted")?;
            let resume_version_id = application
                .resume_version_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("application resume version is missing"))?;
            if !prepared_evidence.iter().any(|item| {
                item.value.kind == "resume"
                    && item.value.resume_version_id.as_deref() == Some(resume_version_id)
            }) {
                anyhow::bail!("final receipt resume does not match the application")
            }
            if runner == "cloud" {
                let phase: Option<String> = tx
                    .query_row(
                        "SELECT phase FROM jobs_execution_leases
                          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                        params![account_id, application_id, run_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if phase.as_deref() != Some("submitted") {
                    anyhow::bail!("matching cloud execution lease is not terminal submitted")
                }
            } else if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'complete', updated_at_ms = ?5
                  WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND ticket_hash = ?4 AND expires_at_ms > ?5
                    AND status IN ('claimed', 'needs_input')",
                params![
                    run_id,
                    account_id,
                    application_id,
                    local_ticket_hash.expect("local ticket checked above"),
                    now
                ],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            for item in &prepared_evidence {
                tx.execute(
                    "INSERT INTO jobs_application_evidence (
                        id, account_id, application_id, kind, provider_event_hash,
                        evidence_json, occurred_at_ms, created_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        item.value.id,
                        account_id,
                        application_id,
                        item.value.kind,
                        item.provider_event_hash,
                        item.payload,
                        item.value.occurred_at_ms,
                        item.value.created_at_ms,
                    ],
                )?;
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations SET status = 'submitted', updated_at_ms = ?3
                  WHERE account_id = ?1 AND application_id = ?2",
                params![account_id, application_id, now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions SET status = 'complete', session_json = ?3,
                            updated_at_ms = ?4
                      WHERE account_id = ?1 AND id = ?2",
                params![
                    account_id,
                    terminal_session.id,
                    terminal_session_payload,
                    now
                ],
            )? != 1
            {
                anyhow::bail!("browser session not found")
            }
            application.receipt = receipt;
            application.state = "submitted".to_string();
            application.updated_at_ms = now;
            application.submitted_at_ms = Some(now);
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'submitted', application_json = ?3,
                        updated_at_ms = ?4, submitted_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, application_id, application_payload, now],
            )? != 1
            {
                anyhow::bail!("application not found")
            }
            tx.commit()?;
            Ok(SubmissionFinalizeResult::Committed(application))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT job_id, application_json FROM jobs_applications
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &application_id],
            )?;
            let Some(row) = row else {
                anyhow::bail!("application not found")
            };
            let job_id: String = row.get(0);
            let mut application =
                parse_application_json(row.get(1), application_id, &job_id, "Jobs application")?;
            if application.state == "submitted" {
                if application
                    .receipt
                    .get("_bluey_server_submission_fingerprint_v1")
                    .and_then(Value::as_str)
                    == Some(request_fingerprint)
                {
                    tx.commit()?;
                    return Ok(SubmissionFinalizeResult::Replayed(application));
                }
                anyhow::bail!("application already has a different final receipt")
            }
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("application browser run does not match final receipt")
            }
            validate_application_transition(&application.state, "submitted")?;
            let resume_version_id = application
                .resume_version_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("application resume version is missing"))?;
            if !prepared_evidence.iter().any(|item| {
                item.value.kind == "resume"
                    && item.value.resume_version_id.as_deref() == Some(resume_version_id)
            }) {
                anyhow::bail!("final receipt resume does not match the application")
            }
            if runner == "cloud" {
                let phase = tx
                    .query_opt(
                        "SELECT phase FROM jobs_execution_leases
                          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                          FOR UPDATE",
                        &[&account_id, &application_id, &run_id],
                    )?
                    .map(|row| row.get::<_, String>(0));
                if phase.as_deref() != Some("submitted") {
                    anyhow::bail!("matching cloud execution lease is not terminal submitted")
                }
            } else if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'complete', updated_at_ms = $5
                  WHERE id = $1 AND account_id = $2 AND application_id = $3
                    AND ticket_hash = $4 AND expires_at_ms > $5
                    AND status IN ('claimed', 'needs_input')",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &local_ticket_hash.expect("local ticket checked above"),
                    &now,
                ],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            for item in &prepared_evidence {
                tx.execute(
                    "INSERT INTO jobs_application_evidence (
                        id, account_id, application_id, kind, provider_event_hash,
                        evidence_json, occurred_at_ms, created_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    &[
                        &item.value.id,
                        &account_id,
                        &application_id,
                        &item.value.kind,
                        &item.provider_event_hash,
                        &item.payload,
                        &item.value.occurred_at_ms,
                        &item.value.created_at_ms,
                    ],
                )?;
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations SET status = 'submitted', updated_at_ms = $3
                  WHERE account_id = $1 AND application_id = $2",
                &[&account_id, &application_id, &now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions SET status = 'complete', session_json = $3,
                            updated_at_ms = $4
                      WHERE account_id = $1 AND id = $2",
                &[
                    &account_id,
                    &terminal_session.id,
                    &terminal_session_payload,
                    &now,
                ],
            )? != 1
            {
                anyhow::bail!("browser session not found")
            }
            application.receipt = receipt;
            application.state = "submitted".to_string();
            application.updated_at_ms = now;
            application.submitted_at_ms = Some(now);
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'submitted', application_json = $3,
                        updated_at_ms = $4, submitted_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &application_id, &application_payload, &now],
            )? != 1
            {
                anyhow::bail!("application not found")
            }
            tx.commit()?;
            Ok(SubmissionFinalizeResult::Committed(application))
        }
    })
}

pub fn list_application_identities(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<ApplicationIdentity>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities WHERE account_id = ?1
                  ORDER BY is_default DESC, updated_at_ms DESC",
            )?;
            let rows = stmt
                .query_map(params![account_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(raw, status, is_default)| {
                    parse_application_identity_row(raw, status, is_default)
                })
                .collect()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities WHERE account_id = $1
                  ORDER BY is_default DESC, updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| {
                parse_application_identity_row(row.get(0), row.get(1), row.get::<_, i32>(2) != 0)
            })
            .collect(),
    })
}

fn parse_application_identity_row(
    raw: String,
    verification_status: String,
    is_default: bool,
) -> Result<ApplicationIdentity> {
    let mut identity: ApplicationIdentity = parse_json(raw, "application identity")?;
    identity.verification_status = verification_status;
    identity.is_default = is_default;
    Ok(identity)
}

pub fn get_application_identity(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
) -> Result<Option<ApplicationIdentity>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let raw: Option<(String, String, i64)> = pool
                .get()?
                .query_row(
                    "SELECT identity_json, verification_status, is_default
                       FROM jobs_application_identities
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, identity_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            raw.map(|(value, status, is_default)| {
                parse_application_identity_row(value, status, is_default != 0)
            })
            .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &identity_id],
            )?
            .map(|row| {
                parse_application_identity_row(row.get(0), row.get(1), row.get::<_, i32>(2) != 0)
            })
            .transpose(),
    })
}

fn application_identity_by_hash(
    pool: &DbPool,
    email_hash: &str,
) -> Result<Option<(String, ApplicationIdentity)>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let row: Option<(String, String, String, i64)> = pool
                .get()?
                .query_row(
                    "SELECT account_id, identity_json, verification_status, is_default
                       FROM jobs_application_identities
                      WHERE email_hash = ?1",
                    params![email_hash],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            row.map(|(owner, raw, status, is_default)| {
                Ok((
                    owner,
                    parse_application_identity_row(raw, status, is_default != 0)?,
                ))
            })
            .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT account_id, identity_json, verification_status, is_default
                   FROM jobs_application_identities
                  WHERE email_hash = $1",
                &[&email_hash],
            )?
            .map(|row| {
                Ok((
                    row.get(0),
                    parse_application_identity_row(
                        row.get(1),
                        row.get(2),
                        row.get::<_, i32>(3) != 0,
                    )?,
                ))
            })
            .transpose(),
    })
}

pub fn ensure_primary_application_identity(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<ApplicationIdentity> {
    let normalized = normalize_application_email(email)?;
    let email_hash = private_lookup_hash("application-email", &normalized)?;
    if let Some((owner, existing)) = application_identity_by_hash(pool, &email_hash)? {
        if owner != account_id {
            anyhow::bail!("this application email belongs to another Bluey Jobs account")
        }
        return Ok(existing);
    }
    let is_default = list_application_identities(pool, account_id)?.is_empty();
    save_application_identity(
        pool,
        account_id,
        &ApplicationIdentity {
            id: String::new(),
            email: normalized,
            label: "Bluey login".to_string(),
            verification_status: "verified".to_string(),
            is_default,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
}

pub fn save_application_identity(
    pool: &DbPool,
    account_id: &str,
    identity: &ApplicationIdentity,
) -> Result<ApplicationIdentity> {
    let mut value = identity.clone();
    value.email = normalize_application_email(&value.email)?;
    if !matches!(value.verification_status.as_str(), "pending" | "verified") {
        anyhow::bail!("invalid application email verification status")
    }
    let email_hash = private_lookup_hash("application-email", &value.email)?;
    if let Some((owner, existing)) = application_identity_by_hash(pool, &email_hash)? {
        if owner != account_id {
            anyhow::bail!("this application email belongs to another Bluey Jobs account")
        }
        if value.id.is_empty() {
            value.id = existing.id;
            value.created_at_ms = existing.created_at_ms;
            value.verification_status = existing.verification_status;
        }
    }

    let existing = if value.id.is_empty() {
        None
    } else {
        get_application_identity(pool, account_id, &value.id)?
    };
    if existing.is_none() {
        let entitlement = get_entitlement(pool, account_id)?;
        if list_application_identities(pool, account_id)?.len() as i64
            >= entitlement.application_identity_limit
        {
            anyhow::bail!("application email limit reached for this Jobs plan")
        }
    } else if existing
        .as_ref()
        .is_some_and(|item| item.verification_status == "verified")
    {
        value.verification_status = "verified".to_string();
    }
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    if value.is_default && value.verification_status != "verified" {
        anyhow::bail!("verify the application email before making it the default")
    }
    let payload = to_json(&value, "application identity")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if value.is_default {
                tx.execute(
                    "UPDATE jobs_application_identities SET is_default = 0
                      WHERE account_id = ?1 AND id <> ?2",
                    params![account_id, value.id],
                )?;
            }
            tx.execute(
                "INSERT INTO jobs_application_identities (
                    id, account_id, email_hash, identity_json, verification_status,
                    is_default, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    email_hash = excluded.email_hash,
                    identity_json = excluded.identity_json,
                    verification_status = excluded.verification_status,
                    is_default = excluded.is_default,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_application_identities.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    email_hash,
                    payload,
                    value.verification_status,
                    i64::from(value.is_default),
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let is_default = i32::from(value.is_default);
            if value.is_default {
                tx.execute(
                    "UPDATE jobs_application_identities SET is_default = 0
                      WHERE account_id = $1 AND id <> $2",
                    &[&account_id, &value.id],
                )?;
            }
            tx.execute(
                "INSERT INTO jobs_application_identities (
                    id, account_id, email_hash, identity_json, verification_status,
                    is_default, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(id) DO UPDATE SET
                    email_hash = EXCLUDED.email_hash,
                    identity_json = EXCLUDED.identity_json,
                    verification_status = EXCLUDED.verification_status,
                    is_default = EXCLUDED.is_default,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_application_identities.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &email_hash,
                    &payload,
                    &value.verification_status,
                    &is_default,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(value)
        }
    })
}

pub fn delete_application_identity(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
) -> Result<bool> {
    let identity = get_application_identity(pool, account_id, identity_id)?
        .ok_or_else(|| anyhow::anyhow!("application email not found"))?;
    if identity.is_default {
        anyhow::bail!("choose another default application email before removing this one")
    }
    if list_tracks(pool, account_id)?
        .iter()
        .any(|track| track.application_identity_id.as_deref() == Some(identity_id))
    {
        anyhow::bail!("choose another email for the Career Track before removing this one")
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "DELETE FROM jobs_application_identities WHERE account_id = ?1 AND id = ?2",
            params![account_id, identity_id],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "DELETE FROM jobs_application_identities WHERE account_id = $1 AND id = $2",
            &[&account_id, &identity_id],
        )? > 0),
    })
}

pub fn save_identity_verification(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
    code: &str,
    ttl_ms: i64,
) -> Result<ApplicationIdentity> {
    let identity = get_application_identity(pool, account_id, identity_id)?
        .ok_or_else(|| anyhow::anyhow!("application email not found"))?;
    if identity.verification_status == "verified" {
        return Ok(identity);
    }
    let created_at = now_ms();
    let expires_at = created_at + ttl_ms;
    let otp_hash = private_lookup_hash(&format!("identity-otp:{identity_id}"), code)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let previous: Option<i64> = conn
                .query_row(
                    "SELECT created_at_ms FROM jobs_identity_verifications
                      WHERE account_id = ?1 AND identity_id = ?2",
                    params![account_id, identity_id],
                    |row| row.get(0),
                )
                .optional()?;
            if previous.is_some_and(|timestamp| timestamp > created_at - 60_000) {
                anyhow::bail!("wait a minute before requesting another verification code")
            }
            conn.execute(
                "INSERT INTO jobs_identity_verifications (
                    identity_id, account_id, otp_hash, attempts, expires_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, 0, ?4, ?5)
                 ON CONFLICT(identity_id) DO UPDATE SET otp_hash = excluded.otp_hash,
                    attempts = 0, expires_at_ms = excluded.expires_at_ms,
                    created_at_ms = excluded.created_at_ms
                 WHERE jobs_identity_verifications.account_id = excluded.account_id",
                params![identity_id, account_id, otp_hash, expires_at, created_at],
            )?;
            Ok(identity)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let previous = conn.query_opt(
                "SELECT created_at_ms FROM jobs_identity_verifications
                  WHERE account_id = $1 AND identity_id = $2",
                &[&account_id, &identity_id],
            )?;
            if previous.is_some_and(|row| row.get::<_, i64>(0) > created_at - 60_000) {
                anyhow::bail!("wait a minute before requesting another verification code")
            }
            conn.execute(
                "INSERT INTO jobs_identity_verifications (
                    identity_id, account_id, otp_hash, attempts, expires_at_ms, created_at_ms
                 ) VALUES ($1, $2, $3, 0, $4, $5)
                 ON CONFLICT(identity_id) DO UPDATE SET otp_hash = EXCLUDED.otp_hash,
                    attempts = 0, expires_at_ms = EXCLUDED.expires_at_ms,
                    created_at_ms = EXCLUDED.created_at_ms
                 WHERE jobs_identity_verifications.account_id = EXCLUDED.account_id",
                &[
                    &identity_id,
                    &account_id,
                    &otp_hash,
                    &expires_at,
                    &created_at,
                ],
            )?;
            Ok(identity)
        }
    })
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.as_bytes()
        .iter()
        .zip(right.as_bytes())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

pub fn verify_application_identity(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
    code: &str,
) -> Result<ApplicationIdentity> {
    let submitted_hash = private_lookup_hash(&format!("identity-otp:{identity_id}"), code)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let (stored_hash, attempts, expires_at): (String, i64, i64) = tx
                .query_row(
                    "SELECT otp_hash, attempts, expires_at_ms FROM jobs_identity_verifications
                      WHERE account_id = ?1 AND identity_id = ?2",
                    params![account_id, identity_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("request a new verification code"))?;
            if expires_at <= now_ms() {
                anyhow::bail!("verification code expired")
            }
            if attempts >= 5 {
                anyhow::bail!("too many verification attempts; request a new code")
            }
            if !constant_time_equal(&stored_hash, &submitted_hash) {
                tx.execute(
                    "UPDATE jobs_identity_verifications SET attempts = attempts + 1
                      WHERE account_id = ?1 AND identity_id = ?2",
                    params![account_id, identity_id],
                )?;
                tx.commit()?;
                anyhow::bail!("verification code is incorrect")
            }
            let raw: String = tx.query_row(
                "SELECT identity_json FROM jobs_application_identities
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, identity_id],
                |row| row.get(0),
            )?;
            let mut identity: ApplicationIdentity = parse_json(raw, "application identity")?;
            identity.verification_status = "verified".to_string();
            identity.updated_at_ms = now_ms();
            let payload = to_json(&identity, "application identity")?;
            tx.execute(
                "UPDATE jobs_application_identities SET verification_status = 'verified',
                    identity_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, identity_id, payload, identity.updated_at_ms],
            )?;
            tx.execute(
                "DELETE FROM jobs_identity_verifications WHERE account_id = ?1 AND identity_id = ?2",
                params![account_id, identity_id],
            )?;
            tx.commit()?;
            Ok(identity)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx
                .query_opt(
                    "SELECT otp_hash, attempts, expires_at_ms FROM jobs_identity_verifications
                      WHERE account_id = $1 AND identity_id = $2 FOR UPDATE",
                    &[&account_id, &identity_id],
                )?
                .ok_or_else(|| anyhow::anyhow!("request a new verification code"))?;
            let stored_hash: String = row.get(0);
            let attempts: i64 = row.get(1);
            let expires_at: i64 = row.get(2);
            if expires_at <= now_ms() {
                anyhow::bail!("verification code expired")
            }
            if attempts >= 5 {
                anyhow::bail!("too many verification attempts; request a new code")
            }
            if !constant_time_equal(&stored_hash, &submitted_hash) {
                tx.execute(
                    "UPDATE jobs_identity_verifications SET attempts = attempts + 1
                      WHERE account_id = $1 AND identity_id = $2",
                    &[&account_id, &identity_id],
                )?;
                tx.commit()?;
                anyhow::bail!("verification code is incorrect")
            }
            let raw: String = tx
                .query_one(
                    "SELECT identity_json FROM jobs_application_identities
                      WHERE account_id = $1 AND id = $2",
                    &[&account_id, &identity_id],
                )?
                .get(0);
            let mut identity: ApplicationIdentity = parse_json(raw, "application identity")?;
            identity.verification_status = "verified".to_string();
            identity.updated_at_ms = now_ms();
            let payload = to_json(&identity, "application identity")?;
            tx.execute(
                "UPDATE jobs_application_identities SET verification_status = 'verified',
                    identity_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &identity_id, &payload, &identity.updated_at_ms],
            )?;
            tx.execute(
                "DELETE FROM jobs_identity_verifications WHERE account_id = $1 AND identity_id = $2",
                &[&account_id, &identity_id],
            )?;
            tx.commit()?;
            Ok(identity)
        }
    })
}

pub fn delete_identity_verification(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "DELETE FROM jobs_identity_verifications
                  WHERE account_id = ?1 AND identity_id = ?2",
                params![account_id, identity_id],
            )?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "DELETE FROM jobs_identity_verifications
                  WHERE account_id = $1 AND identity_id = $2",
                &[&account_id, &identity_id],
            )?;
            Ok(())
        }
    })
}

pub fn list_mailbox_connections(pool: &DbPool, account_id: &str) -> Result<Vec<MailboxConnection>> {
    list_payloads(
        pool,
        account_id,
        "jobs_mailbox_connections",
        "connection_json",
        "updated_at_ms DESC",
        "mailbox connection",
    )
}

fn mailbox_connection_by_subject(
    pool: &DbPool,
    account_id: &str,
    provider: &str,
    subject_hash: &str,
) -> Result<Option<MailboxConnection>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let raw: Option<String> = pool
                .get()?
                .query_row(
                    "SELECT connection_json FROM jobs_mailbox_connections
                      WHERE account_id = ?1 AND provider = ?2 AND provider_subject_hash = ?3",
                    params![account_id, provider, subject_hash],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "mailbox connection"))
                .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT connection_json FROM jobs_mailbox_connections
                  WHERE account_id = $1 AND provider = $2 AND provider_subject_hash = $3",
                &[&account_id, &provider, &subject_hash],
            )?
            .map(|row| parse_json(row.get(0), "mailbox connection"))
            .transpose(),
    })
}

pub fn save_mailbox_connection(
    pool: &DbPool,
    account_id: &str,
    connection: &MailboxConnection,
    provider_subject: &str,
) -> Result<MailboxConnection> {
    let mut value = connection.clone();
    if !matches!(value.provider.as_str(), "gmail" | "outlook") {
        anyhow::bail!("choose Gmail or Outlook")
    }
    if !matches!(
        value.status.as_str(),
        "pending" | "connected" | "disconnected"
    ) {
        anyhow::bail!("invalid mailbox connection status")
    }
    value.account_label = normalize_application_email(&value.account_label)?;
    value.aliases = value
        .aliases
        .iter()
        .filter_map(|alias| normalize_application_email(alias).ok())
        .collect();
    value.aliases.sort();
    value.aliases.dedup();
    let subject = if provider_subject.trim().is_empty() {
        value.account_label.as_str()
    } else {
        provider_subject.trim()
    };
    let subject_hash = private_lookup_hash(&format!("mailbox:{}", value.provider), subject)?;
    let existing = list_mailbox_connections(pool, account_id)?;
    if value.id.is_empty() {
        if let Some(item) =
            mailbox_connection_by_subject(pool, account_id, &value.provider, &subject_hash)?
        {
            value.id = item.id.clone();
            value.created_at_ms = item.created_at_ms;
        } else {
            let entitlement = get_entitlement(pool, account_id)?;
            let active_count = existing
                .iter()
                .filter(|item| item.status != "disconnected")
                .count() as i64;
            if value.status != "disconnected" && active_count >= entitlement.connected_inbox_limit {
                anyhow::bail!("connected inbox limit reached for this Jobs plan")
            }
            value.id = uuid::Uuid::new_v4().to_string();
        }
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    if value.capabilities.is_empty() {
        value.capabilities = vec!["status_sync".to_string(), "follow_ups".to_string()];
    }
    let payload = to_json(&value, "mailbox connection")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_mailbox_connections (
                    id, account_id, provider, provider_subject_hash, status,
                    connection_json, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status,
                    connection_json = excluded.connection_json,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_mailbox_connections.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.provider,
                    subject_hash,
                    value.status,
                    payload,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_mailbox_connections (
                    id, account_id, provider, provider_subject_hash, status,
                    connection_json, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(id) DO UPDATE SET status = EXCLUDED.status,
                    connection_json = EXCLUDED.connection_json,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_mailbox_connections.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.provider,
                    &subject_hash,
                    &value.status,
                    &payload,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn delete_mailbox_connection(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "DELETE FROM jobs_mailbox_connections WHERE account_id = ?1 AND id = ?2",
            params![account_id, connection_id],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "DELETE FROM jobs_mailbox_connections WHERE account_id = $1 AND id = $2",
            &[&account_id, &connection_id],
        )? > 0),
    })
}

pub fn list_integrations(pool: &DbPool, account_id: &str) -> Result<Vec<JobsIntegration>> {
    let mut integrations: Vec<JobsIntegration> = list_payloads(
        pool,
        account_id,
        "jobs_integrations",
        "integration_json",
        "provider ASC",
        "Jobs integration",
    )?;
    integrations.retain(|item| {
        matches!(
            item.provider.as_str(),
            "google_calendar" | "outlook_calendar"
        )
    });
    for (provider, capabilities) in [
        ("google_calendar", vec!["interview_calendar"]),
        ("outlook_calendar", vec!["interview_calendar"]),
    ] {
        if !integrations.iter().any(|item| item.provider == provider) {
            integrations.push(JobsIntegration {
                id: format!("{account_id}:{provider}"),
                provider: provider.to_string(),
                status: "disconnected".to_string(),
                account_label: String::new(),
                capabilities: capabilities.into_iter().map(str::to_string).collect(),
                updated_at_ms: 0,
            });
        }
    }
    Ok(integrations)
}

pub fn save_integration(
    pool: &DbPool,
    account_id: &str,
    integration: &JobsIntegration,
) -> Result<JobsIntegration> {
    let mut value = integration.clone();
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    value.updated_at_ms = now_ms();
    let payload = to_json(&value, "Jobs integration")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_integrations(id, account_id, provider, status, integration_json, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(account_id, provider) DO UPDATE SET
                    status = excluded.status, integration_json = excluded.integration_json,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    value.id,
                    account_id,
                    value.provider,
                    value.status,
                    payload,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_integrations(id, account_id, provider, status, integration_json, updated_at_ms)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(account_id, provider) DO UPDATE SET
                    status = EXCLUDED.status, integration_json = EXCLUDED.integration_json,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[
                    &value.id,
                    &account_id,
                    &value.provider,
                    &value.status,
                    &payload,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

fn list_payloads<T: DeserializeOwned>(
    pool: &DbPool,
    account_id: &str,
    table: &str,
    payload_column: &str,
    order_by: &str,
    label: &str,
) -> Result<Vec<T>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let sql = format!(
                "SELECT {payload_column} FROM {table} WHERE account_id = ?1 ORDER BY {order_by}"
            );
            let mut stmt = conn.prepare(&sql)?;
            let raws = stmt
                .query_map(params![account_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raws.into_iter().map(|raw| parse_json(raw, label)).collect()
        }
        DbPool::Postgres(_) => {
            let sql = format!(
                "SELECT {payload_column} FROM {table} WHERE account_id = $1 ORDER BY {order_by}"
            );
            pool.get_pg()?
                .query(&sql, &[&account_id])?
                .into_iter()
                .map(|row| parse_json(row.get(0), label))
                .collect()
        }
    })
}

pub fn list_run_events(pool: &DbPool, account_id: &str, run_id: &str) -> Result<Vec<RunEvent>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, run_id, event_type, event_json, created_at_ms
                   FROM jobs_run_events WHERE account_id = ?1 AND run_id = ?2
                  ORDER BY created_at_ms ASC",
            )?;
            let rows = stmt.query_map(params![account_id, run_id], |row| {
                let raw: String = row.get(3)?;
                Ok(RunEvent {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    event_type: row.get(2)?,
                    event: parse_json_lossy(&raw).unwrap_or_else(|| json!({})),
                    created_at_ms: row.get(4)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("list Jobs run events")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, run_id, event_type, event_json, created_at_ms
                   FROM jobs_run_events WHERE account_id = $1 AND run_id = $2
                  ORDER BY created_at_ms ASC",
                &[&account_id, &run_id],
            )?
            .into_iter()
            .map(|row| {
                let raw: String = row.get(3);
                Ok(RunEvent {
                    id: row.get(0),
                    run_id: row.get(1),
                    event_type: row.get(2),
                    event: parse_json(raw, "Jobs run event")?,
                    created_at_ms: row.get(4),
                })
            })
            .collect(),
    })
}

pub fn list_account_run_events(pool: &DbPool, account_id: &str) -> Result<Vec<RunEvent>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, run_id, event_type, event_json, created_at_ms
                   FROM jobs_run_events WHERE account_id = ?1
                  ORDER BY created_at_ms ASC",
            )?;
            let rows = stmt.query_map(params![account_id], |row| {
                let raw: String = row.get(3)?;
                Ok(RunEvent {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    event_type: row.get(2)?,
                    event: parse_json_lossy(&raw).unwrap_or_else(|| json!({})),
                    created_at_ms: row.get(4)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("list account Jobs run events")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, run_id, event_type, event_json, created_at_ms
                   FROM jobs_run_events WHERE account_id = $1
                  ORDER BY created_at_ms ASC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| {
                let raw: String = row.get(3);
                Ok(RunEvent {
                    id: row.get(0),
                    run_id: row.get(1),
                    event_type: row.get(2),
                    event: parse_json(raw, "Jobs run event")?,
                    created_at_ms: row.get(4),
                })
            })
            .collect(),
    })
}

pub fn save_run_event(
    pool: &DbPool,
    account_id: &str,
    run_id: &str,
    event_type: &str,
    event: Value,
) -> Result<RunEvent> {
    let value = RunEvent {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: run_id.to_string(),
        event_type: event_type.to_string(),
        event,
        created_at_ms: now_ms(),
    };
    let payload = to_json(&value.event, "Jobs run event")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_run_events(id, account_id, run_id, event_type, event_json, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![value.id, account_id, value.run_id, value.event_type, payload, value.created_at_ms],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_run_events(id, account_id, run_id, event_type, event_json, created_at_ms)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[&value.id, &account_id, &value.run_id, &value.event_type, &payload, &value.created_at_ms],
            )?;
            Ok(value)
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn save_local_run_ticket(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    ticket_hash: &str,
    ticket_secret: &str,
    payload: Value,
    expires_at_ms: i64,
) -> Result<LocalRunTicket> {
    let now = now_ms();
    let value = LocalRunTicket {
        id: run_id.to_string(),
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        ticket_hash: ticket_hash.to_string(),
        ticket_secret: ticket_secret.to_string(),
        payload,
        status: "queued".to_string(),
        expires_at_ms,
        created_at_ms: now,
        updated_at_ms: now,
    };
    let encrypted_ticket = encrypt_payload(&value.ticket_secret)?;
    let encrypted_payload = to_json(&value.payload, "Jobs local browser packet")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_local_run_tickets (
                    id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    ticket_hash = excluded.ticket_hash,
                    ticket_secret = excluded.ticket_secret,
                    payload_json = excluded.payload_json,
                    status = excluded.status,
                    expires_at_ms = excluded.expires_at_ms,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_local_run_tickets.account_id = excluded.account_id
                   AND jobs_local_run_tickets.application_id = excluded.application_id",
                params![
                    value.id,
                    value.account_id,
                    value.application_id,
                    value.ticket_hash,
                    encrypted_ticket,
                    encrypted_payload,
                    value.status,
                    value.expires_at_ms,
                    value.created_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_local_run_tickets (
                    id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
                 ON CONFLICT(id) DO UPDATE SET
                    ticket_hash = EXCLUDED.ticket_hash,
                    ticket_secret = EXCLUDED.ticket_secret,
                    payload_json = EXCLUDED.payload_json,
                    status = EXCLUDED.status,
                    expires_at_ms = EXCLUDED.expires_at_ms,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_local_run_tickets.account_id = EXCLUDED.account_id
                   AND jobs_local_run_tickets.application_id = EXCLUDED.application_id",
                &[
                    &value.id,
                    &value.account_id,
                    &value.application_id,
                    &value.ticket_hash,
                    &encrypted_ticket,
                    &encrypted_payload,
                    &value.status,
                    &value.expires_at_ms,
                    &value.created_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn get_local_run_ticket(
    pool: &DbPool,
    account_id: &str,
    run_id: &str,
) -> Result<Option<LocalRunTicket>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                        payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_local_run_tickets
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, run_id],
                local_run_ticket_from_sqlite_row,
            )
            .optional()
            .context("get Jobs local run ticket"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                        payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_local_run_tickets
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &run_id],
            )?
            .map(local_run_ticket_from_pg_row)
            .transpose(),
    })
}

pub fn get_local_run_ticket_by_hash(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
) -> Result<Option<LocalRunTicket>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                        payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_local_run_tickets
                  WHERE id = ?1 AND ticket_hash = ?2",
                params![run_id, ticket_hash],
                local_run_ticket_from_sqlite_row,
            )
            .optional()
            .context("get Jobs local run capability"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                        payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_local_run_tickets
                  WHERE id = $1 AND ticket_hash = $2",
                &[&run_id, &ticket_hash],
            )?
            .map(local_run_ticket_from_pg_row)
            .transpose(),
    })
}

#[cfg(test)]
fn claim_local_run_ticket(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
) -> Result<Option<LocalRunTicket>> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let value = transaction
                .query_row(
                    "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                            payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_local_run_tickets
                      WHERE id = ?1 AND ticket_hash = ?2 AND expires_at_ms > ?3
                        AND status = 'queued'",
                    params![run_id, ticket_hash, now],
                    local_run_ticket_from_sqlite_row,
                )
                .optional()?;
            if value.is_some()
                && transaction.execute(
                    "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = ?3
                      WHERE id = ?1 AND ticket_hash = ?2 AND status = 'queued'",
                    params![run_id, ticket_hash, now],
                )? != 1
            {
                transaction.commit()?;
                return Ok(None);
            }
            transaction.commit()?;
            Ok(value.map(|mut value| {
                value.status = "claimed".to_string();
                value.updated_at_ms = now;
                value
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut transaction = conn.transaction()?;
            let value = transaction
                .query_opt(
                    "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                            payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_local_run_tickets
                      WHERE id = $1 AND ticket_hash = $2 AND expires_at_ms > $3
                        AND status = 'queued'
                      FOR UPDATE",
                    &[&run_id, &ticket_hash, &now],
                )?
                .map(local_run_ticket_from_pg_row)
                .transpose()?;
            if value.is_some()
                && transaction.execute(
                    "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = $3
                      WHERE id = $1 AND ticket_hash = $2 AND status = 'queued'",
                    &[&run_id, &ticket_hash, &now],
                )? != 1
            {
                transaction.commit()?;
                return Ok(None);
            }
            transaction.commit()?;
            Ok(value.map(|mut value| {
                value.status = "claimed".to_string();
                value.updated_at_ms = now;
                value
            }))
        }
    })
}

/// Claims a local-browser ticket only while the account, application,
/// verified application identity, browser profile, and local-run entitlement
/// still match the exact queued packet. This is the server-side admission
/// authority; the launch URL alone is not enough.
pub fn claim_authorized_local_run_ticket(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
) -> Result<Option<LocalRunTicket>> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let value = sqlite_local_run_authority(
                &tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Claim,
            )?;
            let Some(mut value) = value else {
                tx.commit()?;
                return Ok(None);
            };
            if tx.execute(
                "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = ?3
                  WHERE id = ?1 AND ticket_hash = ?2 AND status = 'queued'
                    AND expires_at_ms > ?3",
                params![run_id, ticket_hash, now],
            )? != 1
            {
                tx.commit()?;
                return Ok(None);
            }
            tx.commit()?;
            value.status = "claimed".to_string();
            value.updated_at_ms = now;
            Ok(Some(value))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let value = postgres_local_run_authority(
                &mut tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Claim,
            )?;
            let Some(mut value) = value else {
                tx.commit()?;
                return Ok(None);
            };
            if tx.execute(
                "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = $3
                  WHERE id = $1 AND ticket_hash = $2 AND status = 'queued'
                    AND expires_at_ms > $3",
                &[&run_id, &ticket_hash, &now],
            )? != 1
            {
                tx.commit()?;
                return Ok(None);
            }
            tx.commit()?;
            value.status = "claimed".to_string();
            value.updated_at_ms = now;
            Ok(Some(value))
        }
    })
}

/// Performs a non-mutating, current-authority check immediately before the
/// local browser crosses the irreversible employer Submit boundary.
pub fn local_run_submit_authorized(pool: &DbPool, run_id: &str, ticket_hash: &str) -> Result<bool> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let authorized = sqlite_local_run_authority(
                &tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Submit,
            )?
            .is_some();
            tx.commit()?;
            Ok(authorized)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let authorized = postgres_local_run_authority(
                &mut tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Submit,
            )?
            .is_some();
            tx.commit()?;
            Ok(authorized)
        }
    })
}

fn sqlite_local_run_authority(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    now: i64,
    phase: LocalRunAuthorityPhase,
) -> Result<Option<LocalRunTicket>> {
    let expected_ticket_status = match phase {
        LocalRunAuthorityPhase::Claim => "queued",
        LocalRunAuthorityPhase::Submit => "claimed",
    };
    let ticket = tx
        .query_row(
            "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
               FROM jobs_local_run_tickets
              WHERE id = ?1 AND ticket_hash = ?2 AND status = ?3
                AND expires_at_ms > ?4",
            params![run_id, ticket_hash, expected_ticket_status, now],
            local_run_ticket_from_sqlite_row,
        )
        .optional()?;
    let Some(ticket) = ticket else {
        return Ok(None);
    };
    let application_row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, ticket.application_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((job_id, application_raw, application_state)) = application_row else {
        return Ok(None);
    };
    let application = parse_application_json(
        application_raw,
        &ticket.application_id,
        &job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let session_row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT session_json, runner, status FROM jobs_browser_sessions
              WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let identity_row: Option<(String, String, i64)> = tx
        .query_row(
            "SELECT identity_json, verification_status, is_default
               FROM jobs_application_identities
              WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, identity_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let local_browser: Option<i64> = tx
        .query_row(
            "SELECT local_browser FROM jobs_entitlements WHERE account_id = ?1",
            params![ticket.account_id],
            |row| row.get(0),
        )
        .optional()?;
    let canonical_url: Option<String> = tx
        .query_row(
            "SELECT canonical_url FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, job_id],
            |row| row.get(0),
        )
        .optional()?;
    let (
        Some((session_raw, session_runner, session_status)),
        Some((identity_raw, identity_status, is_default)),
        Some(canonical_url),
    ) = (session_row, identity_row, canonical_url)
    else {
        return Ok(None);
    };
    let session: BrowserSession = parse_json(session_raw, "browser session")?;
    let identity =
        parse_application_identity_row(identity_raw, identity_status.clone(), is_default != 0)?;
    Ok(local_run_authority_matches(
        &ticket,
        &application,
        &application_state,
        &session,
        &session_runner,
        &session_status,
        &identity,
        &identity_status,
        local_browser == Some(1),
        &canonical_url,
        phase,
        now,
    )
    .then_some(ticket))
}

fn postgres_local_run_authority(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    now: i64,
    phase: LocalRunAuthorityPhase,
) -> Result<Option<LocalRunTicket>> {
    let expected_ticket_status = match phase {
        LocalRunAuthorityPhase::Claim => "queued",
        LocalRunAuthorityPhase::Submit => "claimed",
    };
    let ticket = tx
        .query_opt(
            "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
               FROM jobs_local_run_tickets
              WHERE id = $1 AND ticket_hash = $2 AND status = $3
                AND expires_at_ms > $4
              FOR UPDATE",
            &[&run_id, &ticket_hash, &expected_ticket_status, &now],
        )?
        .map(local_run_ticket_from_pg_row)
        .transpose()?;
    let Some(ticket) = ticket else {
        return Ok(None);
    };
    let Some(application_row) = tx.query_opt(
        "SELECT job_id, application_json, state FROM jobs_applications
          WHERE account_id = $1 AND id = $2",
        &[&ticket.account_id, &ticket.application_id],
    )?
    else {
        return Ok(None);
    };
    let job_id: String = application_row.get(0);
    let application_raw: String = application_row.get(1);
    let application_state: String = application_row.get(2);
    let application = parse_application_json(
        application_raw,
        &ticket.application_id,
        &job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let session_row = tx.query_opt(
        "SELECT session_json, runner, status FROM jobs_browser_sessions
          WHERE account_id = $1 AND id = $2",
        &[&ticket.account_id, &run_id],
    )?;
    let identity_row = tx.query_opt(
        "SELECT identity_json, verification_status, is_default
           FROM jobs_application_identities
          WHERE account_id = $1 AND id = $2",
        &[&ticket.account_id, &identity_id],
    )?;
    let local_browser = tx
        .query_opt(
            "SELECT local_browser FROM jobs_entitlements WHERE account_id = $1",
            &[&ticket.account_id],
        )?
        .map(|row| row.get::<_, i32>(0) != 0)
        .unwrap_or(false);
    let canonical_url = tx
        .query_opt(
            "SELECT canonical_url FROM jobs_postings WHERE account_id = $1 AND id = $2",
            &[&ticket.account_id, &job_id],
        )?
        .map(|row| row.get::<_, String>(0));
    let (Some(session_row), Some(identity_row), Some(canonical_url)) =
        (session_row, identity_row, canonical_url)
    else {
        return Ok(None);
    };
    let session_raw: String = session_row.get(0);
    let session_runner: String = session_row.get(1);
    let session_status: String = session_row.get(2);
    let identity_raw: String = identity_row.get(0);
    let identity_status: String = identity_row.get(1);
    let is_default = identity_row.get::<_, i32>(2) != 0;
    let session: BrowserSession = parse_json(session_raw, "browser session")?;
    let identity =
        parse_application_identity_row(identity_raw, identity_status.clone(), is_default)?;
    Ok(local_run_authority_matches(
        &ticket,
        &application,
        &application_state,
        &session,
        &session_runner,
        &session_status,
        &identity,
        &identity_status,
        local_browser,
        &canonical_url,
        phase,
        now,
    )
    .then_some(ticket))
}

#[allow(clippy::too_many_arguments)]
fn local_run_authority_matches(
    ticket: &LocalRunTicket,
    application: &JobApplication,
    application_state: &str,
    session: &BrowserSession,
    session_runner: &str,
    session_status: &str,
    identity: &ApplicationIdentity,
    identity_status: &str,
    local_browser: bool,
    canonical_url: &str,
    phase: LocalRunAuthorityPhase,
    now: i64,
) -> bool {
    let (expected_ticket_status, expected_application_state, expected_session_status) = match phase
    {
        LocalRunAuthorityPhase::Claim => ("queued", "queued", "queued"),
        LocalRunAuthorityPhase::Submit => ("claimed", "running", "running"),
    };
    let frozen_identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let frozen_identity_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let expected_browser_profile_id =
        execution_browser_profile_id(&ticket.account_id, frozen_identity_id);
    ticket.status == expected_ticket_status
        && ticket.expires_at_ms > now
        && local_browser
        && application.state == application_state
        && application_state == expected_application_state
        && application.run_id.as_deref() == Some(ticket.id.as_str())
        && session.id == ticket.id
        && session.application_id.as_deref() == Some(ticket.application_id.as_str())
        && session.runner == session_runner
        && session_runner == "local"
        && session.status == session_status
        && session_status == expected_session_status
        && identity.verification_status == identity_status
        && identity_status == "verified"
        && !frozen_identity_id.is_empty()
        && identity.id == frozen_identity_id
        && identity.email == frozen_identity_email
        && application
            .receipt
            .pointer("/application_identity/verified")
            .and_then(Value::as_bool)
            == Some(true)
        && ticket.payload.get("accountId").and_then(Value::as_str)
            == Some(ticket.account_id.as_str())
        && ticket.payload.get("applicationId").and_then(Value::as_str)
            == Some(ticket.application_id.as_str())
        && ticket.payload.get("runId").and_then(Value::as_str) == Some(ticket.id.as_str())
        && ticket
            .payload
            .get("applicationIdentityId")
            .and_then(Value::as_str)
            == Some(frozen_identity_id)
        && ticket
            .payload
            .get("browserProfileId")
            .and_then(Value::as_str)
            == Some(expected_browser_profile_id.as_str())
        && ticket.payload.get("runner").and_then(Value::as_str) == Some("local")
        && ticket.payload.get("url").and_then(Value::as_str) == Some(canonical_url)
}

pub fn update_local_run_ticket_status(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    status: &str,
) -> Result<bool> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "UPDATE jobs_local_run_tickets SET status = ?3, updated_at_ms = ?4
              WHERE id = ?1 AND ticket_hash = ?2 AND expires_at_ms > ?4",
            params![run_id, ticket_hash, status, now],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "UPDATE jobs_local_run_tickets SET status = $3, updated_at_ms = $4
              WHERE id = $1 AND ticket_hash = $2 AND expires_at_ms > $4",
            &[&run_id, &ticket_hash, &status, &now],
        )? > 0),
    })
}

pub fn finalize_local_side_effect_unknown(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    ticket_hash: &str,
    reconciliation_receipt: Value,
    session: &BrowserSession,
) -> Result<JobApplication> {
    if reconciliation_receipt.get("status").and_then(Value::as_str) != Some("side_effect_unknown")
        || session.id != run_id
        || session.runner != "local"
        || session.application_id.as_deref() != Some(application_id)
    {
        anyhow::bail!("invalid local reconciliation result")
    }
    let now = now_ms();
    let mut terminal_session = session.clone();
    terminal_session.status = "needs_input".to_string();
    terminal_session.current_step = "Submission outcome needs reconciliation".to_string();
    terminal_session.updated_at_ms = now;
    let session_payload = to_json(&terminal_session, "browser session")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let raw: Option<(String, String)> = tx
                .query_row(
                    "SELECT job_id, application_json FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((job_id, raw)) = raw else {
                anyhow::bail!("application not found")
            };
            let mut application =
                parse_application_json(raw, application_id, &job_id, "Jobs application")?;
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("local run ticket does not match this application")
            }
            if application.state == "side_effect_unknown" {
                let status: Option<String> = tx
                    .query_row(
                        "SELECT status FROM jobs_local_run_tickets
                          WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                            AND ticket_hash = ?4",
                        params![run_id, account_id, application_id, ticket_hash],
                        |row| row.get(0),
                    )
                    .optional()?;
                if status.as_deref() == Some("side_effect_unknown") {
                    tx.commit()?;
                    return Ok(application);
                }
                anyhow::bail!("local run ticket is not active")
            }
            validate_application_transition(&application.state, "side_effect_unknown")?;
            if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'side_effect_unknown', updated_at_ms = ?5
                  WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND ticket_hash = ?4 AND expires_at_ms > ?5
                    AND status IN ('claimed', 'needs_input')",
                params![run_id, account_id, application_id, ticket_hash, now],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations
                    SET status = 'side_effect_unknown', updated_at_ms = ?3
                  WHERE account_id = ?1 AND application_id = ?2",
                params![account_id, application_id, now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions
                    SET status = 'needs_input', session_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, run_id, session_payload, now],
            )? != 1
            {
                anyhow::bail!("browser session not found")
            }
            if !application.receipt.is_object() {
                application.receipt = json!({});
            }
            application
                .receipt
                .as_object_mut()
                .expect("receipt normalized above")
                .insert(
                    "local_reconciliation".to_string(),
                    json!({
                        "status": "side_effect_unknown",
                        "recorded_at_ms": now,
                        "receipt": reconciliation_receipt,
                    }),
                );
            application.state = "side_effect_unknown".to_string();
            application.updated_at_ms = now;
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'side_effect_unknown',
                        application_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, application_id, application_payload, now],
            )? != 1
            {
                anyhow::bail!("application not found")
            }
            tx.commit()?;
            Ok(application)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT job_id, application_json FROM jobs_applications
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &application_id],
            )?;
            let Some(row) = row else {
                anyhow::bail!("application not found")
            };
            let job_id: String = row.get(0);
            let mut application =
                parse_application_json(row.get(1), application_id, &job_id, "Jobs application")?;
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("local run ticket does not match this application")
            }
            if application.state == "side_effect_unknown" {
                let status = tx
                    .query_opt(
                        "SELECT status FROM jobs_local_run_tickets
                          WHERE id = $1 AND account_id = $2 AND application_id = $3
                            AND ticket_hash = $4 FOR UPDATE",
                        &[&run_id, &account_id, &application_id, &ticket_hash],
                    )?
                    .map(|row| row.get::<_, String>(0));
                if status.as_deref() == Some("side_effect_unknown") {
                    tx.commit()?;
                    return Ok(application);
                }
                anyhow::bail!("local run ticket is not active")
            }
            validate_application_transition(&application.state, "side_effect_unknown")?;
            if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'side_effect_unknown', updated_at_ms = $5
                  WHERE id = $1 AND account_id = $2 AND application_id = $3
                    AND ticket_hash = $4 AND expires_at_ms > $5
                    AND status IN ('claimed', 'needs_input')",
                &[&run_id, &account_id, &application_id, &ticket_hash, &now],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations
                    SET status = 'side_effect_unknown', updated_at_ms = $3
                  WHERE account_id = $1 AND application_id = $2",
                &[&account_id, &application_id, &now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions
                    SET status = 'needs_input', session_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &run_id, &session_payload, &now],
            )? != 1
            {
                anyhow::bail!("browser session not found")
            }
            if !application.receipt.is_object() {
                application.receipt = json!({});
            }
            application
                .receipt
                .as_object_mut()
                .expect("receipt normalized above")
                .insert(
                    "local_reconciliation".to_string(),
                    json!({
                        "status": "side_effect_unknown",
                        "recorded_at_ms": now,
                        "receipt": reconciliation_receipt,
                    }),
                );
            application.state = "side_effect_unknown".to_string();
            application.updated_at_ms = now;
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'side_effect_unknown',
                        application_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &application_id, &application_payload, &now],
            )? != 1
            {
                anyhow::bail!("application not found")
            }
            tx.commit()?;
            Ok(application)
        }
    })
}

pub fn approve_local_run_resume_action(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    intervention_id: &str,
) -> Result<Option<LocalRunResumeAction>> {
    if [account_id, application_id, run_id, intervention_id]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 240)
    {
        anyhow::bail!("invalid local resume approval")
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let ticket_expires_at: Option<i64> = tx
                .query_row(
                    "SELECT expires_at_ms FROM jobs_local_run_tickets
                      WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                        AND status = 'needs_input' AND expires_at_ms > ?4",
                    params![run_id, account_id, application_id, now],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(ticket_expires_at) = ticket_expires_at else {
                tx.commit()?;
                return Ok(None);
            };
            let intervention_valid: bool = tx.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_interventions
                     WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                       AND status = 'approved'
                )",
                params![intervention_id, account_id, application_id],
                |row| row.get(0),
            )?;
            if !intervention_valid {
                tx.commit()?;
                return Ok(None);
            }
            let existing: Option<(String, String, i64)> = tx
                .query_row(
                    "SELECT run_id, status, expires_at_ms
                       FROM jobs_local_run_resume_actions WHERE intervention_id = ?1",
                    params![intervention_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            if let Some((existing_run_id, status, expires_at_ms)) = existing {
                tx.commit()?;
                return Ok(
                    (status == "approved" && existing_run_id == run_id).then(|| {
                        LocalRunResumeAction {
                            run_id: run_id.to_string(),
                            intervention_id: intervention_id.to_string(),
                            action: "approve_submission".to_string(),
                            expires_at_ms,
                            account_id: account_id.to_string(),
                            application_id: application_id.to_string(),
                            first_consumption: false,
                        }
                    }),
                );
            }
            let expires_at_ms = ticket_expires_at.min(now + LOCAL_RESUME_ACTION_TTL_MS);
            let id = uuid::Uuid::new_v4().to_string();
            if let Err(error) = tx.execute(
                "INSERT INTO jobs_local_run_resume_actions (
                    id, run_id, account_id, application_id, intervention_id,
                    action, status, expires_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'approve_submission',
                           'approved', ?6, ?7)",
                params![
                    id,
                    run_id,
                    account_id,
                    application_id,
                    intervention_id,
                    expires_at_ms,
                    now,
                ],
            ) {
                if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                    return Ok(None);
                }
                return Err(error.into());
            }
            tx.commit()?;
            Ok(Some(LocalRunResumeAction {
                run_id: run_id.to_string(),
                intervention_id: intervention_id.to_string(),
                action: "approve_submission".to_string(),
                expires_at_ms,
                account_id: account_id.to_string(),
                application_id: application_id.to_string(),
                first_consumption: false,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let ticket_expires_at = tx
                .query_opt(
                    "SELECT expires_at_ms FROM jobs_local_run_tickets
                      WHERE id = $1 AND account_id = $2 AND application_id = $3
                        AND status = 'needs_input' AND expires_at_ms > $4
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id, &now],
                )?
                .map(|row| row.get::<_, i64>(0));
            let Some(ticket_expires_at) = ticket_expires_at else {
                tx.commit()?;
                return Ok(None);
            };
            let intervention_valid: bool = tx
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM jobs_interventions
                         WHERE id = $1 AND account_id = $2 AND application_id = $3
                           AND status = 'approved'
                    )",
                    &[&intervention_id, &account_id, &application_id],
                )?
                .get(0);
            if !intervention_valid {
                tx.commit()?;
                return Ok(None);
            }
            if let Some(row) = tx.query_opt(
                "SELECT run_id, status, expires_at_ms
                   FROM jobs_local_run_resume_actions WHERE intervention_id = $1 FOR UPDATE",
                &[&intervention_id],
            )? {
                let existing_run_id: String = row.get(0);
                let status: String = row.get(1);
                let expires_at_ms: i64 = row.get(2);
                tx.commit()?;
                return Ok(
                    (status == "approved" && existing_run_id == run_id).then(|| {
                        LocalRunResumeAction {
                            run_id: run_id.to_string(),
                            intervention_id: intervention_id.to_string(),
                            action: "approve_submission".to_string(),
                            expires_at_ms,
                            account_id: account_id.to_string(),
                            application_id: application_id.to_string(),
                            first_consumption: false,
                        }
                    }),
                );
            }
            let expires_at_ms = ticket_expires_at.min(now + LOCAL_RESUME_ACTION_TTL_MS);
            let id = uuid::Uuid::new_v4().to_string();
            if let Err(error) = tx.execute(
                "INSERT INTO jobs_local_run_resume_actions (
                    id, run_id, account_id, application_id, intervention_id,
                    action, status, expires_at_ms, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, 'approve_submission',
                           'approved', $6, $7)",
                &[
                    &id,
                    &run_id,
                    &account_id,
                    &application_id,
                    &intervention_id,
                    &expires_at_ms,
                    &now,
                ],
            ) {
                if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                    return Ok(None);
                }
                return Err(error.into());
            }
            tx.commit()?;
            Ok(Some(LocalRunResumeAction {
                run_id: run_id.to_string(),
                intervention_id: intervention_id.to_string(),
                action: "approve_submission".to_string(),
                expires_at_ms,
                account_id: account_id.to_string(),
                application_id: application_id.to_string(),
                first_consumption: false,
            }))
        }
    })
}

pub fn consume_local_run_resume_action(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
) -> Result<Option<LocalRunResumeAction>> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let value: Option<(String, String, String, String, String, i64, String)> = tx
                .query_row(
                    "SELECT a.id, a.account_id, a.application_id, a.intervention_id,
                            a.action, a.expires_at_ms, a.status
                      FROM jobs_local_run_resume_actions a
                       JOIN jobs_local_run_tickets t ON t.id = a.run_id
                       JOIN jobs_interventions i ON i.id = a.intervention_id
                      WHERE a.run_id = ?1 AND t.ticket_hash = ?2
                        AND t.status IN ('needs_input', 'claimed') AND t.expires_at_ms > ?3
                        AND a.status IN ('approved', 'consumed') AND a.expires_at_ms > ?3
                        AND i.account_id = a.account_id
                        AND i.application_id = a.application_id
                        AND i.status = 'approved'
                      ORDER BY a.created_at_ms DESC LIMIT 1",
                    params![run_id, ticket_hash, now],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                id,
                account_id,
                application_id,
                intervention_id,
                action,
                expires_at_ms,
                action_status,
            )) = value
            else {
                tx.commit()?;
                return Ok(None);
            };
            let action_changed = tx.execute(
                "UPDATE jobs_local_run_resume_actions
                    SET status = 'consumed', consumed_at_ms = COALESCE(consumed_at_ms, ?2)
                  WHERE id = ?1 AND status IN ('approved', 'consumed')",
                params![id, now],
            )?;
            let ticket_changed = tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET updated_at_ms = CASE WHEN status = 'needs_input' THEN ?3 ELSE updated_at_ms END,
                        status = 'claimed'
                  WHERE id = ?1 AND ticket_hash = ?2
                    AND status IN ('needs_input', 'claimed')",
                params![run_id, ticket_hash, now],
            )?;
            if action_changed != 1 || ticket_changed != 1 {
                return Ok(None);
            }
            tx.commit()?;
            Ok(Some(LocalRunResumeAction {
                run_id: run_id.to_string(),
                intervention_id,
                action,
                expires_at_ms,
                account_id,
                application_id,
                first_consumption: action_status == "approved",
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let value = tx.query_opt(
                "SELECT a.id, a.account_id, a.application_id, a.intervention_id,
                        a.action, a.expires_at_ms, a.status
                  FROM jobs_local_run_resume_actions a
                   JOIN jobs_local_run_tickets t ON t.id = a.run_id
                   JOIN jobs_interventions i ON i.id = a.intervention_id
                  WHERE a.run_id = $1 AND t.ticket_hash = $2
                    AND t.status IN ('needs_input', 'claimed') AND t.expires_at_ms > $3
                    AND a.status IN ('approved', 'consumed') AND a.expires_at_ms > $3
                    AND i.account_id = a.account_id
                    AND i.application_id = a.application_id
                    AND i.status = 'approved'
                  ORDER BY a.created_at_ms DESC LIMIT 1
                  FOR UPDATE OF a, t",
                &[&run_id, &ticket_hash, &now],
            )?;
            let Some(row) = value else {
                tx.commit()?;
                return Ok(None);
            };
            let id: String = row.get(0);
            let account_id: String = row.get(1);
            let application_id: String = row.get(2);
            let intervention_id: String = row.get(3);
            let action: String = row.get(4);
            let expires_at_ms: i64 = row.get(5);
            let action_status: String = row.get(6);
            let action_changed = tx.execute(
                "UPDATE jobs_local_run_resume_actions
                    SET status = 'consumed', consumed_at_ms = COALESCE(consumed_at_ms, $2)
                  WHERE id = $1 AND status IN ('approved', 'consumed')",
                &[&id, &now],
            )?;
            let ticket_changed = tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET updated_at_ms = CASE WHEN status = 'needs_input' THEN $3 ELSE updated_at_ms END,
                        status = 'claimed'
                  WHERE id = $1 AND ticket_hash = $2
                    AND status IN ('needs_input', 'claimed')",
                &[&run_id, &ticket_hash, &now],
            )?;
            if action_changed != 1 || ticket_changed != 1 {
                return Ok(None);
            }
            tx.commit()?;
            Ok(Some(LocalRunResumeAction {
                run_id: run_id.to_string(),
                intervention_id,
                action,
                expires_at_ms,
                account_id,
                application_id,
                first_consumption: action_status == "approved",
            }))
        }
    })
}

pub fn local_submission_approval_consumed(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_local_run_resume_actions
                     WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                       AND action = 'approve_submission' AND status = 'consumed'
                )",
                params![run_id, account_id, application_id],
                |row| row.get(0),
            )
            .context("check local submission approval"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_one(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_local_run_resume_actions
                     WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                       AND action = 'approve_submission' AND status = 'consumed'
                )",
                &[&run_id, &account_id, &application_id],
            )
            .map(|row| row.get(0))
            .context("check local submission approval"),
    })
}

fn local_run_ticket_from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalRunTicket> {
    let encrypted_ticket: String = row.get(4)?;
    let encrypted_payload: String = row.get(5)?;
    let ticket_secret = decrypt_payload(&encrypted_ticket).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, error.into())
    })?;
    let payload = parse_json(encrypted_payload, "Jobs local browser packet").map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, error.into())
    })?;
    Ok(LocalRunTicket {
        id: row.get(0)?,
        account_id: row.get(1)?,
        application_id: row.get(2)?,
        ticket_hash: row.get(3)?,
        ticket_secret,
        payload,
        status: row.get(6)?,
        expires_at_ms: row.get(7)?,
        created_at_ms: row.get(8)?,
        updated_at_ms: row.get(9)?,
    })
}

fn local_run_ticket_from_pg_row(row: postgres::Row) -> Result<LocalRunTicket> {
    let encrypted_ticket: String = row.get(4);
    let encrypted_payload: String = row.get(5);
    Ok(LocalRunTicket {
        id: row.get(0),
        account_id: row.get(1),
        application_id: row.get(2),
        ticket_hash: row.get(3),
        ticket_secret: decrypt_payload(&encrypted_ticket)?,
        payload: parse_json(encrypted_payload, "Jobs local browser packet")?,
        status: row.get(6),
        expires_at_ms: row.get(7),
        created_at_ms: row.get(8),
        updated_at_ms: row.get(9),
    })
}

type ExecutionLeaseResult<T> = std::result::Result<T, ExecutionLeaseError>;

#[derive(Debug)]
struct StoredExecutionLease {
    account_id: String,
    application_id: String,
    browser_profile_id: String,
    owner_id: String,
    lease_token_sha256: String,
    fence: i64,
    phase: String,
    lease_expires_at_ms: i64,
}

pub fn execution_browser_profile_id(account_id: &str, identity_id: &str) -> String {
    format!(
        "{}:{}",
        execution_scope_digest(account_id),
        execution_scope_digest(identity_id)
    )
}

fn execution_scope_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))[..24].to_string()
}

fn validate_execution_binding(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.trim() == value
        && value.bytes().all(|byte| !byte.is_ascii_control())
}

fn validate_execution_access(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<()> {
    if !validate_execution_binding(account_id, 240)
        || !validate_execution_binding(application_id, 240)
        || !validate_execution_binding(run_id, 240)
        || lease_token.is_empty()
        || lease_token.len() > 256
        || fence <= 0
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    Ok(())
}

fn random_execution_lease_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS random source");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn execution_lease_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn execution_lease_token_matches(stored_hash: &str, token: &str) -> bool {
    let supplied_hash = execution_lease_token_hash(token);
    supplied_hash.len() == stored_hash.len()
        && supplied_hash
            .as_bytes()
            .ct_eq(stored_hash.as_bytes())
            .unwrap_u8()
            == 1
}

fn execution_lease_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredExecutionLease> {
    Ok(StoredExecutionLease {
        account_id: row.get(1)?,
        application_id: row.get(2)?,
        browser_profile_id: row.get(3)?,
        owner_id: row.get(4)?,
        lease_token_sha256: row.get(5)?,
        fence: row.get(6)?,
        phase: row.get(7)?,
        lease_expires_at_ms: row.get(8)?,
    })
}

fn execution_lease_from_pg_row(row: postgres::Row) -> StoredExecutionLease {
    StoredExecutionLease {
        account_id: row.get(1),
        application_id: row.get(2),
        browser_profile_id: row.get(3),
        owner_id: row.get(4),
        lease_token_sha256: row.get(5),
        fence: row.get(6),
        phase: row.get(7),
        lease_expires_at_ms: row.get(8),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_execution_target_payloads(
    application_raw: String,
    application_job_id: &str,
    application_state: &str,
    session_raw: String,
    session_runner: &str,
    identity_raw: String,
    identity_status: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<()> {
    let application = parse_application_json(
        application_raw,
        application_id,
        application_job_id,
        "job application",
    )?;
    if application.run_id.as_deref() != Some(run_id) || application.state != application_state {
        return Err(ExecutionLeaseError::NotFound);
    }
    if !matches!(application_state, "queued" | "running" | "needs_input") {
        return Err(ExecutionLeaseError::Conflict);
    }

    let session: BrowserSession = parse_json(session_raw, "browser session")?;
    if session.id != run_id
        || session.application_id.as_deref() != Some(application_id)
        || session.runner != session_runner
        || session_runner != "cloud"
    {
        return Err(ExecutionLeaseError::NotFound);
    }

    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    let frozen_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?;
    if application
        .receipt
        .pointer("/application_identity/verified")
        .and_then(Value::as_bool)
        != Some(true)
        || identity_status != "verified"
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    let identity: ApplicationIdentity = parse_json(identity_raw, "application identity")?;
    if identity.id != identity_id || identity.email != frozen_email {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn sqlite_execution_target(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<String> {
    let (application_job_id, application_raw, application_state): (String, String, String) = tx
        .query_row(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, application_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let application = parse_application_json(
        application_raw.clone(),
        application_id,
        &application_job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?;
    let (session_raw, session_runner): (String, String) = tx
        .query_row(
            "SELECT session_json, runner FROM jobs_browser_sessions
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let (identity_raw, identity_status): (String, String) = tx
        .query_row(
            "SELECT identity_json, verification_status
               FROM jobs_application_identities
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, identity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::Conflict)?;
    validate_execution_target_payloads(
        application_raw,
        &application_job_id,
        &application_state,
        session_raw,
        &session_runner,
        identity_raw,
        &identity_status,
        application_id,
        run_id,
    )?;
    Ok(execution_browser_profile_id(account_id, identity_id))
}

fn postgres_execution_target(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<String> {
    let row = tx
        .query_opt(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&account_id, &application_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let application_job_id: String = row.get(0);
    let application_raw: String = row.get(1);
    let application_state: String = row.get(2);
    let application = parse_application_json(
        application_raw.clone(),
        application_id,
        &application_job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?
        .to_string();
    let row = tx
        .query_opt(
            "SELECT session_json, runner FROM jobs_browser_sessions
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &run_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let session_raw: String = row.get(0);
    let session_runner: String = row.get(1);
    let row = tx
        .query_opt(
            "SELECT identity_json, verification_status
               FROM jobs_application_identities
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &identity_id],
        )?
        .ok_or(ExecutionLeaseError::Conflict)?;
    let identity_raw: String = row.get(0);
    let identity_status: String = row.get(1);
    validate_execution_target_payloads(
        application_raw,
        &application_job_id,
        &application_state,
        session_raw,
        &session_runner,
        identity_raw,
        &identity_status,
        application_id,
        run_id,
    )?;
    Ok(execution_browser_profile_id(account_id, &identity_id))
}

fn sqlite_next_execution_fence(
    tx: &rusqlite::Transaction<'_>,
    application_id: &str,
    browser_profile_id: &str,
) -> ExecutionLeaseResult<i64> {
    let current: i64 = tx.query_row(
        "SELECT COALESCE(MAX(fence), 0) FROM jobs_execution_leases
          WHERE application_id = ?1 OR browser_profile_id = ?2",
        params![application_id, browser_profile_id],
        |row| row.get(0),
    )?;
    current.checked_add(1).ok_or(ExecutionLeaseError::Conflict)
}

fn postgres_next_execution_fence(
    tx: &mut postgres::Transaction<'_>,
    application_id: &str,
    browser_profile_id: &str,
) -> ExecutionLeaseResult<i64> {
    let current: i64 = tx
        .query_one(
            "SELECT COALESCE(MAX(fence), 0) FROM jobs_execution_leases
              WHERE application_id = $1 OR browser_profile_id = $2",
            &[&application_id, &browser_profile_id],
        )?
        .get(0);
    current.checked_add(1).ok_or(ExecutionLeaseError::Conflict)
}

pub fn claim_execution_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    supplied_browser_profile_id: &str,
    owner_id: &str,
) -> ExecutionLeaseResult<ExecutionLeaseGrant> {
    if !validate_execution_binding(account_id, 240)
        || !validate_execution_binding(application_id, 240)
        || !validate_execution_binding(run_id, 240)
        || !validate_execution_binding(supplied_browser_profile_id, 160)
        || !validate_execution_binding(owner_id, 240)
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let lease_token = random_execution_lease_token();
    let lease_token_sha256 = execution_lease_token_hash(&lease_token);
    let now = now_ms();
    let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let browser_profile_id =
                sqlite_execution_target(&tx, account_id, application_id, run_id)?;
            if browser_profile_id != supplied_browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            let existing = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases WHERE run_id = ?1",
                    params![run_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?;
            let fence = if let Some(existing) = existing {
                if existing.account_id != account_id
                    || existing.application_id != application_id
                    || existing.browser_profile_id != browser_profile_id
                    || existing.phase != "prepared"
                    || (existing.lease_expires_at_ms > now && existing.owner_id != owner_id)
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence = sqlite_next_execution_fence(&tx, application_id, &browser_profile_id)?;
                if tx.execute(
                    "UPDATE jobs_execution_leases
                        SET owner_id = ?2, lease_token_sha256 = ?3, fence = ?4,
                            lease_expires_at_ms = ?5, updated_at_ms = ?6
                      WHERE run_id = ?1 AND phase = 'prepared' AND fence = ?7",
                    params![
                        run_id,
                        owner_id,
                        lease_token_sha256,
                        fence,
                        lease_expires_at_ms,
                        now,
                        existing.fence,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                fence
            } else {
                tx.execute(
                    "UPDATE jobs_execution_leases
                        SET phase = 'released', updated_at_ms = ?3, finished_at_ms = ?3
                      WHERE run_id <> ?1
                        AND (application_id = ?2 OR browser_profile_id = ?4)
                        AND phase = 'prepared' AND lease_expires_at_ms <= ?3",
                    params![run_id, application_id, now, browser_profile_id],
                )?;
                let active: bool = tx.query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM jobs_execution_leases
                         WHERE (application_id = ?1 OR browser_profile_id = ?2)
                           AND phase IN ('prepared', 'click_started')
                    )",
                    params![application_id, browser_profile_id],
                    |row| row.get(0),
                )?;
                if active {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence = sqlite_next_execution_fence(&tx, application_id, &browser_profile_id)?;
                if let Err(error) = tx.execute(
                    "INSERT INTO jobs_execution_leases (
                        run_id, account_id, application_id, browser_profile_id, owner_id,
                        lease_token_sha256, fence, phase, lease_expires_at_ms,
                        created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'prepared', ?8, ?9, ?9)",
                    params![
                        run_id,
                        account_id,
                        application_id,
                        browser_profile_id,
                        owner_id,
                        lease_token_sha256,
                        fence,
                        lease_expires_at_ms,
                        now,
                    ],
                ) {
                    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    return Err(error.into());
                }
                fence
            };
            tx.commit()?;
            Ok(ExecutionLeaseGrant {
                run_id: run_id.to_string(),
                lease_token,
                fence,
                lease_expires_at_ms,
                phase: "prepared".to_string(),
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let browser_profile_id =
                postgres_execution_target(&mut tx, account_id, application_id, run_id)?;
            if browser_profile_id != supplied_browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            let existing = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases WHERE run_id = $1 FOR UPDATE",
                    &[&run_id],
                )?
                .map(execution_lease_from_pg_row);
            let fence = if let Some(existing) = existing {
                if existing.account_id != account_id
                    || existing.application_id != application_id
                    || existing.browser_profile_id != browser_profile_id
                    || existing.phase != "prepared"
                    || (existing.lease_expires_at_ms > now && existing.owner_id != owner_id)
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence =
                    postgres_next_execution_fence(&mut tx, application_id, &browser_profile_id)?;
                if tx.execute(
                    "UPDATE jobs_execution_leases
                        SET owner_id = $2, lease_token_sha256 = $3, fence = $4,
                            lease_expires_at_ms = $5, updated_at_ms = $6
                      WHERE run_id = $1 AND phase = 'prepared' AND fence = $7",
                    &[
                        &run_id,
                        &owner_id,
                        &lease_token_sha256,
                        &fence,
                        &lease_expires_at_ms,
                        &now,
                        &existing.fence,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                fence
            } else {
                tx.execute(
                    "UPDATE jobs_execution_leases
                        SET phase = 'released', updated_at_ms = $3, finished_at_ms = $3
                      WHERE run_id <> $1
                        AND (application_id = $2 OR browser_profile_id = $4)
                        AND phase = 'prepared' AND lease_expires_at_ms <= $3",
                    &[&run_id, &application_id, &now, &browser_profile_id],
                )?;
                let active: bool = tx
                    .query_one(
                        "SELECT EXISTS(
                            SELECT 1 FROM jobs_execution_leases
                             WHERE (application_id = $1 OR browser_profile_id = $2)
                               AND phase IN ('prepared', 'click_started')
                        )",
                        &[&application_id, &browser_profile_id],
                    )?
                    .get(0);
                if active {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence =
                    postgres_next_execution_fence(&mut tx, application_id, &browser_profile_id)?;
                if let Err(error) = tx.execute(
                    "INSERT INTO jobs_execution_leases (
                        run_id, account_id, application_id, browser_profile_id, owner_id,
                        lease_token_sha256, fence, phase, lease_expires_at_ms,
                        created_at_ms, updated_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'prepared', $8, $9, $9)",
                    &[
                        &run_id,
                        &account_id,
                        &application_id,
                        &browser_profile_id,
                        &owner_id,
                        &lease_token_sha256,
                        &fence,
                        &lease_expires_at_ms,
                        &now,
                    ],
                ) {
                    if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    return Err(error.into());
                }
                fence
            };
            tx.commit()?;
            Ok(ExecutionLeaseGrant {
                run_id: run_id.to_string(),
                lease_token,
                fence,
                lease_expires_at_ms,
                phase: "prepared".to_string(),
            })
        }
    })
}

pub fn heartbeat_execution_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<ExecutionLeaseRecord> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    let now = now_ms();
    let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let lease = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    params![run_id, account_id, application_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || !matches!(lease.phase.as_str(), "prepared" | "click_started")
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET lease_expires_at_ms = ?6, updated_at_ms = ?7
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND lease_token_sha256 = ?4 AND fence = ?5 AND phase = ?8
                    AND lease_expires_at_ms > ?7",
                params![
                    run_id,
                    account_id,
                    application_id,
                    lease.lease_token_sha256,
                    fence,
                    lease_expires_at_ms,
                    now,
                    lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: lease.phase,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let lease = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(execution_lease_from_pg_row)
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || !matches!(lease.phase.as_str(), "prepared" | "click_started")
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET lease_expires_at_ms = $6, updated_at_ms = $7
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND lease_token_sha256 = $4 AND fence = $5 AND phase = $8
                    AND lease_expires_at_ms > $7",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &lease.lease_token_sha256,
                    &fence,
                    &lease_expires_at_ms,
                    &now,
                    &lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: lease.phase,
            })
        }
    })
}

pub fn start_irreversible_submission(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<ExecutionLeaseRecord> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let browser_profile_id =
                sqlite_execution_target(&tx, account_id, application_id, run_id)?;
            let lease = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    params![run_id, account_id, application_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.browser_profile_id != browser_profile_id
                || lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || lease.phase != "prepared"
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET phase = 'click_started', updated_at_ms = ?6
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND lease_token_sha256 = ?4 AND fence = ?5
                    AND phase = 'prepared' AND lease_expires_at_ms > ?6",
                params![
                    run_id,
                    account_id,
                    application_id,
                    lease.lease_token_sha256,
                    fence,
                    now,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            let lease_expires_at_ms = lease.lease_expires_at_ms;
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: "click_started".to_string(),
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let browser_profile_id =
                postgres_execution_target(&mut tx, account_id, application_id, run_id)?;
            let lease = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(execution_lease_from_pg_row)
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.browser_profile_id != browser_profile_id
                || lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || lease.phase != "prepared"
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET phase = 'click_started', updated_at_ms = $6
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND lease_token_sha256 = $4 AND fence = $5
                    AND phase = 'prepared' AND lease_expires_at_ms > $6",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &lease.lease_token_sha256,
                    &fence,
                    &now,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            let lease_expires_at_ms = lease.lease_expires_at_ms;
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: "click_started".to_string(),
            })
        }
    })
}

fn execution_finish_allowed(phase: &str, outcome: &str) -> bool {
    match phase {
        "prepared" => matches!(outcome, "failed" | "released"),
        "click_started" => matches!(outcome, "submitted" | "side_effect_unknown"),
        _ => false,
    }
}

pub fn execution_lease_phase_for_application(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<Option<String>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT phase FROM jobs_execution_leases
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                params![run_id, account_id, application_id],
                |row| row.get(0),
            )
            .optional()
            .context("get execution lease phase"),
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query_opt(
                "SELECT phase FROM jobs_execution_leases
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3",
                &[&run_id, &account_id, &application_id],
            )?
            .map(|row| row.get(0))),
    })
}

pub fn finish_execution_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
    outcome: &str,
) -> ExecutionLeaseResult<()> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    if !matches!(
        outcome,
        "submitted" | "failed" | "side_effect_unknown" | "released"
    ) {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let lease = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    params![run_id, account_id, application_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if lease.phase == outcome {
                tx.commit()?;
                return Ok(());
            }
            if !execution_finish_allowed(&lease.phase, outcome) {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = ?6, updated_at_ms = ?7,
                        finished_at_ms = ?7
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND lease_token_sha256 = ?4 AND fence = ?5 AND phase = ?8",
                params![
                    run_id,
                    account_id,
                    application_id,
                    lease.lease_token_sha256,
                    fence,
                    outcome,
                    now,
                    lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let lease = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(execution_lease_from_pg_row)
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if lease.phase == outcome {
                tx.commit()?;
                return Ok(());
            }
            if !execution_finish_allowed(&lease.phase, outcome) {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = $6, updated_at_ms = $7,
                        finished_at_ms = $7
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND lease_token_sha256 = $4 AND fence = $5 AND phase = $8",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &lease.lease_token_sha256,
                    &fence,
                    &outcome,
                    &now,
                    &lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(())
        }
    })
}

pub fn workspace(pool: &DbPool, account_id: &str, email: &str) -> Result<JobsWorkspace> {
    let _ = ensure_primary_application_identity(pool, account_id, email)?;
    let profile = get_profile(pool, account_id, email)?;
    let preferences = get_preferences(pool, account_id)?;
    let applications = list_applications(pool, account_id)?;
    let reservations = list_attempt_reservations(pool, account_id)?;
    let mut matches = list_postings(pool, account_id)?;
    for posting in &mut matches {
        let existing_application_id = applications
            .iter()
            .find(|application| application.job_id == posting.id)
            .map(|application| application.id.as_str());
        let mut eligibility = build_job_eligibility(
            posting,
            &profile,
            &preferences,
            &reservations,
            true,
            existing_application_id,
        );
        apply_discovery_authority(pool, account_id, posting, &mut eligibility)?;
        posting.eligibility = Some(eligibility);
    }
    Ok(JobsWorkspace {
        profile,
        preferences,
        facts: list_facts(pool, account_id)?,
        tracks: list_tracks(pool, account_id)?,
        matches,
        applications,
        application_evidence: list_application_evidence(pool, account_id, None)?,
        browser_sessions: list_browser_sessions(pool, account_id)?,
        interventions: list_interventions(pool, account_id)?,
        answer_memory: list_answer_memory(pool, account_id)?,
        candidate_events: list_candidate_events(pool, account_id)?,
        integrations: list_integrations(pool, account_id)?,
        application_identities: list_application_identities(pool, account_id)?,
        mailbox_connections: list_mailbox_connections(pool, account_id)?,
        discovery_sources: list_discovery_sources(pool, account_id)?
            .iter()
            .map(DiscoverySourceSummary::from)
            .collect(),
        entitlement: get_entitlement(pool, account_id)?,
    })
}

pub fn account_export(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<Option<JobsAccountExport>> {
    let exists = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_profiles WHERE account_id = ?1)",
                params![account_id],
                |row| row.get::<_, bool>(0),
            )
            .context("check Jobs export data"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM jobs_profiles WHERE account_id = $1)",
                &[&account_id],
            )
            .map(|row| row.get::<_, bool>(0))
            .context("check Jobs export data"),
    })?;
    if !exists {
        return Ok(None);
    }

    let mut workspace = workspace(pool, account_id, email)?;
    for session in &mut workspace.browser_sessions {
        session.takeover_url = None;
    }
    Ok(Some(JobsAccountExport {
        workspace,
        resume_versions: list_resume_versions(pool, account_id)?,
        attempt_reservations: list_attempt_reservations(pool, account_id)?,
        run_events: list_account_run_events(pool, account_id)?,
    }))
}

#[cfg(test)]
#[path = "jobs/postgres_local_authority_tests.rs"]
mod postgres_local_authority_tests;

include!("jobs/tests.rs");
