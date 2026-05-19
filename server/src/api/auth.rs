//! Auth endpoints. Stub implementations — full signup/login flow with
//! bcrypt + JWT issued in subsequent commits. For now, just enough to
//! prove the routing wires up.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use super::AppState;

#[derive(Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

pub async fn signup(
    State(_state): State<AppState>,
    Json(_req): Json<SignupRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    // TODO: bcrypt password, create account, issue tokens.
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(_state): State<AppState>,
    Json(_req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

pub async fn refresh(
    State(_state): State<AppState>,
    Json(_req): Json<RefreshRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

// Device flow (used by `bluey login` from the daemon)
#[derive(Serialize)]
pub struct DeviceStartResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

pub async fn device_start(
    State(_state): State<AppState>,
) -> Result<Json<DeviceStartResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[derive(Deserialize)]
pub struct DevicePollRequest {
    pub device_code: String,
}

pub async fn device_poll(
    State(_state): State<AppState>,
    Json(_req): Json<DevicePollRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[derive(Deserialize)]
pub struct DeviceApproveRequest {
    pub user_code: String,
}

pub async fn device_approve(
    State(_state): State<AppState>,
    Json(_req): Json<DeviceApproveRequest>,
) -> Result<StatusCode, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}
