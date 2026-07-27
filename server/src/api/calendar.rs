//! Authenticated calendar-change webhook ingress.
//!
//! These public endpoints receive only provider "something changed" doorbells;
//! OAuth code exchange remains inside the native app's PKCE loopback flow. The
//! current daemon uses a bounded polling safety net, so accepting a doorbell
//! does not expose or store event titles, attendee data, or OAuth credentials.

use axum::{
    body::Bytes,
    extract::Query,
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE},
        HeaderMap, HeaderName, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tracing::{info, warn};

const GOOGLE_WEBHOOK_TOKEN_ENV: &str = "BLUEY_GOOGLE_CALENDAR_WEBHOOK_TOKEN";
const GOOGLE_WEBHOOK_CHANNEL_ID_ENV: &str = "BLUEY_GOOGLE_CALENDAR_WEBHOOK_CHANNEL_ID";
const MICROSOFT_CLIENT_STATE_ENV: &str = "BLUEY_MICROSOFT_CALENDAR_CLIENT_STATE";
const MAX_GOOGLE_CHANNEL_ID_LEN: usize = 64;
const MAX_GOOGLE_CHANNEL_TOKEN_LEN: usize = 256;
const MAX_GOOGLE_RESOURCE_ID_LEN: usize = 1_024;
const MAX_MICROSOFT_CLIENT_STATE_LEN: usize = 128;
pub const MAX_MICROSOFT_NOTIFICATION_BODY_LEN: usize = 64 * 1024;
const MAX_MICROSOFT_NOTIFICATIONS: usize = 256;
const MAX_VALIDATION_TOKEN_LEN: usize = 2_048;
const MIN_WEBHOOK_CREDENTIAL_LEN: usize = 16;

#[derive(Default, Deserialize)]
pub struct MsValidationQuery {
    #[serde(rename = "validationToken")]
    pub validation_token: Option<String>,
}

#[derive(Deserialize)]
pub struct MsNotificationPayload {
    #[serde(default)]
    pub value: Vec<MsNotificationItem>,
}

#[derive(Deserialize)]
pub struct MsNotificationItem {
    #[serde(rename = "subscriptionId", default)]
    pub subscription_id: Option<String>,
    #[serde(rename = "clientState", default)]
    pub client_state: Option<String>,
    #[serde(rename = "changeType", default)]
    pub change_type: Option<String>,
}

