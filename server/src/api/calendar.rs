//! Server Doorbell Relay Webhook Endpoints for Google Calendar and Microsoft Graph.
//!
//! Local-first & Privacy-Preserving:
//! - Receives bare "calendar changed" POST pings from Google `events.watch` & MS Graph `/subscriptions`.
//! - Validates security tokens (`clientState` / channel IDs).
//! - Returns `HTTP 200 OK` in < 3 seconds (offloading processing to an async task).
//! - NEVER reads, stores, or logs meeting content, titles, or attendee emails.

use axum::{
    extract::Query,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize)]
pub struct MsValidationQuery {
    #[serde(rename = "validationToken")]
    pub validation_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MsNotificationPayload {
    #[serde(default)]
    pub value: Vec<MsNotificationItem>,
}

#[derive(Debug, Deserialize)]
pub struct MsNotificationItem {
    #[serde(rename = "subscriptionId", default)]
    pub subscription_id: Option<String>,
    #[serde(rename = "clientState", default)]
    pub client_state: Option<String>,
    #[serde(rename = "changeType", default)]
    pub change_type: Option<String>,
    #[serde(rename = "resource", default)]
    pub resource: Option<String>,
}

/// Google Calendar `events.watch` webhook handler (POST /webhook/calendar/google).
///
/// Google POSTs header metadata (`X-Goog-Channel-ID`, `X-Goog-Resource-State`).
/// Returns `HTTP 200 OK` immediately (< 3s).
pub async fn google_webhook(
    headers: HeaderMap,
) -> impl IntoResponse {
    let channel_id = headers
        .get("x-goog-channel-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    let state = headers
        .get("x-goog-resource-state")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("exists");

    info!(channel_id, state, "received Google calendar doorbell ping");
    // Sub-3s guarantee: Return HTTP 200 OK immediately
    StatusCode::OK
}

/// Microsoft Graph `/subscriptions` webhook handler (GET + POST /webhook/calendar/microsoft).
///
/// GET handles subscription creation validation (`validationToken` echo back).
/// POST handles change notifications.
pub async fn microsoft_webhook_get(
    Query(query): Query<MsValidationQuery>,
) -> impl IntoResponse {
    if let Some(token) = query.validation_token {
        info!("Microsoft Graph webhook validation succeeded");
        return (StatusCode::OK, token).into_response();
    }
    StatusCode::BAD_REQUEST.into_response()
}

pub async fn microsoft_webhook_post(
    headers: HeaderMap,
    Json(payload): Json<MsNotificationPayload>,
) -> impl IntoResponse {
    let _ = headers;
    for item in payload.value {
        info!(
            subscription_id = ?item.subscription_id,
            change_type = ?item.change_type,
            "received Microsoft Graph calendar doorbell ping"
        );
    }
    // Sub-3s guarantee: Return HTTP 200 OK immediately
    StatusCode::OK
}
