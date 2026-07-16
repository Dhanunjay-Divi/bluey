//! Cloud sync and RAG endpoints.
//!
//! These endpoints are intentionally account-scoped and idempotent. The
//! desktop can write locally first, then retry sync batches until the server
//! acknowledges them.

use std::collections::HashSet;

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
use crate::db::object_uploads::{
    self, NewObjectUpload, ObjectKind, ObjectUpload, StorageScope, UploadControlError,
};
use crate::db::sync::{
    self, CloudDeletedSession, CloudSessionBundle, CloudSessionSummary, RagMatch,
    SyncContextArtifactRecord, SyncCounts, SyncCueResponseRecord, SyncRagChunkRecord,
    SyncSessionRecord, SyncTranscriptSegment,
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
    pub deleted_sessions: Vec<CloudDeletedSession>,
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

#[derive(Debug, Serialize)]
pub struct SessionAuditBundleResponse {
    pub session_id: String,
    pub bundle_id: String,
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
    .map_err(sync_write_error)?;
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
    let deleted_sessions =
        sync::list_deleted_sessions(&state.pool, &account.id, limit).map_err(internal)?;
    Ok(Json(SessionListResponse {
        sessions,
        deleted_sessions,
    }))
}

pub async fn get_session(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(session_id): Path<String>,
) -> Result<Json<CloudSessionBundle>, (StatusCode, String)> {
    validate_session_id(&session_id)?;
    let bundle = sync::load_session(&state.pool, &account.id, &session_id)
        .map_err(internal)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "session not found".to_string()))?;
    Ok(Json(bundle))
}

pub async fn delete_session(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(session_id): Path<String>,
) -> Result<Json<SyncBatchResponse>, (StatusCode, String)> {
    validate_session_id(&session_id)?;
    sync::tombstone_session(&state.pool, &account.id, &session_id).map_err(sync_write_error)?;
    if let Some(config) = state.config.object_storage.clone() {
        drain_cleanup_jobs(
            &state,
            &account.id,
            StorageScope::Artifact,
            &ObjectStorage::new(config),
        )
        .await;
    }
    if let Some(config) = state
        .config
        .log_storage
        .clone()
        .or_else(|| state.config.object_storage.clone())
    {
        drain_cleanup_jobs(
            &state,
            &account.id,
            StorageScope::Audit,
            &ObjectStorage::new(config),
        )
        .await;
    }
    Ok(Json(SyncBatchResponse {
        accepted: SyncCounts {
            sessions: 1,
            transcript_segments: 0,
            cue_responses: 0,
            context_artifacts: 0,
            rag_chunks: 0,
        },
        server_time_ms: now_ms(),
    }))
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
    ensure_upload_allowed(&account, "artifact_upload")?;
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
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("application/octet-stream")
        .to_string();
    let hash = sha256_hex(&body);
    let key = storage.artifact_upload_key(&account.id, &artifact_id, &hash);
    drain_cleanup_jobs(&state, &account.id, StorageScope::Artifact, &storage).await;
    let created_at_ms = now_ms();
    let upload = reserve_put_and_finalize(
        &state,
        &storage,
        NewObjectUpload {
            account_id: account.id.clone(),
            object_kind: ObjectKind::Artifact,
            logical_id: artifact_id.clone(),
            session_id: None,
            storage_scope: StorageScope::Artifact,
            object_key: key,
            size_bytes: body.len() as i64,
            sha256: hash,
            content_type,
            expires_at_ms: created_at_ms
                .saturating_add(storage.retention_days().saturating_mul(86_400_000)),
            metadata_json: serde_json::json!({}),
            now_ms: created_at_ms,
            limits: storage.upload_limits(),
        },
        body,
    )
    .await?;

    Ok(Json(ArtifactObjectResponse {
        artifact_id,
        object_key: upload.object_key,
        size_bytes: upload.size_bytes as u64,
        sha256: upload.sha256,
        content_type: upload.content_type,
        expires_at_ms: upload.expires_at_ms,
    }))
}

