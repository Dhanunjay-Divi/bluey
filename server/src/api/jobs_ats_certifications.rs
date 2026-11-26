//! Authenticated ATS certification authority APIs for Bluey Jobs.

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    api::{jobs_worker_auth::JobsWorkerIdentity, AppState},
    auth::AuthedAccount,
    db::jobs::{
        self, AtsCertificationAuthorityEnvelope, AtsCertificationAuthorityError,
        AtsCertificationCanaryAllowlistImportRequest, AtsCertificationCanaryAllowlistImportResult,
        AtsCertificationCanaryAllowlistRevocationRequest,
        AtsCertificationCanaryAllowlistRevocationResult, AtsCertificationCircuitEvent,
        AtsCertificationCircuitState, AtsCertificationHeadResult, AtsCertificationImportResult,
        AtsCertificationManifestAggregateEnvelope, AtsCertificationManifestAggregateImportResult,
        AtsCertificationTargetStatusProjection,
    },
    db::ops_audit,
};

type ApiError = (StatusCode, String);

const ATS_CERTIFICATION_BODY_LIMIT_BYTES: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyAtsCertificationActivationRequest {
    activation_sha256: String,
    expected_head_revision: i64,
    expected_transition_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AtsCertificationTargetStatusQuery {
    canonical_url: String,
    discovery_provider: String,
    discovery_target_key: String,
    discovery_observed_at_ms: i64,
    original_source_provider: String,
    original_source_target_key: String,
    original_source_observed_at_ms: i64,
    runner_target_sha256: Option<String>,
    channel: String,
    expected_account_allowlist_sha256: Option<String>,
}

impl AtsCertificationTargetStatusQuery {
    fn target_evidence(&self) -> jobs::AtsCertificationFreshTargetEvidence {
        jobs::AtsCertificationFreshTargetEvidence {
            canonical_url: self.canonical_url.clone(),
            discovery_provider: self.discovery_provider.clone(),
            discovery_target_key: self.discovery_target_key.clone(),
            discovery_observed_at_ms: self.discovery_observed_at_ms,
            original_source_provider: self.original_source_provider.clone(),
            original_source_target_key: self.original_source_target_key.clone(),
            original_source_observed_at_ms: self.original_source_observed_at_ms,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AtsCertificationCircuitRequest {
    event_id: String,
    scope_kind: String,
    subject_key: String,
    transition: String,
    trigger_kind: String,
    window_started_at_ms: i64,
    window_ended_at_ms: i64,
    failure_count: i64,
    sample_count: i64,
    threshold_count: i64,
    authority_ref: String,
    event_at_ms: i64,
    expected_head_revision: i64,
    expected_event_id: Option<String>,
}

impl AtsCertificationCircuitRequest {
    fn event(&self) -> AtsCertificationCircuitEvent {
        AtsCertificationCircuitEvent {
            event_id: self.event_id.clone(),
            scope_kind: self.scope_kind.clone(),
            subject_key: self.subject_key.clone(),
            transition: self.transition.clone(),
            trigger_kind: self.trigger_kind.clone(),
            window_started_at_ms: self.window_started_at_ms,
            window_ended_at_ms: self.window_ended_at_ms,
            failure_count: self.failure_count,
            sample_count: self.sample_count,
            threshold_count: self.threshold_count,
            authority_ref: self.authority_ref.clone(),
            event_at_ms: self.event_at_ms,
        }
    }
}

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/jobs/ats-certifications/trust-policies",
            post(import_trust_policy),
        )
        .route(
            "/admin/jobs/ats-certifications/layout-observations",
            post(import_layout_observation),
        )
        .route(
            "/admin/jobs/ats-certifications/manifests",
            post(import_manifest_aggregate),
        )
        .route(
            "/admin/jobs/ats-certifications/activations",
            post(import_activation),
        )
        .route(
            "/admin/jobs/ats-certifications/activations/apply",
            post(apply_activation),
        )
        .route(
            "/admin/jobs/ats-certifications/canary-allowlists",
            post(import_canary_allowlist),
        )
        .route(
            "/admin/jobs/ats-certifications/canary-allowlists/revoke",
            post(revoke_canary_allowlist),
        )
        .route(
            "/admin/jobs/ats-certifications/revocations",
            post(import_revocation),
        )
        .route(
            "/admin/jobs/ats-certifications/circuits",
            post(transition_circuit),
        )
        .route(
            "/admin/jobs/ats-certifications/targets/:target_key/status",
            get(target_status),
        )
        .layer(DefaultBodyLimit::max(ATS_CERTIFICATION_BODY_LIMIT_BYTES))
}

pub fn worker_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/jobs/internal/ats-certifications/layout-observations",
            post(import_worker_layout_observation),
        )
        .layer(DefaultBodyLimit::max(ATS_CERTIFICATION_BODY_LIMIT_BYTES))
}

