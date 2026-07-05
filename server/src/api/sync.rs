//! Cloud sync and RAG endpoints.
//!
//! These endpoints are intentionally account-scoped and idempotent. The
//! desktop can write locally first, then retry sync batches until the server
//! acknowledges them.

use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::Response,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::sync::{
    self, CloudSessionBundle, CloudSessionSummary, RagMatch, SyncContextArtifactRecord, SyncCounts,
    SyncCueResponseRecord, SyncRagChunkRecord, SyncSessionRecord, SyncTranscriptSegment,
};
use crate::object_storage::{sha256_hex, ObjectStorage};

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Serialize)]
pub struct SyncBatchResponse {
    pub accepted: SyncCounts,
    pub server_time_ms: i64,
}

#[derive(Debug, Deserialize)]
pub struct SessionListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<CloudSessionSummary>,
}

#[derive(Debug, Deserialize)]
pub struct RagQueryRequest {
    pub query: String,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
    #[serde(default)]
    pub top_k: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RagQueryResponse {
    pub matches: Vec<RagMatch>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactObjectResponse {
    pub artifact_id: String,
    pub object_key: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub content_type: String,
    pub expires_at_ms: i64,
}

pub async fn batch(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<SyncBatchRequest>,
) -> Result<Json<SyncBatchResponse>, (StatusCode, String)> {
    ensure_sync_usage_allowed(&account, "sync_batch")?;
    validate_batch(&req)?;
    let accepted = sync::upsert_batch(
        &state.pool,
        &account.id,
        &req.sessions,
        &req.transcript_segments,
        &req.cue_responses,
        &req.context_artifacts,
        &req.rag_chunks,
    )
    .map_err(internal)?;
    Ok(Json(SyncBatchResponse {
        accepted,
        server_time_ms: now_ms(),
    }))
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Query(query): Query<SessionListQuery>,
) -> Result<Json<SessionListResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let sessions = sync::list_sessions(&state.pool, &account.id, limit).map_err(internal)?;
    Ok(Json(SessionListResponse { sessions }))
}

pub async fn get_session(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(session_id): Path<String>,
) -> Result<Json<CloudSessionBundle>, (StatusCode, String)> {
    let bundle = sync::load_session(&state.pool, &account.id, &session_id)
        .map_err(internal)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "session not found".to_string()))?;
    Ok(Json(bundle))
}

pub async fn rag_query(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<RagQueryRequest>,
) -> Result<Json<RagQueryResponse>, (StatusCode, String)> {
    ensure_sync_usage_allowed(&account, "rag_query")?;
    if req.query.trim().is_empty() && req.embedding.as_ref().is_none_or(Vec::is_empty) {
        return Err((
            StatusCode::BAD_REQUEST,
            "query or embedding is required".to_string(),
        ));
    }
    if let Some(embedding) = req.embedding.as_ref() {
        if embedding.len() > 4096 {
            return Err((
                StatusCode::BAD_REQUEST,
                "embedding has too many dimensions".to_string(),
            ));
        }
    }
    let matches = sync::query_rag(
        &state.pool,
        &account.id,
        &req.query,
        req.embedding.as_deref(),
        req.top_k.unwrap_or(8),
    )
    .map_err(internal)?;
    Ok(Json(RagQueryResponse { matches }))
}

pub async fn upload_artifact_object(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(artifact_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ArtifactObjectResponse>, (StatusCode, String)> {
    ensure_sync_usage_allowed(&account, "artifact_upload")?;
    validate_object_id(&artifact_id)?;
    let storage_config = state.config.object_storage.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "object sync is not configured".into(),
        )
    })?;
    if body.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "object body is empty".into()));
    }
    if body.len() > storage_config.max_object_bytes {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "object is too large".to_string(),
        ));
    }

    let storage = ObjectStorage::new(storage_config);
    let key = storage.artifact_key(&account.id, &artifact_id);
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("application/octet-stream")
        .to_string();
    let hash = sha256_hex(&body);
    storage
        .put(&key, body.clone(), &content_type)
        .await
        .map_err(internal)?;

    Ok(Json(ArtifactObjectResponse {
        artifact_id,
        object_key: key,
        size_bytes: body.len() as u64,
        sha256: hash,
        content_type,
        expires_at_ms: now_ms() + storage.retention_days().saturating_mul(86_400_000),
    }))
}