pub async fn upload_session_audit_bundle(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path((session_id, bundle_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<SessionAuditBundleResponse>, (StatusCode, String)> {
    ensure_upload_allowed(&account, "session_audit_upload")?;
    validate_session_id(&session_id)?;
    validate_audit_bundle_id(&bundle_id)?;
    let storage_config = state
        .config
        .log_storage
        .clone()
        .or_else(|| state.config.object_storage.clone())
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "session audit storage is not configured".into(),
            )
        })?;
    if body.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "audit bundle is empty".into()));
    }
    if body.len() > storage_config.max_object_bytes {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "audit bundle is too large".to_string(),
        ));
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("application/json")
        .to_string();
    let hash = sha256_hex(&body);
    let storage = ObjectStorage::new(storage_config);
    let created_at_ms = now_ms();
    let key = storage.audit_upload_key(&account.id, &session_id, &bundle_id, &hash);
    drain_cleanup_jobs(&state, &account.id, StorageScope::Audit, &storage).await;
    let upload = reserve_put_and_finalize(
        &state,
        &storage,
        NewObjectUpload {
            account_id: account.id.clone(),
            object_kind: ObjectKind::SessionAudit,
            logical_id: format!("{session_id}/{bundle_id}"),
            session_id: Some(session_id.clone()),
            storage_scope: StorageScope::Audit,
            object_key: key,
            size_bytes: body.len() as i64,
            sha256: hash,
            content_type,
            expires_at_ms: created_at_ms
                .saturating_add(storage.retention_days().saturating_mul(86_400_000)),
            metadata_json: serde_json::json!({
                "bundle_id": bundle_id,
                "content_type": headers
                    .get(header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("application/json"),
                "schema_version": headers
                    .get("x-bluey-audit-schema-version")
                    .and_then(|value| value.to_str().ok()),
            }),
            now_ms: created_at_ms,
            limits: storage.upload_limits(),
        },
        body,
    )
    .await?;

    Ok(Json(SessionAuditBundleResponse {
        session_id,
        bundle_id,
        object_key: upload.object_key,
        size_bytes: upload.size_bytes as u64,
        sha256: upload.sha256,
        content_type: upload.content_type,
        expires_at_ms: upload.expires_at_ms,
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
    let storage = ObjectStorage::new(storage_config);
    let indexed = object_uploads::artifact_upload(&state.pool, &account.id, &artifact_id)
        .map_err(internal)?;
    let (key, expires_at_ms, durable_index) = if let Some(upload) = indexed {
        if upload.state != "ready" {
            if upload.state == "delete_pending" {
                drain_cleanup_jobs(&state, &account.id, StorageScope::Artifact, &storage).await;
                return Err((StatusCode::GONE, "artifact object deleted".to_string()));
            }
            return Err((
                StatusCode::NOT_FOUND,
                "artifact object is not ready".to_string(),
            ));
        }
        (upload.object_key, Some(upload.expires_at_ms), true)
    } else {
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
            })?
            .to_string();
        let expires_at_ms = record
            .metadata
            .get("object_expires_at_ms")
            .and_then(|value| value.as_i64());
        (key, expires_at_ms, false)
    };
    if !storage.key_belongs_to_account(&key, &account.id) {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            artifact_id = %artifact_id,
            object_key_hash = %sha256_hex(key.as_bytes()),
            "rejecting cross-account artifact object key"
        );
        return Err((
            StatusCode::FORBIDDEN,
            "artifact object not available".into(),
        ));
    }
    if expires_at_ms.is_some_and(|expires_at_ms| expires_at_ms <= now_ms()) {
        if durable_index {
            drain_cleanup_jobs(&state, &account.id, StorageScope::Artifact, &storage).await;
        } else if let Err(error) = storage.delete(&key).await {
            tracing::warn!(error = %error, artifact_id = %artifact_id, "legacy lazy object delete failed");
        }
        return Err((StatusCode::GONE, "artifact object expired".into()));
    }

    let object = storage.get(&key).await.map_err(internal)?;
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

    let mut session_ids = HashSet::new();
    for session in &req.sessions {
        validate_session_id(&session.session_id)?;
        ensure_unique_sync_id(&mut session_ids, &session.session_id, "session")?;
    }

    let mut segment_ids = HashSet::new();
    for segment in &req.transcript_segments {
        validate_session_id(&segment.session_id)?;
        validate_sync_record_id(&segment.segment_id, "transcript segment", false)?;
        ensure_unique_sync_id(&mut segment_ids, &segment.segment_id, "transcript segment")?;
        if segment.text.len() > 16_000 {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                "transcript segment text too large".to_string(),
            ));
        }
    }

    let mut response_ids = HashSet::new();
    let mut canvas_ids = HashSet::new();
    for response in &req.cue_responses {
        validate_session_id(&response.session_id)?;
        validate_sync_record_id(&response.response_id, "response", false)?;
        ensure_unique_sync_id(&mut response_ids, &response.response_id, "response")?;
        validate_response_artifact_ids(response, &mut canvas_ids)?;
        if response.text.len() > 128_000 {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                "response text too large".to_string(),
            ));
        }
    }

    let mut artifact_ids = HashSet::new();
    for artifact in &req.context_artifacts {
        validate_session_id(&artifact.session_id)?;
        validate_sync_record_id(&artifact.artifact_id, "context artifact", false)?;
        ensure_unique_sync_id(&mut artifact_ids, &artifact.artifact_id, "context artifact")?;
    }

    let mut chunk_ids = HashSet::new();
    for chunk in &req.rag_chunks {
        if let Some(session_id) = chunk.session_id.as_deref() {
            validate_session_id(session_id)?;
        }
        validate_sync_record_id(&chunk.chunk_id, "RAG chunk", true)?;
        validate_sync_record_id(&chunk.source_id, "RAG source", true)?;
        ensure_unique_sync_id(&mut chunk_ids, &chunk.chunk_id, "RAG chunk")?;
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

fn validate_response_artifact_ids(
    response: &SyncCueResponseRecord,
    canvas_ids: &mut HashSet<String>,
) -> Result<(), (StatusCode, String)> {
    if let Some(value) = response.metadata.get("attachment_ids") {
        let values = if value.is_null() {
            &[][..]
        } else if let Some(values) = value.as_array() {
            values.as_slice()
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                "response attachment_ids must be an array".to_string(),
            ));
        };
        let mut response_attachment_ids = HashSet::new();
        for value in values {
            let Some(artifact_id) = value.as_str() else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "response attachment id must be a string".to_string(),
                ));
            };
            validate_sync_record_id(artifact_id, "attachment", false)?;
            ensure_unique_sync_id(&mut response_attachment_ids, artifact_id, "attachment")?;
        }
    }

    if let Some(value) = response.metadata.get("canvas_artifact_id") {
        let Some(canvas_id) = value.as_str() else {
            if value.is_null() {
                return Ok(());
            }
            return Err((
                StatusCode::BAD_REQUEST,
                "canvas_artifact_id must be a string".to_string(),
            ));
        };
        validate_sync_record_id(canvas_id, "canvas artifact", false)?;
        ensure_unique_sync_id(canvas_ids, canvas_id, "canvas artifact")?;
    }
    Ok(())
}