async fn import_trust_policy(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(envelope): Json<AtsCertificationAuthorityEnvelope>,
) -> Result<Json<AtsCertificationImportResult>, ApiError> {
    let result = jobs::import_ats_certification_trust_policy(&state.pool, &envelope, &admin.0.id)
        .map_err(authority_api_error)?;
    record_import_audit(&state, &admin.0.id, "trust_policy", &result)?;
    Ok(Json(result))
}

async fn import_layout_observation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(envelope): Json<AtsCertificationAuthorityEnvelope>,
) -> Result<Json<AtsCertificationImportResult>, ApiError> {
    let result = jobs::import_ats_layout_observation(&state.pool, &envelope, &admin.0.id)
        .map_err(authority_api_error)?;
    record_import_audit(&state, &admin.0.id, "layout_observation", &result)?;
    Ok(Json(result))
}

async fn import_worker_layout_observation(
    State(state): State<AppState>,
    Extension(worker): Extension<JobsWorkerIdentity>,
    Json(envelope): Json<AtsCertificationAuthorityEnvelope>,
) -> Result<Json<AtsCertificationImportResult>, ApiError> {
    let result = jobs::import_ats_layout_observation(&state.pool, &envelope, &worker.worker_id)
        .map_err(authority_api_error)?;
    record_import_audit(
        &state,
        &worker.worker_id,
        "worker_layout_observation",
        &result,
    )?;
    Ok(Json(result))
}

async fn import_manifest_aggregate(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(aggregate): Json<AtsCertificationManifestAggregateEnvelope>,
) -> Result<Json<AtsCertificationManifestAggregateImportResult>, ApiError> {
    let result =
        jobs::import_ats_certification_manifest_aggregate(&state.pool, &aggregate, &admin.0.id)
            .map_err(authority_api_error)?;
    record_manifest_aggregate_audit(&state, &admin.0.id, &result)?;
    Ok(Json(result))
}

async fn import_activation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(envelope): Json<AtsCertificationAuthorityEnvelope>,
) -> Result<Json<AtsCertificationImportResult>, ApiError> {
    let result = jobs::import_ats_certification_activation(&state.pool, &envelope, &admin.0.id)
        .map_err(authority_api_error)?;
    record_import_audit(&state, &admin.0.id, "activation_import", &result)?;
    Ok(Json(result))
}

async fn apply_activation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<ApplyAtsCertificationActivationRequest>,
) -> Result<Json<AtsCertificationHeadResult>, ApiError> {
    let result = jobs::apply_ats_certification_activation(
        &state.pool,
        &request.activation_sha256,
        request.expected_head_revision,
        request.expected_transition_sha256.as_deref(),
        &admin.0.id,
    )
    .map_err(authority_api_error)?;
    record_head_audit(&state, &admin.0.id, "activation_apply", &result)?;
    Ok(Json(result))
}

async fn import_revocation(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(envelope): Json<AtsCertificationAuthorityEnvelope>,
) -> Result<Json<AtsCertificationImportResult>, ApiError> {
    let result = jobs::import_ats_certification_revocation(&state.pool, &envelope, &admin.0.id)
        .map_err(authority_api_error)?;
    record_import_audit(&state, &admin.0.id, "revocation", &result)?;
    Ok(Json(result))
}

async fn import_canary_allowlist(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<AtsCertificationCanaryAllowlistImportRequest>,
) -> Result<Json<AtsCertificationCanaryAllowlistImportResult>, ApiError> {
    let result = jobs::import_ats_certification_canary_allowlist(
        &state.pool,
        &request,
        &admin.0.id,
        jobs::now_ms(),
    )
    .map_err(authority_api_error)?;
    record_audit(
        &state,
        &admin.0.id,
        "canary_allowlist_import",
        serde_json::json!({
            "allowlistSha256": result.allowlist_sha256,
            "memberCount": result.member_count,
            "replayed": result.replayed,
        }),
    )?;
    Ok(Json(result))
}

