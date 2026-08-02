//! Authenticated, one-time Jobs -> Bluey desktop interview-prep handoff.

use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::AppState;
use crate::{
    auth::AuthedAccount,
    db::{jobs, jobs_handoffs},
};

type ApiError = (StatusCode, String);

#[derive(Debug, Serialize)]
pub struct IssueJobsHandoffResponse {
    schema_version: i64,
    audience: &'static str,
    nonce: String,
    deep_link_url: String,
    expires_at_ms: i64,
    expires_in_seconds: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedeemJobsHandoffRequest {
    nonce: String,
}

#[derive(Debug, Serialize)]
pub struct RedeemJobsHandoffResponse {
    schema_version: i64,
    audience: &'static str,
    account_id: String,
    application_id: String,
    snapshot: Value,
}

pub async fn issue(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(application_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !valid_application_id(&application_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "The application ID is invalid.".to_string(),
        ));
    }
    let application = jobs::get_application(&state.pool, &account.id, &application_id)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "Application not found.".to_string()))?;
    let resume_id = application.resume_version_id.as_deref().ok_or((
        StatusCode::CONFLICT,
        "This application does not have an exact submitted resume.".to_string(),
    ))?;
    let resume = jobs::get_resume_version(&state.pool, &account.id, resume_id)
        .map_err(internal)?
        .ok_or((
            StatusCode::CONFLICT,
            "The submitted resume version is unavailable.".to_string(),
        ))?;
    let evidence = jobs::list_application_evidence(&state.pool, &account.id, Some(&application.id))
        .map_err(internal)?;
    let snapshot = super::jobs_interview_prep::build_desktop_handoff_snapshot(
        &account.id,
        &application,
        &resume,
        &evidence,
    )?;
    let issued = jobs_handoffs::issue(
        &state.pool,
        &account.id,
        &application.id,
        jobs_handoffs::BLUEY_DESKTOP_AUDIENCE,
        &snapshot,
    )
    .map_err(internal)?;
    let response = IssueJobsHandoffResponse {
        schema_version: 1,
        audience: jobs_handoffs::BLUEY_DESKTOP_AUDIENCE,
        deep_link_url: deep_link_for_nonce(&issued.nonce),
        nonce: issued.nonce,
        expires_at_ms: issued.expires_at_ms,
        expires_in_seconds: jobs_handoffs::HANDOFF_TTL_MS / 1_000,
    };
    Ok(no_store(Json(response)))
}

pub async fn redeem(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(request): Json<RedeemJobsHandoffRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !jobs_handoffs::valid_nonce(&request.nonce) {
        return Err((
            StatusCode::BAD_REQUEST,
            "The Bluey handoff is invalid.".to_string(),
        ));
    }
    let redeemed = jobs_handoffs::redeem(
        &state.pool,
        &account.id,
        jobs_handoffs::BLUEY_DESKTOP_AUDIENCE,
        &request.nonce,
    )
    .map_err(internal)?
    .ok_or((
        StatusCode::GONE,
        "This Bluey handoff expired or was already used. Open it again from Bluey Jobs."
            .to_string(),
    ))?;
    if redeemed
        .snapshot
        .pointer("/application/application_id")
        .and_then(Value::as_str)
        != Some(redeemed.application_id.as_str())
    {
        return Err(internal("Jobs handoff snapshot binding is invalid"));
    }
    let response = RedeemJobsHandoffResponse {
        schema_version: 1,
        audience: jobs_handoffs::BLUEY_DESKTOP_AUDIENCE,
        account_id: account.id,
        application_id: redeemed.application_id,
        snapshot: redeemed.snapshot,
    };
    Ok(no_store(Json(response)))
}

fn no_store<T: Serialize>(body: Json<T>) -> impl IntoResponse {
    (
        [
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (header::PRAGMA, HeaderValue::from_static("no-cache")),
        ],
        body,
    )
}

fn deep_link_for_nonce(nonce: &str) -> String {
    format!("bluey://jobs/interview-prep?nonce={nonce}")
}

fn valid_application_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %error, "Bluey Jobs desktop handoff failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Bluey could not open this application right now.".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_link_contains_only_the_valid_nonce() {
        let nonce = "A".repeat(jobs_handoffs::ENCODED_NONCE_LEN);
        let link = deep_link_for_nonce(&nonce);
        let parsed = reqwest::Url::parse(&link).unwrap();
        assert_eq!(parsed.scheme(), "bluey");
        assert_eq!(parsed.host_str(), Some("jobs"));
        assert_eq!(parsed.path(), "/interview-prep");
        assert_eq!(parsed.query_pairs().count(), 1);
        assert_eq!(parsed.query_pairs().next().unwrap().0, "nonce");
        assert_eq!(parsed.query_pairs().next().unwrap().1, nonce);
        assert!(parsed.fragment().is_none());
    }

    #[test]
    fn application_ids_are_bounded() {
        assert!(valid_application_id("application-123"));
        assert!(!valid_application_id(""));
        assert!(!valid_application_id("../application"));
        assert!(!valid_application_id(&"a".repeat(241)));
    }
}