fn validate_sync_record_id(
    value: &str,
    label: &str,
    allow_colon: bool,
) -> Result<(), (StatusCode, String)> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 192
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_')
                || (allow_colon && byte == b':')
        });
    if valid {
        Ok(())
    } else {
        Err((StatusCode::BAD_REQUEST, format!("invalid {label} id")))
    }
}

fn ensure_unique_sync_id(
    seen: &mut HashSet<String>,
    value: &str,
    label: &str,
) -> Result<(), (StatusCode, String)> {
    if seen.insert(value.to_string()) {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            format!("duplicate {label} id in sync batch"),
        ))
    }
}

fn validate_session_id(session_id: &str) -> Result<(), (StatusCode, String)> {
    let session_id = session_id.trim();
    let valid = !session_id.is_empty()
        && session_id.len() <= 128
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err((StatusCode::BAD_REQUEST, "invalid session id".to_string()))
    }
}

fn validate_object_id(artifact_id: &str) -> Result<(), (StatusCode, String)> {
    if uuid::Uuid::parse_str(artifact_id).is_ok() {
        return Ok(());
    }
    Err((StatusCode::BAD_REQUEST, "invalid artifact id".to_string()))
}

fn validate_audit_bundle_id(bundle_id: &str) -> Result<(), (StatusCode, String)> {
    let valid = !bundle_id.is_empty()
        && bundle_id.len() <= 96
        && bundle_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "invalid audit bundle id".to_string(),
        ))
    }
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

