//! Short-lived, scoped authentication for private Bluey Jobs workers.

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use super::AppState;

type HmacSha256 = Hmac<Sha256>;
type AuthError = (StatusCode, String);

const SIGNATURE_VERSION: &str = "bluey-jobs-worker-v1";
const SIGNATURE_AUDIENCE: &str = "bluey-jobs-api";
const SIGNATURE_WINDOW_SECS: u64 = 90;
const DEFAULT_SIGNED_BODY_BYTES: usize = 4 * 1024 * 1024;
const DISCOVERY_SIGNED_BODY_BYTES: usize = 32 * 1024 * 1024;
const BROWSER_PROFILE_SIGNED_BODY_BYTES: usize = 64 * 1024 * 1024;
const RECEIPT_SIGNED_BODY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct JobsWorkerIdentity {
    pub worker_id: String,
    pub scope: String,
}

struct VerifiedWorkerRequest {
    identity: JobsWorkerIdentity,
    content_sha256: String,
}

struct WorkerSignatureInput<'a> {
    worker_id: &'a str,
    timestamp: u64,
    nonce: &'a str,
    audience: &'a str,
    scope: &'a str,
    method: &'a str,
    path: &'a str,
    content_sha256: &'a str,
}

pub async fn require_jobs_worker(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, AuthError> {
    #[cfg(debug_assertions)]
    if legacy_debug_token_valid(&request) {
        request.extensions_mut().insert(JobsWorkerIdentity {
            worker_id: "debug-legacy-worker".to_string(),
            scope: "debug".to_string(),
        });
        return Ok(next.run(request).await);
    }

    let verified = verify_request(&request, unix_seconds())?;
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, signed_body_limit(parts.uri.path()))
        .await
        .map_err(|_| unauthorized())?;
    let actual_content_sha256 = hex::encode(Sha256::digest(&body));
    if actual_content_sha256
        .as_bytes()
        .ct_eq(verified.content_sha256.as_bytes())
        .unwrap_u8()
        != 1
    {
        return Err(unauthorized());
    }
    request = Request::from_parts(parts, Body::from(body));
    let identity = verified.identity;
    let replay_key = hex::encode(Sha256::digest(format!(
        "{}\0{}\0{}",
        identity.worker_id,
        identity.scope,
        required_header(&request, "x-bluey-jobs-worker-nonce")?
    )));
    let reserved = state
        .rate_limiters
        .jobs_worker_replay
        .reserve(
            &format!("jobs-worker:{replay_key}"),
            SIGNATURE_WINDOW_SECS * 2,
        )
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "jobs worker replay guard failed closed");
            unauthorized()
        })?;
    if !reserved {
        tracing::warn!(worker_id = %identity.worker_id, scope = %identity.scope, "replayed jobs worker request rejected");
        return Err(unauthorized());
    }

    tracing::debug!(worker_id = %identity.worker_id, scope = %identity.scope, "authenticated jobs worker request");
    request.extensions_mut().insert(identity);
    Ok(next.run(request).await)
}

