//! Cloud sync and RAG endpoints.
//!
//! These endpoints are intentionally account-scoped and idempotent. The
//! desktop can write locally first, then retry sync batches until the server
//! acknowledges them.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::sync::{
    self, CloudSessionBundle, CloudSessionSummary, RagMatch, SyncContextArtifactRecord,
    SyncCounts, SyncCueResponseRecord, SyncRagChunkRecord, SyncSessionRecord,
    SyncTranscriptSegment,
};

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

pub async fn batch(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<SyncBatchRequest>,
) -> Result<Json<SyncBatchResponse>, (StatusCode, String)> {
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

fn internal(e: anyhow::Error) -> (StatusCode, String) {
    tracing::warn!(error = %e, "sync endpoint failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "sync failed".to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
