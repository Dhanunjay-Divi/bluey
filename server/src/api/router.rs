//! Managed Auto Router endpoint — the monetization handle.
//!
//! POST /router/complete   LLM dispatch (proxies to upstream provider)
//! POST /router/embed      Embedding dispatch
//! POST /router/transcribe STT dispatch (Deepgram or local-fallback hint)
//!
//! Full implementation in subsequent commits. Today these are stubs
//! that return 501 so the route table is wired but no real spend can
//! happen yet.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use super::AppState;

#[derive(Deserialize)]
pub struct CompleteRequest {
    pub system: String,
    pub user: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub lane: String, // instant | balanced | deep | vision
    pub estimated_input_tokens: Option<i64>,
}

#[derive(Serialize)]
pub struct CompleteResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
    pub trial_seconds_remaining: i64,
}

pub async fn complete(
    State(_state): State<AppState>,
    Json(_req): Json<CompleteRequest>,
) -> Result<Json<CompleteResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[derive(Deserialize)]
pub struct EmbedRequest {
    pub text: String,
    pub model: Option<String>,
}

#[derive(Serialize)]
pub struct EmbedResponse {
    pub embedding: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
}

pub async fn embed(
    State(_state): State<AppState>,
    Json(_req): Json<EmbedRequest>,
) -> Result<Json<EmbedResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[derive(Deserialize)]
pub struct TranscribeRequest {
    pub audio_base64: String,
    pub language: Option<String>,
    pub format: Option<String>,
}

#[derive(Serialize)]
pub struct TranscribeResponse {
    pub text: String,
    pub provider: String,
    pub duration_ms: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
}

pub async fn transcribe(
    State(_state): State<AppState>,
    Json(_req): Json<TranscribeRequest>,
) -> Result<Json<TranscribeResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}
