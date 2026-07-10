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
use rand::RngCore;
use rusqlite::{params, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::DbPool;

pub const PACKET_OVERAGE_CENTS: i64 = 50;
const ENCRYPTED_PAYLOAD_PREFIX: &str = "bluey-jobs:v1:";

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
    pub track_id: String,
    #[serde(default)]
    pub match_score: i64,
    #[serde(default)]
    pub matched_reasons: Vec<String>,
    #[serde(default)]
    pub missing_requirements: Vec<String>,
    #[serde(default = "default_match_status")]
    pub status: String,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
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
    pub created_at_ms: i64,
    #[serde(default)]
    pub resolved_at_ms: Option<i64>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub id: String,
    pub run_id: String,
    pub event_type: String,
    pub event: Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobsWorkspace {
    pub profile: CareerProfile,
    pub preferences: JobPreferences,
    pub facts: Vec<CareerFact>,
    pub tracks: Vec<CareerTrack>,
    pub matches: Vec<JobPosting>,
    pub applications: Vec<JobApplication>,
    pub browser_sessions: Vec<BrowserSession>,
    pub interventions: Vec<Intervention>,
    pub integrations: Vec<JobsIntegration>,
    pub entitlement: JobsEntitlement,
}

#[derive(Debug, Clone, Serialize)]
pub struct PacketCommitResult {
    pub newly_metered: bool,
    pub included: bool,
    pub amount_cents: i64,
    pub used_packets: i64,
    pub monthly_packet_limit: i64,
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

fn default_application_state() -> String {
    "matched".to_string()
}

fn default_open_status() -> String {
    "open".to_string()
}

fn default_disconnected_status() -> String {
    "disconnected".to_string()
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn parse_json<T: DeserializeOwned>(raw: String, label: &str) -> Result<T> {
    let plain = decrypt_payload(&raw).with_context(|| format!("decrypt {label}"))?;
    serde_json::from_str(&plain).with_context(|| format!("parse {label}"))
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
        posting
            .canonical_url
            .trim()
            .trim_end_matches('/')
            .to_lowercase()
    );
    hex::encode(Sha256::digest(normalized.as_bytes()))
}

pub fn default_profile(email: &str) -> CareerProfile {
    CareerProfile {
        email: email.to_string(),
        ..CareerProfile::default()
    }
}

pub fn get_profile(pool: &DbPool, account_id: &str, email: &str) -> Result<CareerProfile> {
    crate::db::run_blocking_db(|| match pool {
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
    })
}

pub fn save_profile(
    pool: &DbPool,
    account_id: &str,
    profile: &CareerProfile,
) -> Result<CareerProfile> {
    let mut value = profile.clone();
    value.onboarding_step = value.onboarding_step.clamp(0, 6);
    value.auto_submit_threshold = value.auto_submit_threshold.clamp(60, 100);
    value.daily_limit = value.daily_limit.clamp(1, 50);
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
                    &i64::from(value.onboarding_complete),
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
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "Jobs preferences"))
                .transpose()
                .map(|value| value.unwrap_or_default())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "SELECT preferences_json FROM jobs_preferences WHERE account_id = $1",
                &[&account_id],
            )?;
            row.map(|value| parse_json(value.get(0), "Jobs preferences"))
                .transpose()
                .map(|value| value.unwrap_or_default())
        }
    })
}