async fn revoke_canary_allowlist(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<AtsCertificationCanaryAllowlistRevocationRequest>,
) -> Result<Json<AtsCertificationCanaryAllowlistRevocationResult>, ApiError> {
    let result = jobs::revoke_ats_certification_canary_allowlist(
        &state.pool,
        &request,
        &admin.0.id,
        jobs::now_ms(),
    )
    .map_err(authority_api_error)?;
    record_audit(
        &state,
        &admin.0.id,
        "canary_allowlist_revoke",
        serde_json::json!({
            "allowlistSha256": result.allowlist_sha256,
            "replayed": result.replayed,
        }),
    )?;
    Ok(Json(result))
}

async fn transition_circuit(
    State(state): State<AppState>,
    Extension(admin): Extension<AuthedAccount>,
    Json(request): Json<AtsCertificationCircuitRequest>,
) -> Result<Json<AtsCertificationCircuitState>, ApiError> {
    let result = jobs::append_ats_certification_circuit_event(
        &state.pool,
        &request.event(),
        request.expected_head_revision,
        request.expected_event_id.as_deref(),
        &admin.0.id,
    )
    .map_err(authority_api_error)?;
    record_circuit_audit(&state, &admin.0.id, &result)?;
    Ok(Json(result))
}

async fn target_status(
    State(state): State<AppState>,
    Path(target_key): Path<String>,
    Query(query): Query<AtsCertificationTargetStatusQuery>,
) -> Result<Json<AtsCertificationTargetStatusProjection>, ApiError> {
    if query.discovery_target_key != target_key || query.original_source_target_key != target_key {
        return Err(authority_api_error(
            AtsCertificationAuthorityError::NotFound,
        ));
    }
    let projection = jobs::get_ats_certification_target_status_projection(
        &state.pool,
        &query.target_evidence(),
        query.runner_target_sha256.as_deref(),
        &query.channel,
        query.expected_account_allowlist_sha256.as_deref(),
        jobs::now_ms(),
    )
    .map_err(authority_api_error)?;
    Ok(Json(projection))
}

fn record_import_audit(
    state: &AppState,
    actor_id: &str,
    operation: &str,
    result: &AtsCertificationImportResult,
) -> Result<(), ApiError> {
    record_audit(state, actor_id, operation, import_audit_metadata(result))
}

fn import_audit_metadata(result: &AtsCertificationImportResult) -> serde_json::Value {
    serde_json::json!({
        "authorityKind": result.authority_kind,
        "authoritySha256": result.authority_sha256,
        "replayed": result.replayed,
    })
}

fn record_manifest_aggregate_audit(
    state: &AppState,
    actor_id: &str,
    result: &AtsCertificationManifestAggregateImportResult,
) -> Result<(), ApiError> {
    record_audit(
        state,
        actor_id,
        "manifest_import",
        manifest_aggregate_audit_metadata(result),
    )
}

fn manifest_aggregate_audit_metadata(
    result: &AtsCertificationManifestAggregateImportResult,
) -> serde_json::Value {
    let replayed_evidence_count = result
        .evidence
        .iter()
        .filter(|evidence| evidence.replayed)
        .count();
    serde_json::json!({
        "manifestSha256": result.manifest.authority_sha256,
        "manifestReplayed": result.manifest.replayed,
        "evidenceCount": result.evidence.len(),
        "replayedEvidenceCount": replayed_evidence_count,
    })
}

fn record_head_audit(
    state: &AppState,
    actor_id: &str,
    operation: &str,
    result: &AtsCertificationHeadResult,
) -> Result<(), ApiError> {
    record_audit(
        state,
        actor_id,
        operation,
        serde_json::json!({
            "activationSha256": result.activation_sha256,
            "headRevision": result.head_revision,
            "transitionSha256": result.transition_sha256,
            "replayed": result.replayed,
        }),
    )
}

fn record_circuit_audit(
    state: &AppState,
    actor_id: &str,
    result: &AtsCertificationCircuitState,
) -> Result<(), ApiError> {
    record_audit(
        state,
        actor_id,
        "circuit_transition",
        serde_json::json!({
            "eventSha256": result.current_event_sha256,
            "headRevision": result.head_revision,
            "scopeKind": result.scope_kind,
            "state": result.state,
            "replayed": result.replayed,
        }),
    )
}