fn sync_write_error(e: anyhow::Error) -> (StatusCode, String) {
    if let Some(conflict) = e.downcast_ref::<sync::SyncWriteError>() {
        tracing::warn!(error = %conflict, "sync write rejected");
        return (StatusCode::CONFLICT, conflict.to_string());
    }
    internal(e)
}

async fn reserve_put_and_finalize(
    state: &AppState,
    storage: &ObjectStorage,
    input: NewObjectUpload,
    body: Bytes,
) -> Result<ObjectUpload, (StatusCode, String)> {
    let reservation = object_uploads::reserve_upload(&state.pool, &input).map_err(upload_error)?;
    if !reservation.needs_put {
        return Ok(reservation.upload);
    }

    if let Err(error) = storage
        .put(
            &reservation.upload.object_key,
            body,
            &reservation.upload.content_type,
        )
        .await
    {
        if let Err(index_error) = object_uploads::record_put_failure(
            &state.pool,
            &reservation.upload.id,
            &error.to_string(),
            now_ms(),
        ) {
            tracing::error!(
                error = %index_error,
                upload_id_hash = %sha256_hex(reservation.upload.id.as_bytes()),
                "failed to persist object PUT retry state"
            );
        }
        tracing::warn!(
            error = %error,
            upload_id_hash = %sha256_hex(reservation.upload.id.as_bytes()),
            "object PUT failed with durable metadata retained for retry"
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            "object storage write failed".to_string(),
        ));
    }

    match object_uploads::mark_upload_ready(&state.pool, &reservation.upload.id, now_ms()) {
        Ok(upload) => Ok(upload),
        Err(error) => {
            let lifecycle_conflict =
                error
                    .downcast_ref::<UploadControlError>()
                    .is_some_and(|policy| {
                        matches!(
                            policy,
                            UploadControlError::UploadGone | UploadControlError::UploadNotFound
                        )
                    });
            if lifecycle_conflict {
                match storage.delete(&reservation.upload.object_key).await {
                    Ok(()) => {
                        let _ = object_uploads::mark_cleanup_succeeded(
                            &state.pool,
                            &reservation.upload.id,
                            now_ms(),
                        );
                    }
                    Err(cleanup_error) => {
                        let _ = object_uploads::mark_cleanup_failed(
                            &state.pool,
                            &reservation.upload.id,
                            &cleanup_error.to_string(),
                            now_ms(),
                        );
                        tracing::error!(
                            error = %cleanup_error,
                            upload_id_hash = %sha256_hex(reservation.upload.id.as_bytes()),
                            "object PUT completed after lifecycle deletion; cleanup will retry"
                        );
                    }
                }
            }
            Err(upload_error(error))
        }
    }
}