pub fn save_preferences(
    pool: &DbPool,
    account_id: &str,
    preferences: &JobPreferences,
) -> Result<JobPreferences> {
    let mut value = preferences.clone();
    value.daily_limit = value.daily_limit.clamp(1, 50);
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
                    &i64::from(value.active),
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
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "DELETE FROM jobs_tracks WHERE account_id = ?1 AND id = ?2",
            params![account_id, track_id],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "DELETE FROM jobs_tracks WHERE account_id = $1 AND id = $2",
            &[&account_id, &track_id],
        )? > 0),
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
    value.updated_at_ms = now;
    let payload = to_json(&value, "job posting")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO jobs_postings (
                    id, account_id, canonical_key, posting_json, source, canonical_url,
                    company, title, location, match_score, status, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(account_id, canonical_key) DO UPDATE SET
                    posting_json = excluded.posting_json,
                    source = excluded.source,
                    canonical_url = excluded.canonical_url,
                    company = excluded.company,
                    title = excluded.title,
                    location = excluded.location,
                    match_score = excluded.match_score,
                    status = excluded.status,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    value.id,
                    account_id,
                    value.canonical_key,
                    payload,
                    value.source,
                    value.canonical_url,
                    value.company,
                    value.title,
                    value.location,
                    value.match_score,
                    value.status,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            let raw: String = conn.query_row(
                "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND canonical_key = ?2",
                params![account_id, value.canonical_key],
                |row| row.get(0),
            )?;
            parse_json(raw, "job posting")
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_one(
                "INSERT INTO jobs_postings (
                    id, account_id, canonical_key, posting_json, source, canonical_url,
                    company, title, location, match_score, status, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT(account_id, canonical_key) DO UPDATE SET
                    posting_json = EXCLUDED.posting_json,
                    source = EXCLUDED.source,
                    canonical_url = EXCLUDED.canonical_url,
                    company = EXCLUDED.company,
                    title = EXCLUDED.title,
                    location = EXCLUDED.location,
                    match_score = EXCLUDED.match_score,
                    status = EXCLUDED.status,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 RETURNING posting_json",
                &[
                    &value.id,
                    &account_id,
                    &value.canonical_key,
                    &payload,
                    &value.source,
                    &value.canonical_url,
                    &value.company,
                    &value.title,
                    &value.location,
                    &value.match_score,
                    &value.status,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            parse_json(row.get(0), "job posting")
        }
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

    if posting.compensation.is_empty() || preferences.minimum_compensation.is_none() {
        reasons.push("Compensation needs confirmation".to_string());
    }

    (score.clamp(0, 99), reasons, missing)
}

pub fn list_applications(pool: &DbPool, account_id: &str) -> Result<Vec<JobApplication>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT application_json FROM jobs_applications
                  WHERE account_id = ?1 ORDER BY updated_at_ms DESC",
            )?;
            let raws = stmt
                .query_map(params![account_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raws.into_iter()
                .map(|raw| parse_json(raw, "job application"))
                .collect()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT application_json FROM jobs_applications
                  WHERE account_id = $1 ORDER BY updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| parse_json(row.get(0), "job application"))
            .collect(),
    })
}