fn record_audit(
    state: &AppState,
    actor_id: &str,
    operation: &str,
    metadata: serde_json::Value,
) -> Result<(), ApiError> {
    // The authority transaction already records the exact actor on its
    // immutable row. Do not acknowledge that mutation to an API caller until
    // the redacted operations audit is durable as well; an exact replay can
    // safely repair a transient audit failure without widening authority.
    ops_audit::record_event(
        &state.pool,
        ops_audit::OpsAuditEventInput {
            account_id_hash: None,
            actor_account_id_hash: Some(audit_actor_hash(actor_id)),
            event_type: format!("jobs_ats_certification_{operation}"),
            status: "success".to_string(),
            metadata_json: metadata,
        },
    )
    .map_err(|error| {
        tracing::error!(error = %error, operation, "ATS certification audit event failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The ATS certification authority operation failed.".to_string(),
        )
    })
}

fn audit_actor_hash(actor_id: &str) -> String {
    hex::encode(Sha256::digest(actor_id.as_bytes()))
}

fn authority_api_error(error: AtsCertificationAuthorityError) -> ApiError {
    match error {
        AtsCertificationAuthorityError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The ATS certification authority operation failed.".to_string(),
        ),
        AtsCertificationAuthorityError::NotFound
        | AtsCertificationAuthorityError::TrustPolicyNotInitialized => (
            StatusCode::NOT_FOUND,
            "The ATS certification authority was not found.".to_string(),
        ),
        AtsCertificationAuthorityError::IdentityConflict
        | AtsCertificationAuthorityError::CompareAndSwapConflict
        | AtsCertificationAuthorityError::SequenceRegression
        | AtsCertificationAuthorityError::Expired
        | AtsCertificationAuthorityError::Revoked
        | AtsCertificationAuthorityError::Quarantined
        | AtsCertificationAuthorityError::CircuitOpen
        | AtsCertificationAuthorityError::CapacityUnavailable => (
            StatusCode::CONFLICT,
            "The ATS certification authority conflicts with current state.".to_string(),
        ),
        _ => (
            StatusCode::BAD_REQUEST,
            "The ATS certification authority request is invalid.".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn import_result(kind: &str, id: &str, digest_byte: &str) -> AtsCertificationImportResult {
        AtsCertificationImportResult {
            authority_kind: kind.to_string(),
            authority_id: id.to_string(),
            authority_sha256: digest_byte.repeat(32),
            authorization_sha256: "cd".repeat(32),
            trust_policy_sha256: "ef".repeat(32),
            replayed: false,
        }
    }

    #[test]
    fn circuit_request_is_strict_and_preserves_compare_and_swap_fields() {
        let value = json!({
            "eventId": "event-1",
            "scopeKind": "target",
            "subjectKey": "greenhouse:acme",
            "transition": "opened",
            "triggerKind": "layout_drift",
            "windowStartedAtMs": 10,
            "windowEndedAtMs": 20,
            "failureCount": 1,
            "sampleCount": 1,
            "thresholdCount": 1,
            "authorityRef": "incident-1",
            "eventAtMs": 20,
            "expectedHeadRevision": 0,
            "expectedEventId": null,
        });
        let request =
            serde_json::from_value::<AtsCertificationCircuitRequest>(value.clone()).unwrap();
        assert_eq!(request.event().event_id, "event-1");
        assert_eq!(request.expected_head_revision, 0);

        let mut extra = value;
        extra["unexpected"] = json!(true);
        assert!(serde_json::from_value::<AtsCertificationCircuitRequest>(extra).is_err());
    }

    #[test]
    fn authority_errors_do_not_disclose_internal_identity() {
        assert_eq!(
            authority_api_error(AtsCertificationAuthorityError::InvalidSignature).0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            authority_api_error(AtsCertificationAuthorityError::NotFound).0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            authority_api_error(AtsCertificationAuthorityError::CircuitOpen).0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            authority_api_error(AtsCertificationAuthorityError::Storage(anyhow::anyhow!(
                "secret storage detail"
            )))
            .1,
            "The ATS certification authority operation failed."
        );
    }

    #[test]
    fn target_status_query_is_strict_and_path_binding_is_available() {
        let value = json!({
            "canonicalUrl": "https://boards.greenhouse.io/acme/jobs/123",
            "discoveryProvider": "greenhouse",
            "discoveryTargetKey": "greenhouse:acme:123",
            "discoveryObservedAtMs": 100,
            "originalSourceProvider": "greenhouse",
            "originalSourceTargetKey": "greenhouse:acme:123",
            "originalSourceObservedAtMs": 101,
            "runnerTargetSha256": null,
            "channel": "general",
            "expectedAccountAllowlistSha256": null,
        });
        let query =
            serde_json::from_value::<AtsCertificationTargetStatusQuery>(value.clone()).unwrap();
        assert_eq!(
            query.target_evidence().original_source_target_key,
            "greenhouse:acme:123"
        );

        let mut extra = value;
        extra["accountId"] = json!("must-not-be-accepted");
        assert!(serde_json::from_value::<AtsCertificationTargetStatusQuery>(extra).is_err());
    }

    #[test]
    fn canary_allowlist_requests_are_strict_and_do_not_accept_actor_fields() {
        let import = json!({
            "schemaVersion": 1,
            "allowlistId": "allowlist-1",
            "accountIds": ["account-1", "account-2"],
            "approvalRef": "approval-1",
            "notBeforeMs": 100,
            "expiresAtMs": 200,
        });
        let parsed =
            serde_json::from_value::<AtsCertificationCanaryAllowlistImportRequest>(import.clone())
                .unwrap();
        assert_eq!(parsed.account_ids.len(), 2);

        let mut import_with_actor = import;
        import_with_actor["approvedBy"] = json!("customer-controlled");
        assert!(
            serde_json::from_value::<AtsCertificationCanaryAllowlistImportRequest>(
                import_with_actor,
            )
            .is_err()
        );

        let revocation = json!({
            "allowlistSha256": "ab".repeat(32),
            "revocationRef": "incident-1",
        });
        assert!(
            serde_json::from_value::<AtsCertificationCanaryAllowlistRevocationRequest>(
                revocation.clone(),
            )
            .is_ok()
        );
        let mut revocation_with_actor = revocation;
        revocation_with_actor["revokedBy"] = json!("customer-controlled");
        assert!(
            serde_json::from_value::<AtsCertificationCanaryAllowlistRevocationRequest>(
                revocation_with_actor,
            )
            .is_err()
        );
    }

    #[test]
    fn target_status_projection_omits_internal_target_evidence_and_account_fields() {
        let status = AtsCertificationTargetStatusProjection {
            schema_version: 1,
            provider: "greenhouse".to_string(),
            target_key_sha256: "01".repeat(32),
            status: "active".to_string(),
            adapter_version: Some("2026.07.1-beta.1".to_string()),
            manifest_sha256: Some("02".repeat(32)),
            activation_sha256: Some("03".repeat(32)),
            activation_generation: Some(1),
            layout_set_sha256: Some("04".repeat(32)),
            rollout_channel: Some("general".to_string()),
            runner_kinds: vec!["local".to_string()],
            runner_target_sha256s: vec!["05".repeat(32)],
            expires_at_ms: Some(200),
            last_verified_at_ms: Some(150),
            canary_available: false,
        };
        let value = serde_json::to_value(status).unwrap();
        assert_eq!(value["status"], "active");
        assert_eq!(value["provider"], "greenhouse");
        assert_eq!(value["runnerKinds"][0], "local");
        for forbidden in [
            "targetKey",
            "scopeSha256",
            "certificationId",
            "evidenceSha256s",
            "layoutObservationSha256s",
            "accountAllowlistSha256",
            "runtimeId",
        ] {
            assert!(value.get(forbidden).is_none(), "unexpected {forbidden}");
        }
        let encoded = value.to_string();
        assert!(!encoded.contains("private-tenant"));
        assert!(!encoded.contains("private-job"));
        assert!(!encoded.contains("private-runtime-id"));
    }

    #[test]
    fn audit_fields_are_digest_only_and_do_not_include_actor_identity() {
        let result = import_result("manifest", "private-manifest-id", "ab");
        assert_eq!(
            import_audit_metadata(&result),
            json!({
                "authorityKind": "manifest",
                "authoritySha256": "ab".repeat(32),
                "replayed": false,
            })
        );
        let actor = "private-admin-account-id";
        let actor_hash = audit_actor_hash(actor);
        assert_eq!(actor_hash.len(), 64);
        assert_ne!(actor_hash, actor);
        assert!(!import_audit_metadata(&result).to_string().contains(actor));
        assert!(!import_audit_metadata(&result)
            .to_string()
            .contains("private-manifest-id"));

        let aggregate = AtsCertificationManifestAggregateImportResult {
            manifest: result,
            evidence: vec![import_result(
                "evidence",
                "private-evidence-object-key",
                "12",
            )],
        };
        let aggregate_metadata = manifest_aggregate_audit_metadata(&aggregate);
        assert_eq!(aggregate_metadata["evidenceCount"], 1);
        assert!(!aggregate_metadata
            .to_string()
            .contains("private-evidence-object-key"));
    }
}