fn configured_secret(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn valid_credential(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.chars().count() <= max_len
        && !value.chars().any(char::is_control)
}

fn valid_expected_credential(value: &str, max_len: usize) -> bool {
    let normalized = value.to_ascii_lowercase();
    valid_credential(value, max_len)
        && value.len() >= MIN_WEBHOOK_CREDENTIAL_LEN
        && !normalized.starts_with("replace-with-")
        && !normalized.starts_with("placeholder")
        && normalized != "changeme"
}

fn required_expected(expected: Option<&str>, max_len: usize) -> Result<&str, StatusCode> {
    expected
        .filter(|value| valid_expected_credential(value, max_len))
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

/// Compare fixed-length hashes so a length mismatch does not short-circuit the
/// secret comparison itself.
fn secrets_match(expected: &str, received: &str) -> bool {
    let expected_hash = Sha256::digest(expected.as_bytes());
    let received_hash = Sha256::digest(received.as_bytes());
    bool::from(expected_hash.as_slice().ct_eq(received_hash.as_slice()))
}

fn verify_shared_secret(
    expected: Option<&str>,
    received: Option<&str>,
    max_len: usize,
) -> Result<(), StatusCode> {
    let expected = required_expected(expected, max_len)?;
    let received = received
        .filter(|value| valid_credential(value, max_len))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if secrets_match(expected, received) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn content_type_is(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(expected))
}

fn validation_token_response(token: Option<String>) -> Response {
    let Some(token) = token else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if token.trim().is_empty()
        || token.len() > MAX_VALIDATION_TOKEN_LEN
        || token.chars().any(char::is_control)
        || token.contains(['<', '>'])
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let mut response = (StatusCode::OK, token).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn header_value<'a>(headers: &'a HeaderMap, name: &'static str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn verify_google_headers(
    headers: &HeaderMap,
    expected_token: Option<&str>,
    expected_channel_id: Option<&str>,
) -> Result<(), StatusCode> {
    // Validate all server configuration before inspecting credentials. A
    // partially configured endpoint must never accept a request.
    required_expected(expected_token, MAX_GOOGLE_CHANNEL_TOKEN_LEN)?;
    required_expected(expected_channel_id, MAX_GOOGLE_CHANNEL_ID_LEN)?;

    verify_shared_secret(
        expected_token,
        header_value(headers, "x-goog-channel-token"),
        MAX_GOOGLE_CHANNEL_TOKEN_LEN,
    )?;
    verify_shared_secret(
        expected_channel_id,
        header_value(headers, "x-goog-channel-id"),
        MAX_GOOGLE_CHANNEL_ID_LEN,
    )?;

    header_value(headers, "x-goog-resource-id")
        .filter(|value| valid_credential(value, MAX_GOOGLE_RESOURCE_ID_LEN))
        .ok_or(StatusCode::BAD_REQUEST)?;
    Ok(())
}

fn verify_microsoft_notifications(
    body: &[u8],
    expected_client_state: Option<&str>,
) -> Result<usize, StatusCode> {
    // Configuration is checked first so an unconfigured public endpoint fails
    // closed regardless of whether an attacker sends syntactically valid JSON.
    required_expected(expected_client_state, MAX_MICROSOFT_CLIENT_STATE_LEN)?;
    if body.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.len() > MAX_MICROSOFT_NOTIFICATION_BODY_LEN {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let payload: MsNotificationPayload =
        serde_json::from_slice(body).map_err(|_| StatusCode::BAD_REQUEST)?;
    if payload.value.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if payload.value.len() > MAX_MICROSOFT_NOTIFICATIONS {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    for item in &payload.value {
        verify_shared_secret(
            expected_client_state,
            item.client_state.as_deref(),
            MAX_MICROSOFT_CLIENT_STATE_LEN,
        )?;

        let valid_subscription_id = item
            .subscription_id
            .as_deref()
            .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok());
        let valid_change_type = matches!(
            item.change_type.as_deref(),
            Some("created" | "updated" | "deleted")
        );
        if !valid_subscription_id || !valid_change_type {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    Ok(payload.value.len())
}

/// Google Calendar `events.watch` handler.
///
/// Google echoes the opaque channel token supplied when Bluey creates the
/// channel in `X-Goog-Channel-Token`. Rejecting an unset or mismatched token is
/// essential because this route is intentionally unauthenticated at the HTTP
/// middleware layer.
pub async fn google_webhook(headers: HeaderMap) -> Response {
    let expected_token = configured_secret(GOOGLE_WEBHOOK_TOKEN_ENV);
    let expected_channel_id = configured_secret(GOOGLE_WEBHOOK_CHANNEL_ID_ENV);
    if let Err(status) = verify_google_headers(
        &headers,
        expected_token.as_deref(),
        expected_channel_id.as_deref(),
    ) {
        warn!(status = status.as_u16(), "rejected Google calendar webhook");
        return status.into_response();
    }

    info!("received authenticated Google calendar doorbell");
    StatusCode::OK.into_response()
}

/// Compatibility GET for manual validation probes.
///
/// Microsoft performs real subscription validation as a POST with the
/// URL-decoded `validationToken` in the query string; the POST handler below
/// implements that required path.
pub async fn microsoft_webhook_get(Query(query): Query<MsValidationQuery>) -> Response {
    validation_token_response(query.validation_token)
}

/// Microsoft Graph subscription validation and notification handler.
///
/// Validation POSTs can have an empty/non-JSON body, so this handler must not
/// use Axum's `Json` extractor before checking the query token. Normal
/// notifications require the exact `clientState` configured at subscription
/// creation.
pub async fn microsoft_webhook_post(
    Query(query): Query<MsValidationQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if query.validation_token.is_some() {
        if !content_type_is(&headers, "text/plain")
            || body.len() > 1_024
            || !body.iter().all(u8::is_ascii_whitespace)
        {
            return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
        }
        return validation_token_response(query.validation_token);
    }

    if !content_type_is(&headers, "application/json") {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    let expected = configured_secret(MICROSOFT_CLIENT_STATE_ENV);
    if let Err(status) = verify_microsoft_notifications(&body, expected.as_deref()) {
        warn!(
            status = status.as_u16(),
            "rejected Microsoft calendar webhook"
        );
        return status.into_response();
    }

    info!("received authenticated Microsoft calendar doorbell");
    StatusCode::OK.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_SECRET: &str = "opaque-state-1234567890";
    const GOOGLE_TOKEN: &str = "google-token-1234567890";
    const GOOGLE_CHANNEL: &str = "google-channel-1234567890";

    #[test]
    fn shared_secret_fails_closed_when_unconfigured_or_missing() {
        assert_eq!(
            verify_shared_secret(None, Some("received"), 256),
            Err(StatusCode::SERVICE_UNAVAILABLE)
        );
        assert_eq!(
            verify_shared_secret(Some(EXPECTED_SECRET), None, 256),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn shared_secret_requires_an_exact_match() {
        assert!(verify_shared_secret(Some(EXPECTED_SECRET), Some(EXPECTED_SECRET), 256).is_ok());
        assert_eq!(
            verify_shared_secret(Some(EXPECTED_SECRET), Some("wrong"), 256),
            Err(StatusCode::UNAUTHORIZED)
        );
        assert_eq!(
            verify_shared_secret(
                Some(EXPECTED_SECRET),
                Some(" opaque-state-1234567890 "),
                256,
            ),
            Err(StatusCode::UNAUTHORIZED)
        );
        assert_eq!(
            verify_shared_secret(
                Some("replace-with-random-secret"),
                Some("replace-with-random-secret"),
                256,
            ),
            Err(StatusCode::SERVICE_UNAVAILABLE)
        );
    }

    #[test]
    fn google_requires_exact_token_and_channel_id() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-goog-channel-token",
            HeaderValue::from_static(GOOGLE_TOKEN),
        );
        headers.insert(
            "x-goog-channel-id",
            HeaderValue::from_static(GOOGLE_CHANNEL),
        );
        headers.insert(
            "x-goog-resource-id",
            HeaderValue::from_static("resource-id"),
        );

        assert!(verify_google_headers(&headers, Some(GOOGLE_TOKEN), Some(GOOGLE_CHANNEL)).is_ok());
        assert_eq!(
            verify_google_headers(
                &headers,
                Some(GOOGLE_TOKEN),
                Some("wrong-channel-1234567890"),
            ),
            Err(StatusCode::UNAUTHORIZED)
        );
        assert_eq!(
            verify_google_headers(&headers, Some(GOOGLE_TOKEN), None),
            Err(StatusCode::SERVICE_UNAVAILABLE)
        );
        assert_eq!(
            verify_google_headers(&headers, None, Some(GOOGLE_CHANNEL)),
            Err(StatusCode::SERVICE_UNAVAILABLE)
        );
        headers.remove("x-goog-channel-token");
        assert_eq!(
            verify_google_headers(&headers, Some(GOOGLE_TOKEN), Some(GOOGLE_CHANNEL)),
            Err(StatusCode::UNAUTHORIZED)
        );
        headers.insert(
            "x-goog-channel-token",
            HeaderValue::from_static(GOOGLE_TOKEN),
        );
        headers.remove("x-goog-resource-id");
        assert_eq!(
            verify_google_headers(&headers, Some(GOOGLE_TOKEN), Some(GOOGLE_CHANNEL)),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn google_rejects_oversized_credentials() {
        let long_token = "x".repeat(MAX_GOOGLE_CHANNEL_TOKEN_LEN + 1);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-goog-channel-token",
            HeaderValue::from_str(&long_token).expect("valid header"),
        );
        headers.insert(
            "x-goog-channel-id",
            HeaderValue::from_static(GOOGLE_CHANNEL),
        );
        headers.insert(
            "x-goog-resource-id",
            HeaderValue::from_static("resource-id"),
        );
        assert_eq!(
            verify_google_headers(&headers, Some(GOOGLE_TOKEN), Some(GOOGLE_CHANNEL)),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn microsoft_requires_client_state_on_every_item() {
        let body = r#"{"value":[
                {
                    "subscriptionId":"6db33b8e-6c83-42a9-a41b-e7a3ac860759",
                    "clientState":"opaque-state-1234567890",
                    "changeType":"updated"
                },
                {
                    "subscriptionId":"52d87a1b-10c2-4fd8-9240-acb2f5d463b3",
                    "clientState":"wrong",
                    "changeType":"deleted"
                }
            ]}"#;
        assert_eq!(
            verify_microsoft_notifications(body.as_bytes(), Some(EXPECTED_SECRET)),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn microsoft_accepts_only_bounded_well_formed_notifications() {
        let body = r#"{"value":[{
                "subscriptionId":"6db33b8e-6c83-42a9-a41b-e7a3ac860759",
                "clientState":"opaque-state-1234567890",
                "changeType":"created"
            }]}"#;
        assert_eq!(
            verify_microsoft_notifications(body.as_bytes(), Some(EXPECTED_SECRET)),
            Ok(1)
        );
        assert_eq!(
            verify_microsoft_notifications(body.as_bytes(), None),
            Err(StatusCode::SERVICE_UNAVAILABLE)
        );

        let missing_subscription =
            br#"{"value":[{"clientState":"opaque-state-1234567890","changeType":"created"}]}"#;
        assert_eq!(
            verify_microsoft_notifications(missing_subscription, Some(EXPECTED_SECRET)),
            Err(StatusCode::BAD_REQUEST)
        );
        let missing_state = br#"{"value":[{
            "subscriptionId":"6db33b8e-6c83-42a9-a41b-e7a3ac860759",
            "changeType":"created"
        }]}"#;
        assert_eq!(
            verify_microsoft_notifications(missing_state, Some(EXPECTED_SECRET)),
            Err(StatusCode::UNAUTHORIZED)
        );

        let oversized_body = vec![b' '; MAX_MICROSOFT_NOTIFICATION_BODY_LEN + 1];
        assert_eq!(
            verify_microsoft_notifications(&oversized_body, Some(EXPECTED_SECRET)),
            Err(StatusCode::PAYLOAD_TOO_LARGE)
        );
    }

    #[tokio::test]
    async fn microsoft_validation_is_plain_text_and_not_sniffable() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        let response = microsoft_webhook_post(
            Query(MsValidationQuery {
                validation_token: Some("opaque-validation-token".to_string()),
            }),
            headers,
            Bytes::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/plain; charset=utf-8"))
        );
        assert_eq!(
            response.headers().get("x-content-type-options"),
            Some(&HeaderValue::from_static("nosniff"))
        );
        let body = axum::body::to_bytes(response.into_body(), MAX_VALIDATION_TOKEN_LEN)
            .await
            .expect("response body");
        assert_eq!(body, "opaque-validation-token");
    }

    #[tokio::test]
    async fn microsoft_validation_rejects_wrong_media_type_and_markup() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let response = microsoft_webhook_post(
            Query(MsValidationQuery {
                validation_token: Some("opaque-validation-token".to_string()),
            }),
            headers,
            Bytes::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let response = validation_token_response(Some("<script>alert(1)</script>".to_string()));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
