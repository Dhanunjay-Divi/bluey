//! Wire types matching `bluey-server`'s API.
//!
//! These shapes mirror `server/src/api/*::*Request/*Response` in the
//! server crate. Kept in this client crate (instead of a shared types
//! crate) for now to avoid a cross-workspace dependency; if drift
//! becomes a problem we'll factor out `bluey-api-types`.

use serde::{Deserialize, Serialize};

pub use cue_core::AnswerContext;

/// Current provenance-bearing answer-context wire schema understood by Bluey.
pub const ANSWER_CONTEXT_SCHEMA_VERSION_V1: u16 = 1;

// ─── Auth ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub account: AuthAccountSummary,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthAccountSummary {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceStartResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceStartRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceStatusRequest {
    pub device_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceStatusResponse {
    pub active: bool,
}

// ─── Account ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct AccountMe {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
    #[serde(default)]
    pub billing_provider: Option<String>,
    #[serde(default)]
    pub auto_topup_available: Option<bool>,
    #[serde(default)]
    pub auto_topup_unavailable_reason: Option<String>,
    #[serde(default)]
    pub saved_payment_method_label: Option<String>,
    #[serde(default)]
    pub square_environment: Option<String>,
    #[serde(default)]
    pub billing_restricted: Option<bool>,
    #[serde(default)]
    pub billing_restriction_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageWindow {
    pub period_days: i64,
    pub total_cues: i64,
    pub total_cents_spent: i64,
    pub mix: Vec<MixEntry>,
    pub tier_label: String,
    pub projected_days_remaining: f64,
    #[serde(default)]
    pub projection_label: String,
    #[serde(default)]
    pub projection_quality: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MixEntry {
    pub task_type: String,
    pub count: i64,
    pub cost_cents: i64,
    pub percent: f64,
}

// ─── Router (managed Auto Router endpoint) ──────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CompleteRequest {
    /// Idempotency key. REQUIRED. Codex Stage 4 S4.1: a retry after a
    /// network timeout would otherwise be charged twice. Caller mints
    /// a fresh UUID per logical request and reuses it on retry.
    pub request_id: String,
    pub system: String,
    pub user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget_tokens: Option<u32>,
    pub lane: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_input_tokens: Option<i64>,
    /// Data URLs for user-approved screenshot/screen-analysis context.
    ///
    /// Kept as strings here because the desktop/daemon already produces
    /// provider-compatible data URLs. The server validates MIME, count, and
    /// size before routing these to a vision lane.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_data_urls: Vec<String>,
    /// Explicit capability gate for provenance-bearing answer context. New
    /// clients send v1 even when `context` is empty; legacy clients omit it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_schema_version: Option<u16>,
    /// Typed evidence supplied by the daemon. Keeping source kind/title out of
    /// concatenated prompt text prevents an attached document from forging a
    /// transcript, resume, or user-confirmed story boundary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<AnswerContext>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompleteResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
    pub trial_seconds_remaining: i64,
    #[serde(default)]
    pub artifact_type: Option<String>,
    #[serde(default)]
    pub artifact_body: Option<String>,
    #[serde(default)]
    pub cost_label: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub sources: Vec<CompleteSource>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompleteSource {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub snippet: Option<String>,
    #[serde(default)]
    pub source_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedRequest {
    pub request_id: String,
    pub input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedBatchRequest {
    pub request_id: String,
    pub inputs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbedResponse {
    pub vector: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
    #[serde(default)]
    pub trial_seconds_remaining: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbedBatchResponse {
    pub vectors: Vec<Vec<f32>>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
    #[serde(default)]
    pub trial_seconds_remaining: i64,
}

// ─── Cloud Sync / RAG ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSessionRecord {
    pub session_id: String,
    pub title: String,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub last_active_at_ms: Option<i64>,
    #[serde(default)]
    pub answer_style: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTranscriptSegment {
    pub segment_id: String,
    pub session_id: String,
    pub speaker: String,
    pub source: String,
    pub text: String,
    #[serde(default)]
    pub start_ms: Option<i64>,
    #[serde(default)]
    pub end_ms: Option<i64>,
    pub ts_ms: i64,
    pub is_final: bool,
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCueResponseRecord {
    pub response_id: String,
    pub session_id: String,
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub source_text: Option<String>,
    pub ts_ms: i64,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub lane: Option<String>,
    #[serde(default)]
    pub task_type: Option<String>,
    #[serde(default)]
    pub cost_cents: Option<i64>,
    #[serde(default)]
    pub balance_cents_after: Option<i64>,
    #[serde(default)]
    pub cost_label: Option<String>,
    #[serde(default)]
    pub artifact_type: Option<String>,
    #[serde(default)]
    pub artifact_body: Option<String>,
    #[serde(default)]
    pub artifact_confidence: Option<f32>,
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncContextArtifactRecord {
    pub artifact_id: String,
    pub session_id: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub source_uri: Option<String>,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub text_preview: Option<String>,
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRagChunkRecord {
    pub chunk_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub source_kind: String,
    pub source_id: String,
    pub chunk_index: i64,
    pub text: String,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
    #[serde(default)]
    pub embedding_model: Option<String>,
    #[serde(default)]
    pub token_count: Option<i64>,
    #[serde(default)]
    pub content_hash: Option<String>,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncBatchRequest {
    #[serde(default)]
    pub sessions: Vec<SyncSessionRecord>,
    #[serde(default)]
    pub transcript_segments: Vec<SyncTranscriptSegment>,
    #[serde(default)]
    pub cue_responses: Vec<SyncCueResponseRecord>,
    #[serde(default)]
    pub context_artifacts: Vec<SyncContextArtifactRecord>,
    #[serde(default)]
    pub rag_chunks: Vec<SyncRagChunkRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCounts {
    pub sessions: usize,
    pub transcript_segments: usize,
    pub cue_responses: usize,
    pub context_artifacts: usize,
    pub rag_chunks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBatchResponse {
    pub accepted: SyncCounts,
    pub server_time_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactObjectResponse {
    pub artifact_id: String,
    #[serde(default)]
    pub session_id: String,
    pub object_key: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub content_type: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAuditBundleResponse {
    pub session_id: String,
    pub bundle_id: String,
    pub object_key: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub content_type: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSessionSummary {
    pub session_id: String,
    pub title: String,
    pub status: String,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub last_active_at_ms: Option<i64>,
    #[serde(default)]
    pub answer_style: Option<String>,
    pub transcript_count: i64,
    pub response_count: i64,
    pub context_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudDeletedSession {
    pub session_id: String,
    pub deleted_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListResponse {
    pub sessions: Vec<CloudSessionSummary>,
    #[serde(default)]
    pub deleted_sessions: Vec<CloudDeletedSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSessionBundle {
    pub session: SyncSessionRecord,
    pub transcript_segments: Vec<SyncTranscriptSegment>,
    pub cue_responses: Vec<SyncCueResponseRecord>,
    pub context_artifacts: Vec<SyncContextArtifactRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RagQueryRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagMatch {
    pub chunk_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub source_kind: String,
    pub source_id: String,
    pub chunk_index: i64,
    pub text: String,
    pub score: f32,
    #[serde(default)]
    pub embedding_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagQueryResponse {
    pub matches: Vec<RagMatch>,
}

// ─── STT authorization ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SttSessionRequest {
    pub session_id: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttSessionResponse {
    pub mode: String,
    pub provider: String,
    pub model: String,
    pub session_token: String,
    pub expires_at_ms: i64,
    pub max_seconds: i64,
    #[serde(default)]
    pub websocket_url: Option<String>,
    #[serde(default)]
    pub provider_token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SttSessionCancelRequest {
    pub session_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SttSessionCancelResponse {
    pub released: bool,
}

// ─── Usage ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct UsageEvent {
    pub request_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub latency_ms: i64,
    pub cost_cents_to_bluey: i64,
    pub cost_cents_to_customer: i64,
    pub was_speculative: bool,
    pub was_fallback: bool,
}

// ─── 402 reason payload ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InsufficientBalanceBody {
    pub balance_cents: i64,
    #[serde(default)]
    pub estimated_cost_cents: Option<i64>,
    #[serde(default)]
    pub needed_cents: Option<i64>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub reload_url: Option<String>,
}

/// Codex Stage 10: server-owned tier numbers from /pricing/tiers.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PricingTiers {
    pub reload_amount_cents: i64,
    pub minimum_cue_cents: i64,
    pub tiers: Vec<Tier>,
    pub markup_percent: MarkupPercent,
    pub snapshot_date: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Tier {
    pub name: String,
    pub label: String,
    pub cues_per_reload: i64,
    pub typical_duration_label: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MarkupPercent {
    pub easy: u32,
    pub medium: u32,
    pub deep: u32,
    pub vision: u32,
}
