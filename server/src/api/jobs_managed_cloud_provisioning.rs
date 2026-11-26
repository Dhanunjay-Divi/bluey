//! Explicit v2 control-plane HTTP boundary. No serving or business authority.
//!
//! The process context is installed once when the router is built. Requests
//! cannot choose a deployment purpose or supply a verified-context boolean.

use std::sync::Arc;

use axum::body::Bytes;
use serde::de::DeserializeOwned;

use super::{
    authoritative_json, invalid_request, registry_api_error, require_runtime_worker, ApiError,
    AppState, AuthedAccount, DefaultBodyLimit, Extension, IssueRuntimeGrantRequest,
    JobsWorkerIdentity, NewManagedCloudRuntimeGrant, Path, Response, Router, State,
};
use crate::db::jobs::{
    managed_cloud_provisioning_installation as installation,
    managed_cloud_provisioning_protocol as protocol,
    managed_cloud_provisioning_runtime::{self as runtime, ProvisioningProcessContext},
    managed_cloud_provisioning_snapshot as snapshot,
};

const BODY_LIMIT_BYTES: usize = 64 * 1024;
// Two base64url-encoded, independently 128 KiB bounded canonical documents.
const INSTALL_BODY_LIMIT_BYTES: usize = 2 * (128_usize * 1024 * 4).div_ceil(3) + 128;

pub(super) fn worker_router(context: Arc<ProvisioningProcessContext>) -> Router<AppState> {
    Router::new()
        .route(
            "/api/jobs/internal/managed-cloud/v2/runtime-grants/:grant_id/claim",
            axum::routing::post(claim),
        )
        .route(
            "/api/jobs/internal/managed-cloud/v2/runtime-instances/:runtime_instance_id/heartbeats",
            axum::routing::post(heartbeat),
        )
        .route(
            "/api/jobs/internal/managed-cloud/v2/runtime-instances/:runtime_instance_id/source-verifier-boot-registration",
            axum::routing::post(register_boot),
        )
        .route(
            "/api/jobs/internal/managed-cloud/v2/runtime-instances/:runtime_instance_id/source-verifier-boot-startup",
            axum::routing::post(startup),
        )
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .layer(Extension(context))
}

pub(super) fn admin_router(context: Arc<ProvisioningProcessContext>) -> Router<AppState> {
    let ordinary = Router::new()
        .route(
            "/admin/jobs/managed-cloud/v2/queue-generations",
            axum::routing::post(allocate_queue),
        )
        .route(
            "/admin/jobs/managed-cloud/v2/runtime-grants",
            axum::routing::post(issue),
        )
        .route(
            "/admin/jobs/managed-cloud/v2/provisioning-signing-requests",
            axum::routing::post(snapshot_signing_request),
        )
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES));
    let install = Router::new()
        .route(
            "/admin/jobs/managed-cloud/v2/provisioning-heads",
            axum::routing::post(install_head),
        )
        .layer(DefaultBodyLimit::max(INSTALL_BODY_LIMIT_BYTES));
    ordinary.merge(install).layer(Extension(context))
}