pub fn get_application(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
) -> Result<Option<JobApplication>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT application_json FROM jobs_applications WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "job application"))
                .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT application_json FROM jobs_applications WHERE account_id = $1 AND id = $2",
                &[&account_id, &application_id],
            )?
            .map(|row| parse_json(row.get(0), "job application"))
            .transpose(),
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
    let profile = get_profile(pool, account_id, "")?;
    let posting =
        get_posting(pool, account_id, job_id)?.ok_or_else(|| anyhow::anyhow!("job not found"))?;
    let facts = list_facts(pool, account_id)?;
    let approved_fact_ids: Vec<String> = facts
        .iter()
        .filter(|fact| fact.source == "resume_import" || fact.verification_status == "confirmed")
        .map(|fact| fact.id.clone())
        .collect();
    let selected_skills = select_skills(&profile.skills, &posting.description);
    let summary = tailored_summary(&profile, &posting, mode);
    let content = json!({
        "target": {
            "job_id": posting.id,
            "company": posting.company,
            "title": posting.title,
            "location": posting.location,
        },
        "contact": {
            "name": profile.full_name,
            "email": profile.email,
            "phone": profile.phone,
            "location": profile.current_location,
            "linkedin_url": profile.linkedin_url,
            "portfolio_url": profile.portfolio_url,
        },
        "headline": if profile.headline.is_empty() { posting.title.clone() } else { profile.headline.clone() },
        "summary": summary,
        "skills": selected_skills,
        "employment": profile.employment,
        "education": profile.education,
        "projects": profile.projects,
        "certifications": profile.certifications,
        "source_resume_name": profile.source_resume_name,
        "provenance": {
            "fact_ids": approved_fact_ids,
            "mode": mode,
            "generated_for_job_id": posting.id,
        },
    });
    let diff = json!({
        "headline": format!("Focused on {}", posting.title),
        "summary": "Reordered and tightened around the role and company.",
        "skills": "Prioritized skills found in the job description.",
        "claims_added": [],
    });
    let checksum_source = format!("{}|{}|{}", account_id, job_id, content);
    let checksum = hex::encode(Sha256::digest(checksum_source.as_bytes()));
    let resume = save_resume_version(
        pool,
        account_id,
        job_id,
        mode,
        content,
        diff,
        approved_fact_ids,
        checksum,
    )?;

    let now = now_ms();
    let existing = find_application_for_job(pool, account_id, job_id)?;
    let mut application = existing.unwrap_or(JobApplication {
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
    application.resume_version_id = Some(resume.id.clone());
    let auto_submit_eligible = submission_mode == "auto_submit"
        && posting.match_score >= profile.auto_submit_threshold.clamp(60, 100)
        && posting.missing_requirements.is_empty()
        && !posting.source.ends_with("_handoff");
    application.state = if auto_submit_eligible {
        "queued".to_string()
    } else {
        "awaiting_review".to_string()
    };
    application.submission_mode = submission_mode.to_string();
    application.match_score = posting.match_score;
    application.updated_at_ms = now;
    application.receipt = json!({
        "job_snapshot": posting,
        "resume_version_id": resume.id,
        "prepared_at_ms": now,
        "final_answers": application.answers,
        "confirmation": Value::Null,
    });
    save_application(pool, account_id, &application)?;
    Ok((application, resume))
}

fn select_skills(skills: &[String], description: &str) -> Vec<String> {
    let lower = description.to_lowercase();
    let mut matching: Vec<String> = skills
        .iter()
        .filter(|skill| lower.contains(&skill.to_lowercase()))
        .cloned()
        .collect();
    let remaining_slots = 12usize.saturating_sub(matching.len());
    let fallback: Vec<String> = skills
        .iter()
        .filter(|skill| {
            !matching
                .iter()
                .any(|known| known.eq_ignore_ascii_case(skill))
        })
        .take(remaining_slots)
        .cloned()
        .collect();
    matching.extend(fallback);
    matching.truncate(12);
    matching
}

fn tailored_summary(profile: &CareerProfile, posting: &JobPosting, mode: &str) -> String {
    let base = profile.summary.trim();
    if mode == "enhance" {
        if base.is_empty() {
            format!(
                "Candidate targeting the {} role at {}, with experience aligned to the role's core responsibilities.",
                posting.title, posting.company
            )
        } else {
            format!(
                "{} Focused for the {} opportunity at {}.",
                base.trim_end_matches('.'),
                posting.title,
                posting.company
            )
        }
    } else {
        base.to_string()
    }
}

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

fn find_application_for_job(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
) -> Result<Option<JobApplication>> {
    crate::db::run_blocking_db(|| {
        match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT application_json FROM jobs_applications WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, job_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "job application"))
                .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT application_json FROM jobs_applications WHERE account_id = $1 AND job_id = $2",
                &[&account_id, &job_id],
            )?
            .map(|row| parse_json(row.get(0), "job application"))
            .transpose(),
    }
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

pub fn validate_application_state(state: &str) -> Result<()> {
    const STATES: &[&str] = &[
        "matched",
        "preparing",
        "needs_confirmation",
        "awaiting_review",
        "queued",
        "running",
        "needs_input",
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
        "running" => matches!(next, "needs_input" | "submitted" | "failed"),
        "needs_input" => matches!(next, "queued" | "running" | "failed"),
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
                    Ok(JobsEntitlement {
                        plan: row.get(0)?,
                        track_limit: row.get(1)?,
                        monthly_packet_limit: row.get(2)?,
                        used_packets: row.get(3)?,
                        period_start_ms: row.get(4)?,
                        period_end_ms: row.get(5)?,
                        local_browser: row.get::<_, i64>(6)? != 0,
                        cloud_browser: row.get::<_, i64>(7)? != 0,
                        overage_cents: PACKET_OVERAGE_CENTS,
                    })
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
            Ok(JobsEntitlement {
                plan: row.get(0),
                track_limit: row.get(1),
                monthly_packet_limit: row.get(2),
                used_packets: row.get(3),
                period_start_ms: row.get(4),
                period_end_ms: row.get(5),
                local_browser: row.get::<_, i64>(6) != 0,
                cloud_browser: row.get::<_, i64>(7) != 0,
                overage_cents: PACKET_OVERAGE_CENTS,
            })
        }
    })
}