async fn drain_cleanup_jobs(
    state: &AppState,
    account_id: &str,
    storage_scope: StorageScope,
    storage: &ObjectStorage,
) {
    let now = now_ms();
    let stale_before_ms = now.saturating_sub(24 * 60 * 60 * 1000);
    let jobs = match object_uploads::claim_cleanup_jobs(
        &state.pool,
        account_id,
        storage_scope,
        now,
        stale_before_ms,
        5,
    ) {
        Ok(jobs) => jobs,
        Err(error) => {
            tracing::warn!(error = %error, "failed to claim object cleanup jobs");
            return;
        }
    };

    for job in jobs {
        let result = if storage.key_belongs_to_account(&job.object_key, account_id) {
            storage.delete(&job.object_key).await
        } else {
            Err(anyhow::anyhow!(
                "object cleanup key is outside account scope"
            ))
        };
        match result {
            Ok(()) => {
                if let Err(error) =
                    object_uploads::mark_cleanup_succeeded(&state.pool, &job.upload_id, now_ms())
                {
                    tracing::error!(error = %error, "failed to complete object cleanup metadata");
                }
            }
            Err(error) => {
                if let Err(index_error) = object_uploads::mark_cleanup_failed(
                    &state.pool,
                    &job.upload_id,
                    &error.to_string(),
                    now_ms(),
                ) {
                    tracing::error!(error = %index_error, "failed to persist object cleanup retry");
                }
                tracing::warn!(error = %error, "object cleanup will be retried");
            }
        }
    }
}

fn upload_error(error: anyhow::Error) -> (StatusCode, String) {
    let Some(policy) = error.downcast_ref::<UploadControlError>() else {
        return internal(error);
    };
    match policy {
        UploadControlError::ObjectTooLarge => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "object is too large".to_string(),
        ),
        UploadControlError::AccountBytesQuotaExceeded => (
            StatusCode::INSUFFICIENT_STORAGE,
            "account object storage quota exceeded".to_string(),
        ),
        UploadControlError::AccountObjectQuotaExceeded => (
            StatusCode::INSUFFICIENT_STORAGE,
            "account object count quota exceeded".to_string(),
        ),
        UploadControlError::DailyQuotaExceeded => (
            StatusCode::TOO_MANY_REQUESTS,
            "daily object upload quota exceeded".to_string(),
        ),
        UploadControlError::IdempotencyConflict => (
            StatusCode::CONFLICT,
            "object id is already bound to different content".to_string(),
        ),
        UploadControlError::UploadInProgress => (
            StatusCode::CONFLICT,
            "object upload is already in progress".to_string(),
        ),
        UploadControlError::UploadGone => (
            StatusCode::GONE,
            "object upload has been deleted".to_string(),
        ),
        UploadControlError::SessionNotOwned | UploadControlError::UploadNotFound => (
            StatusCode::NOT_FOUND,
            "session or object not found".to_string(),
        ),
        UploadControlError::InvalidMetadata(_) => (
            StatusCode::BAD_REQUEST,
            "invalid object metadata".to_string(),
        ),
    }
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