async fn install_head(
    State(state): State<AppState>,
    Extension(context): Extension<Arc<ProvisioningProcessContext>>,
    Extension(admin): Extension<AuthedAccount>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request = decode_install(&body)?;
    installation::install_provisioning_head(&state.pool, &context, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

fn decode_install(body: &[u8]) -> Result<installation::InstallProvisioningRequest, ApiError> {
    if body.is_empty() || body.len() > INSTALL_BODY_LIMIT_BYTES {
        return Err(invalid_request());
    }
    serde_json::from_slice(body).map_err(|_| invalid_request())
}

async fn allocate_queue(
    State(state): State<AppState>,
    Extension(context): Extension<Arc<ProvisioningProcessContext>>,
    Extension(admin): Extension<AuthedAccount>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: installation::AllocateProvisioningQueueRequest = decode(&body)?;
    installation::allocate_provisioning_queue(&state.pool, &context, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn snapshot_signing_request(
    State(state): State<AppState>,
    Extension(context): Extension<Arc<ProvisioningProcessContext>>,
    Extension(admin): Extension<AuthedAccount>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: snapshot::ProvisioningSigningRequest = decode(&body)?;
    snapshot::snapshot_provisioning_signing_request(&state.pool, &context, &request, &admin.0.id)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

// Do not expose serde diagnostics, which may contain a rejected secret value.
fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    if body.is_empty() || body.len() > BODY_LIMIT_BYTES {
        return Err(invalid_request());
    }
    serde_json::from_slice(body).map_err(|_| invalid_request())
}

// Insert below `decode` in server/src/api/jobs_managed_cloud_provisioning.rs.
// Decoder-only coverage: it does not exercise router middleware, auth, DB, or
// semantic snapshot validation.
#[cfg(test)]
mod snapshot_decode_tests {
    use super::*;
    use crate::db::jobs::managed_cloud_provisioning_snapshot as snapshot;

    fn valid_json() -> String {
        r#"{"version":1,"scope":{"environment":"production","region":"us-east-1","channel":"general"},"queueGenerationId":"queue-gen-1","issuanceRef":"issue-ref-1","manifestSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","cohortSha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","activationId":"activation-1","activationGeneration":1,"issuedAtMs":1000,"notBeforeMs":1000,"expiresAtMs":2000,"maximumInflight":1,"maximumDailyAdmissions":1,"heartbeatTtlMs":5000,"intendedStorageConfigSha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","signatureSetId":"signature-set-1","requestedSignedAtMs":1000}"#.into()
    }

    fn decode_snapshot(body: &[u8]) -> Result<snapshot::ProvisioningSigningRequest, ApiError> {
        decode::<snapshot::ProvisioningSigningRequest>(body)
    }

    #[test]
    fn snapshot_decode_accepts_the_closed_request_shape() {
        let request = decode_snapshot(valid_json().as_bytes()).expect("valid request decodes");
        assert_eq!(request.version, 1);
        assert_eq!(request.scope.channel, "general");
        assert_eq!(request.queue_generation_id, "queue-gen-1");
    }

    #[test]
    fn snapshot_decode_refuses_unknown_fields_without_echoing_the_value() {
        let sentinel = "decoder-secret-sentinel";
        let body = format!(
            "{},\"unexpected\":\"{sentinel}\"}}",
            valid_json().strip_suffix('}').expect("object")
        );
        let error = decode_snapshot(body.as_bytes()).expect_err("unknown field must refuse");
        assert!(!error.1.contains(sentinel));
    }

    #[test]
    fn snapshot_decode_refuses_malformed_empty_missing_and_wrong_type_bodies() {
        let missing = valid_json().replace(r#","signatureSetId":"signature-set-1""#, "");
        let wrong_type = valid_json().replace(r#""version":1"#, r#""version":"one""#);
        for body in [
            b"{".as_slice(),
            b"".as_slice(),
            missing.as_bytes(),
            wrong_type.as_bytes(),
        ] {
            assert!(decode_snapshot(body).is_err(), "decoder must refuse");
        }
    }

    #[test]
    fn snapshot_decode_accepts_exact_body_limit_and_refuses_one_byte_over() {
        let body = valid_json();
        assert!(body.len() <= BODY_LIMIT_BYTES);
        let exact = format!("{body}{}", " ".repeat(BODY_LIMIT_BYTES - body.len()));
        assert_eq!(exact.len(), BODY_LIMIT_BYTES);
        assert!(decode_snapshot(exact.as_bytes()).is_ok());
        let over = format!("{exact} ");
        assert_eq!(over.len(), BODY_LIMIT_BYTES + 1);
        assert!(decode_snapshot(over.as_bytes()).is_err());
    }
}
async fn issue(
    State(state): State<AppState>,
    Extension(context): Extension<Arc<ProvisioningProcessContext>>,
    Extension(admin): Extension<AuthedAccount>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: IssueRuntimeGrantRequest = decode(&body)?;
    let input = NewManagedCloudRuntimeGrant {
        issuance_ref: request.issuance_ref,
        scope: request.scope,
        activation_sha256: request.activation_sha256,
        manifest_sha256: request.manifest_sha256,
        component_id: request.component_id,
        role: request.role,
        expected_worker_id: request.expected_worker_id,
        authorization_ref: request.authorization_ref,
        created_by: admin.0.id.clone(),
        ttl_ms: request.ttl_ms,
    };
    runtime::issue_provisioning_runtime_grant(&state.pool, &context, &input)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn claim(
    State(state): State<AppState>,
    Extension(context): Extension<Arc<ProvisioningProcessContext>>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(grant_id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: protocol::ClaimRequest = decode(&body)?;
    require_runtime_worker(&worker, &request.worker_id)?;
    if request.grant_id != grant_id {
        return Err(invalid_request());
    }
    runtime::claim_provisioning_runtime_grant(&state.pool, &context, &request)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn heartbeat(
    State(state): State<AppState>,
    Extension(context): Extension<Arc<ProvisioningProcessContext>>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(runtime_instance_id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request = protocol::parse_heartbeat_request(&body).map_err(registry_api_error)?;
    require_runtime_worker(&worker, &request.worker_id)?;
    if request.runtime_instance_id != runtime_instance_id {
        return Err(invalid_request());
    }
    runtime::record_provisioning_runtime_heartbeat(&state.pool, &context, &request)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn register_boot(
    State(state): State<AppState>,
    Extension(context): Extension<Arc<ProvisioningProcessContext>>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(runtime_instance_id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: protocol::BootRegistrationRequest = decode(&body)?;
    require_runtime_worker(&worker, &request.worker_id)?;
    if request.runtime_instance_id != runtime_instance_id {
        return Err(invalid_request());
    }
    runtime::register_provisioning_source_verifier_boot(&state.pool, &context, &request)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

async fn startup(
    State(state): State<AppState>,
    Extension(context): Extension<Arc<ProvisioningProcessContext>>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Path(runtime_instance_id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: protocol::BootStartupRequest = decode(&body)?;
    require_runtime_worker(&worker, &request.worker_id)?;
    if request.runtime_instance_id != runtime_instance_id {
        return Err(invalid_request());
    }
    runtime::record_provisioning_source_verifier_startup(&state.pool, &context, &request)
        .map(authoritative_json)
        .map_err(registry_api_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c3_http_decoder_does_not_echo_rejected_values() {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SecretRequest {
            _session_token: String,
        }
        for body in [
            br#"{"_session_token":123,"private":"do-not-log"}"#.as_slice(),
            br#"{"_session_token":"private-secret","_session_token":"duplicate"}"#.as_slice(),
            b"",
        ] {
            let error = match decode::<SecretRequest>(body) {
                Ok(_) => panic!("invalid secret request accepted"),
                Err(error) => error,
            };
            assert_eq!(error, invalid_request());
            assert!(!error.1.contains("private"));
        }
        assert!(decode::<SecretRequest>(&vec![b' '; BODY_LIMIT_BYTES + 1]).is_err());
    }

    #[test]
    fn queue_request_decoder_accepts_bounded_shape_and_rejects_purpose_or_extras() {
        let body = serde_json::to_vec(&serde_json::json!({
            "issuanceRef": "i".repeat(128),
            "scope": {"environment": "production", "region": "a".repeat(64), "channel": "general"},
            "namespace": "n".repeat(240),
            "taskQueue": "q".repeat(240)
        }))
        .unwrap();
        assert!(body.len() < BODY_LIMIT_BYTES);
        assert!(decode::<installation::AllocateProvisioningQueueRequest>(&body).is_ok());
        let bad = br#"{"issuanceRef":"private-rejected-value","scope":{"environment":"production","region":"us-east-1","channel":"general"},"namespace":"n","taskQueue":"q","purpose":"nonproduction_synthetic"}"#;
        let error = decode::<installation::AllocateProvisioningQueueRequest>(bad).unwrap_err();
        assert_eq!(error, invalid_request());
        assert!(!error.1.contains("private-rejected-value"));
    }

    #[test]
    fn install_decoder_bounds_two_envelopes_and_redacts_rejected_fields() {
        let encoded_limit = (128_usize * 1024 * 4).div_ceil(3);
        let body = serde_json::to_vec(&serde_json::json!({"envelope":{
            "canonicalBase64url":"A".repeat(encoded_limit),
            "signatureSetBase64url":"A".repeat(encoded_limit)
        }}))
        .unwrap();
        assert!(body.len() <= INSTALL_BODY_LIMIT_BYTES);
        assert!(decode_install(&body).is_ok());
        assert!(decode_install(&vec![b' '; INSTALL_BODY_LIMIT_BYTES + 1]).is_err());
        let invalid = br#"{"envelope":{"canonicalBase64url":"private-rejected-value","signatureSetBase64url":"A","extra":1}}"#;
        let error = decode_install(invalid).unwrap_err();
        assert_eq!(error, invalid_request());
        assert!(!error.1.contains("private-rejected-value"));
    }
}