pub async fn download_artifact_object(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(artifact_id): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    ensure_sync_usage_allowed(&account, "artifact_download")?;
    validate_object_id(&artifact_id)?;
    let storage_config = state.config.object_storage.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "object sync is not configured".into(),
        )
    })?;
    let record = sync::load_context_artifact(&state.pool, &account.id, &artifact_id)
        .map_err(internal)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "artifact not found".to_string()))?;
    let key = record
        .metadata
        .get("object_key")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "artifact object not found".to_string(),
            )
        })?;
    let storage = ObjectStorage::new(storage_config);
    if !storage.key_belongs_to_account(key, &account.id) {
        tracing::warn!(
            account_id = %account.id,
            artifact_id = %artifact_id,
            object_key = %key,
            "rejecting cross-account artifact object key"
        );
        return Err((
            StatusCode::FORBIDDEN,
            "artifact object not available".into(),
        ));
    }
    if record
        .metadata
        .get("object_expires_at_ms")
        .and_then(|value| value.as_i64())
        .is_some_and(|expires_at_ms| expires_at_ms <= now_ms())
    {
        if let Err(error) = storage.delete(key).await {
            tracing::warn!(
                error = %error,
                artifact_id = %artifact_id,
                "lazy object delete failed"
            );
        }
        return Err((StatusCode::GONE, "artifact object expired".into()));
    }

    let object = storage.get(key).await.map_err(internal)?;
    let mut response = Response::new(Body::from(object.bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&object.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=60"),
    );
    Ok(response)
}

fn validate_batch(req: &SyncBatchRequest) -> Result<(), (StatusCode, String)> {
    let total = req.sessions.len()
        + req.transcript_segments.len()
        + req.cue_responses.len()
        + req.context_artifacts.len()
        + req.rag_chunks.len();
    if total == 0 {
        return Err((StatusCode::BAD_REQUEST, "empty sync batch".to_string()));
    }
    if total > 500 {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "sync batch cannot exceed 500 records".to_string(),
        ));
    }
    for segment in &req.transcript_segments {
        if segment.text.len() > 16_000 {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                "transcript segment text too large".to_string(),
            ));
        }
    }
    for response in &req.cue_responses {
        if response.text.len() > 128_000 {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                "response text too large".to_string(),
            ));
        }
    }
    for chunk in &req.rag_chunks {
        if chunk.text.len() > 16_000 {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                "rag chunk text too large".to_string(),
            ));
        }
        if chunk.embedding.as_ref().is_some_and(|v| v.len() > 4096) {
            return Err((
                StatusCode::BAD_REQUEST,
                "rag chunk embedding has too many dimensions".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_object_id(artifact_id: &str) -> Result<(), (StatusCode, String)> {
    if uuid::Uuid::parse_str(artifact_id).is_ok() {
        return Ok(());
    }
    Err((StatusCode::BAD_REQUEST, "invalid artifact id".to_string()))
}

fn internal(e: anyhow::Error) -> (StatusCode, String) {
    let error_chain = e
        .chain()
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    tracing::warn!(error = %e, error_chain = %error_chain, "sync endpoint failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "sync failed".to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn ensure_sync_usage_allowed(
    account: &crate::db::accounts::Account,
    surface: &str,
) -> Result<(), (StatusCode, String)> {
    if !account.billing_restricted {
        return Ok(());
    }
    tracing::warn!(
        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
        surface,
        reason = account.billing_restriction_reason.as_deref().unwrap_or("billing_restricted"),
        "billing-restricted account blocked from cloud sync/RAG usage"
    );
    Err((
        StatusCode::FORBIDDEN,
        "Account usage is paused while billing is under review.".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_account(billing_restricted: bool) -> crate::db::accounts::Account {
        crate::db::accounts::Account {
            id: "acct-sync-test".to_string(),
            email: "sync@example.com".to_string(),
            email_verified_at: None,
            balance_cents: 0,
            trial_seconds_remaining: 0,
            is_temporary: false,
            temporary_expires_at: None,
            auto_topup_enabled: false,
            auto_topup_threshold_cents: 500,
            auto_topup_amount_cents: 1500,
            is_admin: false,
            stripe_customer_id: None,
            stripe_payment_method_id: None,
            square_customer_id: None,
            square_card_id: None,
            square_card_brand: None,
            square_card_last4: None,
            billing_restricted,
            billing_restriction_reason: billing_restricted.then(|| "refund.created".to_string()),
            billing_restricted_at: billing_restricted.then(|| "2026-06-26T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn validate_rejects_empty_or_huge_batches() {
        let empty = SyncBatchRequest {
            sessions: vec![],
            transcript_segments: vec![],
            cue_responses: vec![],
            context_artifacts: vec![],
            rag_chunks: vec![],
        };
        assert!(validate_batch(&empty).is_err());

        let huge = SyncBatchRequest {
            sessions: (0..501)
                .map(|i| SyncSessionRecord {
                    session_id: format!("s{i}"),
                    title: "s".into(),
                    status: "active".into(),
                    created_at_ms: 0,
                    updated_at_ms: 0,
                    last_active_at_ms: None,
                    answer_style: None,
                    metadata: serde_json::json!({}),
                    deleted_at_ms: None,
                })
                .collect(),
            transcript_segments: vec![],
            cue_responses: vec![],
            context_artifacts: vec![],
            rag_chunks: vec![],
        };
        assert!(validate_batch(&huge).is_err());
    }

    #[test]
    fn billing_restricted_account_cannot_use_sync_compute_surfaces() {
        assert!(ensure_sync_usage_allowed(&test_account(false), "rag_query").is_ok());
        let err = ensure_sync_usage_allowed(&test_account(true), "rag_query").unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("billing is under review"));
    }
}