fn verify_request(
    request: &Request<Body>,
    now_secs: u64,
) -> Result<VerifiedWorkerRequest, AuthError> {
    let worker_id = required_header(request, "x-bluey-jobs-worker-id")?;
    let timestamp_raw = required_header(request, "x-bluey-jobs-worker-timestamp")?;
    let nonce = required_header(request, "x-bluey-jobs-worker-nonce")?;
    let supplied_scope = required_header(request, "x-bluey-jobs-worker-scope")?;
    let supplied_audience = required_header(request, "x-bluey-jobs-worker-audience")?;
    let content_sha256 = required_header(request, "x-bluey-jobs-worker-content-sha256")?;
    let supplied_signature = required_header(request, "x-bluey-jobs-worker-signature")?;

    if !valid_identifier(worker_id, 3, 128)
        || !valid_identifier(nonce, 24, 128)
        || supplied_signature.len() != 64
        || content_sha256.len() != 64
        || !supplied_signature
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !content_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || supplied_audience != SIGNATURE_AUDIENCE
    {
        return Err(unauthorized());
    }
    let timestamp = timestamp_raw.parse::<u64>().map_err(|_| unauthorized())?;
    if timestamp.abs_diff(now_secs) > SIGNATURE_WINDOW_SECS {
        return Err(unauthorized());
    }
    let expected_scope =
        worker_scope(request.method().as_str(), request.uri().path()).ok_or_else(unauthorized)?;
    if supplied_scope != expected_scope {
        return Err(unauthorized());
    }

    let canonical = canonical_request(WorkerSignatureInput {
        worker_id,
        timestamp,
        nonce,
        audience: SIGNATURE_AUDIENCE,
        scope: expected_scope,
        method: request.method().as_str(),
        path: request.uri().path(),
        content_sha256,
    });
    let current = std::env::var("BLUEY_JOBS_WORKER_SIGNING_KEY").unwrap_or_default();
    let previous = std::env::var("BLUEY_JOBS_WORKER_SIGNING_KEY_PREVIOUS").unwrap_or_default();
    let valid = [current.as_str(), previous.as_str()]
        .into_iter()
        .filter(|key| key.len() >= 32)
        .any(|key| signature_matches(key, canonical.as_bytes(), supplied_signature));
    if !valid {
        return Err(unauthorized());
    }

    Ok(VerifiedWorkerRequest {
        identity: JobsWorkerIdentity {
            worker_id: worker_id.to_string(),
            scope: expected_scope.to_string(),
        },
        content_sha256: content_sha256.to_ascii_lowercase(),
    })
}

fn canonical_request(input: WorkerSignatureInput<'_>) -> String {
    format!(
        "{SIGNATURE_VERSION}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        input.timestamp,
        input.nonce,
        input.worker_id,
        input.audience,
        input.scope,
        input.method.to_ascii_uppercase(),
        input.path,
        input.content_sha256,
    )
}

fn signature_matches(key: &str, canonical: &[u8], supplied_hex: &str) -> bool {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(canonical);
    let expected = hex::encode(mac.finalize().into_bytes());
    expected.len() == supplied_hex.len()
        && expected
            .as_bytes()
            .ct_eq(supplied_hex.as_bytes())
            .unwrap_u8()
            == 1
}

fn worker_scope(method: &str, path: &str) -> Option<&'static str> {
    if !method.eq_ignore_ascii_case("POST") || !path.starts_with("/api/jobs/internal/") {
        return None;
    }
    if path.contains("/execution-leases/") {
        Some("execution")
    } else if path.contains("/discovery/") || path.contains("/global-discovery/") {
        Some("discovery")
    } else if path.ends_with("/receipt") {
        Some("receipt")
    } else if path.ends_with("/interventions") {
        Some("intervention")
    } else if path.ends_with("/state") {
        Some("application-state")
    } else if path.ends_with("/events") {
        Some("run-events")
    } else {
        None
    }
}

fn signed_body_limit(path: &str) -> usize {
    if path.ends_with("/receipt") {
        RECEIPT_SIGNED_BODY_BYTES
    } else if path.contains("/execution-leases/") && path.ends_with("/profile/store") {
        BROWSER_PROFILE_SIGNED_BODY_BYTES
    } else if (path.contains("/discovery/") || path.contains("/global-discovery/"))
        && (path.ends_with("/complete") || path.ends_with("/batches"))
    {
        DISCOVERY_SIGNED_BODY_BYTES
    } else {
        DEFAULT_SIGNED_BODY_BYTES
    }
}

fn required_header<'a>(request: &'a Request<Body>, name: &str) -> Result<&'a str, AuthError> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(unauthorized)
}