fn ensure_upload_allowed(
    account: &crate::db::accounts::Account,
    surface: &str,
) -> Result<(), (StatusCode, String)> {
    ensure_sync_usage_allowed(account, surface)?;
    if account.is_temporary_expired() {
        return Err((
            StatusCode::FORBIDDEN,
            "Temporary account access has expired.".to_string(),
        ));
    }
    if account.is_admin || account.balance_cents > 0 || account.trial_seconds_remaining > 0 {
        return Ok(());
    }
    tracing::warn!(
        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
        surface,
        "account without active cloud-upload entitlement was blocked"
    );
    Err((
        StatusCode::PAYMENT_REQUIRED,
        "Cloud uploads require an active trial or credit balance.".to_string(),
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

    #[test]
    fn every_upload_requires_live_billing_and_cloud_entitlement() {
        let mut account = test_account(false);
        let err = ensure_upload_allowed(&account, "artifact_upload").unwrap_err();
        assert_eq!(err.0, StatusCode::PAYMENT_REQUIRED);

        account.trial_seconds_remaining = 60;
        assert!(ensure_upload_allowed(&account, "artifact_upload").is_ok());

        account.billing_restricted = true;
        account.billing_restriction_reason = Some("charge.dispute.created".into());
        let err = ensure_upload_allowed(&account, "session_audit_upload").unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn session_ids_accept_uuid_and_safe_legacy_shapes() {
        assert!(validate_session_id("61b8c310-27de-4cc1-b598-c62bdcc07ba8").is_ok());
        assert!(validate_session_id("sess-cloud-1").is_ok());
        assert!(validate_session_id("local_session_42").is_ok());
    }

    #[test]
    fn session_ids_reject_path_or_control_characters() {
        assert!(validate_session_id("").is_err());
        assert!(validate_session_id("../other-account").is_err());
        assert!(validate_session_id("session/child").is_err());
        assert!(validate_session_id("session\nchild").is_err());
    }

    #[test]
    fn sync_batch_rejects_duplicate_child_ids_before_order_can_choose_a_winner() {
        let response = SyncCueResponseRecord {
            response_id: "stable-response".into(),
            session_id: "session-a".into(),
            kind: "answer".into(),
            text: "answer".into(),
            source_text: Some("question".into()),
            ts_ms: 10,
            provider: None,
            model: None,
            lane: None,
            task_type: None,
            cost_cents: None,
            balance_cents_after: None,
            cost_label: None,
            artifact_type: None,
            artifact_body: None,
            artifact_confidence: None,
            metadata: serde_json::json!({}),
        };
        let mut conflicting = response.clone();
        conflicting.session_id = "session-b".into();
        let request = SyncBatchRequest {
            sessions: vec![],
            transcript_segments: vec![],
            cue_responses: vec![response, conflicting],
            context_artifacts: vec![],
            rag_chunks: vec![],
        };

        let error = validate_batch(&request).unwrap_err();
        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert!(error.1.contains("duplicate response id"));
    }

    #[test]
    fn response_attachment_and_canvas_ids_must_be_stable_safe_identifiers() {
        let mut canvas_ids = HashSet::new();
        let mut response = SyncCueResponseRecord {
            response_id: "stable-response".into(),
            session_id: "stable-session".into(),
            kind: "answer".into(),
            text: "answer".into(),
            source_text: Some("question".into()),
            ts_ms: 10,
            provider: None,
            model: None,
            lane: None,
            task_type: None,
            cost_cents: None,
            balance_cents_after: None,
            cost_label: None,
            artifact_type: Some("code".into()),
            artifact_body: Some("fn main() {}".into()),
            artifact_confidence: Some(0.9),
            metadata: serde_json::json!({
                "attachment_ids": ["stable-attachment"],
                "canvas_artifact_id": "stable-canvas"
            }),
        };
        validate_response_artifact_ids(&response, &mut canvas_ids).unwrap();

        response.metadata["attachment_ids"] = serde_json::json!(["../other-session"]);
        assert!(validate_response_artifact_ids(&response, &mut HashSet::new()).is_err());

        response.metadata = serde_json::json!({
            "attachment_ids": ["stable-attachment"],
            "canvas_artifact_id": "stable-canvas"
        });
        assert!(validate_response_artifact_ids(&response, &mut canvas_ids).is_err());
    }

    #[test]
    fn audit_bundle_ids_are_path_safe() {
        assert!(validate_audit_bundle_id("audit-ABC123-1780000000000").is_ok());
        assert!(validate_audit_bundle_id("audit_ABC123").is_ok());
        assert!(validate_audit_bundle_id("audit.123").is_err());
        assert!(validate_audit_bundle_id("../audit").is_err());
        assert!(validate_audit_bundle_id("audit/slash").is_err());
        assert!(validate_audit_bundle_id("").is_err());
    }
}