pub fn set_entitlement_plan(
    pool: &DbPool,
    account_id: &str,
    plan: &str,
) -> Result<JobsEntitlement> {
    let (track_limit, packet_limit, local_browser, cloud_browser) = match plan {
        "pro" => (3, 50, 1, 0),
        "cloud" => (5, 100, 1, 1),
        _ => (1, 5, 0, 0),
    };
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
            let included = used < limit;
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
                    updated_at_ms = ?2 WHERE account_id = ?1",
                params![account_id, now],
            )?;
            tx.commit()?;
            Ok(PacketCommitResult {
                newly_metered: true,
                included,
                amount_cents,
                used_packets: used + 1,
                monthly_packet_limit: limit,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&metering_key],
            )?;
            if let Some(row) = tx.query_opt(
                "SELECT included, amount_cents FROM jobs_packet_metering
                  WHERE account_id = $1 AND job_id = $2",
                &[&account_id, &application.job_id],
            )? {
                let entitlement = tx.query_one(
                    "SELECT used_packets, monthly_packet_limit FROM jobs_entitlements WHERE account_id = $1",
                    &[&account_id],
                )?;
                return Ok(PacketCommitResult {
                    newly_metered: false,
                    included: row.get::<_, i64>(0) != 0,
                    amount_cents: row.get(1),
                    used_packets: entitlement.get(0),
                    monthly_packet_limit: entitlement.get(1),
                });
            }
            let entitlement = tx.query_one(
                "SELECT used_packets, monthly_packet_limit FROM jobs_entitlements
                  WHERE account_id = $1 FOR UPDATE",
                &[&account_id],
            )?;
            let used: i64 = entitlement.get(0);
            let limit: i64 = entitlement.get(1);
            let included = used < limit;
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
                    &i64::from(included),
                    &amount_cents,
                    &now,
                ],
            )?;
            tx.execute(
                "UPDATE jobs_entitlements SET used_packets = used_packets + 1,
                    updated_at_ms = $2 WHERE account_id = $1",
                &[&account_id, &now],
            )?;
            tx.commit()?;
            Ok(PacketCommitResult {
                newly_metered: true,
                included,
                amount_cents,
                used_packets: used + 1,
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
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    if value.created_at_ms == 0 {
        value.created_at_ms = now_ms();
    }
    if value.status == "resolved" && value.resolved_at_ms.is_none() {
        value.resolved_at_ms = Some(now_ms());
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

pub fn list_integrations(pool: &DbPool, account_id: &str) -> Result<Vec<JobsIntegration>> {
    let mut integrations: Vec<JobsIntegration> = list_payloads(
        pool,
        account_id,
        "jobs_integrations",
        "integration_json",
        "provider ASC",
        "Jobs integration",
    )?;
    for (provider, capabilities) in [
        ("gmail", vec!["status_sync", "follow_ups"]),
        ("outlook_email", vec!["status_sync", "follow_ups"]),
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

pub fn workspace(pool: &DbPool, account_id: &str, email: &str) -> Result<JobsWorkspace> {
    Ok(JobsWorkspace {
        profile: get_profile(pool, account_id, email)?,
        preferences: get_preferences(pool, account_id)?,
        facts: list_facts(pool, account_id)?,
        tracks: list_tracks(pool, account_id)?,
        matches: list_postings(pool, account_id)?,
        applications: list_applications(pool, account_id)?,
        browser_sessions: list_browser_sessions(pool, account_id)?,
        interventions: list_interventions(pool, account_id)?,
        integrations: list_integrations(pool, account_id)?,
        entitlement: get_entitlement(pool, account_id)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn test_pool() -> DbPool {
        let pool = db::open_pool(std::path::Path::new(":memory:")).unwrap();
        db::run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ('acct-jobs', 'jobs@example.com', 'hash', 0)",
            [],
        )
        .unwrap();
        drop(conn);
        pool
    }

    #[test]
    fn canonical_job_key_deduplicates_url_slash_and_case() {
        let a = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "greenhouse".to_string(),
            external_id: String::new(),
            company: "Acme".to_string(),
            title: "Product Designer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: "https://boards.example/jobs/1/".to_string(),
            description: String::new(),
            compensation: String::new(),
            track_id: String::new(),
            match_score: 0,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let mut b = a.clone();
        b.company = "ACME".to_string();
        b.canonical_url = "https://boards.example/jobs/1".to_string();
        assert_eq!(canonical_job_key(&a), canonical_job_key(&b));
    }

    #[test]
    fn packet_resume_is_job_specific_and_idempotent() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        profile.skills = vec!["Rust".to_string(), "TypeScript".to_string()];
        profile.summary = "Builds reliable customer products.".to_string();
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "pasted_link".to_string(),
                external_id: String::new(),
                company: "Northstar".to_string(),
                title: "Senior Product Engineer".to_string(),
                location: "Remote".to_string(),
                workplace: "remote".to_string(),
                canonical_url: "https://example.com/jobs/42".to_string(),
                description: "Rust and TypeScript".to_string(),
                compensation: String::new(),
                track_id: String::new(),
                match_score: 86,
                matched_reasons: vec!["Skills fit".to_string()],
                missing_requirements: Vec::new(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application_a, resume_a) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let (application_b, resume_b) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        assert_eq!(application_a.id, application_b.id);
        assert_eq!(resume_a.id, resume_b.id);
        assert_eq!(resume_a.job_id, posting.id);
    }

    #[test]
    fn packet_metering_only_counts_a_job_once() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "pasted_link".to_string(),
                external_id: String::new(),
                company: "Acme".to_string(),
                title: "Engineer".to_string(),
                location: "Remote".to_string(),
                workplace: "remote".to_string(),
                canonical_url: "https://example.com/jobs/1".to_string(),
                description: String::new(),
                compensation: String::new(),
                track_id: String::new(),
                match_score: 80,
                matched_reasons: Vec::new(),
                missing_requirements: Vec::new(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let first = commit_packet(&pool, "acct-jobs", &application.id).unwrap();
        let second = commit_packet(&pool, "acct-jobs", &application.id).unwrap();
        assert!(first.newly_metered);
        assert!(!second.newly_metered);
        assert_eq!(first.used_packets, second.used_packets);
    }

    #[test]
    fn restricted_sites_never_enter_background_auto_submit() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.auto_submit_threshold = 80;
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "linkedin_handoff".to_string(),
                external_id: String::new(),
                company: "Northstar".to_string(),
                title: "Staff Engineer".to_string(),
                location: "Remote".to_string(),
                workplace: "remote".to_string(),
                canonical_url: "https://linkedin.com/jobs/view/123".to_string(),
                description: "Distributed systems".to_string(),
                compensation: String::new(),
                track_id: String::new(),
                match_score: 96,
                matched_reasons: Vec::new(),
                missing_requirements: Vec::new(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "auto_submit").unwrap();
        assert_eq!(application.state, "awaiting_review");
    }

    #[test]
    fn career_profile_payload_is_encrypted_at_rest() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        profile.street_address = "123 Example Street".to_string();
        save_profile(&pool, "acct-jobs", &profile).unwrap();

        let stored: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
                params!["acct-jobs"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored.starts_with(ENCRYPTED_PAYLOAD_PREFIX));
        assert!(!stored.contains("Example Street"));

        let restored = get_profile(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert_eq!(restored.street_address, profile.street_address);
    }

    #[test]
    fn application_updates_enforce_state_machine_and_submission_mode() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "pasted_link".to_string(),
                external_id: String::new(),
                company: "Acme".to_string(),
                title: "Engineer".to_string(),
                location: "Remote".to_string(),
                workplace: "remote".to_string(),
                canonical_url: "https://example.com/jobs/state-machine".to_string(),
                description: String::new(),
                compensation: String::new(),
                track_id: String::new(),
                match_score: 84,
                matched_reasons: Vec::new(),
                missing_requirements: Vec::new(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        let queued = update_application(
            &pool,
            "acct-jobs",
            &application.id,
            "queued",
            Some("auto_submit"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(queued.state, "queued");
        assert_eq!(queued.submission_mode, "auto_submit");

        let invalid_transition =
            update_application(&pool, "acct-jobs", &application.id, "submitted", None);
        assert!(invalid_transition.is_err());

        let invalid_mode = update_application(
            &pool,
            "acct-jobs",
            &application.id,
            "running",
            Some("surprise_me"),
        );
        assert!(invalid_mode.is_err());

        let unchanged = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.state, "queued");
        assert_eq!(unchanged.submission_mode, "auto_submit");
    }
}