fn valid_identifier(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn unauthorized() -> AuthError {
    (StatusCode::UNAUTHORIZED, "Unauthorized".to_string())
}

#[cfg(debug_assertions)]
fn legacy_debug_token_valid(request: &Request<Body>) -> bool {
    let expected = std::env::var("BLUEY_JOBS_WORKER_TOKEN").unwrap_or_default();
    let supplied = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    !expected.is_empty()
        && supplied.len() == expected.len()
        && supplied.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    fn signed_request(path: &str, timestamp: u64, nonce: &str, scope: &str) -> Request<Body> {
        let content_sha256 = hex::encode(Sha256::digest([]));
        let canonical = canonical_request(WorkerSignatureInput {
            worker_id: "test-worker",
            timestamp,
            nonce,
            audience: SIGNATURE_AUDIENCE,
            scope,
            method: "POST",
            path,
            content_sha256: &content_sha256,
        });
        let mut mac = HmacSha256::new_from_slice(KEY.as_bytes()).unwrap();
        mac.update(canonical.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        Request::post(path)
            .header("x-bluey-jobs-worker-id", "test-worker")
            .header("x-bluey-jobs-worker-timestamp", timestamp.to_string())
            .header("x-bluey-jobs-worker-nonce", nonce)
            .header("x-bluey-jobs-worker-audience", SIGNATURE_AUDIENCE)
            .header("x-bluey-jobs-worker-scope", scope)
            .header("x-bluey-jobs-worker-content-sha256", content_sha256)
            .header("x-bluey-jobs-worker-signature", signature)
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn worker_scope_is_path_and_operation_specific() {
        assert_eq!(
            worker_scope("POST", "/api/jobs/internal/discovery/lease"),
            Some("discovery")
        );
        assert_eq!(
            worker_scope("POST", "/api/jobs/internal/global-discovery/lease"),
            Some("discovery")
        );
        assert_eq!(
            worker_scope("POST", "/api/jobs/internal/execution-leases/run/heartbeat"),
            Some("execution")
        );
        assert_eq!(
            worker_scope("GET", "/api/jobs/internal/discovery/lease"),
            None
        );
        assert_eq!(
            signed_body_limit("/api/jobs/internal/applications/app/receipt"),
            RECEIPT_SIGNED_BODY_BYTES
        );
        assert_eq!(
            signed_body_limit("/api/jobs/internal/execution-leases/run/profile/store"),
            BROWSER_PROFILE_SIGNED_BODY_BYTES
        );
        assert_eq!(
            signed_body_limit("/api/jobs/internal/discovery/source/complete"),
            DISCOVERY_SIGNED_BODY_BYTES
        );
        assert_eq!(
            signed_body_limit("/api/jobs/internal/global-discovery/source/batches"),
            DISCOVERY_SIGNED_BODY_BYTES
        );
        assert_eq!(
            signed_body_limit("/api/jobs/internal/discovery/lease"),
            DEFAULT_SIGNED_BODY_BYTES
        );
    }

    #[test]
    fn worker_signature_matches_javascript_client_vector() {
        let canonical = canonical_request(WorkerSignatureInput {
            worker_id: "workflow-test",
            timestamp: 1_750_000_000,
            nonce: "abcdef0123456789abcdef0123456789",
            audience: SIGNATURE_AUDIENCE,
            scope: "application-state",
            method: "POST",
            path: "/api/jobs/internal/applications/app-123/state",
            content_sha256: "d2bf9fe5a8a5253a3c0f969fdac700d8936d5b728770133ee502efea230979d6",
        });
        assert!(signature_matches(
            KEY,
            canonical.as_bytes(),
            "60d14a1656e9b560ad3bdb871d92be635b68060079b566c1a8ec461a41187bfa",
        ));
    }

    #[test]
    fn signed_request_is_bound_to_scope_and_time() {
        std::env::set_var("BLUEY_JOBS_WORKER_SIGNING_KEY", KEY);
        let now = 1_750_000_000;
        let request = signed_request(
            "/api/jobs/internal/discovery/lease",
            now,
            "abcdef0123456789abcdef0123456789",
            "discovery",
        );
        let verified = verify_request(&request, now).unwrap();
        assert_eq!(verified.identity.worker_id, "test-worker");
        assert_eq!(verified.identity.scope, "discovery");

        let expired = signed_request(
            "/api/jobs/internal/discovery/lease",
            now - SIGNATURE_WINDOW_SECS - 1,
            "abcdef0123456789abcdef0123456790",
            "discovery",
        );
        assert!(verify_request(&expired, now).is_err());

        let wrong_scope = signed_request(
            "/api/jobs/internal/discovery/lease",
            now,
            "abcdef0123456789abcdef0123456791",
            "execution",
        );
        assert!(verify_request(&wrong_scope, now).is_err());
        std::env::remove_var("BLUEY_JOBS_WORKER_SIGNING_KEY");
    }
}
