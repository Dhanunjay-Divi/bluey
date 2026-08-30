const MANAGED_CLOUD_RELEASE_AUDIENCE: &str = "bluey-jobs-managed-cloud-release-v1";
const MANAGED_CLOUD_ACTIVATION_AUDIENCE: &str = "bluey-jobs-managed-cloud-activation-v1";
const MANAGED_CLOUD_COHORT_AUDIENCE: &str = "bluey-jobs-managed-cloud-cohort-v1";
const MANAGED_CLOUD_ROLLBACK_AUDIENCE: &str = "bluey-jobs-managed-cloud-rollback-v1";
const MANAGED_CLOUD_REVOCATION_AUDIENCE: &str = "bluey-jobs-managed-cloud-revocation-v1";
const MANAGED_CLOUD_TRUST_POLICY_AUDIENCE: &str = "bluey-jobs-managed-cloud-trust-policy-v1";
const MANAGED_CLOUD_SIGNATURE_SET_AUDIENCE: &str = "bluey-jobs-managed-cloud-signature-set-v1";
const MANAGED_CLOUD_TRANSITION_AUDIENCE: &str = "bluey-jobs-managed-cloud-head-transition-v1";
const MANAGED_CLOUD_READINESS_AUDIENCE: &str = "bluey-jobs-managed-cloud-readiness-v1";
const MANAGED_CLOUD_BINDING_AUDIENCE: &str = "bluey-jobs-managed-cloud-workflow-binding-v1";
const MANAGED_CLOUD_RECOVERY_AUTHORIZATION_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-recovery-authorization-v1";
const MANAGED_CLOUD_EXECUTION_LEASE_AUTHORITY_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-execution-lease-authority-v1";
const MANAGED_CLOUD_IRREVERSIBLE_EFFECT_RECEIPT_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-irreversible-effect-receipt-v1";
const MANAGED_CLOUD_ROOT_ANCHOR_AUDIENCE: &str = "bluey-jobs-managed-cloud-root-anchor-v1";
const MANAGED_CLOUD_COMPONENT_INVENTORY_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-component-inventory-v1";
const MANAGED_CLOUD_VERIFICATION_EVIDENCE_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-verification-evidence-v1";
const MANAGED_CLOUD_CANARY_EVIDENCE_AUDIENCE: &str = "bluey-jobs-managed-cloud-canary-evidence-v1";
const MANAGED_CLOUD_CLEANUP_EVIDENCE_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-cleanup-evidence-v1";
const MANAGED_CLOUD_FAILURE_CONVERTER_EVIDENCE_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-failure-converter-evidence-v1";
const MANAGED_CLOUD_PORTAL_READBACK_EVIDENCE_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-portal-readback-evidence-v1";
const MANAGED_CLOUD_RUNNER_FLEET_EVIDENCE_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-runner-fleet-evidence-v1";
const MANAGED_CLOUD_STORAGE_EVIDENCE_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-storage-evidence-v1";
const MANAGED_CLOUD_TASK_QUEUE_EVIDENCE_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-task-queue-evidence-v1";
const MANAGED_CLOUD_TEMPORAL_EVIDENCE_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-temporal-evidence-v1";
const MANAGED_CLOUD_ROOT_TRUST_ANCHOR_ENV: &str = "BLUEY_JOBS_MANAGED_CLOUD_ROOT_TRUST_ANCHOR_JSON";
const MANAGED_CLOUD_ROOT_TRUST_ANCHOR_SHA256_ENV: &str =
    "BLUEY_JOBS_MANAGED_CLOUD_ROOT_TRUST_ANCHOR_SHA256";
const MANAGED_CLOUD_SQLITE_MIGRATION_HEAD: &str = "055_jobs_managed_cloud_release_authority.sql";
const MANAGED_CLOUD_POSTGRES_MIGRATION_HEAD: &str = "033_jobs_managed_cloud_release_authority.sql";
const MANAGED_CLOUD_MAX_ENVELOPE_BYTES: usize = 128 * 1024;
const MANAGED_CLOUD_MAX_CONTENT_INVENTORY_BYTES: usize = 12 * 1024 * 1024;
const MANAGED_CLOUD_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const MANAGED_CLOUD_MIN_GRANT_TTL_MS: i64 = 5_000;
const MANAGED_CLOUD_MAX_GRANT_TTL_MS: i64 = 24 * 60 * 60 * 1_000;
const MANAGED_CLOUD_RUNTIME_SESSION_DOMAIN: &[u8] =
    b"bluey-jobs-managed-cloud-runtime-session-v1\0";
const MANAGED_CLOUD_RUNTIME_SESSION_PROOF_DOMAIN: &[u8] =
    b"bluey-jobs-managed-cloud-runtime-session-proof-v1\0";
const MANAGED_CLOUD_RUNTIME_GRANT_TOKEN_DOMAIN: &str =
    "bluey-jobs-managed-cloud-runtime-grant-token-v1";
const MANAGED_CLOUD_TASK_QUEUE_DOMAIN: &[u8] = b"bluey-jobs-managed-cloud-task-queue-v1\0";
const MANAGED_CLOUD_DEPENDENCY_EVIDENCE_DOMAIN: &[u8] =
    b"bluey-jobs-managed-cloud-dependency-evidence-v1\0";
const MANAGED_CLOUD_RUNTIME_IDENTITY_DOMAIN: &[u8] =
    b"bluey-jobs-managed-cloud-runtime-identity-v1\0";
const MANAGED_CLOUD_RUNTIME_MEASUREMENT_AUDIENCE: &str =
    "bluey-jobs-managed-cloud-runtime-measurement-v1";
const MANAGED_CLOUD_RUNTIME_MEASUREMENT_PATH: &str =
    "app/.bluey/managed-cloud-runtime-measurement.json";
const MANAGED_CLOUD_MAX_RUNTIME_MEASUREMENT_FILES: usize = 512;
const MANAGED_CLOUD_RUNNER_NODE_EXECUTABLE: &str = "/usr/local/bin/node";
const MANAGED_CLOUD_RUNNER_NODE_INVENTORY_PATH: &str = "usr/local/bin/node";
const MANAGED_CLOUD_WORKFLOWS_NODE_EXECUTABLE: &str = "/usr/local/bin/node";
const MANAGED_CLOUD_WORKFLOWS_NODE_INVENTORY_PATH: &str = "usr/local/bin/node";
const MANAGED_CLOUD_COHORT_MEMBER_DOMAIN: &[u8] = b"bluey-jobs-managed-cloud-cohort-member-v1\0";
const MANAGED_CLOUD_HEARTBEAT_AUDIT_LIMIT: i64 = 64;

fn managed_cloud_runtime_measurement_file_count_valid(count: usize) -> bool {
    (1..=MANAGED_CLOUD_MAX_RUNTIME_MEASUREMENT_FILES).contains(&count)
}

fn managed_cloud_path_within_runtime_root(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn managed_cloud_runtime_measurement_inventory_path(component_id: &str, path: &str) -> bool {
    match component_id {
        "jobs-api" => path == "usr/local/bin/bluey-jobs-api",
        "jobs-runner" => {
            managed_cloud_path_within_runtime_root(path, "app/automation")
                || managed_cloud_path_within_runtime_root(path, "app/runner")
                || managed_cloud_path_within_runtime_root(path, "ms-playwright")
                || path == MANAGED_CLOUD_RUNNER_NODE_INVENTORY_PATH
        }
        "jobs-workflows" => {
            managed_cloud_path_within_runtime_root(path, "app/automation")
                || managed_cloud_path_within_runtime_root(path, "app/workflows")
                || path == MANAGED_CLOUD_WORKFLOWS_NODE_INVENTORY_PATH
        }
        _ => false,
    }
}

const MANAGED_CLOUD_BASE_RUNTIME_ROLES: [&str; 6] = [
    "jobs_api",
    "managed_runner",
    "workflow_cleanup_dispatcher",
    "workflow_command_dispatcher",
    "workflow_gateway",
    "workflow_worker",
];

const MANAGED_CLOUD_ALL_RUNTIME_ROLES: [&str; 9] = [
    "discovery_worker",
    "global_discovery_worker",
    "jobs_api",
    "managed_runner",
    "original_source_verifier",
    "workflow_cleanup_dispatcher",
    "workflow_command_dispatcher",
    "workflow_gateway",
    "workflow_worker",
];

const MANAGED_CLOUD_PROTOCOLS: [(&str, i64); 11] = [
    ("ats_certification", 1),
    ("execution_lease", 1),
    ("gateway_command", 3),
    ("managed_cloud_release", 1),
    ("object_evidence", 1),
    ("runner_checkpoint", 2),
    ("runner_profile_snapshot", 1),
    ("runner_result", 2),
    ("runtime_heartbeat", 1),
    ("workflow_cleanup", 3),
    ("workflow_command", 2),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudScope {
    pub environment: String,
    pub region: String,
    pub channel: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudAuthorityEnvelope {
    pub canonical_base64url: String,
    pub signature_set_base64url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudReleaseImportRequest {
    pub envelope: ManagedCloudAuthorityEnvelope,
    pub verification_evidence_base64url: String,
    pub inventory_attachments: ManagedCloudContentInventoryAttachments,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedCloudContentInventoryAttachments {
    #[serde(rename = "jobs-api")]
    pub jobs_api: String,
    #[serde(rename = "jobs-portal")]
    pub jobs_portal: String,
    #[serde(rename = "jobs-runner")]
    pub jobs_runner: String,
    #[serde(rename = "jobs-workflows")]
    pub jobs_workflows: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudCohortImportRequest {
    pub envelope: ManagedCloudAuthorityEnvelope,
    pub account_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudActivationImportRequest {
    pub envelope: ManagedCloudAuthorityEnvelope,
    pub evidence: ManagedCloudActivationEvidenceAttachments,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudActivationEvidenceAttachments {
    pub canary_base64url: String,
    pub cleanup_base64url: String,
    pub failure_converter_base64url: String,
    pub portal_readback_base64url: String,
    pub runner_fleet_base64url: String,
    pub storage_base64url: String,
    pub task_queue_base64url: String,
    pub temporal_namespace_base64url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudRollbackImportRequest {
    pub envelope: ManagedCloudAuthorityEnvelope,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudRevocationImportRequest {
    pub envelope: ManagedCloudAuthorityEnvelope,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyManagedCloudActivationRequest {
    pub activation_sha256: String,
    pub expected_head_revision: i64,
    pub expected_transition_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyManagedCloudRollbackRequest {
    pub rollback_sha256: String,
    pub expected_head_revision: i64,
    pub expected_transition_sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudHeadTransition {
    pub scope: ManagedCloudScope,
    pub head_revision: i64,
    pub transition_sha256: String,
    pub activation_sha256: String,
    pub manifest_sha256: String,
    pub trust_generation: i64,
    pub channel_sequence: i64,
    pub transition_kind: String,
    pub authority_sha256: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudImportResult {
    pub authority_kind: String,
    pub authority_id: String,
    pub authority_sha256: String,
    pub signature_set_sha256: String,
    pub trust_policy_sha256: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudReleaseStatus {
    pub scope: ManagedCloudScope,
    pub authority_ready: bool,
    pub customer_admission: bool,
    pub unavailability_reason: Option<String>,
    pub head_revision: i64,
    pub transition_sha256: Option<String>,
    pub activation_sha256: Option<String>,
    pub manifest_sha256: Option<String>,
    pub cohort_sha256: Option<String>,
    pub trust_generation: Option<i64>,
    pub channel_sequence: Option<i64>,
    pub release_id: Option<String>,
    pub release_sequence: Option<i64>,
    pub task_queue_sha256: Option<String>,
    pub failure_converter_sha256: Option<String>,
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudReadinessQuery {
    pub scope: ManagedCloudScope,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudReadiness {
    pub status: ManagedCloudReleaseStatus,
    pub readiness_sha256: String,
    pub evaluated_at_ms: i64,
    pub missing_roles: Vec<String>,
    pub stale_roles: Vec<String>,
    pub cohort_eligible: bool,
}

#[derive(Debug, Clone)]
struct ManagedCloudResolvedHead {
    scope: ManagedCloudScope,
    head_revision: i64,
    transition_sha256: String,
    activation_sha256: String,
    manifest_sha256: String,
    cohort_sha256: String,
    trust_generation: i64,
    channel_sequence: i64,
    release_id: String,
    release_sequence: i64,
    task_queue_sha256: String,
    failure_converter_sha256: String,
    activation_expires_at_ms: i64,
}

#[derive(Debug, Clone)]
struct ManagedCloudRequiredRuntime {
    role: String,
    component_id: String,
    dependency_evidence_sha256: String,
    heartbeat_ttl_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedCloudReadinessDigestAuthority<'a> {
    version: i64,
    audience: &'a str,
    status: &'a ManagedCloudReleaseStatus,
    evaluated_at_ms: i64,
    missing_roles: &'a [String],
    stale_roles: &'a [String],
    cohort_eligible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewManagedCloudRuntimeGrant {
    pub issuance_ref: String,
    pub scope: ManagedCloudScope,
    pub activation_sha256: String,
    pub manifest_sha256: String,
    pub component_id: String,
    pub role: String,
    pub expected_worker_id: String,
    pub authorization_ref: String,
    pub created_by: String,
    pub ttl_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudRuntimeGrant {
    pub grant_id: String,
    pub grant_token: Option<String>,
    pub token_sha256: String,
    pub issuance_ref: String,
    pub scope: ManagedCloudScope,
    pub activation_sha256: String,
    pub manifest_sha256: String,
    pub component_id: String,
    pub role: String,
    pub head_revision: i64,
    pub transition_sha256: String,
    pub artifact_sha256: String,
    pub config_schema_sha256: String,
    pub migration_set_sha256: String,
    pub protocol_set_sha256: String,
    pub task_queue_sha256: String,
    pub failure_converter_sha256: String,
    pub dependency_evidence_sha256: String,
    pub expected_runtime_identity_sha256: String,
    pub expected_worker_id: String,
    pub authorization_ref: String,
    pub created_by: String,
    pub activation_expires_at_ms: i64,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeManagedCloudRuntimeGrant {
    pub grant_id: String,
    pub reason_ref: String,
    pub revoked_by: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudRuntimeGrantRevocation {
    pub grant_id: String,
    pub reason_ref: String,
    pub revoked_by: String,
    pub revoked_at_ms: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimManagedCloudRuntimeGrant {
    pub grant_id: String,
    pub grant_token: String,
    pub runtime_instance_id: String,
    pub worker_id: String,
    pub session_token: String,
    pub runtime_identity_sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudRuntimeInstance {
    pub grant_id: String,
    pub runtime_instance_id: String,
    pub runtime_identity_sha256: String,
    pub worker_id: String,
    pub scope: ManagedCloudScope,
    pub activation_sha256: String,
    pub manifest_sha256: String,
    pub component_id: String,
    pub role: String,
    pub head_revision: i64,
    pub transition_sha256: String,
    pub artifact_sha256: String,
    pub config_schema_sha256: String,
    pub migration_set_sha256: String,
    pub protocol_set_sha256: String,
    pub task_queue_sha256: String,
    pub failure_converter_sha256: String,
    pub dependency_evidence_sha256: String,
    pub activation_expires_at_ms: i64,
    pub instance_epoch: i64,
    pub next_heartbeat_sequence: i64,
    pub claimed_at_ms: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudRuntimeHeartbeatInput {
    pub runtime_instance_id: String,
    pub worker_id: String,
    pub session_token: String,
    pub heartbeat_sequence: i64,
    pub observed_head_revision: i64,
    pub observed_transition_sha256: String,
    pub activation_sha256: String,
    pub manifest_sha256: String,
    pub component_id: String,
    pub role: String,
    pub artifact_sha256: String,
    pub migration_set_sha256: String,
    pub config_schema_sha256: String,
    pub protocol_set_sha256: String,
    pub task_queue_sha256: String,
    pub failure_converter_sha256: String,
    pub dependency_evidence_sha256: String,
    pub health_state: String,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudRuntimeHeartbeat {
    pub runtime_instance_id: String,
    pub worker_id: String,
    pub instance_epoch: i64,
    pub heartbeat_sequence: i64,
    pub observed_head_revision: i64,
    pub observed_transition_sha256: String,
    pub activation_sha256: String,
    pub manifest_sha256: String,
    pub component_id: String,
    pub role: String,
    pub artifact_sha256: String,
    pub migration_set_sha256: String,
    pub config_schema_sha256: String,
    pub protocol_set_sha256: String,
    pub task_queue_sha256: String,
    pub failure_converter_sha256: String,
    pub dependency_evidence_sha256: String,
    pub health_state: String,
    pub reason_code: Option<String>,
    pub heartbeat_at_ms: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudRuntimeMeasurementIdentity {
    pub runtime_measurement_sha256: String,
    pub runtime_identity_sha256: String,
    pub config_schema_sha256: String,
    pub migration_set_sha256: String,
    pub protocol_set_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudAdmissionAuthority {
    pub scope: ManagedCloudScope,
    pub head_revision: i64,
    pub transition_sha256: String,
    pub activation_sha256: String,
    pub manifest_sha256: String,
    pub cohort_sha256: String,
    pub trust_generation: i64,
    pub channel_sequence: i64,
    pub release_id: String,
    pub release_sequence: i64,
    pub task_queue_sha256: String,
    pub failure_converter_sha256: String,
    pub readiness_sha256: String,
    pub activation_expires_at_ms: i64,
    pub resolved_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudWorkflowBindingInput {
    pub command_id: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub workflow_id: String,
    pub scope: ManagedCloudScope,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudWorkflowBinding {
    pub command_id: String,
    pub binding_sha256: String,
    pub release_memo_base64url: String,
    pub release_memo_sha256: String,
    pub admission: ManagedCloudAdmissionAuthority,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudExecutionAuthority {
    pub binding_sha256: String,
    #[serde(flatten)]
    pub admission: ManagedCloudAdmissionAuthority,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudReleaseMemoAuthority {
    pub version: i64,
    #[serde(flatten)]
    pub execution: ManagedCloudExecutionAuthority,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudCurrentAuthorization {
    pub current_head_revision: i64,
    pub current_transition_sha256: String,
    pub current_activation_sha256: String,
    pub current_manifest_sha256: String,
    pub current_activation_expires_at_ms: i64,
    pub current_task_queue_sha256: String,
    pub current_failure_converter_sha256: String,
    pub current_readiness_sha256: String,
    pub recovery_accepted: bool,
    pub recovery_authorization_sha256: String,
    pub authorized_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudGatewayAuthority {
    pub version: i64,
    pub execution: ManagedCloudExecutionAuthority,
    pub authorization: ManagedCloudCurrentAuthorization,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudRequestStartAuthority {
    pub binding: ManagedCloudWorkflowBinding,
    pub managed_cloud: ManagedCloudGatewayAuthority,
    pub replayed: bool,
    pub reconcile_only: bool,
    pub attempt_replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedCloudExecutionLeaseClaimInput {
    pub workflow_request_id: String,
    pub managed_cloud_release: ManagedCloudReleaseMemoAuthority,
    pub managed_cloud_release_sha256: String,
    pub managed_cloud_runtime_instance_id: String,
    pub managed_cloud_runtime_instance_epoch: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCloudExecutionLeaseAuthority {
    pub managed_cloud: ManagedCloudGatewayAuthority,
    pub managed_cloud_workflow_request_id: String,
    #[serde(skip_serializing)]
    pub request_command_id: String,
    #[serde(skip_serializing)]
    pub execution_command_id: String,
    #[serde(skip_serializing)]
    pub binding_sha256: String,
    #[serde(skip_serializing)]
    pub release_memo_base64url: String,
    #[serde(skip_serializing)]
    pub release_sha256: String,
    pub managed_cloud_runtime_instance_id: String,
    pub managed_cloud_runtime_instance_epoch: i64,
    pub managed_cloud_worker_id: String,
    #[serde(skip_serializing)]
    pub gateway_authority_base64url: String,
    #[serde(skip_serializing)]
    pub gateway_authority_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundManagedCloudExecutionLeaseAuthority {
    pub authority: ManagedCloudExecutionLeaseAuthority,
    pub lease_authority_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedCloudIrreversibleEffectReceipt {
    pub authority: ManagedCloudExecutionLeaseAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedCloudRequestStartPreflight {
    Historical,
    FreshEffect,
    Reconcile,
}

#[derive(Debug, Error)]
pub enum ManagedCloudRegistryError {
    #[error("invalid managed cloud authority envelope")]
    InvalidEnvelope,
    #[error("invalid managed cloud authority")]
    InvalidAuthority,
    #[error("invalid managed cloud request")]
    InvalidRequest,
    #[error("managed cloud authority was not found")]
    NotFound,
    #[error("managed cloud identity conflicts with stored authority")]
    IdentityConflict,
    #[error("managed cloud compare-and-swap failed")]
    CompareAndSwapConflict,
    #[error("managed cloud sequence regressed")]
    SequenceRegression,
    #[error("managed cloud downgrade requires rollback authority")]
    DowngradeRequiresRollback,
    #[error("managed cloud authority is revoked")]
    Revoked,
    #[error("managed cloud authority is unavailable")]
    Unavailable,
    #[error("account is not eligible for the managed cloud cohort")]
    CohortIneligible,
    #[error("managed cloud runtime grant expired")]
    GrantExpired,
    #[error("managed cloud runtime grant was already consumed")]
    GrantConsumed,
    #[error("managed cloud heartbeat sequence conflicts")]
    HeartbeatSequenceConflict,
    #[error("managed cloud recovery release is not accepted")]
    RecoveryNotAccepted,
    #[error("managed cloud storage failed: {0}")]
    Storage(#[source] anyhow::Error),
}

pub type ManagedCloudResult<T> = std::result::Result<T, ManagedCloudRegistryError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudArtifactAuthority {
    component_id: String,
    artifact_kind: String,
    artifact_ref: String,
    artifact_sha256: String,
    build_id: String,
    source_commit: String,
    platform: String,
    architecture: String,
    sbom_sha256: String,
    provenance_sha256: String,
    config_schema_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudCapabilityAuthority {
    component_id: String,
    capability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudProtocolAuthority {
    protocol_id: String,
    protocol_version: i64,
    schema_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudVerificationComponentAuthority {
    component_id: String,
    artifact_sha256: String,
    content_inventory_sha256: String,
    sbom_sha256: String,
    provenance_sha256: String,
    runtime_path: Option<String>,
    runtime_user: String,
    entrypoint: Vec<String>,
    cmd: Vec<String>,
    required_paths: Vec<String>,
    runtime_measurement: Option<ManagedCloudRuntimeMeasurementAuthority>,
    runtime_measurement_sha256: Option<String>,
    runtime_identities: Vec<ManagedCloudRuntimeIdentityAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudRuntimeMeasurementFileAuthority {
    path: String,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudRuntimeMeasurementAuthority {
    version: i64,
    audience: String,
    build_id: String,
    component_id: String,
    config_schema_sha256: String,
    measured_files: Vec<ManagedCloudRuntimeMeasurementFileAuthority>,
    migration_set_sha256: String,
    protocol_set_sha256: String,
    roles: Vec<String>,
    source_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudRuntimeIdentityAuthority {
    role: String,
    runtime_identity_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudVerificationEvidenceAuthority {
    version: i64,
    audience: String,
    source_commit: String,
    builder_policy_sha256: String,
    jobs_lock_sha256: String,
    server_lock_sha256: String,
    migration_contract_sha256: String,
    config_contract_sha256: String,
    protocol_contract_sha256: String,
    test_evidence_sha256: String,
    components: Vec<ManagedCloudVerificationComponentAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudContentInventoryEntry {
    mode: String,
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolved_target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size_bytes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(rename = "type")]
    entry_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudContentInventoryRuntime {
    cmd: Vec<String>,
    environment: Vec<String>,
    entrypoint: Vec<String>,
    exposed_ports: Vec<String>,
    user: String,
    working_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudContentInventoryAuthority {
    version: i64,
    audience: String,
    component_id: String,
    artifact_kind: String,
    artifact_sha256: String,
    entries: Vec<ManagedCloudContentInventoryEntry>,
    runtime: Option<ManagedCloudContentInventoryRuntime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudFeatureAuthority {
    cloud_distribution: bool,
    workflow_command_dispatch: bool,
    workflow_cleanup: bool,
    direct_discovery: bool,
    global_discovery: bool,
    source_verification: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudReleaseAuthority {
    version: i64,
    audience: String,
    manifest_id: String,
    manifest_generation: i64,
    release_id: String,
    release_sequence: i64,
    source_commit: String,
    published_at_ms: i64,
    sqlite_migration_head: String,
    postgres_migration_head: String,
    migration_set_sha256: String,
    config_schema_sha256: String,
    protocol_set_sha256: String,
    component_set_sha256: String,
    feature_authority_sha256: String,
    feature_authority: ManagedCloudFeatureAuthority,
    verification_evidence_sha256: String,
    components: Vec<ManagedCloudArtifactAuthority>,
    capabilities: Vec<ManagedCloudCapabilityAuthority>,
    protocols: Vec<ManagedCloudProtocolAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudRecoveryAcceptanceAuthority {
    activation_sha256: String,
    manifest_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudActivationAuthority {
    version: i64,
    audience: String,
    activation_id: String,
    activation_generation: i64,
    trust_generation: i64,
    scope: ManagedCloudScope,
    channel_sequence: i64,
    expected_head_revision: i64,
    expected_transition_sha256: Option<String>,
    predecessor_activation_sha256: Option<String>,
    manifest_sha256: String,
    manifest_signature_set_sha256: String,
    cohort_sha256: String,
    cohort_signature_set_sha256: String,
    feature_authority_sha256: String,
    feature_authority: ManagedCloudFeatureAuthority,
    runner_fleet_evidence_sha256: String,
    cleanup_authority_sha256: String,
    temporal_namespace_sha256: String,
    storage_config_sha256: String,
    task_queue_sha256: String,
    failure_converter_sha256: String,
    canary_evidence_sha256: String,
    portal_readback_evidence_sha256: String,
    portal_readback_at_ms: i64,
    portal_readback_ttl_ms: i64,
    maximum_inflight: i64,
    maximum_daily_admissions: i64,
    heartbeat_ttl_ms: i64,
    recovery_acceptances: Vec<ManagedCloudRecoveryAcceptanceAuthority>,
    issued_at_ms: i64,
    not_before_ms: i64,
    expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudCanaryCheckAuthority {
    check_id: String,
    evidence_sha256: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudCanaryEvidenceAuthority {
    version: i64,
    audience: String,
    manifest_sha256: String,
    scope: ManagedCloudScope,
    status: String,
    observed_at_ms: i64,
    expires_at_ms: i64,
    checks: Vec<ManagedCloudCanaryCheckAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudCleanupEvidenceAuthority {
    version: i64,
    audience: String,
    manifest_sha256: String,
    scope: ManagedCloudScope,
    status: String,
    observed_at_ms: i64,
    expires_at_ms: i64,
    cleanup_authority_sha256: String,
    dispatcher_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudFailureConverterEvidenceAuthority {
    version: i64,
    audience: String,
    manifest_sha256: String,
    scope: ManagedCloudScope,
    status: String,
    observed_at_ms: i64,
    expires_at_ms: i64,
    deployed_file_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudPortalReadbackEvidenceAuthority {
    version: i64,
    audience: String,
    manifest_sha256: String,
    scope: ManagedCloudScope,
    status: String,
    observed_at_ms: i64,
    expires_at_ms: i64,
    portal_artifact_sha256: String,
    readback_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudRunnerFleetEvidenceAuthority {
    version: i64,
    audience: String,
    manifest_sha256: String,
    scope: ManagedCloudScope,
    status: String,
    observed_at_ms: i64,
    expires_at_ms: i64,
    fleet_id_sha256: String,
    ready_instances: i64,
    required_instances: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudStorageEvidenceAuthority {
    version: i64,
    audience: String,
    manifest_sha256: String,
    scope: ManagedCloudScope,
    status: String,
    observed_at_ms: i64,
    expires_at_ms: i64,
    read_probe_sha256: String,
    storage_config_sha256: String,
    write_probe_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudTaskQueueEvidenceAuthority {
    version: i64,
    audience: String,
    manifest_sha256: String,
    scope: ManagedCloudScope,
    status: String,
    observed_at_ms: i64,
    expires_at_ms: i64,
    namespace: String,
    task_queue: String,
    task_queue_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudTemporalEvidenceAuthority {
    version: i64,
    audience: String,
    manifest_sha256: String,
    scope: ManagedCloudScope,
    status: String,
    observed_at_ms: i64,
    expires_at_ms: i64,
    gateway_ready: bool,
    namespace: String,
    namespace_sha256: String,
    workflow_worker_ready: bool,
}

#[derive(Debug, Clone)]
struct ValidatedManagedCloudActivationEvidence {
    canary_sha256: String,
    cleanup_authority_sha256: String,
    failure_converter_sha256: String,
    portal_readback_sha256: String,
    runner_fleet_sha256: String,
    storage_config_sha256: String,
    task_queue_sha256: String,
    temporal_namespace_sha256: String,
    portal_expires_at_ms: i64,
}

#[derive(Debug, Clone)]
struct ParsedManagedCloudActivationEvidence {
    canary_bytes: Vec<u8>,
    canary: ManagedCloudCanaryEvidenceAuthority,
    cleanup: ManagedCloudCleanupEvidenceAuthority,
    failure_converter: ManagedCloudFailureConverterEvidenceAuthority,
    portal_bytes: Vec<u8>,
    portal: ManagedCloudPortalReadbackEvidenceAuthority,
    runner_bytes: Vec<u8>,
    runner: ManagedCloudRunnerFleetEvidenceAuthority,
    storage: ManagedCloudStorageEvidenceAuthority,
    task_queue: ManagedCloudTaskQueueEvidenceAuthority,
    temporal: ManagedCloudTemporalEvidenceAuthority,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudCohortAuthority {
    version: i64,
    audience: String,
    cohort_id: String,
    cohort_generation: i64,
    trust_generation: i64,
    scope: ManagedCloudScope,
    rollout_mode: String,
    account_id_sha256s: Vec<String>,
    approval_ref: String,
    issued_at_ms: i64,
    not_before_ms: i64,
    expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudRollbackAuthority {
    version: i64,
    audience: String,
    rollback_id: String,
    rollback_generation: i64,
    trust_generation: i64,
    scope: ManagedCloudScope,
    expected_head_revision: i64,
    expected_transition_sha256: String,
    from_activation_sha256: String,
    from_manifest_sha256: String,
    to_activation_sha256: String,
    to_manifest_sha256: String,
    evidence_sha256: String,
    reason_ref: String,
    issued_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudRevocationAuthority {
    version: i64,
    audience: String,
    revocation_id: String,
    revocation_generation: i64,
    predecessor_revocation_sha256: Option<String>,
    trust_generation: i64,
    subject_kind: String,
    subject_id: String,
    subject_sha256: String,
    reason_ref: String,
    issued_at_ms: i64,
    effective_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ManagedCloudHeadTransitionAuthority {
    version: i64,
    audience: String,
    scope: ManagedCloudScope,
    head_revision: i64,
    previous_head_revision: i64,
    previous_transition_sha256: Option<String>,
    previous_activation_sha256: Option<String>,
    previous_manifest_sha256: Option<String>,
    previous_trust_generation: Option<i64>,
    previous_channel_sequence: Option<i64>,
    next_activation_sha256: String,
    next_manifest_sha256: String,
    next_trust_generation: i64,
    next_channel_sequence: i64,
    transition_kind: String,
    authority_sha256: String,
    rollback_authority_sha256: Option<String>,
    recorded_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedCloudWorkflowBindingAuthority<'a> {
    version: i64,
    audience: &'a str,
    command_id: &'a str,
    account_id_hmac_sha256: &'a str,
    application_id_hmac_sha256: &'a str,
    run_id_hmac_sha256: &'a str,
    workflow_id_hmac_sha256: &'a str,
    admission: &'a ManagedCloudAdmissionAuthority,
    bound_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedCloudRecoveryAuthorizationDigest<'a> {
    version: i64,
    audience: &'a str,
    binding_sha256: &'a str,
    current_head_revision: i64,
    current_transition_sha256: &'a str,
    current_activation_sha256: &'a str,
    current_manifest_sha256: &'a str,
    current_activation_expires_at_ms: i64,
    current_task_queue_sha256: &'a str,
    current_failure_converter_sha256: &'a str,
    current_readiness_sha256: &'a str,
    frozen_activation_sha256: &'a str,
    frozen_manifest_sha256: &'a str,
    recovery_accepted: bool,
    authorized_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedCloudExecutionLeaseAuthorityDigest<'a> {
    version: i64,
    audience: &'a str,
    run_id: &'a str,
    fence: i64,
    lease_token_sha256: &'a str,
    workflow_request_id: &'a str,
    request_command_id: &'a str,
    execution_command_id: &'a str,
    binding_sha256: &'a str,
    release_sha256: &'a str,
    runtime_instance_id: &'a str,
    runtime_instance_epoch: i64,
    worker_id: &'a str,
    gateway_authority_sha256: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedCloudIrreversibleEffectReceiptDigest<'a> {
    version: i64,
    audience: &'a str,
    run_id: &'a str,
    fence: i64,
    lease_token_sha256: &'a str,
    account_id_hmac_sha256: &'a str,
    application_id_hmac_sha256: &'a str,
    workflow_request_id: &'a str,
    request_command_id: &'a str,
    execution_command_id: &'a str,
    binding_sha256: &'a str,
    release_sha256: &'a str,
    runtime_instance_id: &'a str,
    runtime_instance_epoch: i64,
    worker_id: &'a str,
    gateway_authority_sha256: &'a str,
}

#[derive(Debug, Clone)]
struct ManagedCloudStoredWorkflowBinding {
    command_id: String,
    binding_sha256: String,
    account_id_hmac_sha256: String,
    application_id_hmac_sha256: String,
    run_id_hmac_sha256: String,
    workflow_id_hmac_sha256: String,
    admission: ManagedCloudAdmissionAuthority,
    release_memo_base64url: String,
    release_memo_sha256: String,
    bound_at_ms: i64,
}

#[derive(Debug, Clone)]
struct ManagedCloudStoredRequestStartAuthority {
    attempt_id: String,
    account_id: String,
    command_id: String,
    fence: i64,
    binding_sha256: String,
    gateway_authority_base64url: String,
    gateway_authority_sha256: String,
    authorized_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedCloudStoredExecutionLeaseAuthority {
    workflow_request_id: Option<String>,
    request_command_id: Option<String>,
    execution_command_id: Option<String>,
    binding_sha256: Option<String>,
    release_memo_base64url: Option<String>,
    release_sha256: Option<String>,
    runtime_instance_id: Option<String>,
    runtime_instance_epoch: Option<i64>,
    worker_id: Option<String>,
    gateway_authority_base64url: Option<String>,
    gateway_authority_sha256: Option<String>,
    lease_authority_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudTrustRoleAuthority {
    role: String,
    threshold: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudTrustKeyAuthority {
    key_id: String,
    role: String,
    public_key: String,
    state: String,
    valid_from_ms: i64,
    valid_until_ms: i64,
    minimum_trust_generation: i64,
    maximum_trust_generation: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudTrustPolicyAuthority {
    version: i64,
    audience: String,
    policy_id: String,
    trust_generation: i64,
    predecessor_policy_sha256: Option<String>,
    issued_at_ms: i64,
    valid_from_ms: i64,
    expires_at_ms: i64,
    roles: Vec<ManagedCloudTrustRoleAuthority>,
    keys: Vec<ManagedCloudTrustKeyAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudDetachedSignatureAuthority {
    key_id: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudSignatureSetAuthority {
    version: i64,
    audience: String,
    signature_set_id: String,
    trust_generation: i64,
    role: String,
    target_audience: String,
    target_sha256: String,
    signed_at_ms: i64,
    signatures: Vec<ManagedCloudDetachedSignatureAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudRootTrustAnchor {
    version: i64,
    audience: String,
    threshold: i64,
    keys: Vec<ManagedCloudRootTrustKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManagedCloudRootTrustKey {
    key_id: String,
    public_key: String,
    valid_from_ms: i64,
    valid_until_ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedCloudSignaturePayload<'a> {
    version: i64,
    audience: &'a str,
    signature_set_id: &'a str,
    trust_generation: i64,
    role: &'a str,
    target_audience: &'a str,
    target_sha256: &'a str,
    signed_at_ms: i64,
}

fn managed_cloud_storage(error: impl Into<anyhow::Error>) -> ManagedCloudRegistryError {
    ManagedCloudRegistryError::Storage(error.into())
}

fn managed_cloud_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn managed_cloud_canonical_json<T: Serialize>(value: &T) -> ManagedCloudResult<Vec<u8>> {
    fn write_value(value: &Value, output: &mut Vec<u8>) -> ManagedCloudResult<()> {
        match value {
            Value::Null => output.extend_from_slice(b"null"),
            Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
            Value::Number(value) => {
                let valid = value.as_i64().is_some_and(|number| {
                    (-MANAGED_CLOUD_MAX_SAFE_INTEGER..=MANAGED_CLOUD_MAX_SAFE_INTEGER)
                        .contains(&number)
                }) || value
                    .as_u64()
                    .is_some_and(|number| number <= MANAGED_CLOUD_MAX_SAFE_INTEGER as u64);
                if !valid || value.is_f64() {
                    return Err(ManagedCloudRegistryError::InvalidAuthority);
                }
                output.extend_from_slice(value.to_string().as_bytes());
            }
            Value::String(value) => output.extend_from_slice(
                serde_json::to_string(value)
                    .map_err(managed_cloud_storage)?
                    .as_bytes(),
            ),
            Value::Array(values) => {
                output.push(b'[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    write_value(value, output)?;
                }
                output.push(b']');
            }
            Value::Object(values) => {
                output.push(b'{');
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort_unstable();
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    output.extend_from_slice(
                        serde_json::to_string(key)
                            .map_err(managed_cloud_storage)?
                            .as_bytes(),
                    );
                    output.push(b':');
                    write_value(&values[key], output)?;
                }
                output.push(b'}');
            }
        }
        Ok(())
    }

    let value = serde_json::to_value(value).map_err(managed_cloud_storage)?;
    let mut bytes = Vec::new();
    write_value(&value, &mut bytes)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn managed_cloud_digest<T: Serialize>(value: &T) -> ManagedCloudResult<String> {
    Ok(managed_cloud_sha256(&managed_cloud_canonical_json(value)?))
}

fn managed_cloud_decode_base64url(value: &str) -> ManagedCloudResult<Vec<u8>> {
    if value.is_empty() || value.len() > MANAGED_CLOUD_MAX_ENVELOPE_BYTES * 2 {
        return Err(ManagedCloudRegistryError::InvalidEnvelope);
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ManagedCloudRegistryError::InvalidEnvelope)?;
    if decoded.is_empty()
        || decoded.len() > MANAGED_CLOUD_MAX_ENVELOPE_BYTES
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != value
    {
        return Err(ManagedCloudRegistryError::InvalidEnvelope);
    }
    Ok(decoded)
}

fn managed_cloud_decode_content_inventory(value: &str) -> ManagedCloudResult<Vec<u8>> {
    if value.is_empty() || value.len() > (MANAGED_CLOUD_MAX_CONTENT_INVENTORY_BYTES * 4 / 3) + 4 {
        return Err(ManagedCloudRegistryError::InvalidEnvelope);
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ManagedCloudRegistryError::InvalidEnvelope)?;
    if decoded.is_empty()
        || decoded.len() > MANAGED_CLOUD_MAX_CONTENT_INVENTORY_BYTES
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != value
    {
        return Err(ManagedCloudRegistryError::InvalidEnvelope);
    }
    Ok(decoded)
}

fn managed_cloud_decode_exact(value: &str, length: usize) -> ManagedCloudResult<Vec<u8>> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    if decoded.len() != length
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != value
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(decoded)
}

fn managed_cloud_parse_canonical<T>(bytes: &[u8]) -> ManagedCloudResult<T>
where
    T: DeserializeOwned + Serialize,
{
    if bytes.is_empty() || bytes.len() > MANAGED_CLOUD_MAX_ENVELOPE_BYTES {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let value: T =
        serde_json::from_slice(bytes).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    if managed_cloud_canonical_json(&value)? != bytes {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(value)
}

fn managed_cloud_decode_envelope(
    envelope: &ManagedCloudAuthorityEnvelope,
) -> ManagedCloudResult<(Vec<u8>, Vec<u8>)> {
    Ok((
        managed_cloud_decode_base64url(&envelope.canonical_base64url)?,
        managed_cloud_decode_base64url(&envelope.signature_set_base64url)?,
    ))
}

fn managed_cloud_safe_integer(value: i64, positive: bool) -> bool {
    value >= i64::from(positive) && value <= MANAGED_CLOUD_MAX_SAFE_INTEGER
}

fn managed_cloud_hex64(value: &str) -> bool {
    value.len() == 64
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn managed_cloud_token(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

fn managed_cloud_route_id(value: &str) -> bool {
    (20..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn managed_cloud_base64url_32(value: &str) -> bool {
    value.len() == 43 && managed_cloud_decode_exact(value, 32).is_ok()
}

fn managed_cloud_region(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value == value.to_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
}

pub fn derive_managed_cloud_runtime_session_token(
    grant_token: &str,
    grant_id: &str,
    runtime_instance_id: &str,
) -> ManagedCloudResult<String> {
    if !managed_cloud_base64url_32(grant_token)
        || !managed_cloud_route_id(grant_id)
        || !managed_cloud_route_id(runtime_instance_id)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let key = managed_cloud_decode_exact(grant_token, 32)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&key)
        .map_err(|_| ManagedCloudRegistryError::InvalidRequest)?;
    mac.update(MANAGED_CLOUD_RUNTIME_SESSION_DOMAIN);
    mac.update(grant_id.as_bytes());
    mac.update(&[0]);
    mac.update(runtime_instance_id.as_bytes());
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

fn managed_cloud_runtime_session_proof_hmac(
    session_token: &str,
    grant_id: &str,
    worker_id: &str,
    runtime_instance_id: &str,
    instance_epoch: i64,
) -> ManagedCloudResult<String> {
    if !managed_cloud_base64url_32(session_token)
        || !managed_cloud_route_id(grant_id)
        || !managed_cloud_route_id(worker_id)
        || !managed_cloud_route_id(runtime_instance_id)
        || !managed_cloud_safe_integer(instance_epoch, true)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let key = jobs_data_key().map_err(managed_cloud_storage)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&key)
        .map_err(|_| ManagedCloudRegistryError::InvalidRequest)?;
    mac.update(MANAGED_CLOUD_RUNTIME_SESSION_PROOF_DOMAIN);
    for value in [session_token, grant_id, worker_id, runtime_instance_id] {
        mac.update(value.as_bytes());
        mac.update(&[0]);
    }
    mac.update(&instance_epoch.to_be_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

pub fn managed_cloud_task_queue_sha256(
    namespace: &str,
    task_queue: &str,
) -> ManagedCloudResult<String> {
    let valid = |value: &str| {
        !value.is_empty()
            && value.len() <= 240
            && value.trim() == value
            && !value.contains('\u{fffd}')
            && !value.chars().any(char::is_control)
    };
    if !valid(namespace) || !valid(task_queue) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let mut bytes = MANAGED_CLOUD_TASK_QUEUE_DOMAIN.to_vec();
    bytes.extend_from_slice(namespace.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(task_queue.as_bytes());
    Ok(managed_cloud_sha256(&bytes))
}

pub fn managed_cloud_failure_converter_sha256(bytes: &[u8]) -> String {
    managed_cloud_sha256(bytes)
}

pub fn managed_cloud_dependency_evidence_sha256(
    role: &str,
    activation_sha256: &str,
    manifest_sha256: &str,
    component_id: &str,
    artifact_sha256: &str,
    task_queue_sha256: &str,
    failure_converter_sha256: &str,
) -> ManagedCloudResult<String> {
    if !managed_cloud_runtime_role(role)
        || !managed_cloud_hex64(activation_sha256)
        || !managed_cloud_hex64(manifest_sha256)
        || !managed_cloud_token(component_id, 128)
        || !managed_cloud_hex64(artifact_sha256)
        || !managed_cloud_hex64(task_queue_sha256)
        || !managed_cloud_hex64(failure_converter_sha256)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let mut bytes = MANAGED_CLOUD_DEPENDENCY_EVIDENCE_DOMAIN.to_vec();
    for value in [
        role,
        activation_sha256,
        manifest_sha256,
        component_id,
        artifact_sha256,
        task_queue_sha256,
        failure_converter_sha256,
    ] {
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
    }
    Ok(managed_cloud_sha256(&bytes))
}

pub fn managed_cloud_runtime_identity_sha256(
    runtime_measurement_sha256: &str,
    component_id: &str,
    role: &str,
) -> ManagedCloudResult<String> {
    if !managed_cloud_hex64(runtime_measurement_sha256)
        || !managed_cloud_token(component_id, 128)
        || !managed_cloud_runtime_role(role)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let mut bytes = MANAGED_CLOUD_RUNTIME_IDENTITY_DOMAIN.to_vec();
    bytes.extend_from_slice(runtime_measurement_sha256.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(component_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(role.as_bytes());
    Ok(managed_cloud_sha256(&bytes))
}

pub fn inspect_managed_cloud_runtime_identity(
    artifact_root: &std::path::Path,
    component_id: &str,
    role: &str,
) -> ManagedCloudResult<ManagedCloudRuntimeMeasurementIdentity> {
    if !matches!(component_id, "jobs-api" | "jobs-runner" | "jobs-workflows")
        || !managed_cloud_runtime_role(role)
        || role == "original_source_verifier"
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let measurement_path = artifact_root.join(MANAGED_CLOUD_RUNTIME_MEASUREMENT_PATH);
    let metadata = std::fs::symlink_metadata(&measurement_path).map_err(managed_cloud_storage)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let bytes = std::fs::read(&measurement_path).map_err(managed_cloud_storage)?;
    let measurement: ManagedCloudRuntimeMeasurementAuthority =
        managed_cloud_parse_canonical(&bytes)?;
    if measurement.version != 1
        || measurement.audience != MANAGED_CLOUD_RUNTIME_MEASUREMENT_AUDIENCE
        || measurement.component_id != component_id
        || !managed_cloud_token(&measurement.build_id, 128)
        || !managed_cloud_hex64(&measurement.config_schema_sha256)
        || !managed_cloud_hex64(&measurement.migration_set_sha256)
        || !managed_cloud_hex64(&measurement.protocol_set_sha256)
        || measurement.source_commit.len() != 40
        || !measurement
            .source_commit
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || measurement.roles.is_empty()
        || measurement.roles.len() > 8
        || measurement
            .roles
            .iter()
            .any(|candidate| !managed_cloud_runtime_role(candidate))
        || measurement.roles.windows(2).any(|pair| pair[0] >= pair[1])
        || !measurement.roles.iter().any(|candidate| candidate == role)
        || !managed_cloud_runtime_measurement_file_count_valid(measurement.measured_files.len())
        || measurement
            .measured_files
            .windows(2)
            .any(|pair| pair[0].path >= pair[1].path)
        || (component_id == "jobs-api"
            && (measurement.roles.iter().map(String::as_str).ne([
                "jobs_api",
                "workflow_cleanup_dispatcher",
                "workflow_command_dispatcher",
            ]) || measurement.measured_files.len() != 1
                || measurement.measured_files[0].path != "usr/local/bin/bluey-jobs-api"))
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    for measured in &measurement.measured_files {
        if !managed_cloud_safe_inventory_path(&measured.path)
            || !managed_cloud_hex64(&measured.sha256)
            || measured.path == MANAGED_CLOUD_RUNTIME_MEASUREMENT_PATH
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        let path = artifact_root.join(&measured.path);
        let metadata = std::fs::symlink_metadata(&path).map_err(managed_cloud_storage)?;
        if !metadata.file_type().is_file() || metadata.len() == 0 {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        let file_bytes = std::fs::read(path).map_err(managed_cloud_storage)?;
        if managed_cloud_sha256(&file_bytes) != measured.sha256 {
            return Err(ManagedCloudRegistryError::IdentityConflict);
        }
    }
    let runtime_measurement_sha256 = managed_cloud_sha256(&bytes);
    let runtime_identity_sha256 =
        managed_cloud_runtime_identity_sha256(&runtime_measurement_sha256, component_id, role)?;
    Ok(ManagedCloudRuntimeMeasurementIdentity {
        runtime_measurement_sha256,
        runtime_identity_sha256,
        config_schema_sha256: measurement.config_schema_sha256,
        migration_set_sha256: measurement.migration_set_sha256,
        protocol_set_sha256: measurement.protocol_set_sha256,
    })
}

fn managed_cloud_cohort_member_sha256(
    cohort_id: &str,
    account_id: &str,
) -> ManagedCloudResult<String> {
    if !managed_cloud_token(cohort_id, 128) || !managed_cloud_token(account_id, 128) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let mut bytes = MANAGED_CLOUD_COHORT_MEMBER_DOMAIN.to_vec();
    bytes.extend_from_slice(cohort_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(account_id.as_bytes());
    Ok(managed_cloud_sha256(&bytes))
}

fn validate_managed_cloud_scope(scope: &ManagedCloudScope) -> ManagedCloudResult<()> {
    if !matches!(scope.environment.as_str(), "production" | "staging")
        || !matches!(scope.channel.as_str(), "canary" | "general" | "shadow")
        || !managed_cloud_region(&scope.region)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    Ok(())
}

fn managed_cloud_runtime_role(value: &str) -> bool {
    MANAGED_CLOUD_ALL_RUNTIME_ROLES.contains(&value)
}

fn managed_cloud_health(value: &str, reason: Option<&str>) -> bool {
    const DEGRADED: [&str; 8] = [
        "artifact_mismatch",
        "config_mismatch",
        "dependency_unavailable",
        "head_mismatch",
        "migration_mismatch",
        "probe_failed",
        "protocol_mismatch",
        "startup",
    ];
    matches!(
        (value, reason),
        ("ready", None) | ("draining", Some("draining"))
    ) || (value == "degraded" && reason.is_some_and(|value| DEGRADED.contains(&value)))
}

fn managed_cloud_db_now_sqlite(tx: &rusqlite::Transaction<'_>) -> ManagedCloudResult<i64> {
    tx.query_row(
        "SELECT CAST(strftime('%s', 'now') AS INTEGER) * 1000",
        [],
        |row| row.get(0),
    )
    .map_err(managed_cloud_storage)
}

fn managed_cloud_db_now_postgres(tx: &mut postgres::Transaction<'_>) -> ManagedCloudResult<i64> {
    tx.query_one(
        "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
        &[],
    )
    .map(|row| row.get(0))
    .map_err(managed_cloud_storage)
}

fn managed_cloud_subject_hmac(kind: &str, value: &str) -> ManagedCloudResult<String> {
    let key = jobs_data_key().map_err(managed_cloud_storage)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&key)
        .map_err(|_| ManagedCloudRegistryError::InvalidRequest)?;
    mac.update(b"bluey-jobs-managed-cloud-subject-v1\0");
    mac.update(kind.as_bytes());
    mac.update(&[0]);
    mac.update(value.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn validate_managed_cloud_trust_policy(
    policy: &ManagedCloudTrustPolicyAuthority,
) -> ManagedCloudResult<()> {
    const ROLES: [&str; 5] = [
        "general_promotion",
        "incident",
        "promotion",
        "release",
        "root",
    ];
    if policy.version != 1
        || policy.audience != MANAGED_CLOUD_TRUST_POLICY_AUDIENCE
        || !managed_cloud_token(&policy.policy_id, 128)
        || !managed_cloud_safe_integer(policy.trust_generation, true)
        || !managed_cloud_safe_integer(policy.issued_at_ms, false)
        || !managed_cloud_safe_integer(policy.valid_from_ms, false)
        || !managed_cloud_safe_integer(policy.expires_at_ms, false)
        || policy.valid_from_ms > policy.issued_at_ms
        || policy.issued_at_ms >= policy.expires_at_ms
        || (policy.trust_generation == 1) != policy.predecessor_policy_sha256.is_none()
        || policy
            .predecessor_policy_sha256
            .as_ref()
            .is_some_and(|value| !managed_cloud_hex64(value))
        || policy.roles.len() != ROLES.len()
        || policy.roles.iter().map(|role| role.role.as_str()).ne(ROLES)
        || !(5..=64).contains(&policy.keys.len())
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let mut previous_key: Option<&str> = None;
    for key in &policy.keys {
        if previous_key.is_some_and(|previous| previous >= key.key_id.as_str())
            || !managed_cloud_token(&key.key_id, 128)
            || !ROLES.contains(&key.role.as_str())
            || managed_cloud_decode_exact(&key.public_key, 32).is_err()
            || !matches!(key.state.as_str(), "active" | "retired" | "revoked")
            || !managed_cloud_safe_integer(key.valid_from_ms, false)
            || !managed_cloud_safe_integer(key.valid_until_ms, false)
            || key.valid_until_ms <= key.valid_from_ms
            || !managed_cloud_safe_integer(key.minimum_trust_generation, true)
            || !managed_cloud_safe_integer(key.maximum_trust_generation, true)
            || key.maximum_trust_generation < key.minimum_trust_generation
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        previous_key = Some(&key.key_id);
    }
    for role in &policy.roles {
        if !(1..=32).contains(&role.threshold) {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        let active = policy
            .keys
            .iter()
            .filter(|key| {
                key.role == role.role
                    && key.state == "active"
                    && managed_cloud_key_authorizes(
                        key,
                        policy.trust_generation,
                        policy.issued_at_ms,
                    )
            })
            .count();
        if active < role.threshold as usize {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
    }
    Ok(())
}

fn validate_managed_cloud_signature_set(
    signature_set: &ManagedCloudSignatureSetAuthority,
) -> ManagedCloudResult<()> {
    const TARGETS: [&str; 6] = [
        MANAGED_CLOUD_ACTIVATION_AUDIENCE,
        MANAGED_CLOUD_COHORT_AUDIENCE,
        MANAGED_CLOUD_RELEASE_AUDIENCE,
        MANAGED_CLOUD_REVOCATION_AUDIENCE,
        MANAGED_CLOUD_ROLLBACK_AUDIENCE,
        MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
    ];
    if signature_set.version != 1
        || signature_set.audience != MANAGED_CLOUD_SIGNATURE_SET_AUDIENCE
        || !managed_cloud_token(&signature_set.signature_set_id, 128)
        || !managed_cloud_safe_integer(signature_set.trust_generation, true)
        || !matches!(
            signature_set.role.as_str(),
            "general_promotion" | "incident" | "promotion" | "release" | "root"
        )
        || !TARGETS.contains(&signature_set.target_audience.as_str())
        || !managed_cloud_hex64(&signature_set.target_sha256)
        || !managed_cloud_safe_integer(signature_set.signed_at_ms, false)
        || signature_set.signatures.is_empty()
        || signature_set.signatures.len() > 32
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let mut previous: Option<&str> = None;
    for signature in &signature_set.signatures {
        if previous.is_some_and(|value| value >= signature.key_id.as_str())
            || !managed_cloud_token(&signature.key_id, 128)
            || managed_cloud_decode_exact(&signature.signature, 64).is_err()
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        previous = Some(&signature.key_id);
    }
    Ok(())
}

fn managed_cloud_key_authorizes(
    key: &ManagedCloudTrustKeyAuthority,
    trust_generation: i64,
    signed_at_ms: i64,
) -> bool {
    key.state == "active"
        && trust_generation >= key.minimum_trust_generation
        && trust_generation <= key.maximum_trust_generation
        && signed_at_ms >= key.valid_from_ms
        && signed_at_ms < key.valid_until_ms
}

fn managed_cloud_signature_payload(
    signature_set: &ManagedCloudSignatureSetAuthority,
) -> ManagedCloudResult<Vec<u8>> {
    managed_cloud_canonical_json(&ManagedCloudSignaturePayload {
        version: signature_set.version,
        audience: &signature_set.audience,
        signature_set_id: &signature_set.signature_set_id,
        trust_generation: signature_set.trust_generation,
        role: &signature_set.role,
        target_audience: &signature_set.target_audience,
        target_sha256: &signature_set.target_sha256,
        signed_at_ms: signature_set.signed_at_ms,
    })
}

fn verify_managed_cloud_detached_signature(
    public_key_base64url: &str,
    message: &[u8],
    signature_base64url: &str,
) -> ManagedCloudResult<()> {
    let key: [u8; 32] = managed_cloud_decode_exact(public_key_base64url, 32)?
        .try_into()
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    let signature: [u8; 64] = managed_cloud_decode_exact(signature_base64url, 64)?
        .try_into()
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key)
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    use ed25519_dalek::Verifier as _;
    key.verify(message, &ed25519_dalek::Signature::from_bytes(&signature))
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)
}

fn verify_managed_cloud_signature_set(
    target_bytes: &[u8],
    signature_set: &ManagedCloudSignatureSetAuthority,
    policy: &ManagedCloudTrustPolicyAuthority,
    role: &str,
    audience: &str,
    target_issued_at_ms: i64,
    verification_time_ms: i64,
) -> ManagedCloudResult<()> {
    validate_managed_cloud_signature_set(signature_set)?;
    if signature_set.role != role
        || signature_set.target_audience != audience
        || signature_set.target_sha256 != managed_cloud_sha256(target_bytes)
        || signature_set.trust_generation != policy.trust_generation
        || target_issued_at_ms > verification_time_ms
        || signature_set.signed_at_ms < target_issued_at_ms
        || signature_set.signed_at_ms > verification_time_ms
        || signature_set.signed_at_ms < policy.valid_from_ms
        || signature_set.signed_at_ms >= policy.expires_at_ms
        || verification_time_ms < policy.valid_from_ms
        || verification_time_ms >= policy.expires_at_ms
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let threshold = policy
        .roles
        .iter()
        .find(|entry| entry.role == role)
        .map(|entry| entry.threshold)
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    let message = managed_cloud_signature_payload(signature_set)?;
    let mut verified = 0_i64;
    for signature in &signature_set.signatures {
        let key = policy
            .keys
            .iter()
            .find(|key| key.key_id == signature.key_id)
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
        if key.role != role
            || !managed_cloud_key_authorizes(
                key,
                signature_set.trust_generation,
                signature_set.signed_at_ms,
            )
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        verify_managed_cloud_detached_signature(&key.public_key, &message, &signature.signature)?;
        verified += 1;
    }
    if verified < threshold {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn validate_managed_cloud_root_anchor(
    anchor: &ManagedCloudRootTrustAnchor,
) -> ManagedCloudResult<()> {
    if anchor.version != 1
        || anchor.audience != MANAGED_CLOUD_ROOT_ANCHOR_AUDIENCE
        || !(1..=32).contains(&anchor.threshold)
        || anchor.keys.len() < anchor.threshold as usize
        || anchor.keys.len() > 32
        || anchor
            .keys
            .windows(2)
            .any(|keys| keys[0].key_id >= keys[1].key_id)
        || anchor.keys.iter().any(|key| {
            !managed_cloud_token(&key.key_id, 128)
                || managed_cloud_decode_exact(&key.public_key, 32).is_err()
                || !managed_cloud_safe_integer(key.valid_from_ms, false)
                || !managed_cloud_safe_integer(key.valid_until_ms, true)
                || key.valid_until_ms <= key.valid_from_ms
        })
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn managed_cloud_root_anchor_from_environment() -> ManagedCloudResult<ManagedCloudRootTrustAnchor> {
    let raw = std::env::var(MANAGED_CLOUD_ROOT_TRUST_ANCHOR_ENV)
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    let expected_sha256 = std::env::var(MANAGED_CLOUD_ROOT_TRUST_ANCHOR_SHA256_ENV)
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    if !managed_cloud_hex64(&expected_sha256)
        || managed_cloud_sha256(raw.as_bytes()) != expected_sha256
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let anchor: ManagedCloudRootTrustAnchor = managed_cloud_parse_canonical(raw.as_bytes())?;
    validate_managed_cloud_root_anchor(&anchor)?;
    Ok(anchor)
}

fn managed_cloud_root_threshold(
    policy: &ManagedCloudTrustPolicyAuthority,
) -> ManagedCloudResult<i64> {
    policy
        .roles
        .iter()
        .find(|role| role.role == "root")
        .map(|role| role.threshold)
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)
}

fn verify_managed_cloud_bootstrap_union_signature_set(
    target_bytes: &[u8],
    signature_set: &ManagedCloudSignatureSetAuthority,
    anchor: &ManagedCloudRootTrustAnchor,
    successor: &ManagedCloudTrustPolicyAuthority,
    verification_time_ms: i64,
) -> ManagedCloudResult<()> {
    validate_managed_cloud_signature_set(signature_set)?;
    if successor.trust_generation != 1
        || signature_set.role != "root"
        || signature_set.target_audience != MANAGED_CLOUD_TRUST_POLICY_AUDIENCE
        || signature_set.target_sha256 != managed_cloud_sha256(target_bytes)
        || signature_set.trust_generation != 1
        || signature_set.signed_at_ms < successor.issued_at_ms
        || signature_set.signed_at_ms > verification_time_ms
        || signature_set.signed_at_ms < successor.valid_from_ms
        || signature_set.signed_at_ms >= successor.expires_at_ms
        || verification_time_ms < successor.valid_from_ms
        || verification_time_ms >= successor.expires_at_ms
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let message = managed_cloud_signature_payload(signature_set)?;
    let mut anchor_verified = 0_i64;
    let mut successor_verified = 0_i64;
    for signature in &signature_set.signatures {
        let anchor_key = anchor.keys.iter().find(|key| {
            key.key_id == signature.key_id
                && signature_set.signed_at_ms >= key.valid_from_ms
                && signature_set.signed_at_ms < key.valid_until_ms
        });
        let successor_key = successor.keys.iter().find(|key| {
            key.key_id == signature.key_id
                && key.role == "root"
                && managed_cloud_key_authorizes(key, 1, signature_set.signed_at_ms)
        });
        let anchor_valid = anchor_key.is_some_and(|key| {
            verify_managed_cloud_detached_signature(&key.public_key, &message, &signature.signature)
                .is_ok()
        });
        let successor_valid = successor_key.is_some_and(|key| {
            verify_managed_cloud_detached_signature(&key.public_key, &message, &signature.signature)
                .is_ok()
        });
        if !anchor_valid && !successor_valid {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        anchor_verified += i64::from(anchor_valid);
        successor_verified += i64::from(successor_valid);
    }
    if anchor_verified < anchor.threshold
        || successor_verified < managed_cloud_root_threshold(successor)?
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn verify_managed_cloud_rotation_union_signature_set(
    target_bytes: &[u8],
    signature_set: &ManagedCloudSignatureSetAuthority,
    predecessor: &ManagedCloudTrustPolicyAuthority,
    successor: &ManagedCloudTrustPolicyAuthority,
    verification_time_ms: i64,
) -> ManagedCloudResult<()> {
    validate_managed_cloud_signature_set(signature_set)?;
    if successor.trust_generation != predecessor.trust_generation + 1
        || successor.predecessor_policy_sha256.as_deref()
            != Some(managed_cloud_sha256(&managed_cloud_canonical_json(predecessor)?).as_str())
        || signature_set.role != "root"
        || signature_set.target_audience != MANAGED_CLOUD_TRUST_POLICY_AUDIENCE
        || signature_set.target_sha256 != managed_cloud_sha256(target_bytes)
        || signature_set.trust_generation != successor.trust_generation
        || signature_set.signed_at_ms < successor.issued_at_ms
        || signature_set.signed_at_ms > verification_time_ms
        || signature_set.signed_at_ms < predecessor.valid_from_ms
        || signature_set.signed_at_ms >= predecessor.expires_at_ms
        || signature_set.signed_at_ms < successor.valid_from_ms
        || signature_set.signed_at_ms >= successor.expires_at_ms
        || verification_time_ms < predecessor.valid_from_ms
        || verification_time_ms >= predecessor.expires_at_ms
        || verification_time_ms < successor.valid_from_ms
        || verification_time_ms >= successor.expires_at_ms
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let message = managed_cloud_signature_payload(signature_set)?;
    let mut predecessor_verified = 0_i64;
    let mut successor_verified = 0_i64;
    for signature in &signature_set.signatures {
        let predecessor_key = predecessor.keys.iter().find(|key| {
            key.key_id == signature.key_id
                && key.role == "root"
                && managed_cloud_key_authorizes(
                    key,
                    predecessor.trust_generation,
                    signature_set.signed_at_ms,
                )
        });
        let successor_key = successor.keys.iter().find(|key| {
            key.key_id == signature.key_id
                && key.role == "root"
                && managed_cloud_key_authorizes(
                    key,
                    successor.trust_generation,
                    signature_set.signed_at_ms,
                )
        });
        let predecessor_valid = predecessor_key.is_some_and(|key| {
            verify_managed_cloud_detached_signature(&key.public_key, &message, &signature.signature)
                .is_ok()
        });
        let successor_valid = successor_key.is_some_and(|key| {
            verify_managed_cloud_detached_signature(&key.public_key, &message, &signature.signature)
                .is_ok()
        });
        if !predecessor_valid && !successor_valid {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        predecessor_verified += i64::from(predecessor_valid);
        successor_verified += i64::from(successor_valid);
    }
    if predecessor_verified < managed_cloud_root_threshold(predecessor)?
        || successor_verified < managed_cloud_root_threshold(successor)?
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn managed_cloud_artifact_ref(
    kind: &str,
    value: &str,
    release_id: &str,
    artifact_sha256: &str,
) -> bool {
    if value.is_empty()
        || value.len() > 2_048
        || !value.is_ascii()
        || value.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return false;
    }
    if kind == "oci_image" {
        let Some((repository, digest)) = value.rsplit_once("@sha256:") else {
            return false;
        };
        let mut segments = repository.split('/');
        let Some(registry) = segments.next() else {
            return false;
        };
        let registry_valid = !registry.is_empty()
            && registry
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            && registry.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-' | b':')
            });
        let repository_segments = segments.collect::<Vec<_>>();
        return digest == artifact_sha256
            && managed_cloud_hex64(digest)
            && registry_valid
            && !repository_segments.is_empty()
            && repository_segments.iter().all(|segment| {
                !segment.is_empty()
                    && segment
                        .as_bytes()
                        .first()
                        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'.' | b'_' | b'-')
                    })
            });
    }
    if kind != "static_bundle" {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && segments.contains(&release_id)
        && segments.iter().any(|segment| {
            *segment == artifact_sha256
                || segment
                    .strip_suffix(".tar")
                    .is_some_and(|sha256| sha256 == artifact_sha256)
        })
}

fn validate_managed_cloud_release(
    release: &ManagedCloudReleaseAuthority,
) -> ManagedCloudResult<()> {
    if release.version != 1
        || release.audience != MANAGED_CLOUD_RELEASE_AUDIENCE
        || !managed_cloud_token(&release.manifest_id, 128)
        || !managed_cloud_safe_integer(release.manifest_generation, true)
        || !managed_cloud_token(&release.release_id, 128)
        || !managed_cloud_safe_integer(release.release_sequence, true)
        || release.source_commit.len() != 40
        || !release
            .source_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || release.source_commit != release.source_commit.to_ascii_lowercase()
        || !managed_cloud_safe_integer(release.published_at_ms, false)
        || release.sqlite_migration_head != MANAGED_CLOUD_SQLITE_MIGRATION_HEAD
        || release.postgres_migration_head != MANAGED_CLOUD_POSTGRES_MIGRATION_HEAD
        || !managed_cloud_hex64(&release.migration_set_sha256)
        || !managed_cloud_hex64(&release.config_schema_sha256)
        || !managed_cloud_hex64(&release.protocol_set_sha256)
        || !managed_cloud_hex64(&release.component_set_sha256)
        || !managed_cloud_hex64(&release.feature_authority_sha256)
        || !managed_cloud_hex64(&release.verification_evidence_sha256)
        || release.components.len() != 4
        || release.protocols.len() != MANAGED_CLOUD_PROTOCOLS.len()
        || !release.feature_authority.cloud_distribution
        || !release.feature_authority.workflow_command_dispatch
        || !release.feature_authority.workflow_cleanup
        || release.feature_authority.direct_discovery
        || release.feature_authority.global_discovery
        || release.feature_authority.source_verification
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let component_inventory = json!({
        "version": 1,
        "audience": MANAGED_CLOUD_COMPONENT_INVENTORY_AUDIENCE,
        "components": &release.components,
        "capabilities": &release.capabilities,
    });
    if managed_cloud_digest(&release.feature_authority)? != release.feature_authority_sha256
        || managed_cloud_digest(&component_inventory)? != release.component_set_sha256
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    const COMPONENTS: [(&str, &str, &str, &str); 4] = [
        ("jobs-api", "oci_image", "linux", "x86_64"),
        ("jobs-portal", "static_bundle", "web", "wasm"),
        ("jobs-runner", "oci_image", "linux", "x86_64"),
        ("jobs-workflows", "oci_image", "linux", "x86_64"),
    ];
    for (component, (component_id, kind, platform, architecture)) in
        release.components.iter().zip(COMPONENTS)
    {
        if component.component_id != component_id
            || component.artifact_kind != kind
            || component.platform != platform
            || component.architecture != architecture
            || !managed_cloud_artifact_ref(
                kind,
                &component.artifact_ref,
                &release.release_id,
                &component.artifact_sha256,
            )
            || !managed_cloud_hex64(&component.artifact_sha256)
            || !managed_cloud_token(&component.build_id, 128)
            || component.source_commit != release.source_commit
            || !managed_cloud_hex64(&component.sbom_sha256)
            || !managed_cloud_hex64(&component.provenance_sha256)
            || !managed_cloud_hex64(&component.config_schema_sha256)
            || component.config_schema_sha256 != release.config_schema_sha256
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
    }
    let mut expected_capabilities = vec![
        ("jobs-api", "jobs_api"),
        ("jobs-api", "workflow_cleanup_dispatcher"),
        ("jobs-api", "workflow_command_dispatcher"),
        ("jobs-portal", "portal_static"),
        ("jobs-runner", "managed_runner"),
    ];
    if release.feature_authority.direct_discovery {
        expected_capabilities.push(("jobs-workflows", "discovery_worker"));
    }
    if release.feature_authority.global_discovery {
        expected_capabilities.push(("jobs-workflows", "global_discovery_worker"));
    }
    expected_capabilities.extend([
        ("jobs-workflows", "workflow_gateway"),
        ("jobs-workflows", "workflow_worker"),
    ]);
    if release.capabilities.len() != expected_capabilities.len() {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    for (capability, (component_id, name)) in release.capabilities.iter().zip(expected_capabilities)
    {
        if capability.component_id != component_id || capability.capability != name {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
    }
    for ((expected_id, expected_version), protocol) in
        MANAGED_CLOUD_PROTOCOLS.iter().zip(&release.protocols)
    {
        if protocol.protocol_id != *expected_id
            || protocol.protocol_version != *expected_version
            || !managed_cloud_hex64(&protocol.schema_sha256)
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
    }
    Ok(())
}

type ManagedCloudVerificationRuntimeContract = (
    &'static str,
    Option<&'static str>,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

type ManagedCloudOciInventoryContract = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

type ManagedCloudInventoryContract = (&'static str, Option<ManagedCloudOciInventoryContract>);

fn validate_managed_cloud_verification_evidence(
    evidence: &ManagedCloudVerificationEvidenceAuthority,
    evidence_bytes: &[u8],
    release: &ManagedCloudReleaseAuthority,
) -> ManagedCloudResult<()> {
    if evidence.version != 1
        || evidence.audience != MANAGED_CLOUD_VERIFICATION_EVIDENCE_AUDIENCE
        || evidence.source_commit != release.source_commit
        || managed_cloud_sha256(evidence_bytes) != release.verification_evidence_sha256
        || !managed_cloud_hex64(&evidence.builder_policy_sha256)
        || !managed_cloud_hex64(&evidence.jobs_lock_sha256)
        || !managed_cloud_hex64(&evidence.server_lock_sha256)
        || evidence.migration_contract_sha256 != release.migration_set_sha256
        || evidence.config_contract_sha256 != release.config_schema_sha256
        || evidence.protocol_contract_sha256 != release.protocol_set_sha256
        || !managed_cloud_hex64(&evidence.test_evidence_sha256)
        || evidence.components.len() != release.components.len()
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    const OCI_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
    let runtime_contracts: [ManagedCloudVerificationRuntimeContract; 4] = [
        (
            "jobs-api",
            Some(OCI_PATH),
            "65532:65532",
            &["/usr/local/bin/bluey-jobs-api"],
            &[],
            &[
                "app/.bluey/managed-cloud-runtime-measurement.json",
                "usr/local/bin/bluey-jobs-api",
            ],
        ),
        ("jobs-portal", None, "static", &[], &[], &["index.html"]),
        (
            "jobs-runner",
            Some(OCI_PATH),
            "pwuser",
            &[],
            &[MANAGED_CLOUD_RUNNER_NODE_EXECUTABLE, "runner/dist/server.js"],
            &[
                "app/.bluey/managed-cloud-runtime-measurement.json",
                "app/runner/dist/native/bluey_jobs_runner_native_storage.node",
                "app/runner/dist/server.js",
                MANAGED_CLOUD_RUNNER_NODE_INVENTORY_PATH,
            ],
        ),
        (
            "jobs-workflows",
            Some(OCI_PATH),
            "node",
            &[],
            &[
                MANAGED_CLOUD_WORKFLOWS_NODE_EXECUTABLE,
                "workflows/dist/worker.js",
            ],
            &[
                "app/.bluey/managed-cloud-runtime-measurement.json",
                "app/workflows/dist/discovery-worker.js",
                "app/workflows/dist/failure-converter.js",
                "app/workflows/dist/gateway.js",
                "app/workflows/dist/global-discovery-worker.js",
                "app/workflows/dist/worker.js",
                MANAGED_CLOUD_WORKFLOWS_NODE_INVENTORY_PATH,
            ],
        ),
    ];
    for (((evidence_component, release_component), component_id), contract) in evidence
        .components
        .iter()
        .zip(&release.components)
        .zip(runtime_contracts.map(|entry| entry.0))
        .zip(runtime_contracts)
    {
        let (_, runtime_path, runtime_user, entrypoint, cmd, required_paths) = contract;
        let expected_runtime_roles = release
            .capabilities
            .iter()
            .filter(|capability| {
                capability.component_id == component_id && capability.capability != "portal_static"
            })
            .map(|capability| capability.capability.as_str())
            .collect::<Vec<_>>();
        let runtime_measurement_valid = if component_id == "jobs-portal" {
            evidence_component.runtime_measurement.is_none()
                && evidence_component.runtime_measurement_sha256.is_none()
                && evidence_component.runtime_identities.is_empty()
        } else {
            evidence_component
                .runtime_measurement
                .as_ref()
                .zip(evidence_component.runtime_measurement_sha256.as_deref())
                .is_some_and(|(measurement, measurement_sha256)| {
                    measurement.version == 1
                        && measurement.audience == MANAGED_CLOUD_RUNTIME_MEASUREMENT_AUDIENCE
                        && measurement.build_id == release_component.build_id
                        && measurement.component_id == component_id
                        && measurement.config_schema_sha256 == release.config_schema_sha256
                        && measurement.migration_set_sha256 == release.migration_set_sha256
                        && measurement.protocol_set_sha256 == release.protocol_set_sha256
                        && measurement.source_commit == release.source_commit
                        && measurement
                            .roles
                            .iter()
                            .map(String::as_str)
                            .eq(expected_runtime_roles.iter().copied())
                        && managed_cloud_runtime_measurement_file_count_valid(
                            measurement.measured_files.len(),
                        )
                        && measurement.measured_files.windows(2).all(|pair| {
                            pair[0].path < pair[1].path && managed_cloud_hex64(&pair[0].sha256)
                        })
                        && measurement
                            .measured_files
                            .last()
                            .is_some_and(|file| managed_cloud_hex64(&file.sha256))
                        && managed_cloud_digest(measurement)
                            .is_ok_and(|digest| digest == measurement_sha256)
                        && evidence_component.runtime_identities.len()
                            == expected_runtime_roles.len()
                        && evidence_component
                            .runtime_identities
                            .iter()
                            .zip(expected_runtime_roles.iter().copied())
                            .all(|(identity, role)| {
                                identity.role == role
                                    && managed_cloud_runtime_identity_sha256(
                                        measurement_sha256,
                                        component_id,
                                        role,
                                    )
                                    .is_ok_and(|digest| digest == identity.runtime_identity_sha256)
                            })
                })
        };
        let required_paths_match = if component_id == "jobs-runner" {
            let paths = &evidence_component.required_paths;
            required_paths
                .iter()
                .all(|required| paths.iter().any(|path| path == required))
                && paths.windows(2).all(|pair| pair[0] < pair[1])
                && paths.iter().all(|path| {
                    required_paths.contains(&path.as_str())
                        || managed_cloud_runner_headless_shell_path(path)
                })
                && paths
                    .iter()
                    .any(|path| managed_cloud_runner_headless_shell_path(path))
        } else {
            evidence_component
                .required_paths
                .iter()
                .map(String::as_str)
                .eq(required_paths.iter().copied())
        };
        if evidence_component.component_id != component_id
            || release_component.component_id != component_id
            || evidence_component.artifact_sha256 != release_component.artifact_sha256
            || evidence_component.sbom_sha256 != release_component.sbom_sha256
            || evidence_component.provenance_sha256 != release_component.provenance_sha256
            || !managed_cloud_hex64(&evidence_component.content_inventory_sha256)
            || evidence_component.runtime_path.as_deref() != runtime_path
            || evidence_component.runtime_user != runtime_user
            || evidence_component
                .entrypoint
                .iter()
                .map(String::as_str)
                .ne(entrypoint.iter().copied())
            || evidence_component
                .cmd
                .iter()
                .map(String::as_str)
                .ne(cmd.iter().copied())
            || !required_paths_match
            || !runtime_measurement_valid
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
    }
    Ok(())
}

fn managed_cloud_runner_headless_shell_path(path: &str) -> bool {
    let segments = path.split('/').collect::<Vec<_>>();
    segments.len() == 4
        && segments[0] == "ms-playwright"
        && segments[1]
            .strip_prefix("chromium_headless_shell-")
            .is_some_and(|revision| {
                !revision.is_empty() && revision.bytes().all(|byte| byte.is_ascii_digit())
            })
        && !segments[2].is_empty()
        && segments[2].bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
        && segments[3] == "chrome-headless-shell"
}

fn managed_cloud_runner_playwright_inventory_path(path: &str) -> bool {
    if path == "ms-playwright" {
        return true;
    }
    let Some(remainder) = path.strip_prefix("ms-playwright/") else {
        return true;
    };
    let top_level = remainder.split('/').next().unwrap_or_default();
    ["chromium_headless_shell-", "ffmpeg-"]
        .iter()
        .any(|prefix| {
            top_level.strip_prefix(prefix).is_some_and(|revision| {
                !revision.is_empty() && revision.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
}

fn managed_cloud_safe_inventory_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path.nfc().collect::<String>() == path
        && !path
            .chars()
            .any(|character| character.is_control() || character == '\u{fffd}')
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn managed_cloud_resolve_inventory_symlink(path: &str, target: &str) -> Option<String> {
    if !managed_cloud_safe_inventory_path(path)
        || target.is_empty()
        || target.contains('\\')
        || target.nfc().collect::<String>() != target
        || target
            .chars()
            .any(|character| character.is_control() || character == '\u{fffd}')
    {
        return None;
    }
    let mut parts = if target.starts_with('/') {
        Vec::new()
    } else {
        path.rsplit_once('/')
            .map(|(parent, _)| parent.split('/').map(str::to_string).collect())
            .unwrap_or_default()
    };
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part.to_string()),
        }
    }
    let resolved = parts.join("/");
    managed_cloud_safe_inventory_path(&resolved).then_some(resolved)
}

fn validate_managed_cloud_content_inventories(
    attachments: &ManagedCloudContentInventoryAttachments,
    release: &ManagedCloudReleaseAuthority,
    evidence: &ManagedCloudVerificationEvidenceAuthority,
) -> ManagedCloudResult<String> {
    let encoded_inventories = [
        &attachments.jobs_api,
        &attachments.jobs_portal,
        &attachments.jobs_runner,
        &attachments.jobs_workflows,
    ];
    let contracts: [ManagedCloudInventoryContract; 4] = [
        (
            "jobs-api",
            Some((
                "65532:65532",
                &["/usr/local/bin/bluey-jobs-api"],
                &[],
                &["8081/tcp"],
                &[
                    "BLUEY_JOBS_API_HOST=0.0.0.0",
                    "BLUEY_JOBS_API_PORT=8081",
                    "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                ],
            )),
        ),
        ("jobs-portal", None),
        (
            "jobs-runner",
            Some((
                "pwuser",
                &[],
                &[MANAGED_CLOUD_RUNNER_NODE_EXECUTABLE, "runner/dist/server.js"],
                &["8091/tcp"],
                &[
                    "BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs-runner",
                    "NODE_ENV=production",
                    "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                    "PLAYWRIGHT_BROWSERS_PATH=/ms-playwright",
                ],
            )),
        ),
        (
            "jobs-workflows",
            Some((
                "node",
                &[],
                &[
                    MANAGED_CLOUD_WORKFLOWS_NODE_EXECUTABLE,
                    "workflows/dist/worker.js",
                ],
                &[],
                &[
                    "NODE_ENV=production",
                    "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                ],
            )),
        ),
    ];
    let mut converter_sha256 = None;
    let mut decoded_total = 0_usize;
    for ((((encoded, component), evidence_component), expected_component), contract) in
        encoded_inventories
            .iter()
            .zip(&release.components)
            .zip(&evidence.components)
            .zip(contracts.map(|contract| contract.0))
            .zip(contracts)
    {
        let bytes = managed_cloud_decode_content_inventory(encoded)?;
        decoded_total = decoded_total
            .checked_add(bytes.len())
            .filter(|total| *total <= MANAGED_CLOUD_MAX_CONTENT_INVENTORY_BYTES)
            .ok_or(ManagedCloudRegistryError::InvalidEnvelope)?;
        let inventory: ManagedCloudContentInventoryAuthority =
            managed_cloud_parse_canonical(&bytes)?;
        let (_, runtime_contract) = contract;
        if inventory.version != 1
            || inventory.audience != "bluey-jobs-managed-cloud-content-inventory-v1"
            || inventory.component_id != expected_component
            || component.component_id != expected_component
            || evidence_component.component_id != expected_component
            || inventory.artifact_kind != component.artifact_kind
            || inventory.artifact_sha256 != component.artifact_sha256
            || managed_cloud_sha256(&bytes) != evidence_component.content_inventory_sha256
            || inventory.entries.is_empty()
            || inventory.entries.len() > 100_000
            || inventory
                .entries
                .windows(2)
                .any(|entries| entries[0].path >= entries[1].path)
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        for entry in &inventory.entries {
            let path_valid = managed_cloud_safe_inventory_path(&entry.path)
                && !entry.path.ends_with(".map")
                && !entry
                    .path
                    .split('/')
                    .any(|part| matches!(part, "src" | "test" | "tests"))
                && (expected_component != "jobs-portal"
                    || !entry.path.split('/').any(|part| part == "node_modules"));
            let mode_valid =
                entry.mode.len() == 4 && entry.mode.bytes().all(|byte| matches!(byte, b'0'..=b'7'));
            let shape_valid = match entry.entry_type.as_str() {
                "directory" => {
                    entry.sha256.is_none()
                        && entry.size_bytes.is_none()
                        && entry.target.is_none()
                        && entry.resolved_target.is_none()
                }
                "file" => {
                    entry.sha256.as_deref().is_some_and(managed_cloud_hex64)
                        && entry.size_bytes.is_some_and(|size| {
                            (0..=MANAGED_CLOUD_MAX_SAFE_INTEGER).contains(&size)
                        })
                        && entry.target.is_none()
                        && entry.resolved_target.is_none()
                }
                "symlink" => {
                    entry.sha256.is_none()
                        && entry.size_bytes.is_none()
                        && entry
                            .target
                            .as_ref()
                            .zip(entry.resolved_target.as_ref())
                            .is_some_and(|(target, resolved)| {
                                managed_cloud_resolve_inventory_symlink(&entry.path, target)
                                    .as_deref()
                                    == Some(resolved.as_str())
                            })
                }
                _ => false,
            };
            if !path_valid || !mode_valid || !shape_valid {
                return Err(ManagedCloudRegistryError::InvalidAuthority);
            }
        }
        let entries_by_path = inventory
            .entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect::<BTreeMap<_, _>>();
        if expected_component == "jobs-portal" {
            if inventory
                .entries
                .iter()
                .any(|entry| !matches!(entry.entry_type.as_str(), "directory" | "file"))
                || !inventory.entries.iter().any(|entry| {
                    entry.path == "index.html"
                        && entry.entry_type == "file"
                        && entry.size_bytes.is_some_and(|size| size > 0)
                        && entry.sha256.as_deref().is_some_and(managed_cloud_hex64)
                })
            {
                return Err(ManagedCloudRegistryError::InvalidAuthority);
            }
        } else {
            if inventory.entries.iter().any(|entry| {
                entry.entry_type == "symlink"
                    && managed_cloud_runtime_measurement_inventory_path(
                        expected_component,
                        &entry.path,
                    )
            }) {
                return Err(ManagedCloudRegistryError::InvalidAuthority);
            }
            for entry in inventory
                .entries
                .iter()
                .filter(|entry| entry.entry_type == "symlink")
            {
                let mut target = entry
                    .resolved_target
                    .as_deref()
                    .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
                let mut visited = BTreeSet::from([entry.path.as_str()]);
                loop {
                    if !visited.insert(target) {
                        return Err(ManagedCloudRegistryError::InvalidAuthority);
                    }
                    let resolved = entries_by_path
                        .get(target)
                        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
                    if resolved.entry_type != "symlink" {
                        break;
                    }
                    target = resolved
                        .resolved_target
                        .as_deref()
                        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
                }
            }
            let measurement_sha256 = evidence_component
                .runtime_measurement_sha256
                .as_deref()
                .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
            let measurement = evidence_component
                .runtime_measurement
                .as_ref()
                .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
            let measurement_entry = entries_by_path
                .get(MANAGED_CLOUD_RUNTIME_MEASUREMENT_PATH)
                .copied()
                .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
            let expected_measured_files = inventory
                .entries
                .iter()
                .filter(|entry| {
                    entry.entry_type == "file"
                        && entry.size_bytes.is_some_and(|size| size > 0)
                        && entry.sha256.as_deref().is_some_and(managed_cloud_hex64)
                        && managed_cloud_runtime_measurement_inventory_path(
                            expected_component,
                            &entry.path,
                        )
                })
                .map(|entry| {
                    (
                        entry.path.as_str(),
                        entry.sha256.as_deref().unwrap_or_default(),
                    )
                })
                .collect::<Vec<_>>();
            if measurement_entry.entry_type != "file"
                || measurement_entry.sha256.as_deref() != Some(measurement_sha256)
                || measurement_entry.size_bytes.is_none_or(|size| size <= 0)
                || measurement.measured_files.len() != expected_measured_files.len()
                || measurement
                    .measured_files
                    .iter()
                    .zip(expected_measured_files)
                    .any(|(measured, expected)| {
                        measured.path != expected.0 || measured.sha256 != expected.1
                    })
            {
                return Err(ManagedCloudRegistryError::InvalidAuthority);
            }
        }
        let required_paths = &evidence_component.required_paths;
        if required_paths.iter().any(|required| {
            let Some(entry) = entries_by_path.get(required.as_str()).copied() else {
                return true;
            };
            entry.entry_type != "file"
                || entry.size_bytes.is_none_or(|size| size <= 0)
                || entry
                    .sha256
                    .as_deref()
                    .is_none_or(|sha256| !managed_cloud_hex64(sha256))
        }) {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        match (runtime_contract, inventory.runtime.as_ref()) {
            (None, None) if component.artifact_kind == "static_bundle" => {}
            (Some((user, entrypoint, cmd, ports, environment)), Some(runtime))
                if component.artifact_kind == "oci_image"
                    && runtime.user == user
                    && runtime.working_directory == "/app"
                    && runtime
                        .entrypoint
                        .iter()
                        .map(String::as_str)
                        .eq(entrypoint.iter().copied())
                    && runtime
                        .cmd
                        .iter()
                        .map(String::as_str)
                        .eq(cmd.iter().copied())
                    && runtime
                        .exposed_ports
                        .iter()
                        .map(String::as_str)
                        .eq(ports.iter().copied())
                    && runtime
                        .environment
                        .iter()
                        .map(String::as_str)
                        .eq(environment.iter().copied()) => {}
            _ => return Err(ManagedCloudRegistryError::InvalidAuthority),
        }
        if expected_component == "jobs-runner" {
            let browser_inventory_exact = inventory
                .entries
                .iter()
                .all(|entry| managed_cloud_runner_playwright_inventory_path(&entry.path));
            let chromium_ready = inventory.entries.iter().any(|entry| {
                entry.entry_type == "file"
                    && managed_cloud_runner_headless_shell_path(&entry.path)
                    && i64::from_str_radix(&entry.mode, 8).is_ok_and(|mode| mode & 0o111 != 0)
            });
            if !browser_inventory_exact || !chromium_ready {
                return Err(ManagedCloudRegistryError::InvalidAuthority);
            }
        }
        if expected_component == "jobs-workflows" {
            converter_sha256 = inventory
                .entries
                .iter()
                .find(|entry| {
                    entry.path == "app/workflows/dist/failure-converter.js"
                        && entry.entry_type == "file"
                })
                .and_then(|entry| entry.sha256.clone());
        }
    }
    converter_sha256.ok_or(ManagedCloudRegistryError::InvalidAuthority)
}

fn managed_cloud_required_roles_for_feature(feature: &ManagedCloudFeatureAuthority) -> Vec<String> {
    let mut roles = MANAGED_CLOUD_BASE_RUNTIME_ROLES
        .iter()
        .map(|role| (*role).to_string())
        .collect::<Vec<_>>();
    if feature.direct_discovery {
        roles.push("discovery_worker".to_string());
    }
    if feature.global_discovery {
        roles.push("global_discovery_worker".to_string());
    }
    if feature.source_verification {
        roles.push("original_source_verifier".to_string());
    }
    roles.sort();
    roles
}

fn validate_managed_cloud_cohort(cohort: &ManagedCloudCohortAuthority) -> ManagedCloudResult<()> {
    validate_managed_cloud_scope(&cohort.scope)?;
    let valid_rollout = matches!(
        (cohort.scope.channel.as_str(), cohort.rollout_mode.as_str()),
        ("canary", "allowlist") | ("general", "all_eligible_accounts") | ("shadow", "none")
    );
    if cohort.version != 1
        || cohort.audience != MANAGED_CLOUD_COHORT_AUDIENCE
        || !managed_cloud_token(&cohort.cohort_id, 128)
        || !managed_cloud_safe_integer(cohort.cohort_generation, true)
        || !managed_cloud_safe_integer(cohort.trust_generation, true)
        || !valid_rollout
        || !managed_cloud_token(&cohort.approval_ref, 1024)
        || !managed_cloud_safe_integer(cohort.issued_at_ms, false)
        || cohort.not_before_ms < cohort.issued_at_ms
        || cohort.expires_at_ms <= cohort.not_before_ms
        || cohort.account_id_sha256s.len() > 512
        || (cohort.scope.channel == "canary") == cohort.account_id_sha256s.is_empty()
        || cohort
            .account_id_sha256s
            .iter()
            .any(|value| !managed_cloud_hex64(value))
        || cohort
            .account_id_sha256s
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn validate_managed_cloud_activation(
    activation: &ManagedCloudActivationAuthority,
) -> ManagedCloudResult<()> {
    validate_managed_cloud_scope(&activation.scope)?;
    if activation.version != 1
        || activation.audience != MANAGED_CLOUD_ACTIVATION_AUDIENCE
        || !managed_cloud_token(&activation.activation_id, 128)
        || !managed_cloud_safe_integer(activation.activation_generation, true)
        || !managed_cloud_safe_integer(activation.trust_generation, true)
        || !managed_cloud_safe_integer(activation.channel_sequence, true)
        || !managed_cloud_safe_integer(activation.expected_head_revision, false)
        || (activation.expected_head_revision == 0)
            != activation.expected_transition_sha256.is_none()
        || activation
            .expected_transition_sha256
            .as_ref()
            .is_some_and(|value| !managed_cloud_hex64(value))
        || activation
            .predecessor_activation_sha256
            .as_ref()
            .is_some_and(|value| !managed_cloud_hex64(value))
        || (activation.expected_head_revision == 0)
            != activation.predecessor_activation_sha256.is_none()
        || !managed_cloud_hex64(&activation.manifest_sha256)
        || !managed_cloud_hex64(&activation.manifest_signature_set_sha256)
        || !managed_cloud_hex64(&activation.cohort_sha256)
        || !managed_cloud_hex64(&activation.cohort_signature_set_sha256)
        || !managed_cloud_hex64(&activation.feature_authority_sha256)
        || managed_cloud_digest(&activation.feature_authority)?
            != activation.feature_authority_sha256
        || !activation.feature_authority.cloud_distribution
        || !activation.feature_authority.workflow_command_dispatch
        || !activation.feature_authority.workflow_cleanup
        || activation.feature_authority.direct_discovery
        || activation.feature_authority.global_discovery
        || activation.feature_authority.source_verification
        || !managed_cloud_hex64(&activation.runner_fleet_evidence_sha256)
        || !managed_cloud_hex64(&activation.cleanup_authority_sha256)
        || !managed_cloud_hex64(&activation.temporal_namespace_sha256)
        || !managed_cloud_hex64(&activation.storage_config_sha256)
        || !managed_cloud_hex64(&activation.task_queue_sha256)
        || !managed_cloud_hex64(&activation.failure_converter_sha256)
        || !managed_cloud_hex64(&activation.canary_evidence_sha256)
        || !managed_cloud_hex64(&activation.portal_readback_evidence_sha256)
        || !(60_000..=2_592_000_000).contains(&activation.portal_readback_ttl_ms)
        || !(5_000..=300_000).contains(&activation.heartbeat_ttl_ms)
        || activation.portal_readback_at_ms > activation.issued_at_ms
        || activation
            .portal_readback_at_ms
            .checked_add(activation.portal_readback_ttl_ms)
            .is_none_or(|expiry| activation.expires_at_ms > expiry)
        || activation.not_before_ms < activation.issued_at_ms
        || activation.expires_at_ms <= activation.not_before_ms
        || activation.recovery_acceptances.len() > 32
        || activation.recovery_acceptances.iter().any(|entry| {
            !managed_cloud_hex64(&entry.activation_sha256)
                || !managed_cloud_hex64(&entry.manifest_sha256)
        })
        || activation.recovery_acceptances.windows(2).any(|pair| {
            (&pair[0].activation_sha256, &pair[0].manifest_sha256)
                >= (&pair[1].activation_sha256, &pair[1].manifest_sha256)
        })
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let customer = activation.scope.channel != "shadow";
    if customer != (activation.maximum_inflight > 0 && activation.maximum_daily_admissions > 0)
        || (!customer
            && (activation.maximum_inflight != 0 || activation.maximum_daily_admissions != 0))
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn parse_managed_cloud_activation_evidence(
    attachments: &ManagedCloudActivationEvidenceAttachments,
) -> ManagedCloudResult<ParsedManagedCloudActivationEvidence> {
    let canary_bytes = managed_cloud_decode_base64url(&attachments.canary_base64url)?;
    let cleanup_bytes = managed_cloud_decode_base64url(&attachments.cleanup_base64url)?;
    let failure_converter_bytes =
        managed_cloud_decode_base64url(&attachments.failure_converter_base64url)?;
    let portal_bytes = managed_cloud_decode_base64url(&attachments.portal_readback_base64url)?;
    let runner_bytes = managed_cloud_decode_base64url(&attachments.runner_fleet_base64url)?;
    let storage_bytes = managed_cloud_decode_base64url(&attachments.storage_base64url)?;
    let task_queue_bytes = managed_cloud_decode_base64url(&attachments.task_queue_base64url)?;
    let temporal_bytes = managed_cloud_decode_base64url(&attachments.temporal_namespace_base64url)?;
    let total_bytes = [
        canary_bytes.len(),
        cleanup_bytes.len(),
        failure_converter_bytes.len(),
        portal_bytes.len(),
        runner_bytes.len(),
        storage_bytes.len(),
        task_queue_bytes.len(),
        temporal_bytes.len(),
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(ManagedCloudRegistryError::InvalidEnvelope)?;
    if total_bytes > 1024 * 1024 {
        return Err(ManagedCloudRegistryError::InvalidEnvelope);
    }
    Ok(ParsedManagedCloudActivationEvidence {
        canary: managed_cloud_parse_canonical(&canary_bytes)?,
        cleanup: managed_cloud_parse_canonical(&cleanup_bytes)?,
        failure_converter: managed_cloud_parse_canonical(&failure_converter_bytes)?,
        portal: managed_cloud_parse_canonical(&portal_bytes)?,
        runner: managed_cloud_parse_canonical(&runner_bytes)?,
        storage: managed_cloud_parse_canonical(&storage_bytes)?,
        task_queue: managed_cloud_parse_canonical(&task_queue_bytes)?,
        temporal: managed_cloud_parse_canonical(&temporal_bytes)?,
        canary_bytes,
        portal_bytes,
        runner_bytes,
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_managed_cloud_evidence_envelope(
    version: i64,
    audience: &str,
    expected_audience: &str,
    manifest_sha256: &str,
    scope: &ManagedCloudScope,
    status: &str,
    observed_at_ms: i64,
    expires_at_ms: i64,
    activation: &ManagedCloudActivationAuthority,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    if version != 1
        || audience != expected_audience
        || manifest_sha256 != activation.manifest_sha256
        || scope != &activation.scope
        || status != "pass"
        || !managed_cloud_safe_integer(observed_at_ms, false)
        || !managed_cloud_safe_integer(expires_at_ms, true)
        || observed_at_ms > activation.issued_at_ms
        || expires_at_ms <= observed_at_ms
        || expires_at_ms - observed_at_ms > 2_592_000_000
        || activation.expires_at_ms > expires_at_ms
        || now_ms < observed_at_ms
        || now_ms >= expires_at_ms
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn validate_managed_cloud_activation_evidence(
    evidence: &ParsedManagedCloudActivationEvidence,
    activation: &ManagedCloudActivationAuthority,
    portal_artifact_sha256: &str,
    manifest_failure_converter_sha256: &str,
    now_ms: i64,
) -> ManagedCloudResult<ValidatedManagedCloudActivationEvidence> {
    let envelope = |version,
                    audience: &str,
                    expected_audience: &str,
                    manifest_sha256: &str,
                    scope: &ManagedCloudScope,
                    status: &str,
                    observed_at_ms,
                    expires_at_ms| {
        validate_managed_cloud_evidence_envelope(
            version,
            audience,
            expected_audience,
            manifest_sha256,
            scope,
            status,
            observed_at_ms,
            expires_at_ms,
            activation,
            now_ms,
        )
    };
    envelope(
        evidence.canary.version,
        &evidence.canary.audience,
        MANAGED_CLOUD_CANARY_EVIDENCE_AUDIENCE,
        &evidence.canary.manifest_sha256,
        &evidence.canary.scope,
        &evidence.canary.status,
        evidence.canary.observed_at_ms,
        evidence.canary.expires_at_ms,
    )?;
    envelope(
        evidence.cleanup.version,
        &evidence.cleanup.audience,
        MANAGED_CLOUD_CLEANUP_EVIDENCE_AUDIENCE,
        &evidence.cleanup.manifest_sha256,
        &evidence.cleanup.scope,
        &evidence.cleanup.status,
        evidence.cleanup.observed_at_ms,
        evidence.cleanup.expires_at_ms,
    )?;
    envelope(
        evidence.failure_converter.version,
        &evidence.failure_converter.audience,
        MANAGED_CLOUD_FAILURE_CONVERTER_EVIDENCE_AUDIENCE,
        &evidence.failure_converter.manifest_sha256,
        &evidence.failure_converter.scope,
        &evidence.failure_converter.status,
        evidence.failure_converter.observed_at_ms,
        evidence.failure_converter.expires_at_ms,
    )?;
    envelope(
        evidence.portal.version,
        &evidence.portal.audience,
        MANAGED_CLOUD_PORTAL_READBACK_EVIDENCE_AUDIENCE,
        &evidence.portal.manifest_sha256,
        &evidence.portal.scope,
        &evidence.portal.status,
        evidence.portal.observed_at_ms,
        evidence.portal.expires_at_ms,
    )?;
    envelope(
        evidence.runner.version,
        &evidence.runner.audience,
        MANAGED_CLOUD_RUNNER_FLEET_EVIDENCE_AUDIENCE,
        &evidence.runner.manifest_sha256,
        &evidence.runner.scope,
        &evidence.runner.status,
        evidence.runner.observed_at_ms,
        evidence.runner.expires_at_ms,
    )?;
    envelope(
        evidence.storage.version,
        &evidence.storage.audience,
        MANAGED_CLOUD_STORAGE_EVIDENCE_AUDIENCE,
        &evidence.storage.manifest_sha256,
        &evidence.storage.scope,
        &evidence.storage.status,
        evidence.storage.observed_at_ms,
        evidence.storage.expires_at_ms,
    )?;
    envelope(
        evidence.task_queue.version,
        &evidence.task_queue.audience,
        MANAGED_CLOUD_TASK_QUEUE_EVIDENCE_AUDIENCE,
        &evidence.task_queue.manifest_sha256,
        &evidence.task_queue.scope,
        &evidence.task_queue.status,
        evidence.task_queue.observed_at_ms,
        evidence.task_queue.expires_at_ms,
    )?;
    envelope(
        evidence.temporal.version,
        &evidence.temporal.audience,
        MANAGED_CLOUD_TEMPORAL_EVIDENCE_AUDIENCE,
        &evidence.temporal.manifest_sha256,
        &evidence.temporal.scope,
        &evidence.temporal.status,
        evidence.temporal.observed_at_ms,
        evidence.temporal.expires_at_ms,
    )?;

    let mut expected_check_ids = vec![
        "artifact-registry-readback",
        "jobs-api-runtime-artifact-attestation",
        "jobs-api-readiness",
        "jobs-portal-readback",
        "jobs-workflows-runtime-artifact-attestation",
        "managed-runner-runtime-artifact-attestation",
        "managed-runner-readiness",
        "migration-contract-readback",
        "protocol-contract-readback",
        "read-only-rootfs-policy",
        "workflow-cleanup-dispatcher-readiness",
        "workflow-command-dispatcher-readiness",
        "workflow-gateway-readiness",
        "workflow-worker-readiness",
    ];
    if activation.feature_authority.direct_discovery {
        expected_check_ids.push("discovery-worker-readiness");
    }
    if activation.feature_authority.global_discovery {
        expected_check_ids.push("global-discovery-worker-readiness");
    }
    expected_check_ids.sort_unstable();
    if evidence.canary.checks.len() != expected_check_ids.len()
        || evidence
            .canary
            .checks
            .iter()
            .zip(expected_check_ids)
            .any(|(check, expected_id)| {
                check.check_id != expected_id
                    || check.status != "pass"
                    || !managed_cloud_hex64(&check.evidence_sha256)
            })
        || !managed_cloud_hex64(&evidence.cleanup.cleanup_authority_sha256)
        || !evidence.cleanup.dispatcher_ready
        || evidence.failure_converter.deployed_file_sha256 != manifest_failure_converter_sha256
        || evidence.portal.portal_artifact_sha256 != portal_artifact_sha256
        || evidence.portal.readback_sha256 != portal_artifact_sha256
        || evidence.portal.observed_at_ms != activation.portal_readback_at_ms
        || !managed_cloud_hex64(&evidence.runner.fleet_id_sha256)
        || !managed_cloud_safe_integer(evidence.runner.required_instances, true)
        || evidence.runner.ready_instances < evidence.runner.required_instances
        || !managed_cloud_safe_integer(evidence.runner.ready_instances, true)
        || !managed_cloud_hex64(&evidence.storage.read_probe_sha256)
        || !managed_cloud_hex64(&evidence.storage.storage_config_sha256)
        || !managed_cloud_hex64(&evidence.storage.write_probe_sha256)
        || managed_cloud_task_queue_sha256(
            &evidence.task_queue.namespace,
            &evidence.task_queue.task_queue,
        )? != evidence.task_queue.task_queue_sha256
        || evidence.temporal.namespace != evidence.task_queue.namespace
        || managed_cloud_sha256(evidence.temporal.namespace.as_bytes())
            != evidence.temporal.namespace_sha256
        || !evidence.temporal.gateway_ready
        || !evidence.temporal.workflow_worker_ready
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }

    Ok(ValidatedManagedCloudActivationEvidence {
        canary_sha256: managed_cloud_sha256(&evidence.canary_bytes),
        cleanup_authority_sha256: evidence.cleanup.cleanup_authority_sha256.clone(),
        failure_converter_sha256: evidence.failure_converter.deployed_file_sha256.clone(),
        portal_readback_sha256: managed_cloud_sha256(&evidence.portal_bytes),
        runner_fleet_sha256: managed_cloud_sha256(&evidence.runner_bytes),
        storage_config_sha256: evidence.storage.storage_config_sha256.clone(),
        task_queue_sha256: evidence.task_queue.task_queue_sha256.clone(),
        temporal_namespace_sha256: evidence.temporal.namespace_sha256.clone(),
        portal_expires_at_ms: evidence.portal.expires_at_ms,
    })
}

fn validate_managed_cloud_rollback(
    rollback: &ManagedCloudRollbackAuthority,
) -> ManagedCloudResult<()> {
    validate_managed_cloud_scope(&rollback.scope)?;
    if rollback.version != 1
        || rollback.audience != MANAGED_CLOUD_ROLLBACK_AUDIENCE
        || !managed_cloud_token(&rollback.rollback_id, 128)
        || !managed_cloud_safe_integer(rollback.rollback_generation, true)
        || !managed_cloud_safe_integer(rollback.trust_generation, true)
        || !managed_cloud_safe_integer(rollback.expected_head_revision, true)
        || !managed_cloud_hex64(&rollback.expected_transition_sha256)
        || !managed_cloud_hex64(&rollback.from_activation_sha256)
        || !managed_cloud_hex64(&rollback.from_manifest_sha256)
        || !managed_cloud_hex64(&rollback.to_activation_sha256)
        || !managed_cloud_hex64(&rollback.to_manifest_sha256)
        || rollback.from_activation_sha256 == rollback.to_activation_sha256
        || !managed_cloud_hex64(&rollback.evidence_sha256)
        || !managed_cloud_token(&rollback.reason_ref, 1024)
        || !managed_cloud_safe_integer(rollback.issued_at_ms, false)
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn validate_managed_cloud_revocation(
    revocation: &ManagedCloudRevocationAuthority,
) -> ManagedCloudResult<()> {
    if revocation.version != 1
        || revocation.audience != MANAGED_CLOUD_REVOCATION_AUDIENCE
        || !managed_cloud_token(&revocation.revocation_id, 128)
        || !managed_cloud_safe_integer(revocation.revocation_generation, true)
        || !managed_cloud_safe_integer(revocation.trust_generation, true)
        || !matches!(
            revocation.subject_kind.as_str(),
            "activation"
                | "cohort"
                | "component"
                | "manifest"
                | "release"
                | "rollback"
                | "runtime_grant"
                | "runtime_instance"
                | "signing_key"
                | "trust_policy"
        )
        || !managed_cloud_token(&revocation.subject_id, 128)
        || !managed_cloud_hex64(&revocation.subject_sha256)
        || !managed_cloud_token(&revocation.reason_ref, 1024)
        || revocation
            .predecessor_revocation_sha256
            .as_ref()
            .is_some_and(|value| !managed_cloud_hex64(value))
        || (revocation.revocation_generation == 1)
            != revocation.predecessor_revocation_sha256.is_none()
        || revocation.effective_at_ms < revocation.issued_at_ms
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn managed_cloud_actor(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[derive(Debug, Clone)]
struct StoredManagedCloudTrustPolicy {
    policy_sha256: String,
    authorization_signature_set_sha256: String,
    canonical_policy_base64url: String,
    policy: ManagedCloudTrustPolicyAuthority,
}

fn managed_cloud_policy_threshold(
    policy: &ManagedCloudTrustPolicyAuthority,
    role: &str,
) -> ManagedCloudResult<i64> {
    policy
        .roles
        .iter()
        .find(|candidate| candidate.role == role)
        .map(|candidate| candidate.threshold)
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)
}

fn stored_managed_cloud_policy(
    policy_sha256: String,
    authorization_signature_set_sha256: String,
    canonical_policy_base64url: String,
) -> ManagedCloudResult<StoredManagedCloudTrustPolicy> {
    let canonical_bytes = managed_cloud_decode_base64url(&canonical_policy_base64url)?;
    let policy = managed_cloud_parse_canonical(&canonical_bytes)?;
    if managed_cloud_sha256(&canonical_bytes) != policy_sha256 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(StoredManagedCloudTrustPolicy {
        policy_sha256,
        authorization_signature_set_sha256,
        canonical_policy_base64url,
        policy,
    })
}

fn sqlite_latest_managed_cloud_policy(
    tx: &rusqlite::Transaction<'_>,
) -> ManagedCloudResult<Option<StoredManagedCloudTrustPolicy>> {
    tx.query_row(
        "SELECT policy_sha256,authorization_signature_set_sha256,
                canonical_policy_base64url
           FROM jobs_managed_cloud_trust_policies
          ORDER BY trust_generation DESC LIMIT 1",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )
    .optional()
    .map_err(managed_cloud_storage)?
    .map(|(sha256, signature_sha256, canonical)| {
        stored_managed_cloud_policy(sha256, signature_sha256, canonical)
    })
    .transpose()
}

fn postgres_latest_managed_cloud_policy(
    tx: &mut postgres::Transaction<'_>,
) -> ManagedCloudResult<Option<StoredManagedCloudTrustPolicy>> {
    tx.query_opt(
        "SELECT policy_sha256,authorization_signature_set_sha256,
                canonical_policy_base64url
           FROM jobs_managed_cloud_trust_policies
          ORDER BY trust_generation DESC LIMIT 1 FOR UPDATE",
        &[],
    )
    .map_err(managed_cloud_storage)?
    .map(|row| stored_managed_cloud_policy(row.get(0), row.get(1), row.get(2)))
    .transpose()
}

fn sqlite_managed_cloud_policy_identity(
    tx: &rusqlite::Transaction<'_>,
    policy_sha256: &str,
    policy_id: &str,
    trust_generation: i64,
) -> ManagedCloudResult<Option<StoredManagedCloudTrustPolicy>> {
    tx.query_row(
        "SELECT policy_sha256,authorization_signature_set_sha256,
                canonical_policy_base64url
           FROM jobs_managed_cloud_trust_policies
          WHERE policy_sha256=?1 OR policy_id=?2 OR trust_generation=?3
          LIMIT 1",
        params![policy_sha256, policy_id, trust_generation],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )
    .optional()
    .map_err(managed_cloud_storage)?
    .map(|(sha256, signature_sha256, canonical)| {
        stored_managed_cloud_policy(sha256, signature_sha256, canonical)
    })
    .transpose()
}

fn postgres_managed_cloud_policy_identity(
    tx: &mut postgres::Transaction<'_>,
    policy_sha256: &str,
    policy_id: &str,
    trust_generation: i64,
) -> ManagedCloudResult<Option<StoredManagedCloudTrustPolicy>> {
    tx.query_opt(
        "SELECT policy_sha256,authorization_signature_set_sha256,
                canonical_policy_base64url
           FROM jobs_managed_cloud_trust_policies
          WHERE policy_sha256=$1 OR policy_id=$2 OR trust_generation=$3
          LIMIT 1 FOR UPDATE",
        &[&policy_sha256, &policy_id, &trust_generation],
    )
    .map_err(managed_cloud_storage)?
    .map(|row| stored_managed_cloud_policy(row.get(0), row.get(1), row.get(2)))
    .transpose()
}

fn sqlite_managed_cloud_policy_by_generation(
    tx: &rusqlite::Transaction<'_>,
    trust_generation: i64,
) -> ManagedCloudResult<StoredManagedCloudTrustPolicy> {
    tx.query_row(
        "SELECT policy_sha256,authorization_signature_set_sha256,
                canonical_policy_base64url
           FROM jobs_managed_cloud_trust_policies WHERE trust_generation=?1",
        params![trust_generation],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )
    .optional()
    .map_err(managed_cloud_storage)?
    .ok_or(ManagedCloudRegistryError::Unavailable)
    .and_then(|(sha256, signature_sha256, canonical)| {
        stored_managed_cloud_policy(sha256, signature_sha256, canonical)
    })
}

fn postgres_managed_cloud_policy_by_generation(
    tx: &mut postgres::Transaction<'_>,
    trust_generation: i64,
) -> ManagedCloudResult<StoredManagedCloudTrustPolicy> {
    tx.query_opt(
        "SELECT policy_sha256,authorization_signature_set_sha256,
                canonical_policy_base64url
           FROM jobs_managed_cloud_trust_policies
          WHERE trust_generation=$1 FOR SHARE",
        &[&trust_generation],
    )
    .map_err(managed_cloud_storage)?
    .ok_or(ManagedCloudRegistryError::Unavailable)
    .and_then(|row| stored_managed_cloud_policy(row.get(0), row.get(1), row.get(2)))
}

fn require_sqlite_managed_cloud_signing_authority_not_revoked(
    tx: &rusqlite::Transaction<'_>,
    policy: &StoredManagedCloudTrustPolicy,
    signature_set: &ManagedCloudSignatureSetAuthority,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    let policy_revoked = tx
        .query_row(
            "SELECT 1 FROM jobs_managed_cloud_revocations
              WHERE effective_at_ms<=?1 AND subject_kind='trust_policy'
                AND subject_id=?2 AND subject_sha256=?3 LIMIT 1",
            params![now_ms, policy.policy.policy_id, policy.policy_sha256],
            |_| Ok(()),
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .is_some();
    if policy_revoked {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    for signature in &signature_set.signatures {
        let key_revoked = tx
            .query_row(
                "SELECT 1 FROM jobs_managed_cloud_revocations
                  WHERE effective_at_ms<=?1 AND subject_kind='signing_key'
                    AND subject_id=?2 LIMIT 1",
                params![now_ms, signature.key_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(managed_cloud_storage)?
            .is_some();
        if key_revoked {
            return Err(ManagedCloudRegistryError::Revoked);
        }
    }
    Ok(())
}

fn require_postgres_managed_cloud_signing_authority_not_revoked(
    tx: &mut postgres::Transaction<'_>,
    policy: &StoredManagedCloudTrustPolicy,
    signature_set: &ManagedCloudSignatureSetAuthority,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    if tx
        .query_opt(
            "SELECT 1 FROM jobs_managed_cloud_revocations
              WHERE effective_at_ms<=$1 AND subject_kind='trust_policy'
                AND subject_id=$2 AND subject_sha256=$3 LIMIT 1",
            &[&now_ms, &policy.policy.policy_id, &policy.policy_sha256],
        )
        .map_err(managed_cloud_storage)?
        .is_some()
    {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    for signature in &signature_set.signatures {
        if tx
            .query_opt(
                "SELECT 1 FROM jobs_managed_cloud_revocations
                  WHERE effective_at_ms<=$1 AND subject_kind='signing_key'
                    AND subject_id=$2 LIMIT 1",
                &[&now_ms, &signature.key_id],
            )
            .map_err(managed_cloud_storage)?
            .is_some()
        {
            return Err(ManagedCloudRegistryError::Revoked);
        }
    }
    Ok(())
}

fn require_sqlite_managed_cloud_signature_set_replay(
    tx: &rusqlite::Transaction<'_>,
    signature_set: &ManagedCloudSignatureSetAuthority,
    signature_set_sha256: &str,
    canonical_base64url: &str,
) -> ManagedCloudResult<bool> {
    let existing = tx
        .query_row(
            "SELECT signature_set_sha256,canonical_signature_set_base64url
               FROM jobs_managed_cloud_signature_sets
              WHERE signature_set_sha256=?1 OR signature_set_id=?2
                 OR (target_audience=?3 AND target_sha256=?4
                     AND trust_generation=?5 AND role=?6)
              LIMIT 1",
            params![
                signature_set_sha256,
                signature_set.signature_set_id,
                signature_set.target_audience,
                signature_set.target_sha256,
                signature_set.trust_generation,
                signature_set.role,
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(managed_cloud_storage)?;
    if let Some((stored_sha256, stored_canonical)) = existing {
        if stored_sha256 != signature_set_sha256 || stored_canonical != canonical_base64url {
            return Err(ManagedCloudRegistryError::IdentityConflict);
        }
        return Ok(true);
    }
    Ok(false)
}

fn require_postgres_managed_cloud_signature_set_replay(
    tx: &mut postgres::Transaction<'_>,
    signature_set: &ManagedCloudSignatureSetAuthority,
    signature_set_sha256: &str,
    canonical_base64url: &str,
) -> ManagedCloudResult<bool> {
    let existing = tx
        .query_opt(
            "SELECT signature_set_sha256,canonical_signature_set_base64url
               FROM jobs_managed_cloud_signature_sets
              WHERE signature_set_sha256=$1 OR signature_set_id=$2
                 OR (target_audience=$3 AND target_sha256=$4
                     AND trust_generation=$5 AND role=$6)
              LIMIT 1 FOR UPDATE",
            &[
                &signature_set_sha256,
                &signature_set.signature_set_id,
                &signature_set.target_audience,
                &signature_set.target_sha256,
                &signature_set.trust_generation,
                &signature_set.role,
            ],
        )
        .map_err(managed_cloud_storage)?;
    if let Some(row) = existing {
        let stored_sha256: String = row.get(0);
        let stored_canonical: String = row.get(1);
        if stored_sha256 != signature_set_sha256 || stored_canonical != canonical_base64url {
            return Err(ManagedCloudRegistryError::IdentityConflict);
        }
        return Ok(true);
    }
    Ok(false)
}

fn insert_sqlite_managed_cloud_signature_set(
    tx: &rusqlite::Transaction<'_>,
    signature_set: &ManagedCloudSignatureSetAuthority,
    signature_set_sha256: &str,
    canonical_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> ManagedCloudResult<()> {
    if require_sqlite_managed_cloud_signature_set_replay(
        tx,
        signature_set,
        signature_set_sha256,
        canonical_base64url,
    )? {
        return Ok(());
    }
    tx.execute(
        "INSERT INTO jobs_managed_cloud_signature_sets(
           signature_set_sha256,signature_set_id,trust_generation,role,
           target_audience,target_sha256,signed_at_ms,signature_count,
           canonical_signature_set_base64url,recorded_by,recorded_at_ms
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            signature_set_sha256,
            signature_set.signature_set_id,
            signature_set.trust_generation,
            signature_set.role,
            signature_set.target_audience,
            signature_set.target_sha256,
            signature_set.signed_at_ms,
            i64::try_from(signature_set.signatures.len())
                .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            canonical_base64url,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    for signature in &signature_set.signatures {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_signatures(
               signature_set_sha256,key_id,signature_base64url
             ) VALUES(?1,?2,?3)",
            params![signature_set_sha256, signature.key_id, signature.signature],
        )
        .map_err(managed_cloud_storage)?;
    }
    Ok(())
}

fn insert_postgres_managed_cloud_signature_set(
    tx: &mut postgres::Transaction<'_>,
    signature_set: &ManagedCloudSignatureSetAuthority,
    signature_set_sha256: &str,
    canonical_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> ManagedCloudResult<()> {
    if require_postgres_managed_cloud_signature_set_replay(
        tx,
        signature_set,
        signature_set_sha256,
        canonical_base64url,
    )? {
        return Ok(());
    }
    let signature_count = i64::try_from(signature_set.signatures.len())
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    tx.execute(
        "INSERT INTO jobs_managed_cloud_signature_sets(
           signature_set_sha256,signature_set_id,trust_generation,role,
           target_audience,target_sha256,signed_at_ms,signature_count,
           canonical_signature_set_base64url,recorded_by,recorded_at_ms
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        &[
            &signature_set_sha256,
            &signature_set.signature_set_id,
            &signature_set.trust_generation,
            &signature_set.role,
            &signature_set.target_audience,
            &signature_set.target_sha256,
            &signature_set.signed_at_ms,
            &signature_count,
            &canonical_base64url,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    for signature in &signature_set.signatures {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_signatures(
               signature_set_sha256,key_id,signature_base64url
             ) VALUES($1,$2,$3)",
            &[
                &signature_set_sha256,
                &signature.key_id,
                &signature.signature,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    Ok(())
}

fn insert_sqlite_managed_cloud_policy(
    tx: &rusqlite::Transaction<'_>,
    policy: &ManagedCloudTrustPolicyAuthority,
    policy_sha256: &str,
    canonical_base64url: &str,
    signature_set_sha256: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> ManagedCloudResult<()> {
    tx.execute(
        "INSERT INTO jobs_managed_cloud_trust_policies(
           policy_sha256,policy_id,trust_generation,predecessor_policy_sha256,
           predecessor_trust_generation,root_threshold,release_threshold,
           promotion_threshold,general_promotion_threshold,incident_threshold,
           key_count,canonical_policy_base64url,authorization_signature_set_sha256,
           issued_at_ms,valid_from_ms,expires_at_ms,recorded_by,recorded_at_ms
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        params![
            policy_sha256,
            policy.policy_id,
            policy.trust_generation,
            policy.predecessor_policy_sha256,
            policy.trust_generation - 1,
            managed_cloud_policy_threshold(policy, "root")?,
            managed_cloud_policy_threshold(policy, "release")?,
            managed_cloud_policy_threshold(policy, "promotion")?,
            managed_cloud_policy_threshold(policy, "general_promotion")?,
            managed_cloud_policy_threshold(policy, "incident")?,
            i64::try_from(policy.keys.len())
                .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            canonical_base64url,
            signature_set_sha256,
            policy.issued_at_ms,
            policy.valid_from_ms,
            policy.expires_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    for key in &policy.keys {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_trust_keys(
               policy_sha256,trust_generation,key_id,role,public_key_base64url,state,
               valid_from_ms,valid_until_ms,minimum_trust_generation,
               maximum_trust_generation
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                policy_sha256,
                policy.trust_generation,
                key.key_id,
                key.role,
                key.public_key,
                key.state,
                key.valid_from_ms,
                key.valid_until_ms,
                key.minimum_trust_generation,
                key.maximum_trust_generation,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    Ok(())
}

fn insert_postgres_managed_cloud_policy(
    tx: &mut postgres::Transaction<'_>,
    policy: &ManagedCloudTrustPolicyAuthority,
    policy_sha256: &str,
    canonical_base64url: &str,
    signature_set_sha256: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> ManagedCloudResult<()> {
    let predecessor_generation = policy.trust_generation - 1;
    let key_count = i64::try_from(policy.keys.len())
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    tx.execute(
        "INSERT INTO jobs_managed_cloud_trust_policies(
           policy_sha256,policy_id,trust_generation,predecessor_policy_sha256,
           predecessor_trust_generation,root_threshold,release_threshold,
           promotion_threshold,general_promotion_threshold,incident_threshold,
           key_count,canonical_policy_base64url,authorization_signature_set_sha256,
           issued_at_ms,valid_from_ms,expires_at_ms,recorded_by,recorded_at_ms
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
        &[
            &policy_sha256,
            &policy.policy_id,
            &policy.trust_generation,
            &policy.predecessor_policy_sha256,
            &predecessor_generation,
            &managed_cloud_policy_threshold(policy, "root")?,
            &managed_cloud_policy_threshold(policy, "release")?,
            &managed_cloud_policy_threshold(policy, "promotion")?,
            &managed_cloud_policy_threshold(policy, "general_promotion")?,
            &managed_cloud_policy_threshold(policy, "incident")?,
            &key_count,
            &canonical_base64url,
            &signature_set_sha256,
            &policy.issued_at_ms,
            &policy.valid_from_ms,
            &policy.expires_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    for key in &policy.keys {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_trust_keys(
               policy_sha256,trust_generation,key_id,role,public_key_base64url,state,
               valid_from_ms,valid_until_ms,minimum_trust_generation,
               maximum_trust_generation
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            &[
                &policy_sha256,
                &policy.trust_generation,
                &key.key_id,
                &key.role,
                &key.public_key,
                &key.state,
                &key.valid_from_ms,
                &key.valid_until_ms,
                &key.minimum_trust_generation,
                &key.maximum_trust_generation,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    Ok(())
}

fn managed_cloud_policy_import_result(
    policy: &ManagedCloudTrustPolicyAuthority,
    policy_sha256: &str,
    signature_set_sha256: &str,
    replayed: bool,
) -> ManagedCloudImportResult {
    ManagedCloudImportResult {
        authority_kind: "trust_policy".to_string(),
        authority_id: policy.policy_id.clone(),
        authority_sha256: policy_sha256.to_string(),
        signature_set_sha256: signature_set_sha256.to_string(),
        trust_policy_sha256: policy_sha256.to_string(),
        replayed,
    }
}

pub fn import_managed_cloud_trust_policy(
    pool: &DbPool,
    envelope: &ManagedCloudAuthorityEnvelope,
    recorded_by: &str,
) -> ManagedCloudResult<ManagedCloudImportResult> {
    if !managed_cloud_actor(recorded_by) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let (policy_bytes, signature_set_bytes) = managed_cloud_decode_envelope(envelope)?;
    let policy: ManagedCloudTrustPolicyAuthority = managed_cloud_parse_canonical(&policy_bytes)?;
    let signature_set: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&signature_set_bytes)?;
    validate_managed_cloud_trust_policy(&policy)?;
    validate_managed_cloud_signature_set(&signature_set)?;
    let policy_sha256 = managed_cloud_sha256(&policy_bytes);
    let signature_set_sha256 = managed_cloud_sha256(&signature_set_bytes);

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            if let Some(existing) = sqlite_managed_cloud_policy_identity(
                &tx,
                &policy_sha256,
                &policy.policy_id,
                policy.trust_generation,
            )? {
                if existing.policy_sha256 != policy_sha256
                    || existing.canonical_policy_base64url != envelope.canonical_base64url
                    || existing.authorization_signature_set_sha256 != signature_set_sha256
                    || !require_sqlite_managed_cloud_signature_set_replay(
                        &tx,
                        &signature_set,
                        &signature_set_sha256,
                        &envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_policy_import_result(
                    &policy,
                    &policy_sha256,
                    &signature_set_sha256,
                    true,
                ));
            }
            let latest = sqlite_latest_managed_cloud_policy(&tx)?;
            match latest.as_ref() {
                None if policy.trust_generation == 1 => {
                    let anchor = managed_cloud_root_anchor_from_environment()?;
                    verify_managed_cloud_bootstrap_union_signature_set(
                        &policy_bytes,
                        &signature_set,
                        &anchor,
                        &policy,
                        now_ms,
                    )?;
                }
                Some(predecessor)
                    if policy.trust_generation == predecessor.policy.trust_generation + 1 =>
                {
                    verify_managed_cloud_rotation_union_signature_set(
                        &policy_bytes,
                        &signature_set,
                        &predecessor.policy,
                        &policy,
                        now_ms,
                    )?;
                }
                _ => return Err(ManagedCloudRegistryError::SequenceRegression),
            }
            insert_sqlite_managed_cloud_signature_set(
                &tx,
                &signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            insert_sqlite_managed_cloud_policy(
                &tx,
                &policy,
                &policy_sha256,
                &envelope.canonical_base64url,
                &signature_set_sha256,
                recorded_by,
                now_ms,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_policy_import_result(
                &policy,
                &policy_sha256,
                &signature_set_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                   hashtextextended('jobs-managed-cloud-release-registry',0))",
                &[],
            )
            .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            if let Some(existing) = postgres_managed_cloud_policy_identity(
                &mut tx,
                &policy_sha256,
                &policy.policy_id,
                policy.trust_generation,
            )? {
                if existing.policy_sha256 != policy_sha256
                    || existing.canonical_policy_base64url != envelope.canonical_base64url
                    || existing.authorization_signature_set_sha256 != signature_set_sha256
                    || !require_postgres_managed_cloud_signature_set_replay(
                        &mut tx,
                        &signature_set,
                        &signature_set_sha256,
                        &envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_policy_import_result(
                    &policy,
                    &policy_sha256,
                    &signature_set_sha256,
                    true,
                ));
            }
            let latest = postgres_latest_managed_cloud_policy(&mut tx)?;
            match latest.as_ref() {
                None if policy.trust_generation == 1 => {
                    let anchor = managed_cloud_root_anchor_from_environment()?;
                    verify_managed_cloud_bootstrap_union_signature_set(
                        &policy_bytes,
                        &signature_set,
                        &anchor,
                        &policy,
                        now_ms,
                    )?;
                }
                Some(predecessor)
                    if policy.trust_generation == predecessor.policy.trust_generation + 1 =>
                {
                    verify_managed_cloud_rotation_union_signature_set(
                        &policy_bytes,
                        &signature_set,
                        &predecessor.policy,
                        &policy,
                        now_ms,
                    )?;
                }
                _ => return Err(ManagedCloudRegistryError::SequenceRegression),
            }
            insert_postgres_managed_cloud_signature_set(
                &mut tx,
                &signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            insert_postgres_managed_cloud_policy(
                &mut tx,
                &policy,
                &policy_sha256,
                &envelope.canonical_base64url,
                &signature_set_sha256,
                recorded_by,
                now_ms,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_policy_import_result(
                &policy,
                &policy_sha256,
                &signature_set_sha256,
                false,
            ))
        }
    })
}

fn managed_cloud_release_import_result(
    release: &ManagedCloudReleaseAuthority,
    manifest_sha256: &str,
    signature_set_sha256: &str,
    policy_sha256: &str,
    replayed: bool,
) -> ManagedCloudImportResult {
    ManagedCloudImportResult {
        authority_kind: "release".to_string(),
        authority_id: release.manifest_id.clone(),
        authority_sha256: manifest_sha256.to_string(),
        signature_set_sha256: signature_set_sha256.to_string(),
        trust_policy_sha256: policy_sha256.to_string(),
        replayed,
    }
}

#[derive(Clone, Copy)]
struct ManagedCloudReleaseInsertRecord<'a> {
    manifest_sha256: &'a str,
    failure_converter_sha256: &'a str,
    canonical_base64url: &'a str,
    signature_set_sha256: &'a str,
    trust_generation: i64,
    recorded_by: &'a str,
    recorded_at_ms: i64,
}

fn insert_sqlite_managed_cloud_release(
    tx: &rusqlite::Transaction<'_>,
    release: &ManagedCloudReleaseAuthority,
    evidence: &ManagedCloudVerificationEvidenceAuthority,
    record: ManagedCloudReleaseInsertRecord<'_>,
) -> ManagedCloudResult<()> {
    let ManagedCloudReleaseInsertRecord {
        manifest_sha256,
        failure_converter_sha256,
        canonical_base64url,
        signature_set_sha256,
        trust_generation,
        recorded_by,
        recorded_at_ms,
    } = record;
    tx.execute(
        "INSERT INTO jobs_managed_cloud_manifests(
           manifest_sha256,manifest_id,manifest_generation,trust_generation,
           release_id,release_sequence,source_commit,sqlite_migration_head,
           postgres_migration_head,migration_set_sha256,config_schema_sha256,
           protocol_set_sha256,component_set_sha256,feature_authority_sha256,
           cloud_distribution_enabled,workflow_command_dispatch_enabled,
           workflow_cleanup_enabled,direct_discovery_enabled,global_discovery_enabled,
           source_verification_enabled,verification_evidence_sha256,
           failure_converter_sha256,component_count,capability_count,protocol_count,
           canonical_manifest_base64url,authorization_signature_set_sha256,
           published_at_ms,recorded_by,recorded_at_ms
         ) VALUES(
           ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
           ?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30
         )",
        params![
            manifest_sha256,
            release.manifest_id,
            release.manifest_generation,
            trust_generation,
            release.release_id,
            release.release_sequence,
            release.source_commit,
            release.sqlite_migration_head,
            release.postgres_migration_head,
            release.migration_set_sha256,
            release.config_schema_sha256,
            release.protocol_set_sha256,
            release.component_set_sha256,
            release.feature_authority_sha256,
            i64::from(release.feature_authority.cloud_distribution),
            i64::from(release.feature_authority.workflow_command_dispatch),
            i64::from(release.feature_authority.workflow_cleanup),
            i64::from(release.feature_authority.direct_discovery),
            i64::from(release.feature_authority.global_discovery),
            i64::from(release.feature_authority.source_verification),
            release.verification_evidence_sha256,
            failure_converter_sha256,
            i64::try_from(release.components.len())
                .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            i64::try_from(release.capabilities.len())
                .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            i64::try_from(release.protocols.len())
                .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            canonical_base64url,
            signature_set_sha256,
            release.published_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    for (ordinal, (component, evidence_component)) in release
        .components
        .iter()
        .zip(&evidence.components)
        .enumerate()
    {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_manifest_components(
               manifest_sha256,component_id,artifact_kind,artifact_ref,artifact_sha256,
               build_id,source_commit,platform,architecture,sbom_sha256,
               provenance_sha256,config_schema_sha256,runtime_measurement_sha256,
               ordinal
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                manifest_sha256,
                component.component_id,
                component.artifact_kind,
                component.artifact_ref,
                component.artifact_sha256,
                component.build_id,
                component.source_commit,
                component.platform,
                component.architecture,
                component.sbom_sha256,
                component.provenance_sha256,
                component.config_schema_sha256,
                evidence_component.runtime_measurement_sha256,
                i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    for (ordinal, capability) in release.capabilities.iter().enumerate() {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_manifest_capabilities(
               manifest_sha256,component_id,capability,ordinal
             ) VALUES(?1,?2,?3,?4)",
            params![
                manifest_sha256,
                capability.component_id,
                capability.capability,
                i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    for evidence_component in &evidence.components {
        let Some(runtime_measurement_sha256) =
            evidence_component.runtime_measurement_sha256.as_deref()
        else {
            continue;
        };
        for (ordinal, identity) in evidence_component.runtime_identities.iter().enumerate() {
            tx.execute(
                "INSERT INTO jobs_managed_cloud_manifest_runtime_identities(
                   manifest_sha256,component_id,role,runtime_measurement_sha256,
                   runtime_identity_sha256,ordinal
                 ) VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    manifest_sha256,
                    evidence_component.component_id,
                    identity.role,
                    runtime_measurement_sha256,
                    identity.runtime_identity_sha256,
                    i64::try_from(ordinal)
                        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
                ],
            )
            .map_err(managed_cloud_storage)?;
        }
    }
    for (ordinal, protocol) in release.protocols.iter().enumerate() {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_manifest_protocols(
               manifest_sha256,protocol_id,protocol_version,schema_sha256,ordinal
             ) VALUES(?1,?2,?3,?4,?5)",
            params![
                manifest_sha256,
                protocol.protocol_id,
                protocol.protocol_version,
                protocol.schema_sha256,
                i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    Ok(())
}

fn insert_postgres_managed_cloud_release(
    tx: &mut postgres::Transaction<'_>,
    release: &ManagedCloudReleaseAuthority,
    evidence: &ManagedCloudVerificationEvidenceAuthority,
    record: ManagedCloudReleaseInsertRecord<'_>,
) -> ManagedCloudResult<()> {
    let ManagedCloudReleaseInsertRecord {
        manifest_sha256,
        failure_converter_sha256,
        canonical_base64url,
        signature_set_sha256,
        trust_generation,
        recorded_by,
        recorded_at_ms,
    } = record;
    let component_count = i64::try_from(release.components.len())
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    let capability_count = i64::try_from(release.capabilities.len())
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    let protocol_count = i64::try_from(release.protocols.len())
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    tx.execute(
        "INSERT INTO jobs_managed_cloud_manifests(
           manifest_sha256,manifest_id,manifest_generation,trust_generation,
           release_id,release_sequence,source_commit,sqlite_migration_head,
           postgres_migration_head,migration_set_sha256,config_schema_sha256,
           protocol_set_sha256,component_set_sha256,feature_authority_sha256,
           cloud_distribution_enabled,workflow_command_dispatch_enabled,
           workflow_cleanup_enabled,direct_discovery_enabled,global_discovery_enabled,
           source_verification_enabled,verification_evidence_sha256,
           failure_converter_sha256,component_count,capability_count,protocol_count,
           canonical_manifest_base64url,authorization_signature_set_sha256,
           published_at_ms,recorded_by,recorded_at_ms
         ) VALUES(
           $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
           $18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30
         )",
        &[
            &manifest_sha256,
            &release.manifest_id,
            &release.manifest_generation,
            &trust_generation,
            &release.release_id,
            &release.release_sequence,
            &release.source_commit,
            &release.sqlite_migration_head,
            &release.postgres_migration_head,
            &release.migration_set_sha256,
            &release.config_schema_sha256,
            &release.protocol_set_sha256,
            &release.component_set_sha256,
            &release.feature_authority_sha256,
            &release.feature_authority.cloud_distribution,
            &release.feature_authority.workflow_command_dispatch,
            &release.feature_authority.workflow_cleanup,
            &release.feature_authority.direct_discovery,
            &release.feature_authority.global_discovery,
            &release.feature_authority.source_verification,
            &release.verification_evidence_sha256,
            &failure_converter_sha256,
            &component_count,
            &capability_count,
            &protocol_count,
            &canonical_base64url,
            &signature_set_sha256,
            &release.published_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    for (ordinal, (component, evidence_component)) in release
        .components
        .iter()
        .zip(&evidence.components)
        .enumerate()
    {
        let ordinal =
            i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
        tx.execute(
            "INSERT INTO jobs_managed_cloud_manifest_components(
               manifest_sha256,component_id,artifact_kind,artifact_ref,artifact_sha256,
               build_id,source_commit,platform,architecture,sbom_sha256,
               provenance_sha256,config_schema_sha256,runtime_measurement_sha256,
               ordinal
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
            &[
                &manifest_sha256,
                &component.component_id,
                &component.artifact_kind,
                &component.artifact_ref,
                &component.artifact_sha256,
                &component.build_id,
                &component.source_commit,
                &component.platform,
                &component.architecture,
                &component.sbom_sha256,
                &component.provenance_sha256,
                &component.config_schema_sha256,
                &evidence_component.runtime_measurement_sha256,
                &ordinal,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    for (ordinal, capability) in release.capabilities.iter().enumerate() {
        let ordinal =
            i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
        tx.execute(
            "INSERT INTO jobs_managed_cloud_manifest_capabilities(
               manifest_sha256,component_id,capability,ordinal
             ) VALUES($1,$2,$3,$4)",
            &[
                &manifest_sha256,
                &capability.component_id,
                &capability.capability,
                &ordinal,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    for evidence_component in &evidence.components {
        let Some(runtime_measurement_sha256) =
            evidence_component.runtime_measurement_sha256.as_deref()
        else {
            continue;
        };
        for (ordinal, identity) in evidence_component.runtime_identities.iter().enumerate() {
            let ordinal =
                i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_manifest_runtime_identities(
                   manifest_sha256,component_id,role,runtime_measurement_sha256,
                   runtime_identity_sha256,ordinal
                 ) VALUES($1,$2,$3,$4,$5,$6)",
                &[
                    &manifest_sha256,
                    &evidence_component.component_id,
                    &identity.role,
                    &runtime_measurement_sha256,
                    &identity.runtime_identity_sha256,
                    &ordinal,
                ],
            )
            .map_err(managed_cloud_storage)?;
        }
    }
    for (ordinal, protocol) in release.protocols.iter().enumerate() {
        let ordinal =
            i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
        tx.execute(
            "INSERT INTO jobs_managed_cloud_manifest_protocols(
               manifest_sha256,protocol_id,protocol_version,schema_sha256,ordinal
             ) VALUES($1,$2,$3,$4,$5)",
            &[
                &manifest_sha256,
                &protocol.protocol_id,
                &protocol.protocol_version,
                &protocol.schema_sha256,
                &ordinal,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    Ok(())
}

pub fn import_managed_cloud_release(
    pool: &DbPool,
    request: &ManagedCloudReleaseImportRequest,
    recorded_by: &str,
) -> ManagedCloudResult<ManagedCloudImportResult> {
    if !managed_cloud_actor(recorded_by) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let (release_bytes, signature_set_bytes) = managed_cloud_decode_envelope(&request.envelope)?;
    let release: ManagedCloudReleaseAuthority = managed_cloud_parse_canonical(&release_bytes)?;
    let signature_set: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&signature_set_bytes)?;
    let evidence_bytes = managed_cloud_decode_base64url(&request.verification_evidence_base64url)?;
    let evidence: ManagedCloudVerificationEvidenceAuthority =
        managed_cloud_parse_canonical(&evidence_bytes)?;
    validate_managed_cloud_release(&release)?;
    validate_managed_cloud_verification_evidence(&evidence, &evidence_bytes, &release)?;
    let failure_converter_sha256 = validate_managed_cloud_content_inventories(
        &request.inventory_attachments,
        &release,
        &evidence,
    )?;
    let manifest_sha256 = managed_cloud_sha256(&release_bytes);
    let signature_set_sha256 = managed_cloud_sha256(&signature_set_bytes);

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            let existing = tx
                .query_row(
                    "SELECT manifest_sha256,canonical_manifest_base64url,
                            authorization_signature_set_sha256,trust_generation,
                            failure_converter_sha256
                       FROM jobs_managed_cloud_manifests
                      WHERE manifest_sha256=?1 OR manifest_id=?2 OR manifest_generation=?3
                         OR release_id=?4 OR release_sequence=?5 LIMIT 1",
                    params![
                        manifest_sha256,
                        release.manifest_id,
                        release.manifest_generation,
                        release.release_id,
                        release.release_sequence,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                if existing.0 != manifest_sha256
                    || existing.1 != request.envelope.canonical_base64url
                    || existing.2 != signature_set_sha256
                    || existing.3 != signature_set.trust_generation
                    || existing.4 != failure_converter_sha256
                    || !require_sqlite_managed_cloud_signature_set_replay(
                        &tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let policy = sqlite_managed_cloud_policy_by_generation(&tx, existing.3)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_release_import_result(
                    &release,
                    &manifest_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy =
                sqlite_managed_cloud_policy_by_generation(&tx, signature_set.trust_generation)?;
            verify_managed_cloud_signature_set(
                &release_bytes,
                &signature_set,
                &policy.policy,
                "release",
                MANAGED_CLOUD_RELEASE_AUDIENCE,
                release.published_at_ms,
                now_ms,
            )?;
            require_sqlite_managed_cloud_signing_authority_not_revoked(
                &tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            let sequence = tx
                .query_row(
                    "SELECT COALESCE(MAX(manifest_generation),0),
                            COALESCE(MAX(release_sequence),0)
                       FROM jobs_managed_cloud_manifests",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .map_err(managed_cloud_storage)?;
            if release.manifest_generation != sequence.0 + 1
                || release.release_sequence != sequence.1 + 1
            {
                return Err(ManagedCloudRegistryError::SequenceRegression);
            }
            insert_sqlite_managed_cloud_signature_set(
                &tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            insert_sqlite_managed_cloud_release(
                &tx,
                &release,
                &evidence,
                ManagedCloudReleaseInsertRecord {
                    manifest_sha256: &manifest_sha256,
                    failure_converter_sha256: &failure_converter_sha256,
                    canonical_base64url: &request.envelope.canonical_base64url,
                    signature_set_sha256: &signature_set_sha256,
                    trust_generation: signature_set.trust_generation,
                    recorded_by,
                    recorded_at_ms: now_ms,
                },
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_release_import_result(
                &release,
                &manifest_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                   hashtextextended('jobs-managed-cloud-release-registry',0))",
                &[],
            )
            .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            let existing = tx
                .query_opt(
                    "SELECT manifest_sha256,canonical_manifest_base64url,
                            authorization_signature_set_sha256,trust_generation,
                            failure_converter_sha256
                       FROM jobs_managed_cloud_manifests
                      WHERE manifest_sha256=$1 OR manifest_id=$2 OR manifest_generation=$3
                         OR release_id=$4 OR release_sequence=$5 LIMIT 1 FOR UPDATE",
                    &[
                        &manifest_sha256,
                        &release.manifest_id,
                        &release.manifest_generation,
                        &release.release_id,
                        &release.release_sequence,
                    ],
                )
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                let stored_manifest_sha256: String = existing.get(0);
                let stored_canonical: String = existing.get(1);
                let stored_signature_sha256: String = existing.get(2);
                let stored_generation: i64 = existing.get(3);
                let stored_converter_sha256: String = existing.get(4);
                if stored_manifest_sha256 != manifest_sha256
                    || stored_canonical != request.envelope.canonical_base64url
                    || stored_signature_sha256 != signature_set_sha256
                    || stored_generation != signature_set.trust_generation
                    || stored_converter_sha256 != failure_converter_sha256
                    || !require_postgres_managed_cloud_signature_set_replay(
                        &mut tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let policy =
                    postgres_managed_cloud_policy_by_generation(&mut tx, stored_generation)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_release_import_result(
                    &release,
                    &manifest_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy = postgres_managed_cloud_policy_by_generation(
                &mut tx,
                signature_set.trust_generation,
            )?;
            verify_managed_cloud_signature_set(
                &release_bytes,
                &signature_set,
                &policy.policy,
                "release",
                MANAGED_CLOUD_RELEASE_AUDIENCE,
                release.published_at_ms,
                now_ms,
            )?;
            require_postgres_managed_cloud_signing_authority_not_revoked(
                &mut tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            let sequence = tx
                .query_one(
                    "SELECT COALESCE(MAX(manifest_generation),0),
                            COALESCE(MAX(release_sequence),0)
                       FROM jobs_managed_cloud_manifests",
                    &[],
                )
                .map_err(managed_cloud_storage)?;
            let latest_manifest_generation: i64 = sequence.get(0);
            let latest_release_sequence: i64 = sequence.get(1);
            if release.manifest_generation != latest_manifest_generation + 1
                || release.release_sequence != latest_release_sequence + 1
            {
                return Err(ManagedCloudRegistryError::SequenceRegression);
            }
            insert_postgres_managed_cloud_signature_set(
                &mut tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            insert_postgres_managed_cloud_release(
                &mut tx,
                &release,
                &evidence,
                ManagedCloudReleaseInsertRecord {
                    manifest_sha256: &manifest_sha256,
                    failure_converter_sha256: &failure_converter_sha256,
                    canonical_base64url: &request.envelope.canonical_base64url,
                    signature_set_sha256: &signature_set_sha256,
                    trust_generation: signature_set.trust_generation,
                    recorded_by,
                    recorded_at_ms: now_ms,
                },
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_release_import_result(
                &release,
                &manifest_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
    })
}

fn managed_cloud_cohort_import_result(
    cohort: &ManagedCloudCohortAuthority,
    cohort_sha256: &str,
    signature_set_sha256: &str,
    policy_sha256: &str,
    replayed: bool,
) -> ManagedCloudImportResult {
    ManagedCloudImportResult {
        authority_kind: "cohort".to_string(),
        authority_id: cohort.cohort_id.clone(),
        authority_sha256: cohort_sha256.to_string(),
        signature_set_sha256: signature_set_sha256.to_string(),
        trust_policy_sha256: policy_sha256.to_string(),
        replayed,
    }
}

fn managed_cloud_cohort_members(
    cohort: &ManagedCloudCohortAuthority,
    account_ids: &[String],
) -> ManagedCloudResult<Vec<(String, String)>> {
    if account_ids.len() != cohort.account_id_sha256s.len()
        || account_ids.len() > 512
        || account_ids
            .iter()
            .any(|account_id| !managed_cloud_token(account_id, 128))
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let mut members = account_ids
        .iter()
        .map(|account_id| {
            Ok((
                managed_cloud_cohort_member_sha256(&cohort.cohort_id, account_id)?,
                account_id.clone(),
            ))
        })
        .collect::<ManagedCloudResult<Vec<_>>>()?;
    members.sort();
    if members.windows(2).any(|pair| pair[0] >= pair[1])
        || members
            .iter()
            .map(|member| member.0.as_str())
            .ne(cohort.account_id_sha256s.iter().map(String::as_str))
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(members)
}

pub fn import_managed_cloud_cohort(
    pool: &DbPool,
    request: &ManagedCloudCohortImportRequest,
    recorded_by: &str,
) -> ManagedCloudResult<ManagedCloudImportResult> {
    if !managed_cloud_actor(recorded_by) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let (cohort_bytes, signature_set_bytes) = managed_cloud_decode_envelope(&request.envelope)?;
    let cohort: ManagedCloudCohortAuthority = managed_cloud_parse_canonical(&cohort_bytes)?;
    let signature_set: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&signature_set_bytes)?;
    validate_managed_cloud_cohort(&cohort)?;
    validate_managed_cloud_signature_set(&signature_set)?;
    let members = managed_cloud_cohort_members(&cohort, &request.account_ids)?;
    let cohort_sha256 = managed_cloud_sha256(&cohort_bytes);
    let signature_set_sha256 = managed_cloud_sha256(&signature_set_bytes);
    let role = if cohort.scope.channel == "general" {
        "general_promotion"
    } else {
        "promotion"
    };

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            let existing = tx
                .query_row(
                    "SELECT cohort_sha256,canonical_cohort_base64url,
                            authorization_signature_set_sha256,trust_generation
                       FROM jobs_managed_cloud_cohorts
                      WHERE cohort_sha256=?1 OR cohort_id=?2
                         OR (environment=?3 AND region=?4 AND channel=?5
                             AND cohort_generation=?6)
                      LIMIT 1",
                    params![
                        cohort_sha256,
                        cohort.cohort_id,
                        cohort.scope.environment,
                        cohort.scope.region,
                        cohort.scope.channel,
                        cohort.cohort_generation,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                if existing.0 != cohort_sha256
                    || existing.1 != request.envelope.canonical_base64url
                    || existing.2 != signature_set_sha256
                    || existing.3 != cohort.trust_generation
                    || !require_sqlite_managed_cloud_signature_set_replay(
                        &tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let stored_members = tx
                    .prepare(
                        "SELECT account_id_sha256,account_id
                           FROM jobs_managed_cloud_cohort_members
                          WHERE cohort_sha256=?1 ORDER BY ordinal",
                    )
                    .map_err(managed_cloud_storage)?
                    .query_map(params![cohort_sha256], |row| Ok((row.get(0)?, row.get(1)?)))
                    .map_err(managed_cloud_storage)?
                    .collect::<rusqlite::Result<Vec<(String, String)>>>()
                    .map_err(managed_cloud_storage)?;
                if stored_members != members {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let policy = sqlite_managed_cloud_policy_by_generation(&tx, existing.3)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_cohort_import_result(
                    &cohort,
                    &cohort_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy = sqlite_managed_cloud_policy_by_generation(&tx, cohort.trust_generation)?;
            verify_managed_cloud_signature_set(
                &cohort_bytes,
                &signature_set,
                &policy.policy,
                role,
                MANAGED_CLOUD_COHORT_AUDIENCE,
                cohort.issued_at_ms,
                now_ms,
            )?;
            require_sqlite_managed_cloud_signing_authority_not_revoked(
                &tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            if cohort.not_before_ms < policy.policy.valid_from_ms
                || cohort.expires_at_ms > policy.policy.expires_at_ms
                || cohort.expires_at_ms <= now_ms
            {
                return Err(ManagedCloudRegistryError::Unavailable);
            }
            let next_generation = tx
                .query_row(
                    "SELECT COALESCE(MAX(cohort_generation),0)+1
                       FROM jobs_managed_cloud_cohorts
                      WHERE environment=?1 AND region=?2 AND channel=?3",
                    params![
                        cohort.scope.environment,
                        cohort.scope.region,
                        cohort.scope.channel,
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(managed_cloud_storage)?;
            if cohort.cohort_generation != next_generation {
                return Err(ManagedCloudRegistryError::SequenceRegression);
            }
            for account_id in request.account_ids.iter() {
                if tx
                    .query_row(
                        "SELECT 1 FROM accounts WHERE id=?1",
                        params![account_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(managed_cloud_storage)?
                    .is_none()
                {
                    return Err(ManagedCloudRegistryError::NotFound);
                }
            }
            insert_sqlite_managed_cloud_signature_set(
                &tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_cohorts(
                   cohort_sha256,cohort_id,cohort_generation,trust_generation,
                   environment,region,channel,rollout_mode,member_count,approval_ref,
                   canonical_cohort_base64url,authorization_signature_set_sha256,
                   issued_at_ms,not_before_ms,expires_at_ms,recorded_by,recorded_at_ms
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                params![
                    cohort_sha256,
                    cohort.cohort_id,
                    cohort.cohort_generation,
                    cohort.trust_generation,
                    cohort.scope.environment,
                    cohort.scope.region,
                    cohort.scope.channel,
                    cohort.rollout_mode,
                    i64::try_from(members.len())
                        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
                    cohort.approval_ref,
                    request.envelope.canonical_base64url,
                    signature_set_sha256,
                    cohort.issued_at_ms,
                    cohort.not_before_ms,
                    cohort.expires_at_ms,
                    recorded_by,
                    now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            for (ordinal, (account_sha256, account_id)) in members.iter().enumerate() {
                let ordinal = i64::try_from(ordinal)
                    .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
                tx.execute(
                    "INSERT INTO jobs_managed_cloud_cohort_members(
                       cohort_sha256,ordinal,account_id,account_id_sha256
                     ) VALUES(?1,?2,?3,?4)",
                    params![cohort_sha256, ordinal, account_id, account_sha256],
                )
                .map_err(managed_cloud_storage)?;
            }
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_cohort_import_result(
                &cohort,
                &cohort_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                   hashtextextended('jobs-managed-cloud-release-registry',0))",
                &[],
            )
            .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            let existing = tx
                .query_opt(
                    "SELECT cohort_sha256,canonical_cohort_base64url,
                            authorization_signature_set_sha256,trust_generation
                       FROM jobs_managed_cloud_cohorts
                      WHERE cohort_sha256=$1 OR cohort_id=$2
                         OR (environment=$3 AND region=$4 AND channel=$5
                             AND cohort_generation=$6)
                      LIMIT 1 FOR UPDATE",
                    &[
                        &cohort_sha256,
                        &cohort.cohort_id,
                        &cohort.scope.environment,
                        &cohort.scope.region,
                        &cohort.scope.channel,
                        &cohort.cohort_generation,
                    ],
                )
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                let stored_sha256: String = existing.get(0);
                let stored_canonical: String = existing.get(1);
                let stored_signature: String = existing.get(2);
                let stored_generation: i64 = existing.get(3);
                if stored_sha256 != cohort_sha256
                    || stored_canonical != request.envelope.canonical_base64url
                    || stored_signature != signature_set_sha256
                    || stored_generation != cohort.trust_generation
                    || !require_postgres_managed_cloud_signature_set_replay(
                        &mut tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let stored_members = tx
                    .query(
                        "SELECT account_id_sha256,account_id
                           FROM jobs_managed_cloud_cohort_members
                          WHERE cohort_sha256=$1 ORDER BY ordinal",
                        &[&cohort_sha256],
                    )
                    .map_err(managed_cloud_storage)?
                    .iter()
                    .map(|row| (row.get(0), row.get(1)))
                    .collect::<Vec<(String, String)>>();
                if stored_members != members {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let policy =
                    postgres_managed_cloud_policy_by_generation(&mut tx, stored_generation)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_cohort_import_result(
                    &cohort,
                    &cohort_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy =
                postgres_managed_cloud_policy_by_generation(&mut tx, cohort.trust_generation)?;
            verify_managed_cloud_signature_set(
                &cohort_bytes,
                &signature_set,
                &policy.policy,
                role,
                MANAGED_CLOUD_COHORT_AUDIENCE,
                cohort.issued_at_ms,
                now_ms,
            )?;
            require_postgres_managed_cloud_signing_authority_not_revoked(
                &mut tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            if cohort.not_before_ms < policy.policy.valid_from_ms
                || cohort.expires_at_ms > policy.policy.expires_at_ms
                || cohort.expires_at_ms <= now_ms
            {
                return Err(ManagedCloudRegistryError::Unavailable);
            }
            let next_generation: i64 = tx
                .query_one(
                    "SELECT COALESCE(MAX(cohort_generation),0)+1
                       FROM jobs_managed_cloud_cohorts
                      WHERE environment=$1 AND region=$2 AND channel=$3",
                    &[
                        &cohort.scope.environment,
                        &cohort.scope.region,
                        &cohort.scope.channel,
                    ],
                )
                .map_err(managed_cloud_storage)?
                .get(0);
            if cohort.cohort_generation != next_generation {
                return Err(ManagedCloudRegistryError::SequenceRegression);
            }
            for account_id in request.account_ids.iter() {
                if tx
                    .query_opt(
                        "SELECT 1 FROM accounts WHERE id=$1 FOR KEY SHARE",
                        &[account_id],
                    )
                    .map_err(managed_cloud_storage)?
                    .is_none()
                {
                    return Err(ManagedCloudRegistryError::NotFound);
                }
            }
            insert_postgres_managed_cloud_signature_set(
                &mut tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            let member_count = i64::try_from(members.len())
                .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_cohorts(
                   cohort_sha256,cohort_id,cohort_generation,trust_generation,
                   environment,region,channel,rollout_mode,member_count,approval_ref,
                   canonical_cohort_base64url,authorization_signature_set_sha256,
                   issued_at_ms,not_before_ms,expires_at_ms,recorded_by,recorded_at_ms
                 ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
                &[
                    &cohort_sha256,
                    &cohort.cohort_id,
                    &cohort.cohort_generation,
                    &cohort.trust_generation,
                    &cohort.scope.environment,
                    &cohort.scope.region,
                    &cohort.scope.channel,
                    &cohort.rollout_mode,
                    &member_count,
                    &cohort.approval_ref,
                    &request.envelope.canonical_base64url,
                    &signature_set_sha256,
                    &cohort.issued_at_ms,
                    &cohort.not_before_ms,
                    &cohort.expires_at_ms,
                    &recorded_by,
                    &now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            for (ordinal, (account_sha256, account_id)) in members.iter().enumerate() {
                let ordinal = i64::try_from(ordinal)
                    .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
                tx.execute(
                    "INSERT INTO jobs_managed_cloud_cohort_members(
                       cohort_sha256,ordinal,account_id,account_id_sha256
                     ) VALUES($1,$2,$3,$4)",
                    &[&cohort_sha256, &ordinal, account_id, account_sha256],
                )
                .map_err(managed_cloud_storage)?;
            }
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_cohort_import_result(
                &cohort,
                &cohort_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
    })
}

#[derive(Debug, Clone)]
struct ManagedCloudActivationDependencies {
    trust_generation: i64,
    manifest_signature_set_sha256: String,
    cohort_signature_set_sha256: String,
    feature_authority_sha256: String,
    feature_authority: ManagedCloudFeatureAuthority,
    failure_converter_sha256: String,
    portal_artifact_sha256: String,
    manifest_published_at_ms: i64,
    cohort_issued_at_ms: i64,
    cohort_not_before_ms: i64,
    cohort_expires_at_ms: i64,
}

fn sqlite_managed_cloud_activation_dependencies(
    tx: &rusqlite::Transaction<'_>,
    activation: &ManagedCloudActivationAuthority,
) -> ManagedCloudResult<ManagedCloudActivationDependencies> {
    tx.query_row(
        "SELECT manifest.trust_generation,
                manifest.authorization_signature_set_sha256,
                cohort.authorization_signature_set_sha256,
                manifest.feature_authority_sha256,
                manifest.cloud_distribution_enabled,
                manifest.workflow_command_dispatch_enabled,
                manifest.workflow_cleanup_enabled,
                manifest.direct_discovery_enabled,
                manifest.global_discovery_enabled,
                manifest.source_verification_enabled,
                manifest.failure_converter_sha256,portal.artifact_sha256,
                manifest.published_at_ms,cohort.issued_at_ms,
                cohort.not_before_ms,cohort.expires_at_ms
           FROM jobs_managed_cloud_manifests manifest
           JOIN jobs_managed_cloud_manifest_components portal
             ON portal.manifest_sha256=manifest.manifest_sha256
            AND portal.component_id='jobs-portal'
           JOIN jobs_managed_cloud_cohorts cohort
             ON cohort.cohort_sha256=?2
            AND cohort.trust_generation=manifest.trust_generation
            AND cohort.environment=?3 AND cohort.region=?4 AND cohort.channel=?5
          WHERE manifest.manifest_sha256=?1",
        params![
            activation.manifest_sha256,
            activation.cohort_sha256,
            activation.scope.environment,
            activation.scope.region,
            activation.scope.channel,
        ],
        |row| {
            Ok(ManagedCloudActivationDependencies {
                trust_generation: row.get(0)?,
                manifest_signature_set_sha256: row.get(1)?,
                cohort_signature_set_sha256: row.get(2)?,
                feature_authority_sha256: row.get(3)?,
                feature_authority: ManagedCloudFeatureAuthority {
                    cloud_distribution: row.get(4)?,
                    workflow_command_dispatch: row.get(5)?,
                    workflow_cleanup: row.get(6)?,
                    direct_discovery: row.get(7)?,
                    global_discovery: row.get(8)?,
                    source_verification: row.get(9)?,
                },
                failure_converter_sha256: row.get(10)?,
                portal_artifact_sha256: row.get(11)?,
                manifest_published_at_ms: row.get(12)?,
                cohort_issued_at_ms: row.get(13)?,
                cohort_not_before_ms: row.get(14)?,
                cohort_expires_at_ms: row.get(15)?,
            })
        },
    )
    .optional()
    .map_err(managed_cloud_storage)?
    .ok_or(ManagedCloudRegistryError::NotFound)
}

fn postgres_managed_cloud_activation_dependencies(
    tx: &mut postgres::Transaction<'_>,
    activation: &ManagedCloudActivationAuthority,
) -> ManagedCloudResult<ManagedCloudActivationDependencies> {
    tx.query_opt(
        "SELECT manifest.trust_generation,
                manifest.authorization_signature_set_sha256,
                cohort.authorization_signature_set_sha256,
                manifest.feature_authority_sha256,
                manifest.cloud_distribution_enabled,
                manifest.workflow_command_dispatch_enabled,
                manifest.workflow_cleanup_enabled,
                manifest.direct_discovery_enabled,
                manifest.global_discovery_enabled,
                manifest.source_verification_enabled,
                manifest.failure_converter_sha256,portal.artifact_sha256,
                manifest.published_at_ms,cohort.issued_at_ms,
                cohort.not_before_ms,cohort.expires_at_ms
           FROM jobs_managed_cloud_manifests manifest
           JOIN jobs_managed_cloud_manifest_components portal
             ON portal.manifest_sha256=manifest.manifest_sha256
            AND portal.component_id='jobs-portal'
           JOIN jobs_managed_cloud_cohorts cohort
             ON cohort.cohort_sha256=$2
            AND cohort.trust_generation=manifest.trust_generation
            AND cohort.environment=$3 AND cohort.region=$4 AND cohort.channel=$5
          WHERE manifest.manifest_sha256=$1
          FOR SHARE OF manifest,portal,cohort",
        &[
            &activation.manifest_sha256,
            &activation.cohort_sha256,
            &activation.scope.environment,
            &activation.scope.region,
            &activation.scope.channel,
        ],
    )
    .map_err(managed_cloud_storage)?
    .map(|row| ManagedCloudActivationDependencies {
        trust_generation: row.get(0),
        manifest_signature_set_sha256: row.get(1),
        cohort_signature_set_sha256: row.get(2),
        feature_authority_sha256: row.get(3),
        feature_authority: ManagedCloudFeatureAuthority {
            cloud_distribution: row.get(4),
            workflow_command_dispatch: row.get(5),
            workflow_cleanup: row.get(6),
            direct_discovery: row.get(7),
            global_discovery: row.get(8),
            source_verification: row.get(9),
        },
        failure_converter_sha256: row.get(10),
        portal_artifact_sha256: row.get(11),
        manifest_published_at_ms: row.get(12),
        cohort_issued_at_ms: row.get(13),
        cohort_not_before_ms: row.get(14),
        cohort_expires_at_ms: row.get(15),
    })
    .ok_or(ManagedCloudRegistryError::NotFound)
}

fn validate_managed_cloud_activation_dependencies(
    activation: &ManagedCloudActivationAuthority,
    dependencies: &ManagedCloudActivationDependencies,
) -> ManagedCloudResult<()> {
    if dependencies.trust_generation != activation.trust_generation
        || dependencies.manifest_signature_set_sha256 != activation.manifest_signature_set_sha256
        || dependencies.cohort_signature_set_sha256 != activation.cohort_signature_set_sha256
        || dependencies.feature_authority_sha256 != activation.feature_authority_sha256
        || dependencies.feature_authority != activation.feature_authority
        || dependencies.failure_converter_sha256 != activation.failure_converter_sha256
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn managed_cloud_activation_import_result(
    activation: &ManagedCloudActivationAuthority,
    activation_sha256: &str,
    signature_set_sha256: &str,
    policy_sha256: &str,
    replayed: bool,
) -> ManagedCloudImportResult {
    ManagedCloudImportResult {
        authority_kind: "activation".to_string(),
        authority_id: activation.activation_id.clone(),
        authority_sha256: activation_sha256.to_string(),
        signature_set_sha256: signature_set_sha256.to_string(),
        trust_policy_sha256: policy_sha256.to_string(),
        replayed,
    }
}

fn validate_managed_cloud_activation_evidence_bindings(
    activation: &ManagedCloudActivationAuthority,
    evidence: &ValidatedManagedCloudActivationEvidence,
) -> ManagedCloudResult<()> {
    if evidence.canary_sha256 != activation.canary_evidence_sha256
        || evidence.cleanup_authority_sha256 != activation.cleanup_authority_sha256
        || evidence.failure_converter_sha256 != activation.failure_converter_sha256
        || evidence.portal_readback_sha256 != activation.portal_readback_evidence_sha256
        || evidence.runner_fleet_sha256 != activation.runner_fleet_evidence_sha256
        || evidence.storage_config_sha256 != activation.storage_config_sha256
        || evidence.task_queue_sha256 != activation.task_queue_sha256
        || evidence.temporal_namespace_sha256 != activation.temporal_namespace_sha256
        || activation.expires_at_ms > evidence.portal_expires_at_ms
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn sqlite_managed_cloud_activation_head_matches(
    tx: &rusqlite::Transaction<'_>,
    activation: &ManagedCloudActivationAuthority,
) -> ManagedCloudResult<()> {
    let head = tx
        .query_row(
            "SELECT head_revision,current_transition_sha256,current_activation_sha256,
                    current_trust_generation,current_channel_sequence
               FROM jobs_managed_cloud_heads
              WHERE environment=?1 AND region=?2 AND channel=?3",
            params![
                activation.scope.environment,
                activation.scope.region,
                activation.scope.channel,
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()
        .map_err(managed_cloud_storage)?;
    match head {
        None if activation.expected_head_revision == 0
            && activation.expected_transition_sha256.is_none()
            && activation.predecessor_activation_sha256.is_none()
            && activation.channel_sequence == 1 => {}
        Some((revision, transition, predecessor, trust_generation, channel_sequence))
            if activation.expected_head_revision == revision
                && activation.expected_transition_sha256.as_deref() == Some(&transition)
                && activation.predecessor_activation_sha256.as_deref() == Some(&predecessor)
                && (activation.trust_generation > trust_generation
                    || (activation.trust_generation == trust_generation
                        && activation.channel_sequence > channel_sequence)) => {}
        _ => return Err(ManagedCloudRegistryError::CompareAndSwapConflict),
    }
    Ok(())
}

fn postgres_managed_cloud_activation_head_matches(
    tx: &mut postgres::Transaction<'_>,
    activation: &ManagedCloudActivationAuthority,
) -> ManagedCloudResult<()> {
    let head = tx
        .query_opt(
            "SELECT head_revision,current_transition_sha256,current_activation_sha256,
                    current_trust_generation,current_channel_sequence
               FROM jobs_managed_cloud_heads
              WHERE environment=$1 AND region=$2 AND channel=$3 FOR UPDATE",
            &[
                &activation.scope.environment,
                &activation.scope.region,
                &activation.scope.channel,
            ],
        )
        .map_err(managed_cloud_storage)?
        .map(|row| {
            (
                row.get::<_, i64>(0),
                row.get::<_, String>(1),
                row.get::<_, String>(2),
                row.get::<_, i64>(3),
                row.get::<_, i64>(4),
            )
        });
    match head {
        None if activation.expected_head_revision == 0
            && activation.expected_transition_sha256.is_none()
            && activation.predecessor_activation_sha256.is_none()
            && activation.channel_sequence == 1 => {}
        Some((revision, transition, predecessor, trust_generation, channel_sequence))
            if activation.expected_head_revision == revision
                && activation.expected_transition_sha256.as_deref() == Some(&transition)
                && activation.predecessor_activation_sha256.as_deref() == Some(&predecessor)
                && (activation.trust_generation > trust_generation
                    || (activation.trust_generation == trust_generation
                        && activation.channel_sequence > channel_sequence)) => {}
        _ => return Err(ManagedCloudRegistryError::CompareAndSwapConflict),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_sqlite_managed_cloud_activation(
    tx: &rusqlite::Transaction<'_>,
    activation: &ManagedCloudActivationAuthority,
    activation_sha256: &str,
    canonical_base64url: &str,
    signature_set_sha256: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> ManagedCloudResult<()> {
    let roles = managed_cloud_required_roles_for_feature(&activation.feature_authority);
    tx.execute(
        "INSERT INTO jobs_managed_cloud_activations(
           activation_sha256,activation_id,activation_generation,trust_generation,
           environment,region,channel,channel_sequence,expected_head_revision,
           expected_transition_sha256,predecessor_activation_sha256,manifest_sha256,
           manifest_signature_set_sha256,cohort_sha256,cohort_signature_set_sha256,
           feature_authority_sha256,cloud_distribution_enabled,
           workflow_command_dispatch_enabled,workflow_cleanup_enabled,
           direct_discovery_enabled,global_discovery_enabled,
           source_verification_enabled,runner_fleet_evidence_sha256,
           cleanup_authority_sha256,temporal_namespace_sha256,storage_config_sha256,
           task_queue_sha256,failure_converter_sha256,canary_evidence_sha256,
           portal_readback_evidence_sha256,portal_readback_at_ms,
           portal_readback_ttl_ms,maximum_inflight,maximum_daily_admissions,
           requirement_count,heartbeat_ttl_ms,recovery_acceptance_count,
           canonical_activation_base64url,authorization_signature_set_sha256,
           issued_at_ms,not_before_ms,expires_at_ms,recorded_by,recorded_at_ms
         ) VALUES(
           ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,
           ?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,
           ?31,?32,?33,?34,?35,?36,?37,?38,?39,?40,?41,?42,?43,?44)",
        params![
            activation_sha256,
            activation.activation_id,
            activation.activation_generation,
            activation.trust_generation,
            activation.scope.environment,
            activation.scope.region,
            activation.scope.channel,
            activation.channel_sequence,
            activation.expected_head_revision,
            activation.expected_transition_sha256,
            activation.predecessor_activation_sha256,
            activation.manifest_sha256,
            activation.manifest_signature_set_sha256,
            activation.cohort_sha256,
            activation.cohort_signature_set_sha256,
            activation.feature_authority_sha256,
            i64::from(activation.feature_authority.cloud_distribution),
            i64::from(activation.feature_authority.workflow_command_dispatch),
            i64::from(activation.feature_authority.workflow_cleanup),
            i64::from(activation.feature_authority.direct_discovery),
            i64::from(activation.feature_authority.global_discovery),
            i64::from(activation.feature_authority.source_verification),
            activation.runner_fleet_evidence_sha256,
            activation.cleanup_authority_sha256,
            activation.temporal_namespace_sha256,
            activation.storage_config_sha256,
            activation.task_queue_sha256,
            activation.failure_converter_sha256,
            activation.canary_evidence_sha256,
            activation.portal_readback_evidence_sha256,
            activation.portal_readback_at_ms,
            activation.portal_readback_ttl_ms,
            activation.maximum_inflight,
            activation.maximum_daily_admissions,
            i64::try_from(roles.len()).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            activation.heartbeat_ttl_ms,
            i64::try_from(activation.recovery_acceptances.len())
                .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            canonical_base64url,
            signature_set_sha256,
            activation.issued_at_ms,
            activation.not_before_ms,
            activation.expires_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    for (ordinal, role) in roles.iter().enumerate() {
        let (component_id, artifact_sha256): (String, String) = tx
            .query_row(
                "SELECT capability.component_id,component.artifact_sha256
                   FROM jobs_managed_cloud_manifest_capabilities capability
                   JOIN jobs_managed_cloud_manifest_components component
                     ON component.manifest_sha256=capability.manifest_sha256
                    AND component.component_id=capability.component_id
                  WHERE capability.manifest_sha256=?1 AND capability.capability=?2",
                params![activation.manifest_sha256, role],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(managed_cloud_storage)?
            .ok_or(ManagedCloudRegistryError::IdentityConflict)?;
        let dependency_sha256 = managed_cloud_dependency_evidence_sha256(
            role,
            activation_sha256,
            &activation.manifest_sha256,
            &component_id,
            &artifact_sha256,
            &activation.task_queue_sha256,
            &activation.failure_converter_sha256,
        )?;
        tx.execute(
            "INSERT INTO jobs_managed_cloud_activation_requirements(
               activation_sha256,role,minimum_ready_instances,heartbeat_ttl_ms,
               dependency_evidence_sha256,ordinal
             ) VALUES(?1,?2,1,?3,?4,?5)",
            params![
                activation_sha256,
                role,
                activation.heartbeat_ttl_ms,
                dependency_sha256,
                i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    for (ordinal, recovery) in activation.recovery_acceptances.iter().enumerate() {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_activation_recovery_acceptances(
               activation_sha256,recovery_activation_sha256,
               recovery_manifest_sha256,ordinal
             ) VALUES(?1,?2,?3,?4)",
            params![
                activation_sha256,
                recovery.activation_sha256,
                recovery.manifest_sha256,
                i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_postgres_managed_cloud_activation(
    tx: &mut postgres::Transaction<'_>,
    activation: &ManagedCloudActivationAuthority,
    activation_sha256: &str,
    canonical_base64url: &str,
    signature_set_sha256: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> ManagedCloudResult<()> {
    let roles = managed_cloud_required_roles_for_feature(&activation.feature_authority);
    let requirement_count =
        i64::try_from(roles.len()).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    let recovery_count = i64::try_from(activation.recovery_acceptances.len())
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    tx.execute(
        "INSERT INTO jobs_managed_cloud_activations(
           activation_sha256,activation_id,activation_generation,trust_generation,
           environment,region,channel,channel_sequence,expected_head_revision,
           expected_transition_sha256,predecessor_activation_sha256,manifest_sha256,
           manifest_signature_set_sha256,cohort_sha256,cohort_signature_set_sha256,
           feature_authority_sha256,cloud_distribution_enabled,
           workflow_command_dispatch_enabled,workflow_cleanup_enabled,
           direct_discovery_enabled,global_discovery_enabled,
           source_verification_enabled,runner_fleet_evidence_sha256,
           cleanup_authority_sha256,temporal_namespace_sha256,storage_config_sha256,
           task_queue_sha256,failure_converter_sha256,canary_evidence_sha256,
           portal_readback_evidence_sha256,portal_readback_at_ms,
           portal_readback_ttl_ms,maximum_inflight,maximum_daily_admissions,
           requirement_count,heartbeat_ttl_ms,recovery_acceptance_count,
           canonical_activation_base64url,authorization_signature_set_sha256,
           issued_at_ms,not_before_ms,expires_at_ms,recorded_by,recorded_at_ms
         ) VALUES(
           $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
           $17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,
           $31,$32,$33,$34,$35,$36,$37,$38,$39,$40,$41,$42,$43,$44)",
        &[
            &activation_sha256,
            &activation.activation_id,
            &activation.activation_generation,
            &activation.trust_generation,
            &activation.scope.environment,
            &activation.scope.region,
            &activation.scope.channel,
            &activation.channel_sequence,
            &activation.expected_head_revision,
            &activation.expected_transition_sha256,
            &activation.predecessor_activation_sha256,
            &activation.manifest_sha256,
            &activation.manifest_signature_set_sha256,
            &activation.cohort_sha256,
            &activation.cohort_signature_set_sha256,
            &activation.feature_authority_sha256,
            &activation.feature_authority.cloud_distribution,
            &activation.feature_authority.workflow_command_dispatch,
            &activation.feature_authority.workflow_cleanup,
            &activation.feature_authority.direct_discovery,
            &activation.feature_authority.global_discovery,
            &activation.feature_authority.source_verification,
            &activation.runner_fleet_evidence_sha256,
            &activation.cleanup_authority_sha256,
            &activation.temporal_namespace_sha256,
            &activation.storage_config_sha256,
            &activation.task_queue_sha256,
            &activation.failure_converter_sha256,
            &activation.canary_evidence_sha256,
            &activation.portal_readback_evidence_sha256,
            &activation.portal_readback_at_ms,
            &activation.portal_readback_ttl_ms,
            &activation.maximum_inflight,
            &activation.maximum_daily_admissions,
            &requirement_count,
            &activation.heartbeat_ttl_ms,
            &recovery_count,
            &canonical_base64url,
            &signature_set_sha256,
            &activation.issued_at_ms,
            &activation.not_before_ms,
            &activation.expires_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    for (ordinal, role) in roles.iter().enumerate() {
        let row = tx
            .query_opt(
                "SELECT capability.component_id,component.artifact_sha256
                   FROM jobs_managed_cloud_manifest_capabilities capability
                   JOIN jobs_managed_cloud_manifest_components component
                     ON component.manifest_sha256=capability.manifest_sha256
                    AND component.component_id=capability.component_id
                  WHERE capability.manifest_sha256=$1 AND capability.capability=$2
                  FOR SHARE OF capability,component",
                &[&activation.manifest_sha256, role],
            )
            .map_err(managed_cloud_storage)?
            .ok_or(ManagedCloudRegistryError::IdentityConflict)?;
        let component_id: String = row.get(0);
        let artifact_sha256: String = row.get(1);
        let dependency_sha256 = managed_cloud_dependency_evidence_sha256(
            role,
            activation_sha256,
            &activation.manifest_sha256,
            &component_id,
            &artifact_sha256,
            &activation.task_queue_sha256,
            &activation.failure_converter_sha256,
        )?;
        let ordinal =
            i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
        tx.execute(
            "INSERT INTO jobs_managed_cloud_activation_requirements(
               activation_sha256,role,minimum_ready_instances,heartbeat_ttl_ms,
               dependency_evidence_sha256,ordinal
             ) VALUES($1,$2,1,$3,$4,$5)",
            &[
                &activation_sha256,
                role,
                &activation.heartbeat_ttl_ms,
                &dependency_sha256,
                &ordinal,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    for (ordinal, recovery) in activation.recovery_acceptances.iter().enumerate() {
        let ordinal =
            i64::try_from(ordinal).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
        tx.execute(
            "INSERT INTO jobs_managed_cloud_activation_recovery_acceptances(
               activation_sha256,recovery_activation_sha256,
               recovery_manifest_sha256,ordinal
             ) VALUES($1,$2,$3,$4)",
            &[
                &activation_sha256,
                &recovery.activation_sha256,
                &recovery.manifest_sha256,
                &ordinal,
            ],
        )
        .map_err(managed_cloud_storage)?;
    }
    Ok(())
}

pub fn import_managed_cloud_activation(
    pool: &DbPool,
    request: &ManagedCloudActivationImportRequest,
    recorded_by: &str,
) -> ManagedCloudResult<ManagedCloudImportResult> {
    if !managed_cloud_actor(recorded_by) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let (activation_bytes, signature_set_bytes) = managed_cloud_decode_envelope(&request.envelope)?;
    let activation: ManagedCloudActivationAuthority =
        managed_cloud_parse_canonical(&activation_bytes)?;
    let signature_set: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&signature_set_bytes)?;
    validate_managed_cloud_activation(&activation)?;
    validate_managed_cloud_signature_set(&signature_set)?;
    let parsed_evidence = parse_managed_cloud_activation_evidence(&request.evidence)?;
    let activation_sha256 = managed_cloud_sha256(&activation_bytes);
    let signature_set_sha256 = managed_cloud_sha256(&signature_set_bytes);
    let role = if activation.scope.channel == "general" {
        "general_promotion"
    } else {
        "promotion"
    };

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            let dependencies = sqlite_managed_cloud_activation_dependencies(&tx, &activation)?;
            validate_managed_cloud_activation_dependencies(&activation, &dependencies)?;
            let existing = tx
                .query_row(
                    "SELECT activation_sha256,canonical_activation_base64url,
                            authorization_signature_set_sha256,trust_generation
                       FROM jobs_managed_cloud_activations
                      WHERE activation_sha256=?1 OR activation_id=?2
                         OR (environment=?3 AND region=?4 AND channel=?5
                             AND trust_generation=?6 AND channel_sequence=?7)
                      LIMIT 1",
                    params![
                        activation_sha256,
                        activation.activation_id,
                        activation.scope.environment,
                        activation.scope.region,
                        activation.scope.channel,
                        activation.trust_generation,
                        activation.channel_sequence,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                if existing.0 != activation_sha256
                    || existing.1 != request.envelope.canonical_base64url
                    || existing.2 != signature_set_sha256
                    || existing.3 != activation.trust_generation
                    || !require_sqlite_managed_cloud_signature_set_replay(
                        &tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let evidence = validate_managed_cloud_activation_evidence(
                    &parsed_evidence,
                    &activation,
                    &dependencies.portal_artifact_sha256,
                    &dependencies.failure_converter_sha256,
                    activation.issued_at_ms,
                )?;
                validate_managed_cloud_activation_evidence_bindings(&activation, &evidence)?;
                let policy =
                    sqlite_managed_cloud_policy_by_generation(&tx, activation.trust_generation)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_activation_import_result(
                    &activation,
                    &activation_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy =
                sqlite_managed_cloud_policy_by_generation(&tx, activation.trust_generation)?;
            verify_managed_cloud_signature_set(
                &activation_bytes,
                &signature_set,
                &policy.policy,
                role,
                MANAGED_CLOUD_ACTIVATION_AUDIENCE,
                activation.issued_at_ms,
                now_ms,
            )?;
            require_sqlite_managed_cloud_signing_authority_not_revoked(
                &tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            if tx
                .query_row(
                    "SELECT 1 FROM jobs_managed_cloud_revocations
                      WHERE effective_at_ms<=?1 AND (
                        (subject_kind='manifest' AND subject_sha256=?2)
                        OR (subject_kind='cohort' AND subject_sha256=?3)
                        OR (subject_kind='release' AND subject_sha256=?2)
                      ) LIMIT 1",
                    params![now_ms, activation.manifest_sha256, activation.cohort_sha256,],
                    |_| Ok(()),
                )
                .optional()
                .map_err(managed_cloud_storage)?
                .is_some()
            {
                return Err(ManagedCloudRegistryError::Revoked);
            }
            let evidence = validate_managed_cloud_activation_evidence(
                &parsed_evidence,
                &activation,
                &dependencies.portal_artifact_sha256,
                &dependencies.failure_converter_sha256,
                now_ms,
            )?;
            validate_managed_cloud_activation_evidence_bindings(&activation, &evidence)?;
            if activation.issued_at_ms < dependencies.manifest_published_at_ms
                || activation.issued_at_ms < dependencies.cohort_issued_at_ms
                || activation.not_before_ms < dependencies.cohort_not_before_ms
                || activation.not_before_ms < policy.policy.valid_from_ms
                || activation.expires_at_ms > dependencies.cohort_expires_at_ms
                || activation.expires_at_ms > policy.policy.expires_at_ms
                || activation.expires_at_ms > evidence.portal_expires_at_ms
                || now_ms < activation.not_before_ms
                || now_ms >= activation.expires_at_ms
            {
                return Err(ManagedCloudRegistryError::Unavailable);
            }
            sqlite_managed_cloud_activation_head_matches(&tx, &activation)?;
            let next_generation = tx
                .query_row(
                    "SELECT COALESCE(MAX(activation_generation),0)+1
                       FROM jobs_managed_cloud_activations",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(managed_cloud_storage)?;
            if activation.activation_generation != next_generation {
                return Err(ManagedCloudRegistryError::SequenceRegression);
            }
            for recovery in &activation.recovery_acceptances {
                if tx
                    .query_row(
                        "SELECT 1 FROM jobs_managed_cloud_activations
                          WHERE activation_sha256=?1 AND manifest_sha256=?2
                            AND environment=?3 AND region=?4 AND channel=?5",
                        params![
                            recovery.activation_sha256,
                            recovery.manifest_sha256,
                            activation.scope.environment,
                            activation.scope.region,
                            activation.scope.channel,
                        ],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(managed_cloud_storage)?
                    .is_none()
                {
                    return Err(ManagedCloudRegistryError::NotFound);
                }
            }
            insert_sqlite_managed_cloud_signature_set(
                &tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            insert_sqlite_managed_cloud_activation(
                &tx,
                &activation,
                &activation_sha256,
                &request.envelope.canonical_base64url,
                &signature_set_sha256,
                recorded_by,
                now_ms,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_activation_import_result(
                &activation,
                &activation_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                   hashtextextended('jobs-managed-cloud-release-registry',0))",
                &[],
            )
            .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            let dependencies =
                postgres_managed_cloud_activation_dependencies(&mut tx, &activation)?;
            validate_managed_cloud_activation_dependencies(&activation, &dependencies)?;
            let existing = tx
                .query_opt(
                    "SELECT activation_sha256,canonical_activation_base64url,
                            authorization_signature_set_sha256,trust_generation
                       FROM jobs_managed_cloud_activations
                      WHERE activation_sha256=$1 OR activation_id=$2
                         OR (environment=$3 AND region=$4 AND channel=$5
                             AND trust_generation=$6 AND channel_sequence=$7)
                      LIMIT 1 FOR UPDATE",
                    &[
                        &activation_sha256,
                        &activation.activation_id,
                        &activation.scope.environment,
                        &activation.scope.region,
                        &activation.scope.channel,
                        &activation.trust_generation,
                        &activation.channel_sequence,
                    ],
                )
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                let stored_sha256: String = existing.get(0);
                let stored_canonical: String = existing.get(1);
                let stored_signature: String = existing.get(2);
                let stored_generation: i64 = existing.get(3);
                if stored_sha256 != activation_sha256
                    || stored_canonical != request.envelope.canonical_base64url
                    || stored_signature != signature_set_sha256
                    || stored_generation != activation.trust_generation
                    || !require_postgres_managed_cloud_signature_set_replay(
                        &mut tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let evidence = validate_managed_cloud_activation_evidence(
                    &parsed_evidence,
                    &activation,
                    &dependencies.portal_artifact_sha256,
                    &dependencies.failure_converter_sha256,
                    activation.issued_at_ms,
                )?;
                validate_managed_cloud_activation_evidence_bindings(&activation, &evidence)?;
                let policy = postgres_managed_cloud_policy_by_generation(
                    &mut tx,
                    activation.trust_generation,
                )?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_activation_import_result(
                    &activation,
                    &activation_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy =
                postgres_managed_cloud_policy_by_generation(&mut tx, activation.trust_generation)?;
            verify_managed_cloud_signature_set(
                &activation_bytes,
                &signature_set,
                &policy.policy,
                role,
                MANAGED_CLOUD_ACTIVATION_AUDIENCE,
                activation.issued_at_ms,
                now_ms,
            )?;
            require_postgres_managed_cloud_signing_authority_not_revoked(
                &mut tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            if tx
                .query_opt(
                    "SELECT 1 FROM jobs_managed_cloud_revocations
                      WHERE effective_at_ms<=$1 AND (
                        (subject_kind='manifest' AND subject_sha256=$2)
                        OR (subject_kind='cohort' AND subject_sha256=$3)
                        OR (subject_kind='release' AND subject_sha256=$2)
                      ) LIMIT 1",
                    &[
                        &now_ms,
                        &activation.manifest_sha256,
                        &activation.cohort_sha256,
                    ],
                )
                .map_err(managed_cloud_storage)?
                .is_some()
            {
                return Err(ManagedCloudRegistryError::Revoked);
            }
            let evidence = validate_managed_cloud_activation_evidence(
                &parsed_evidence,
                &activation,
                &dependencies.portal_artifact_sha256,
                &dependencies.failure_converter_sha256,
                now_ms,
            )?;
            validate_managed_cloud_activation_evidence_bindings(&activation, &evidence)?;
            if activation.issued_at_ms < dependencies.manifest_published_at_ms
                || activation.issued_at_ms < dependencies.cohort_issued_at_ms
                || activation.not_before_ms < dependencies.cohort_not_before_ms
                || activation.not_before_ms < policy.policy.valid_from_ms
                || activation.expires_at_ms > dependencies.cohort_expires_at_ms
                || activation.expires_at_ms > policy.policy.expires_at_ms
                || activation.expires_at_ms > evidence.portal_expires_at_ms
                || now_ms < activation.not_before_ms
                || now_ms >= activation.expires_at_ms
            {
                return Err(ManagedCloudRegistryError::Unavailable);
            }
            postgres_managed_cloud_activation_head_matches(&mut tx, &activation)?;
            let next_generation: i64 = tx
                .query_one(
                    "SELECT COALESCE(MAX(activation_generation),0)+1
                       FROM jobs_managed_cloud_activations",
                    &[],
                )
                .map_err(managed_cloud_storage)?
                .get(0);
            if activation.activation_generation != next_generation {
                return Err(ManagedCloudRegistryError::SequenceRegression);
            }
            for recovery in &activation.recovery_acceptances {
                if tx
                    .query_opt(
                        "SELECT 1 FROM jobs_managed_cloud_activations
                          WHERE activation_sha256=$1 AND manifest_sha256=$2
                            AND environment=$3 AND region=$4 AND channel=$5
                          FOR SHARE",
                        &[
                            &recovery.activation_sha256,
                            &recovery.manifest_sha256,
                            &activation.scope.environment,
                            &activation.scope.region,
                            &activation.scope.channel,
                        ],
                    )
                    .map_err(managed_cloud_storage)?
                    .is_none()
                {
                    return Err(ManagedCloudRegistryError::NotFound);
                }
            }
            insert_postgres_managed_cloud_signature_set(
                &mut tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            insert_postgres_managed_cloud_activation(
                &mut tx,
                &activation,
                &activation_sha256,
                &request.envelope.canonical_base64url,
                &signature_set_sha256,
                recorded_by,
                now_ms,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_activation_import_result(
                &activation,
                &activation_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
    })
}

fn stored_managed_cloud_activation(
    activation_sha256: &str,
    canonical_activation_base64url: String,
    canonical_signature_set_base64url: String,
) -> ManagedCloudResult<(
    Vec<u8>,
    ManagedCloudActivationAuthority,
    ManagedCloudSignatureSetAuthority,
)> {
    let activation_bytes = managed_cloud_decode_base64url(&canonical_activation_base64url)?;
    let signature_bytes = managed_cloud_decode_base64url(&canonical_signature_set_base64url)?;
    let activation = managed_cloud_parse_canonical(&activation_bytes)?;
    let signature_set = managed_cloud_parse_canonical(&signature_bytes)?;
    if managed_cloud_sha256(&activation_bytes) != activation_sha256 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok((activation_bytes, activation, signature_set))
}

fn managed_cloud_head_transition_result(
    transition: &ManagedCloudHeadTransitionAuthority,
    transition_sha256: &str,
    replayed: bool,
) -> ManagedCloudHeadTransition {
    ManagedCloudHeadTransition {
        scope: transition.scope.clone(),
        head_revision: transition.head_revision,
        transition_sha256: transition_sha256.to_string(),
        activation_sha256: transition.next_activation_sha256.clone(),
        manifest_sha256: transition.next_manifest_sha256.clone(),
        trust_generation: transition.next_trust_generation,
        channel_sequence: transition.next_channel_sequence,
        transition_kind: transition.transition_kind.clone(),
        authority_sha256: transition.authority_sha256.clone(),
        replayed,
    }
}

fn sqlite_managed_cloud_transition_by_authority(
    tx: &rusqlite::Transaction<'_>,
    authority_sha256: &str,
) -> ManagedCloudResult<Option<(ManagedCloudHeadTransitionAuthority, String)>> {
    tx.query_row(
        "SELECT transition_sha256,environment,region,channel,head_revision,
                previous_head_revision,previous_transition_sha256,
                previous_activation_sha256,previous_manifest_sha256,
                previous_trust_generation,previous_channel_sequence,
                next_activation_sha256,next_manifest_sha256,next_trust_generation,
                next_channel_sequence,transition_kind,authority_sha256,
                rollback_authority_sha256,recorded_at_ms
           FROM jobs_managed_cloud_head_transitions WHERE authority_sha256=?1",
        params![authority_sha256],
        |row| {
            let transition_sha256 = row.get(0)?;
            Ok((
                ManagedCloudHeadTransitionAuthority {
                    version: 1,
                    audience: MANAGED_CLOUD_TRANSITION_AUDIENCE.to_string(),
                    scope: ManagedCloudScope {
                        environment: row.get(1)?,
                        region: row.get(2)?,
                        channel: row.get(3)?,
                    },
                    head_revision: row.get(4)?,
                    previous_head_revision: row.get(5)?,
                    previous_transition_sha256: row.get(6)?,
                    previous_activation_sha256: row.get(7)?,
                    previous_manifest_sha256: row.get(8)?,
                    previous_trust_generation: row.get(9)?,
                    previous_channel_sequence: row.get(10)?,
                    next_activation_sha256: row.get(11)?,
                    next_manifest_sha256: row.get(12)?,
                    next_trust_generation: row.get(13)?,
                    next_channel_sequence: row.get(14)?,
                    transition_kind: row.get(15)?,
                    authority_sha256: row.get(16)?,
                    rollback_authority_sha256: row.get(17)?,
                    recorded_at_ms: row.get(18)?,
                },
                transition_sha256,
            ))
        },
    )
    .optional()
    .map_err(managed_cloud_storage)
}

fn postgres_managed_cloud_transition_by_authority(
    tx: &mut postgres::Transaction<'_>,
    authority_sha256: &str,
) -> ManagedCloudResult<Option<(ManagedCloudHeadTransitionAuthority, String)>> {
    Ok(tx
        .query_opt(
            "SELECT transition_sha256,environment,region,channel,head_revision,
                previous_head_revision,previous_transition_sha256,
                previous_activation_sha256,previous_manifest_sha256,
                previous_trust_generation,previous_channel_sequence,
                next_activation_sha256,next_manifest_sha256,next_trust_generation,
                next_channel_sequence,transition_kind,authority_sha256,
                rollback_authority_sha256,recorded_at_ms
           FROM jobs_managed_cloud_head_transitions
          WHERE authority_sha256=$1 FOR UPDATE",
            &[&authority_sha256],
        )
        .map_err(managed_cloud_storage)?
        .map(|row| {
            let transition_sha256 = row.get(0);
            (
                ManagedCloudHeadTransitionAuthority {
                    version: 1,
                    audience: MANAGED_CLOUD_TRANSITION_AUDIENCE.to_string(),
                    scope: ManagedCloudScope {
                        environment: row.get(1),
                        region: row.get(2),
                        channel: row.get(3),
                    },
                    head_revision: row.get(4),
                    previous_head_revision: row.get(5),
                    previous_transition_sha256: row.get(6),
                    previous_activation_sha256: row.get(7),
                    previous_manifest_sha256: row.get(8),
                    previous_trust_generation: row.get(9),
                    previous_channel_sequence: row.get(10),
                    next_activation_sha256: row.get(11),
                    next_manifest_sha256: row.get(12),
                    next_trust_generation: row.get(13),
                    next_channel_sequence: row.get(14),
                    transition_kind: row.get(15),
                    authority_sha256: row.get(16),
                    rollback_authority_sha256: row.get(17),
                    recorded_at_ms: row.get(18),
                },
                transition_sha256,
            )
        }))
}

fn insert_sqlite_managed_cloud_head_transition(
    tx: &rusqlite::Transaction<'_>,
    transition: &ManagedCloudHeadTransitionAuthority,
    transition_sha256: &str,
    recorded_by: &str,
) -> ManagedCloudResult<()> {
    tx.execute(
        "INSERT INTO jobs_managed_cloud_head_transitions(
           transition_sha256,environment,region,channel,head_revision,
           previous_head_revision,previous_transition_sha256,
           previous_activation_sha256,previous_manifest_sha256,
           previous_trust_generation,previous_channel_sequence,
           next_activation_sha256,next_manifest_sha256,next_trust_generation,
           next_channel_sequence,transition_kind,authority_sha256,
           rollback_authority_sha256,recorded_by,recorded_at_ms
         ) VALUES(
           ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,
           ?17,?18,?19,?20)",
        params![
            transition_sha256,
            transition.scope.environment,
            transition.scope.region,
            transition.scope.channel,
            transition.head_revision,
            transition.previous_head_revision,
            transition.previous_transition_sha256,
            transition.previous_activation_sha256,
            transition.previous_manifest_sha256,
            transition.previous_trust_generation,
            transition.previous_channel_sequence,
            transition.next_activation_sha256,
            transition.next_manifest_sha256,
            transition.next_trust_generation,
            transition.next_channel_sequence,
            transition.transition_kind,
            transition.authority_sha256,
            transition.rollback_authority_sha256,
            recorded_by,
            transition.recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    if transition.previous_head_revision == 0 {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_heads(
               environment,region,channel,head_revision,current_transition_sha256,
               current_activation_sha256,current_manifest_sha256,
               current_trust_generation,current_channel_sequence,updated_at_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                transition.scope.environment,
                transition.scope.region,
                transition.scope.channel,
                transition.head_revision,
                transition_sha256,
                transition.next_activation_sha256,
                transition.next_manifest_sha256,
                transition.next_trust_generation,
                transition.next_channel_sequence,
                transition.recorded_at_ms,
            ],
        )
        .map_err(managed_cloud_storage)?;
    } else {
        let changed = tx
            .execute(
                "UPDATE jobs_managed_cloud_heads
                    SET head_revision=?4,current_transition_sha256=?5,
                        current_activation_sha256=?6,current_manifest_sha256=?7,
                        current_trust_generation=?8,current_channel_sequence=?9,
                        updated_at_ms=?10
                  WHERE environment=?1 AND region=?2 AND channel=?3
                    AND head_revision=?11 AND current_transition_sha256=?12",
                params![
                    transition.scope.environment,
                    transition.scope.region,
                    transition.scope.channel,
                    transition.head_revision,
                    transition_sha256,
                    transition.next_activation_sha256,
                    transition.next_manifest_sha256,
                    transition.next_trust_generation,
                    transition.next_channel_sequence,
                    transition.recorded_at_ms,
                    transition.previous_head_revision,
                    transition.previous_transition_sha256,
                ],
            )
            .map_err(managed_cloud_storage)?;
        if changed != 1 {
            return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
        }
    }
    Ok(())
}

fn insert_postgres_managed_cloud_head_transition(
    tx: &mut postgres::Transaction<'_>,
    transition: &ManagedCloudHeadTransitionAuthority,
    transition_sha256: &str,
    recorded_by: &str,
) -> ManagedCloudResult<()> {
    tx.execute(
        "INSERT INTO jobs_managed_cloud_head_transitions(
           transition_sha256,environment,region,channel,head_revision,
           previous_head_revision,previous_transition_sha256,
           previous_activation_sha256,previous_manifest_sha256,
           previous_trust_generation,previous_channel_sequence,
           next_activation_sha256,next_manifest_sha256,next_trust_generation,
           next_channel_sequence,transition_kind,authority_sha256,
           rollback_authority_sha256,recorded_by,recorded_at_ms
         ) VALUES(
           $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
           $17,$18,$19,$20)",
        &[
            &transition_sha256,
            &transition.scope.environment,
            &transition.scope.region,
            &transition.scope.channel,
            &transition.head_revision,
            &transition.previous_head_revision,
            &transition.previous_transition_sha256,
            &transition.previous_activation_sha256,
            &transition.previous_manifest_sha256,
            &transition.previous_trust_generation,
            &transition.previous_channel_sequence,
            &transition.next_activation_sha256,
            &transition.next_manifest_sha256,
            &transition.next_trust_generation,
            &transition.next_channel_sequence,
            &transition.transition_kind,
            &transition.authority_sha256,
            &transition.rollback_authority_sha256,
            &recorded_by,
            &transition.recorded_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    if transition.previous_head_revision == 0 {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_heads(
               environment,region,channel,head_revision,current_transition_sha256,
               current_activation_sha256,current_manifest_sha256,
               current_trust_generation,current_channel_sequence,updated_at_ms
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            &[
                &transition.scope.environment,
                &transition.scope.region,
                &transition.scope.channel,
                &transition.head_revision,
                &transition_sha256,
                &transition.next_activation_sha256,
                &transition.next_manifest_sha256,
                &transition.next_trust_generation,
                &transition.next_channel_sequence,
                &transition.recorded_at_ms,
            ],
        )
        .map_err(managed_cloud_storage)?;
    } else {
        let changed = tx
            .execute(
                "UPDATE jobs_managed_cloud_heads
                    SET head_revision=$4,current_transition_sha256=$5,
                        current_activation_sha256=$6,current_manifest_sha256=$7,
                        current_trust_generation=$8,current_channel_sequence=$9,
                        updated_at_ms=$10
                  WHERE environment=$1 AND region=$2 AND channel=$3
                    AND head_revision=$11 AND current_transition_sha256=$12",
                &[
                    &transition.scope.environment,
                    &transition.scope.region,
                    &transition.scope.channel,
                    &transition.head_revision,
                    &transition_sha256,
                    &transition.next_activation_sha256,
                    &transition.next_manifest_sha256,
                    &transition.next_trust_generation,
                    &transition.next_channel_sequence,
                    &transition.recorded_at_ms,
                    &transition.previous_head_revision,
                    &transition.previous_transition_sha256,
                ],
            )
            .map_err(managed_cloud_storage)?;
        if changed != 1 {
            return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
        }
    }
    Ok(())
}

pub fn apply_managed_cloud_activation(
    pool: &DbPool,
    request: &ApplyManagedCloudActivationRequest,
    recorded_by: &str,
) -> ManagedCloudResult<ManagedCloudHeadTransition> {
    if !managed_cloud_hex64(&request.activation_sha256)
        || !managed_cloud_safe_integer(request.expected_head_revision, false)
        || request
            .expected_transition_sha256
            .as_ref()
            .is_some_and(|value| !managed_cloud_hex64(value))
        || (request.expected_head_revision == 0) != request.expected_transition_sha256.is_none()
        || !managed_cloud_actor(recorded_by)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let stored = tx
                .query_row(
                    "SELECT activation.canonical_activation_base64url,
                            signature_set.canonical_signature_set_base64url
                       FROM jobs_managed_cloud_activations activation
                       JOIN jobs_managed_cloud_signature_sets signature_set
                         ON signature_set.signature_set_sha256=
                            activation.authorization_signature_set_sha256
                      WHERE activation.activation_sha256=?1",
                    params![request.activation_sha256],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            let (activation_bytes, activation, signature_set) =
                stored_managed_cloud_activation(&request.activation_sha256, stored.0, stored.1)?;
            if request.expected_head_revision != activation.expected_head_revision
                || request.expected_transition_sha256 != activation.expected_transition_sha256
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
            if let Some((transition, transition_sha256)) =
                sqlite_managed_cloud_transition_by_authority(&tx, &request.activation_sha256)?
            {
                let is_head = tx
                    .query_row(
                        "SELECT 1 FROM jobs_managed_cloud_heads
                          WHERE environment=?1 AND region=?2 AND channel=?3
                            AND current_transition_sha256=?4",
                        params![
                            transition.scope.environment,
                            transition.scope.region,
                            transition.scope.channel,
                            transition_sha256,
                        ],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(managed_cloud_storage)?
                    .is_some();
                if !is_head {
                    return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
                }
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_head_transition_result(
                    &transition,
                    &transition_sha256,
                    true,
                ));
            }
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            let policy =
                sqlite_managed_cloud_policy_by_generation(&tx, activation.trust_generation)?;
            let role = if activation.scope.channel == "general" {
                "general_promotion"
            } else {
                "promotion"
            };
            verify_managed_cloud_signature_set(
                &activation_bytes,
                &signature_set,
                &policy.policy,
                role,
                MANAGED_CLOUD_ACTIVATION_AUDIENCE,
                activation.issued_at_ms,
                now_ms,
            )?;
            require_sqlite_managed_cloud_signing_authority_not_revoked(
                &tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            let dependencies = sqlite_managed_cloud_activation_dependencies(&tx, &activation)?;
            validate_managed_cloud_activation_dependencies(&activation, &dependencies)?;
            if now_ms < activation.not_before_ms
                || now_ms >= activation.expires_at_ms
                || now_ms < dependencies.cohort_not_before_ms
                || now_ms >= dependencies.cohort_expires_at_ms
                || tx
                    .query_row(
                        "SELECT 1 FROM jobs_managed_cloud_revocations
                          WHERE effective_at_ms<=?1 AND (
                            (subject_kind='activation' AND subject_sha256=?2)
                            OR (subject_kind='manifest' AND subject_sha256=?3)
                            OR (subject_kind='release' AND subject_sha256=?3)
                            OR (subject_kind='cohort' AND subject_sha256=?4)
                          ) LIMIT 1",
                        params![
                            now_ms,
                            request.activation_sha256,
                            activation.manifest_sha256,
                            activation.cohort_sha256,
                        ],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(managed_cloud_storage)?
                    .is_some()
            {
                return Err(ManagedCloudRegistryError::Unavailable);
            }
            sqlite_managed_cloud_activation_head_matches(&tx, &activation)?;
            let previous = tx
                .query_row(
                    "SELECT head_revision,current_transition_sha256,
                            current_activation_sha256,current_manifest_sha256,
                            current_trust_generation,current_channel_sequence
                       FROM jobs_managed_cloud_heads
                      WHERE environment=?1 AND region=?2 AND channel=?3",
                    params![
                        activation.scope.environment,
                        activation.scope.region,
                        activation.scope.channel,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            let transition = ManagedCloudHeadTransitionAuthority {
                version: 1,
                audience: MANAGED_CLOUD_TRANSITION_AUDIENCE.to_string(),
                scope: activation.scope.clone(),
                head_revision: activation.expected_head_revision + 1,
                previous_head_revision: activation.expected_head_revision,
                previous_transition_sha256: previous.as_ref().map(|value| value.1.clone()),
                previous_activation_sha256: previous.as_ref().map(|value| value.2.clone()),
                previous_manifest_sha256: previous.as_ref().map(|value| value.3.clone()),
                previous_trust_generation: previous.as_ref().map(|value| value.4),
                previous_channel_sequence: previous.as_ref().map(|value| value.5),
                next_activation_sha256: request.activation_sha256.clone(),
                next_manifest_sha256: activation.manifest_sha256.clone(),
                next_trust_generation: activation.trust_generation,
                next_channel_sequence: activation.channel_sequence,
                transition_kind: "activation".to_string(),
                authority_sha256: request.activation_sha256.clone(),
                rollback_authority_sha256: None,
                recorded_at_ms: now_ms,
            };
            let transition_sha256 = managed_cloud_digest(&transition)?;
            insert_sqlite_managed_cloud_head_transition(
                &tx,
                &transition,
                &transition_sha256,
                recorded_by,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_head_transition_result(
                &transition,
                &transition_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                   hashtextextended('jobs-managed-cloud-release-registry',0))",
                &[],
            )
            .map_err(managed_cloud_storage)?;
            let stored = tx
                .query_opt(
                    "SELECT activation.canonical_activation_base64url,
                            signature_set.canonical_signature_set_base64url
                       FROM jobs_managed_cloud_activations activation
                       JOIN jobs_managed_cloud_signature_sets signature_set
                         ON signature_set.signature_set_sha256=
                            activation.authorization_signature_set_sha256
                      WHERE activation.activation_sha256=$1
                      FOR SHARE OF activation,signature_set",
                    &[&request.activation_sha256],
                )
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            let (activation_bytes, activation, signature_set) = stored_managed_cloud_activation(
                &request.activation_sha256,
                stored.get(0),
                stored.get(1),
            )?;
            if request.expected_head_revision != activation.expected_head_revision
                || request.expected_transition_sha256 != activation.expected_transition_sha256
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
            if let Some((transition, transition_sha256)) =
                postgres_managed_cloud_transition_by_authority(&mut tx, &request.activation_sha256)?
            {
                let is_head = tx
                    .query_opt(
                        "SELECT 1 FROM jobs_managed_cloud_heads
                          WHERE environment=$1 AND region=$2 AND channel=$3
                            AND current_transition_sha256=$4 FOR SHARE",
                        &[
                            &transition.scope.environment,
                            &transition.scope.region,
                            &transition.scope.channel,
                            &transition_sha256,
                        ],
                    )
                    .map_err(managed_cloud_storage)?
                    .is_some();
                if !is_head {
                    return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
                }
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_head_transition_result(
                    &transition,
                    &transition_sha256,
                    true,
                ));
            }
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            let policy =
                postgres_managed_cloud_policy_by_generation(&mut tx, activation.trust_generation)?;
            let role = if activation.scope.channel == "general" {
                "general_promotion"
            } else {
                "promotion"
            };
            verify_managed_cloud_signature_set(
                &activation_bytes,
                &signature_set,
                &policy.policy,
                role,
                MANAGED_CLOUD_ACTIVATION_AUDIENCE,
                activation.issued_at_ms,
                now_ms,
            )?;
            require_postgres_managed_cloud_signing_authority_not_revoked(
                &mut tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            let dependencies =
                postgres_managed_cloud_activation_dependencies(&mut tx, &activation)?;
            validate_managed_cloud_activation_dependencies(&activation, &dependencies)?;
            if now_ms < activation.not_before_ms
                || now_ms >= activation.expires_at_ms
                || now_ms < dependencies.cohort_not_before_ms
                || now_ms >= dependencies.cohort_expires_at_ms
                || tx
                    .query_opt(
                        "SELECT 1 FROM jobs_managed_cloud_revocations
                          WHERE effective_at_ms<=$1 AND (
                            (subject_kind='activation' AND subject_sha256=$2)
                            OR (subject_kind='manifest' AND subject_sha256=$3)
                            OR (subject_kind='release' AND subject_sha256=$3)
                            OR (subject_kind='cohort' AND subject_sha256=$4)
                          ) LIMIT 1",
                        &[
                            &now_ms,
                            &request.activation_sha256,
                            &activation.manifest_sha256,
                            &activation.cohort_sha256,
                        ],
                    )
                    .map_err(managed_cloud_storage)?
                    .is_some()
            {
                return Err(ManagedCloudRegistryError::Unavailable);
            }
            postgres_managed_cloud_activation_head_matches(&mut tx, &activation)?;
            let previous = tx
                .query_opt(
                    "SELECT head_revision,current_transition_sha256,
                            current_activation_sha256,current_manifest_sha256,
                            current_trust_generation,current_channel_sequence
                       FROM jobs_managed_cloud_heads
                      WHERE environment=$1 AND region=$2 AND channel=$3 FOR UPDATE",
                    &[
                        &activation.scope.environment,
                        &activation.scope.region,
                        &activation.scope.channel,
                    ],
                )
                .map_err(managed_cloud_storage)?
                .map(|row| {
                    (
                        row.get::<_, i64>(0),
                        row.get::<_, String>(1),
                        row.get::<_, String>(2),
                        row.get::<_, String>(3),
                        row.get::<_, i64>(4),
                        row.get::<_, i64>(5),
                    )
                });
            let transition = ManagedCloudHeadTransitionAuthority {
                version: 1,
                audience: MANAGED_CLOUD_TRANSITION_AUDIENCE.to_string(),
                scope: activation.scope.clone(),
                head_revision: activation.expected_head_revision + 1,
                previous_head_revision: activation.expected_head_revision,
                previous_transition_sha256: previous.as_ref().map(|value| value.1.clone()),
                previous_activation_sha256: previous.as_ref().map(|value| value.2.clone()),
                previous_manifest_sha256: previous.as_ref().map(|value| value.3.clone()),
                previous_trust_generation: previous.as_ref().map(|value| value.4),
                previous_channel_sequence: previous.as_ref().map(|value| value.5),
                next_activation_sha256: request.activation_sha256.clone(),
                next_manifest_sha256: activation.manifest_sha256.clone(),
                next_trust_generation: activation.trust_generation,
                next_channel_sequence: activation.channel_sequence,
                transition_kind: "activation".to_string(),
                authority_sha256: request.activation_sha256.clone(),
                rollback_authority_sha256: None,
                recorded_at_ms: now_ms,
            };
            let transition_sha256 = managed_cloud_digest(&transition)?;
            insert_postgres_managed_cloud_head_transition(
                &mut tx,
                &transition,
                &transition_sha256,
                recorded_by,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_head_transition_result(
                &transition,
                &transition_sha256,
                false,
            ))
        }
    })
}

fn managed_cloud_rollback_import_result(
    rollback: &ManagedCloudRollbackAuthority,
    rollback_sha256: &str,
    signature_set_sha256: &str,
    policy_sha256: &str,
    replayed: bool,
) -> ManagedCloudImportResult {
    ManagedCloudImportResult {
        authority_kind: "rollback".to_string(),
        authority_id: rollback.rollback_id.clone(),
        authority_sha256: rollback_sha256.to_string(),
        signature_set_sha256: signature_set_sha256.to_string(),
        trust_policy_sha256: policy_sha256.to_string(),
        replayed,
    }
}

pub fn import_managed_cloud_rollback(
    pool: &DbPool,
    request: &ManagedCloudRollbackImportRequest,
    recorded_by: &str,
) -> ManagedCloudResult<ManagedCloudImportResult> {
    if !managed_cloud_actor(recorded_by) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let (rollback_bytes, signature_set_bytes) = managed_cloud_decode_envelope(&request.envelope)?;
    let rollback: ManagedCloudRollbackAuthority = managed_cloud_parse_canonical(&rollback_bytes)?;
    let signature_set: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&signature_set_bytes)?;
    validate_managed_cloud_rollback(&rollback)?;
    validate_managed_cloud_signature_set(&signature_set)?;
    let rollback_sha256 = managed_cloud_sha256(&rollback_bytes);
    let signature_set_sha256 = managed_cloud_sha256(&signature_set_bytes);

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            let existing = tx
                .query_row(
                    "SELECT rollback_sha256,canonical_rollback_base64url,
                            authorization_signature_set_sha256,trust_generation
                       FROM jobs_managed_cloud_rollbacks
                      WHERE rollback_sha256=?1 OR rollback_id=?2
                         OR (trust_generation=?3 AND rollback_generation=?4)
                      LIMIT 1",
                    params![
                        rollback_sha256,
                        rollback.rollback_id,
                        rollback.trust_generation,
                        rollback.rollback_generation,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                if existing.0 != rollback_sha256
                    || existing.1 != request.envelope.canonical_base64url
                    || existing.2 != signature_set_sha256
                    || existing.3 != rollback.trust_generation
                    || !require_sqlite_managed_cloud_signature_set_replay(
                        &tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let policy =
                    sqlite_managed_cloud_policy_by_generation(&tx, rollback.trust_generation)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_rollback_import_result(
                    &rollback,
                    &rollback_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy = sqlite_managed_cloud_policy_by_generation(&tx, rollback.trust_generation)?;
            verify_managed_cloud_signature_set(
                &rollback_bytes,
                &signature_set,
                &policy.policy,
                "incident",
                MANAGED_CLOUD_ROLLBACK_AUDIENCE,
                rollback.issued_at_ms,
                now_ms,
            )?;
            require_sqlite_managed_cloud_signing_authority_not_revoked(
                &tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            let head = tx
                .query_row(
                    "SELECT head_revision,current_transition_sha256,
                            current_activation_sha256,current_manifest_sha256,
                            current_trust_generation,current_channel_sequence
                       FROM jobs_managed_cloud_heads
                      WHERE environment=?1 AND region=?2 AND channel=?3",
                    params![
                        rollback.scope.environment,
                        rollback.scope.region,
                        rollback.scope.channel,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::CompareAndSwapConflict)?;
            if head.0 != rollback.expected_head_revision
                || head.1 != rollback.expected_transition_sha256
                || head.2 != rollback.from_activation_sha256
                || head.3 != rollback.from_manifest_sha256
            {
                return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
            }
            let successor = tx
                .query_row(
                    "SELECT trust_generation,channel_sequence,expected_head_revision,
                            expected_transition_sha256,predecessor_activation_sha256,
                            issued_at_ms,not_before_ms,expires_at_ms
                       FROM jobs_managed_cloud_activations
                      WHERE activation_sha256=?1 AND manifest_sha256=?2
                        AND environment=?3 AND region=?4 AND channel=?5",
                    params![
                        rollback.to_activation_sha256,
                        rollback.to_manifest_sha256,
                        rollback.scope.environment,
                        rollback.scope.region,
                        rollback.scope.channel,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, i64>(7)?,
                        ))
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            if successor.0 != rollback.trust_generation
                || successor.2 != rollback.expected_head_revision
                || successor.3.as_deref() != Some(&rollback.expected_transition_sha256)
                || successor.4.as_deref() != Some(&rollback.from_activation_sha256)
                || !(successor.0 > head.4 || (successor.0 == head.4 && successor.1 > head.5))
                || successor.5 > rollback.issued_at_ms
                || now_ms < successor.6
                || now_ms >= successor.7
            {
                return Err(ManagedCloudRegistryError::DowngradeRequiresRollback);
            }
            let next_generation = tx
                .query_row(
                    "SELECT COALESCE(MAX(rollback_generation),0)+1
                       FROM jobs_managed_cloud_rollbacks WHERE trust_generation=?1",
                    params![rollback.trust_generation],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(managed_cloud_storage)?;
            if rollback.rollback_generation != next_generation {
                return Err(ManagedCloudRegistryError::SequenceRegression);
            }
            insert_sqlite_managed_cloud_signature_set(
                &tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_rollbacks(
                   rollback_sha256,rollback_id,rollback_generation,trust_generation,
                   environment,region,channel,expected_head_revision,
                   expected_transition_sha256,from_activation_sha256,
                   from_manifest_sha256,to_activation_sha256,to_manifest_sha256,
                   evidence_sha256,reason_ref,canonical_rollback_base64url,
                   authorization_signature_set_sha256,issued_at_ms,recorded_by,
                   recorded_at_ms
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,
                          ?14,?15,?16,?17,?18,?19,?20)",
                params![
                    rollback_sha256,
                    rollback.rollback_id,
                    rollback.rollback_generation,
                    rollback.trust_generation,
                    rollback.scope.environment,
                    rollback.scope.region,
                    rollback.scope.channel,
                    rollback.expected_head_revision,
                    rollback.expected_transition_sha256,
                    rollback.from_activation_sha256,
                    rollback.from_manifest_sha256,
                    rollback.to_activation_sha256,
                    rollback.to_manifest_sha256,
                    rollback.evidence_sha256,
                    rollback.reason_ref,
                    request.envelope.canonical_base64url,
                    signature_set_sha256,
                    rollback.issued_at_ms,
                    recorded_by,
                    now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_rollback_import_result(
                &rollback,
                &rollback_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                   hashtextextended('jobs-managed-cloud-release-registry',0))",
                &[],
            )
            .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            let existing = tx
                .query_opt(
                    "SELECT rollback_sha256,canonical_rollback_base64url,
                            authorization_signature_set_sha256,trust_generation
                       FROM jobs_managed_cloud_rollbacks
                      WHERE rollback_sha256=$1 OR rollback_id=$2
                         OR (trust_generation=$3 AND rollback_generation=$4)
                      LIMIT 1 FOR UPDATE",
                    &[
                        &rollback_sha256,
                        &rollback.rollback_id,
                        &rollback.trust_generation,
                        &rollback.rollback_generation,
                    ],
                )
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                if existing.get::<_, String>(0) != rollback_sha256
                    || existing.get::<_, String>(1) != request.envelope.canonical_base64url
                    || existing.get::<_, String>(2) != signature_set_sha256
                    || existing.get::<_, i64>(3) != rollback.trust_generation
                    || !require_postgres_managed_cloud_signature_set_replay(
                        &mut tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let policy = postgres_managed_cloud_policy_by_generation(
                    &mut tx,
                    rollback.trust_generation,
                )?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_rollback_import_result(
                    &rollback,
                    &rollback_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy =
                postgres_managed_cloud_policy_by_generation(&mut tx, rollback.trust_generation)?;
            verify_managed_cloud_signature_set(
                &rollback_bytes,
                &signature_set,
                &policy.policy,
                "incident",
                MANAGED_CLOUD_ROLLBACK_AUDIENCE,
                rollback.issued_at_ms,
                now_ms,
            )?;
            require_postgres_managed_cloud_signing_authority_not_revoked(
                &mut tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            let head = tx
                .query_opt(
                    "SELECT head_revision,current_transition_sha256,
                            current_activation_sha256,current_manifest_sha256,
                            current_trust_generation,current_channel_sequence
                       FROM jobs_managed_cloud_heads
                      WHERE environment=$1 AND region=$2 AND channel=$3 FOR UPDATE",
                    &[
                        &rollback.scope.environment,
                        &rollback.scope.region,
                        &rollback.scope.channel,
                    ],
                )
                .map_err(managed_cloud_storage)?
                .map(|row| {
                    (
                        row.get::<_, i64>(0),
                        row.get::<_, String>(1),
                        row.get::<_, String>(2),
                        row.get::<_, String>(3),
                        row.get::<_, i64>(4),
                        row.get::<_, i64>(5),
                    )
                })
                .ok_or(ManagedCloudRegistryError::CompareAndSwapConflict)?;
            if head.0 != rollback.expected_head_revision
                || head.1 != rollback.expected_transition_sha256
                || head.2 != rollback.from_activation_sha256
                || head.3 != rollback.from_manifest_sha256
            {
                return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
            }
            let successor = tx
                .query_opt(
                    "SELECT trust_generation,channel_sequence,expected_head_revision,
                            expected_transition_sha256,predecessor_activation_sha256,
                            issued_at_ms,not_before_ms,expires_at_ms
                       FROM jobs_managed_cloud_activations
                      WHERE activation_sha256=$1 AND manifest_sha256=$2
                        AND environment=$3 AND region=$4 AND channel=$5
                      FOR SHARE",
                    &[
                        &rollback.to_activation_sha256,
                        &rollback.to_manifest_sha256,
                        &rollback.scope.environment,
                        &rollback.scope.region,
                        &rollback.scope.channel,
                    ],
                )
                .map_err(managed_cloud_storage)?
                .map(|row| {
                    (
                        row.get::<_, i64>(0),
                        row.get::<_, i64>(1),
                        row.get::<_, i64>(2),
                        row.get::<_, Option<String>>(3),
                        row.get::<_, Option<String>>(4),
                        row.get::<_, i64>(5),
                        row.get::<_, i64>(6),
                        row.get::<_, i64>(7),
                    )
                })
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            if successor.0 != rollback.trust_generation
                || successor.2 != rollback.expected_head_revision
                || successor.3.as_deref() != Some(&rollback.expected_transition_sha256)
                || successor.4.as_deref() != Some(&rollback.from_activation_sha256)
                || !(successor.0 > head.4 || (successor.0 == head.4 && successor.1 > head.5))
                || successor.5 > rollback.issued_at_ms
                || now_ms < successor.6
                || now_ms >= successor.7
            {
                return Err(ManagedCloudRegistryError::DowngradeRequiresRollback);
            }
            let next_generation: i64 = tx
                .query_one(
                    "SELECT COALESCE(MAX(rollback_generation),0)+1
                       FROM jobs_managed_cloud_rollbacks WHERE trust_generation=$1",
                    &[&rollback.trust_generation],
                )
                .map_err(managed_cloud_storage)?
                .get(0);
            if rollback.rollback_generation != next_generation {
                return Err(ManagedCloudRegistryError::SequenceRegression);
            }
            insert_postgres_managed_cloud_signature_set(
                &mut tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_rollbacks(
                   rollback_sha256,rollback_id,rollback_generation,trust_generation,
                   environment,region,channel,expected_head_revision,
                   expected_transition_sha256,from_activation_sha256,
                   from_manifest_sha256,to_activation_sha256,to_manifest_sha256,
                   evidence_sha256,reason_ref,canonical_rollback_base64url,
                   authorization_signature_set_sha256,issued_at_ms,recorded_by,
                   recorded_at_ms
                 ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,
                          $15,$16,$17,$18,$19,$20)",
                &[
                    &rollback_sha256,
                    &rollback.rollback_id,
                    &rollback.rollback_generation,
                    &rollback.trust_generation,
                    &rollback.scope.environment,
                    &rollback.scope.region,
                    &rollback.scope.channel,
                    &rollback.expected_head_revision,
                    &rollback.expected_transition_sha256,
                    &rollback.from_activation_sha256,
                    &rollback.from_manifest_sha256,
                    &rollback.to_activation_sha256,
                    &rollback.to_manifest_sha256,
                    &rollback.evidence_sha256,
                    &rollback.reason_ref,
                    &request.envelope.canonical_base64url,
                    &signature_set_sha256,
                    &rollback.issued_at_ms,
                    &recorded_by,
                    &now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_rollback_import_result(
                &rollback,
                &rollback_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
    })
}

pub fn apply_managed_cloud_rollback(
    pool: &DbPool,
    request: &ApplyManagedCloudRollbackRequest,
    recorded_by: &str,
) -> ManagedCloudResult<ManagedCloudHeadTransition> {
    if !managed_cloud_hex64(&request.rollback_sha256)
        || !managed_cloud_safe_integer(request.expected_head_revision, true)
        || !managed_cloud_hex64(&request.expected_transition_sha256)
        || !managed_cloud_actor(recorded_by)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let stored = tx
                .query_row(
                    "SELECT rollback.canonical_rollback_base64url,
                            signature_set.canonical_signature_set_base64url
                       FROM jobs_managed_cloud_rollbacks rollback
                       JOIN jobs_managed_cloud_signature_sets signature_set
                         ON signature_set.signature_set_sha256=
                            rollback.authorization_signature_set_sha256
                      WHERE rollback.rollback_sha256=?1",
                    params![request.rollback_sha256],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            let rollback_bytes = managed_cloud_decode_base64url(&stored.0)?;
            let signature_bytes = managed_cloud_decode_base64url(&stored.1)?;
            let rollback: ManagedCloudRollbackAuthority =
                managed_cloud_parse_canonical(&rollback_bytes)?;
            let rollback_signature: ManagedCloudSignatureSetAuthority =
                managed_cloud_parse_canonical(&signature_bytes)?;
            if managed_cloud_sha256(&rollback_bytes) != request.rollback_sha256
                || rollback.expected_head_revision != request.expected_head_revision
                || rollback.expected_transition_sha256 != request.expected_transition_sha256
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
            if let Some((transition, transition_sha256)) =
                sqlite_managed_cloud_transition_by_authority(&tx, &request.rollback_sha256)?
            {
                let is_head = tx
                    .query_row(
                        "SELECT 1 FROM jobs_managed_cloud_heads
                          WHERE environment=?1 AND region=?2 AND channel=?3
                            AND current_transition_sha256=?4",
                        params![
                            transition.scope.environment,
                            transition.scope.region,
                            transition.scope.channel,
                            transition_sha256,
                        ],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(managed_cloud_storage)?
                    .is_some();
                if !is_head {
                    return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
                }
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_head_transition_result(
                    &transition,
                    &transition_sha256,
                    true,
                ));
            }
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            let policy = sqlite_managed_cloud_policy_by_generation(&tx, rollback.trust_generation)?;
            verify_managed_cloud_signature_set(
                &rollback_bytes,
                &rollback_signature,
                &policy.policy,
                "incident",
                MANAGED_CLOUD_ROLLBACK_AUDIENCE,
                rollback.issued_at_ms,
                now_ms,
            )?;
            require_sqlite_managed_cloud_signing_authority_not_revoked(
                &tx,
                &policy,
                &rollback_signature,
                now_ms,
            )?;
            let activation_stored = tx
                .query_row(
                    "SELECT activation.canonical_activation_base64url,
                            signature_set.canonical_signature_set_base64url
                       FROM jobs_managed_cloud_activations activation
                       JOIN jobs_managed_cloud_signature_sets signature_set
                         ON signature_set.signature_set_sha256=
                            activation.authorization_signature_set_sha256
                      WHERE activation.activation_sha256=?1
                        AND activation.manifest_sha256=?2",
                    params![rollback.to_activation_sha256, rollback.to_manifest_sha256,],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            let (activation_bytes, activation, activation_signature) =
                stored_managed_cloud_activation(
                    &rollback.to_activation_sha256,
                    activation_stored.0,
                    activation_stored.1,
                )?;
            let activation_role = if activation.scope.channel == "general" {
                "general_promotion"
            } else {
                "promotion"
            };
            verify_managed_cloud_signature_set(
                &activation_bytes,
                &activation_signature,
                &policy.policy,
                activation_role,
                MANAGED_CLOUD_ACTIVATION_AUDIENCE,
                activation.issued_at_ms,
                now_ms,
            )?;
            require_sqlite_managed_cloud_signing_authority_not_revoked(
                &tx,
                &policy,
                &activation_signature,
                now_ms,
            )?;
            let dependencies = sqlite_managed_cloud_activation_dependencies(&tx, &activation)?;
            validate_managed_cloud_activation_dependencies(&activation, &dependencies)?;
            let head = tx
                .query_row(
                    "SELECT head_revision,current_transition_sha256,
                            current_activation_sha256,current_manifest_sha256,
                            current_trust_generation,current_channel_sequence
                       FROM jobs_managed_cloud_heads
                      WHERE environment=?1 AND region=?2 AND channel=?3",
                    params![
                        rollback.scope.environment,
                        rollback.scope.region,
                        rollback.scope.channel,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::CompareAndSwapConflict)?;
            if head.0 != rollback.expected_head_revision
                || head.1 != rollback.expected_transition_sha256
                || head.2 != rollback.from_activation_sha256
                || head.3 != rollback.from_manifest_sha256
                || activation.expected_head_revision != head.0
                || activation.expected_transition_sha256.as_deref() != Some(&head.1)
                || activation.predecessor_activation_sha256.as_deref() != Some(&head.2)
                || !(activation.trust_generation > head.4
                    || (activation.trust_generation == head.4
                        && activation.channel_sequence > head.5))
                || now_ms < activation.not_before_ms
                || now_ms >= activation.expires_at_ms
                || now_ms < dependencies.cohort_not_before_ms
                || now_ms >= dependencies.cohort_expires_at_ms
                || tx
                    .query_row(
                        "SELECT 1 FROM jobs_managed_cloud_revocations
                          WHERE effective_at_ms<=?1 AND (
                            (subject_kind='rollback' AND subject_sha256=?2)
                            OR (subject_kind='activation' AND subject_sha256=?3)
                            OR (subject_kind='manifest' AND subject_sha256=?4)
                            OR (subject_kind='release' AND subject_sha256=?4)
                            OR (subject_kind='cohort' AND subject_sha256=?5)
                          ) LIMIT 1",
                        params![
                            now_ms,
                            request.rollback_sha256,
                            rollback.to_activation_sha256,
                            activation.manifest_sha256,
                            activation.cohort_sha256,
                        ],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(managed_cloud_storage)?
                    .is_some()
            {
                return Err(ManagedCloudRegistryError::Unavailable);
            }
            let transition = ManagedCloudHeadTransitionAuthority {
                version: 1,
                audience: MANAGED_CLOUD_TRANSITION_AUDIENCE.to_string(),
                scope: rollback.scope.clone(),
                head_revision: head.0 + 1,
                previous_head_revision: head.0,
                previous_transition_sha256: Some(head.1),
                previous_activation_sha256: Some(head.2),
                previous_manifest_sha256: Some(head.3),
                previous_trust_generation: Some(head.4),
                previous_channel_sequence: Some(head.5),
                next_activation_sha256: rollback.to_activation_sha256.clone(),
                next_manifest_sha256: rollback.to_manifest_sha256.clone(),
                next_trust_generation: activation.trust_generation,
                next_channel_sequence: activation.channel_sequence,
                transition_kind: "rollback".to_string(),
                authority_sha256: request.rollback_sha256.clone(),
                rollback_authority_sha256: Some(request.rollback_sha256.clone()),
                recorded_at_ms: now_ms,
            };
            let transition_sha256 = managed_cloud_digest(&transition)?;
            insert_sqlite_managed_cloud_head_transition(
                &tx,
                &transition,
                &transition_sha256,
                recorded_by,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_head_transition_result(
                &transition,
                &transition_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                   hashtextextended('jobs-managed-cloud-release-registry',0))",
                &[],
            )
            .map_err(managed_cloud_storage)?;
            let stored = tx
                .query_opt(
                    "SELECT rollback.canonical_rollback_base64url,
                            signature_set.canonical_signature_set_base64url
                       FROM jobs_managed_cloud_rollbacks rollback
                       JOIN jobs_managed_cloud_signature_sets signature_set
                         ON signature_set.signature_set_sha256=
                            rollback.authorization_signature_set_sha256
                      WHERE rollback.rollback_sha256=$1
                      FOR SHARE OF rollback,signature_set",
                    &[&request.rollback_sha256],
                )
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            let rollback_bytes = managed_cloud_decode_base64url(&stored.get::<_, String>(0))?;
            let signature_bytes = managed_cloud_decode_base64url(&stored.get::<_, String>(1))?;
            let rollback: ManagedCloudRollbackAuthority =
                managed_cloud_parse_canonical(&rollback_bytes)?;
            let rollback_signature: ManagedCloudSignatureSetAuthority =
                managed_cloud_parse_canonical(&signature_bytes)?;
            if managed_cloud_sha256(&rollback_bytes) != request.rollback_sha256
                || rollback.expected_head_revision != request.expected_head_revision
                || rollback.expected_transition_sha256 != request.expected_transition_sha256
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
            if let Some((transition, transition_sha256)) =
                postgres_managed_cloud_transition_by_authority(&mut tx, &request.rollback_sha256)?
            {
                let is_head = tx
                    .query_opt(
                        "SELECT 1 FROM jobs_managed_cloud_heads
                          WHERE environment=$1 AND region=$2 AND channel=$3
                            AND current_transition_sha256=$4 FOR SHARE",
                        &[
                            &transition.scope.environment,
                            &transition.scope.region,
                            &transition.scope.channel,
                            &transition_sha256,
                        ],
                    )
                    .map_err(managed_cloud_storage)?
                    .is_some();
                if !is_head {
                    return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
                }
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_head_transition_result(
                    &transition,
                    &transition_sha256,
                    true,
                ));
            }
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            let policy =
                postgres_managed_cloud_policy_by_generation(&mut tx, rollback.trust_generation)?;
            verify_managed_cloud_signature_set(
                &rollback_bytes,
                &rollback_signature,
                &policy.policy,
                "incident",
                MANAGED_CLOUD_ROLLBACK_AUDIENCE,
                rollback.issued_at_ms,
                now_ms,
            )?;
            require_postgres_managed_cloud_signing_authority_not_revoked(
                &mut tx,
                &policy,
                &rollback_signature,
                now_ms,
            )?;
            let activation_stored = tx
                .query_opt(
                    "SELECT activation.canonical_activation_base64url,
                            signature_set.canonical_signature_set_base64url
                       FROM jobs_managed_cloud_activations activation
                       JOIN jobs_managed_cloud_signature_sets signature_set
                         ON signature_set.signature_set_sha256=
                            activation.authorization_signature_set_sha256
                      WHERE activation.activation_sha256=$1
                        AND activation.manifest_sha256=$2
                      FOR SHARE OF activation,signature_set",
                    &[&rollback.to_activation_sha256, &rollback.to_manifest_sha256],
                )
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            let (activation_bytes, activation, activation_signature) =
                stored_managed_cloud_activation(
                    &rollback.to_activation_sha256,
                    activation_stored.get(0),
                    activation_stored.get(1),
                )?;
            let activation_role = if activation.scope.channel == "general" {
                "general_promotion"
            } else {
                "promotion"
            };
            verify_managed_cloud_signature_set(
                &activation_bytes,
                &activation_signature,
                &policy.policy,
                activation_role,
                MANAGED_CLOUD_ACTIVATION_AUDIENCE,
                activation.issued_at_ms,
                now_ms,
            )?;
            require_postgres_managed_cloud_signing_authority_not_revoked(
                &mut tx,
                &policy,
                &activation_signature,
                now_ms,
            )?;
            let dependencies =
                postgres_managed_cloud_activation_dependencies(&mut tx, &activation)?;
            validate_managed_cloud_activation_dependencies(&activation, &dependencies)?;
            let head = tx
                .query_opt(
                    "SELECT head_revision,current_transition_sha256,
                            current_activation_sha256,current_manifest_sha256,
                            current_trust_generation,current_channel_sequence
                       FROM jobs_managed_cloud_heads
                      WHERE environment=$1 AND region=$2 AND channel=$3 FOR UPDATE",
                    &[
                        &rollback.scope.environment,
                        &rollback.scope.region,
                        &rollback.scope.channel,
                    ],
                )
                .map_err(managed_cloud_storage)?
                .map(|row| {
                    (
                        row.get::<_, i64>(0),
                        row.get::<_, String>(1),
                        row.get::<_, String>(2),
                        row.get::<_, String>(3),
                        row.get::<_, i64>(4),
                        row.get::<_, i64>(5),
                    )
                })
                .ok_or(ManagedCloudRegistryError::CompareAndSwapConflict)?;
            if head.0 != rollback.expected_head_revision
                || head.1 != rollback.expected_transition_sha256
                || head.2 != rollback.from_activation_sha256
                || head.3 != rollback.from_manifest_sha256
                || activation.expected_head_revision != head.0
                || activation.expected_transition_sha256.as_deref() != Some(&head.1)
                || activation.predecessor_activation_sha256.as_deref() != Some(&head.2)
                || !(activation.trust_generation > head.4
                    || (activation.trust_generation == head.4
                        && activation.channel_sequence > head.5))
                || now_ms < activation.not_before_ms
                || now_ms >= activation.expires_at_ms
                || now_ms < dependencies.cohort_not_before_ms
                || now_ms >= dependencies.cohort_expires_at_ms
                || tx
                    .query_opt(
                        "SELECT 1 FROM jobs_managed_cloud_revocations
                          WHERE effective_at_ms<=$1 AND (
                            (subject_kind='rollback' AND subject_sha256=$2)
                            OR (subject_kind='activation' AND subject_sha256=$3)
                            OR (subject_kind='manifest' AND subject_sha256=$4)
                            OR (subject_kind='release' AND subject_sha256=$4)
                            OR (subject_kind='cohort' AND subject_sha256=$5)
                          ) LIMIT 1",
                        &[
                            &now_ms,
                            &request.rollback_sha256,
                            &rollback.to_activation_sha256,
                            &rollback.to_manifest_sha256,
                            &activation.cohort_sha256,
                        ],
                    )
                    .map_err(managed_cloud_storage)?
                    .is_some()
            {
                return Err(ManagedCloudRegistryError::Unavailable);
            }
            let transition = ManagedCloudHeadTransitionAuthority {
                version: 1,
                audience: MANAGED_CLOUD_TRANSITION_AUDIENCE.to_string(),
                scope: rollback.scope.clone(),
                head_revision: head.0 + 1,
                previous_head_revision: head.0,
                previous_transition_sha256: Some(head.1),
                previous_activation_sha256: Some(head.2),
                previous_manifest_sha256: Some(head.3),
                previous_trust_generation: Some(head.4),
                previous_channel_sequence: Some(head.5),
                next_activation_sha256: rollback.to_activation_sha256.clone(),
                next_manifest_sha256: rollback.to_manifest_sha256.clone(),
                next_trust_generation: activation.trust_generation,
                next_channel_sequence: activation.channel_sequence,
                transition_kind: "rollback".to_string(),
                authority_sha256: request.rollback_sha256.clone(),
                rollback_authority_sha256: Some(request.rollback_sha256.clone()),
                recorded_at_ms: now_ms,
            };
            let transition_sha256 = managed_cloud_digest(&transition)?;
            insert_postgres_managed_cloud_head_transition(
                &mut tx,
                &transition,
                &transition_sha256,
                recorded_by,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_head_transition_result(
                &transition,
                &transition_sha256,
                false,
            ))
        }
    })
}

fn sqlite_managed_cloud_revocation_subject_exists(
    tx: &rusqlite::Transaction<'_>,
    revocation: &ManagedCloudRevocationAuthority,
) -> ManagedCloudResult<bool> {
    if revocation.subject_kind == "signing_key" {
        let public_keys = tx
            .prepare(
                "SELECT public_key_base64url FROM jobs_managed_cloud_trust_keys
                  WHERE key_id=?1",
            )
            .map_err(managed_cloud_storage)?
            .query_map(params![revocation.subject_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(managed_cloud_storage)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(managed_cloud_storage)?;
        return Ok(public_keys.into_iter().any(|key| {
            managed_cloud_decode_exact(&key, 32)
                .is_ok_and(|bytes| managed_cloud_sha256(&bytes) == revocation.subject_sha256)
        }));
    }
    let exists = match revocation.subject_kind.as_str() {
        "activation" => tx.query_row(
            "SELECT 1 FROM jobs_managed_cloud_activations
              WHERE activation_id=?1 AND activation_sha256=?2",
            params![revocation.subject_id, revocation.subject_sha256],
            |_| Ok(()),
        ),
        "cohort" => tx.query_row(
            "SELECT 1 FROM jobs_managed_cloud_cohorts
              WHERE cohort_id=?1 AND cohort_sha256=?2",
            params![revocation.subject_id, revocation.subject_sha256],
            |_| Ok(()),
        ),
        "component" => tx.query_row(
            "SELECT 1 FROM jobs_managed_cloud_manifest_components
              WHERE component_id=?1 AND artifact_sha256=?2 LIMIT 1",
            params![revocation.subject_id, revocation.subject_sha256],
            |_| Ok(()),
        ),
        "manifest" => tx.query_row(
            "SELECT 1 FROM jobs_managed_cloud_manifests
              WHERE manifest_id=?1 AND manifest_sha256=?2",
            params![revocation.subject_id, revocation.subject_sha256],
            |_| Ok(()),
        ),
        "release" => tx.query_row(
            "SELECT 1 FROM jobs_managed_cloud_manifests
              WHERE release_id=?1 AND manifest_sha256=?2 LIMIT 1",
            params![revocation.subject_id, revocation.subject_sha256],
            |_| Ok(()),
        ),
        "rollback" => tx.query_row(
            "SELECT 1 FROM jobs_managed_cloud_rollbacks
              WHERE rollback_id=?1 AND rollback_sha256=?2",
            params![revocation.subject_id, revocation.subject_sha256],
            |_| Ok(()),
        ),
        "runtime_grant" => tx.query_row(
            "SELECT 1 FROM jobs_managed_cloud_runtime_grants
              WHERE grant_id=?1 AND token_sha256=?2",
            params![revocation.subject_id, revocation.subject_sha256],
            |_| Ok(()),
        ),
        "runtime_instance" => tx.query_row(
            "SELECT 1 FROM jobs_managed_cloud_runtime_instances
              WHERE runtime_instance_id=?1 AND runtime_identity_sha256=?2",
            params![revocation.subject_id, revocation.subject_sha256],
            |_| Ok(()),
        ),
        "signing_key" => unreachable!("signing keys are handled above"),
        "trust_policy" => tx.query_row(
            "SELECT 1 FROM jobs_managed_cloud_trust_policies
              WHERE policy_id=?1 AND policy_sha256=?2",
            params![revocation.subject_id, revocation.subject_sha256],
            |_| Ok(()),
        ),
        _ => return Err(ManagedCloudRegistryError::InvalidAuthority),
    }
    .optional()
    .map_err(managed_cloud_storage)?
    .is_some();
    Ok(exists)
}

fn postgres_managed_cloud_revocation_subject_exists(
    tx: &mut postgres::Transaction<'_>,
    revocation: &ManagedCloudRevocationAuthority,
) -> ManagedCloudResult<bool> {
    if revocation.subject_kind == "signing_key" {
        let rows = tx
            .query(
                "SELECT public_key_base64url FROM jobs_managed_cloud_trust_keys
                  WHERE key_id=$1 FOR SHARE",
                &[&revocation.subject_id],
            )
            .map_err(managed_cloud_storage)?;
        return Ok(rows.into_iter().any(|row| {
            let key: String = row.get(0);
            managed_cloud_decode_exact(&key, 32)
                .is_ok_and(|bytes| managed_cloud_sha256(&bytes) == revocation.subject_sha256)
        }));
    }
    let (query, values): (&str, [&(dyn postgres::types::ToSql + Sync); 2]) =
        match revocation.subject_kind.as_str() {
            "activation" => (
                "SELECT 1 FROM jobs_managed_cloud_activations
                  WHERE activation_id=$1 AND activation_sha256=$2 FOR SHARE",
                [&revocation.subject_id, &revocation.subject_sha256],
            ),
            "cohort" => (
                "SELECT 1 FROM jobs_managed_cloud_cohorts
                  WHERE cohort_id=$1 AND cohort_sha256=$2 FOR SHARE",
                [&revocation.subject_id, &revocation.subject_sha256],
            ),
            "component" => (
                "SELECT 1 FROM jobs_managed_cloud_manifest_components
                  WHERE component_id=$1 AND artifact_sha256=$2 LIMIT 1 FOR SHARE",
                [&revocation.subject_id, &revocation.subject_sha256],
            ),
            "manifest" => (
                "SELECT 1 FROM jobs_managed_cloud_manifests
                  WHERE manifest_id=$1 AND manifest_sha256=$2 FOR SHARE",
                [&revocation.subject_id, &revocation.subject_sha256],
            ),
            "release" => (
                "SELECT 1 FROM jobs_managed_cloud_manifests
                  WHERE release_id=$1 AND manifest_sha256=$2 LIMIT 1 FOR SHARE",
                [&revocation.subject_id, &revocation.subject_sha256],
            ),
            "rollback" => (
                "SELECT 1 FROM jobs_managed_cloud_rollbacks
                  WHERE rollback_id=$1 AND rollback_sha256=$2 FOR SHARE",
                [&revocation.subject_id, &revocation.subject_sha256],
            ),
            "runtime_grant" => (
                "SELECT 1 FROM jobs_managed_cloud_runtime_grants
                  WHERE grant_id=$1 AND token_sha256=$2 FOR SHARE",
                [&revocation.subject_id, &revocation.subject_sha256],
            ),
            "runtime_instance" => (
                "SELECT 1 FROM jobs_managed_cloud_runtime_instances
                  WHERE runtime_instance_id=$1 AND runtime_identity_sha256=$2 FOR SHARE",
                [&revocation.subject_id, &revocation.subject_sha256],
            ),
            "trust_policy" => (
                "SELECT 1 FROM jobs_managed_cloud_trust_policies
                  WHERE policy_id=$1 AND policy_sha256=$2 FOR SHARE",
                [&revocation.subject_id, &revocation.subject_sha256],
            ),
            _ => return Err(ManagedCloudRegistryError::InvalidAuthority),
        };
    tx.query_opt(query, &values)
        .map(|row| row.is_some())
        .map_err(managed_cloud_storage)
}

fn managed_cloud_revocation_import_result(
    revocation: &ManagedCloudRevocationAuthority,
    revocation_sha256: &str,
    signature_set_sha256: &str,
    policy_sha256: &str,
    replayed: bool,
) -> ManagedCloudImportResult {
    ManagedCloudImportResult {
        authority_kind: "revocation".to_string(),
        authority_id: revocation.revocation_id.clone(),
        authority_sha256: revocation_sha256.to_string(),
        signature_set_sha256: signature_set_sha256.to_string(),
        trust_policy_sha256: policy_sha256.to_string(),
        replayed,
    }
}

pub fn import_managed_cloud_revocation(
    pool: &DbPool,
    request: &ManagedCloudRevocationImportRequest,
    recorded_by: &str,
) -> ManagedCloudResult<ManagedCloudImportResult> {
    if !managed_cloud_actor(recorded_by) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let (revocation_bytes, signature_set_bytes) = managed_cloud_decode_envelope(&request.envelope)?;
    let revocation: ManagedCloudRevocationAuthority =
        managed_cloud_parse_canonical(&revocation_bytes)?;
    let signature_set: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&signature_set_bytes)?;
    validate_managed_cloud_revocation(&revocation)?;
    validate_managed_cloud_signature_set(&signature_set)?;
    let revocation_sha256 = managed_cloud_sha256(&revocation_bytes);
    let signature_set_sha256 = managed_cloud_sha256(&signature_set_bytes);

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            let existing = tx
                .query_row(
                    "SELECT revocation_sha256,canonical_revocation_base64url,
                            authorization_signature_set_sha256,trust_generation
                       FROM jobs_managed_cloud_revocations
                      WHERE revocation_sha256=?1 OR revocation_id=?2
                         OR (trust_generation=?3 AND revocation_generation=?4)
                      LIMIT 1",
                    params![
                        revocation_sha256,
                        revocation.revocation_id,
                        revocation.trust_generation,
                        revocation.revocation_generation,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                if existing.0 != revocation_sha256
                    || existing.1 != request.envelope.canonical_base64url
                    || existing.2 != signature_set_sha256
                    || existing.3 != revocation.trust_generation
                    || !require_sqlite_managed_cloud_signature_set_replay(
                        &tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let policy =
                    sqlite_managed_cloud_policy_by_generation(&tx, revocation.trust_generation)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_revocation_import_result(
                    &revocation,
                    &revocation_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy =
                sqlite_managed_cloud_policy_by_generation(&tx, revocation.trust_generation)?;
            verify_managed_cloud_signature_set(
                &revocation_bytes,
                &signature_set,
                &policy.policy,
                "incident",
                MANAGED_CLOUD_REVOCATION_AUDIENCE,
                revocation.issued_at_ms,
                now_ms,
            )?;
            require_sqlite_managed_cloud_signing_authority_not_revoked(
                &tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            let latest = tx
                .query_row(
                    "SELECT revocation_generation,revocation_sha256
                       FROM jobs_managed_cloud_revocations
                      WHERE trust_generation=?1
                      ORDER BY revocation_generation DESC LIMIT 1",
                    params![revocation.trust_generation],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            match latest {
                None if revocation.revocation_generation == 1
                    && revocation.predecessor_revocation_sha256.is_none() => {}
                Some((generation, sha256))
                    if revocation.revocation_generation == generation + 1
                        && revocation.predecessor_revocation_sha256.as_deref() == Some(&sha256) => {
                }
                _ => return Err(ManagedCloudRegistryError::SequenceRegression),
            }
            if !sqlite_managed_cloud_revocation_subject_exists(&tx, &revocation)? {
                return Err(ManagedCloudRegistryError::NotFound);
            }
            insert_sqlite_managed_cloud_signature_set(
                &tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_revocations(
                   revocation_sha256,revocation_id,revocation_generation,
                   predecessor_revocation_sha256,trust_generation,subject_kind,
                   subject_id,subject_sha256,reason_ref,
                   canonical_revocation_base64url,authorization_signature_set_sha256,
                   issued_at_ms,effective_at_ms,recorded_by,recorded_at_ms
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                params![
                    revocation_sha256,
                    revocation.revocation_id,
                    revocation.revocation_generation,
                    revocation.predecessor_revocation_sha256,
                    revocation.trust_generation,
                    revocation.subject_kind,
                    revocation.subject_id,
                    revocation.subject_sha256,
                    revocation.reason_ref,
                    request.envelope.canonical_base64url,
                    signature_set_sha256,
                    revocation.issued_at_ms,
                    revocation.effective_at_ms,
                    recorded_by,
                    now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_revocation_import_result(
                &revocation,
                &revocation_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                   hashtextextended('jobs-managed-cloud-release-registry',0))",
                &[],
            )
            .map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            let existing = tx
                .query_opt(
                    "SELECT revocation_sha256,canonical_revocation_base64url,
                            authorization_signature_set_sha256,trust_generation
                       FROM jobs_managed_cloud_revocations
                      WHERE revocation_sha256=$1 OR revocation_id=$2
                         OR (trust_generation=$3 AND revocation_generation=$4)
                      LIMIT 1 FOR UPDATE",
                    &[
                        &revocation_sha256,
                        &revocation.revocation_id,
                        &revocation.trust_generation,
                        &revocation.revocation_generation,
                    ],
                )
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                if existing.get::<_, String>(0) != revocation_sha256
                    || existing.get::<_, String>(1) != request.envelope.canonical_base64url
                    || existing.get::<_, String>(2) != signature_set_sha256
                    || existing.get::<_, i64>(3) != revocation.trust_generation
                    || !require_postgres_managed_cloud_signature_set_replay(
                        &mut tx,
                        &signature_set,
                        &signature_set_sha256,
                        &request.envelope.signature_set_base64url,
                    )?
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                let policy = postgres_managed_cloud_policy_by_generation(
                    &mut tx,
                    revocation.trust_generation,
                )?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_revocation_import_result(
                    &revocation,
                    &revocation_sha256,
                    &signature_set_sha256,
                    &policy.policy_sha256,
                    true,
                ));
            }
            let policy =
                postgres_managed_cloud_policy_by_generation(&mut tx, revocation.trust_generation)?;
            verify_managed_cloud_signature_set(
                &revocation_bytes,
                &signature_set,
                &policy.policy,
                "incident",
                MANAGED_CLOUD_REVOCATION_AUDIENCE,
                revocation.issued_at_ms,
                now_ms,
            )?;
            require_postgres_managed_cloud_signing_authority_not_revoked(
                &mut tx,
                &policy,
                &signature_set,
                now_ms,
            )?;
            let latest = tx
                .query_opt(
                    "SELECT revocation_generation,revocation_sha256
                       FROM jobs_managed_cloud_revocations
                      WHERE trust_generation=$1
                      ORDER BY revocation_generation DESC LIMIT 1 FOR UPDATE",
                    &[&revocation.trust_generation],
                )
                .map_err(managed_cloud_storage)?
                .map(|row| (row.get::<_, i64>(0), row.get::<_, String>(1)));
            match latest {
                None if revocation.revocation_generation == 1
                    && revocation.predecessor_revocation_sha256.is_none() => {}
                Some((generation, sha256))
                    if revocation.revocation_generation == generation + 1
                        && revocation.predecessor_revocation_sha256.as_deref() == Some(&sha256) => {
                }
                _ => return Err(ManagedCloudRegistryError::SequenceRegression),
            }
            if !postgres_managed_cloud_revocation_subject_exists(&mut tx, &revocation)? {
                return Err(ManagedCloudRegistryError::NotFound);
            }
            insert_postgres_managed_cloud_signature_set(
                &mut tx,
                &signature_set,
                &signature_set_sha256,
                &request.envelope.signature_set_base64url,
                recorded_by,
                now_ms,
            )?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_revocations(
                   revocation_sha256,revocation_id,revocation_generation,
                   predecessor_revocation_sha256,trust_generation,subject_kind,
                   subject_id,subject_sha256,reason_ref,
                   canonical_revocation_base64url,authorization_signature_set_sha256,
                   issued_at_ms,effective_at_ms,recorded_by,recorded_at_ms
                 ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
                &[
                    &revocation_sha256,
                    &revocation.revocation_id,
                    &revocation.revocation_generation,
                    &revocation.predecessor_revocation_sha256,
                    &revocation.trust_generation,
                    &revocation.subject_kind,
                    &revocation.subject_id,
                    &revocation.subject_sha256,
                    &revocation.reason_ref,
                    &request.envelope.canonical_base64url,
                    &signature_set_sha256,
                    &revocation.issued_at_ms,
                    &revocation.effective_at_ms,
                    &recorded_by,
                    &now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_revocation_import_result(
                &revocation,
                &revocation_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
    })
}

fn sqlite_managed_cloud_resolved_head(
    tx: &rusqlite::Transaction<'_>,
    scope: &ManagedCloudScope,
) -> ManagedCloudResult<Option<ManagedCloudResolvedHead>> {
    tx.query_row(
        "SELECT head.head_revision,head.current_transition_sha256,
                head.current_activation_sha256,head.current_manifest_sha256,
                activation.cohort_sha256,activation.trust_generation,
                activation.channel_sequence,manifest.release_id,
                manifest.release_sequence,activation.task_queue_sha256,
                activation.failure_converter_sha256,activation.expires_at_ms
           FROM jobs_managed_cloud_heads head
           JOIN jobs_managed_cloud_activations activation
             ON activation.activation_sha256=head.current_activation_sha256
            AND activation.manifest_sha256=head.current_manifest_sha256
            AND activation.environment=head.environment
            AND activation.region=head.region AND activation.channel=head.channel
           JOIN jobs_managed_cloud_manifests manifest
             ON manifest.manifest_sha256=activation.manifest_sha256
          WHERE head.environment=?1 AND head.region=?2 AND head.channel=?3",
        params![scope.environment, scope.region, scope.channel],
        |row| {
            Ok(ManagedCloudResolvedHead {
                scope: scope.clone(),
                head_revision: row.get(0)?,
                transition_sha256: row.get(1)?,
                activation_sha256: row.get(2)?,
                manifest_sha256: row.get(3)?,
                cohort_sha256: row.get(4)?,
                trust_generation: row.get(5)?,
                channel_sequence: row.get(6)?,
                release_id: row.get(7)?,
                release_sequence: row.get(8)?,
                task_queue_sha256: row.get(9)?,
                failure_converter_sha256: row.get(10)?,
                activation_expires_at_ms: row.get(11)?,
            })
        },
    )
    .optional()
    .map_err(managed_cloud_storage)
}

fn postgres_managed_cloud_resolved_head(
    tx: &mut postgres::Transaction<'_>,
    scope: &ManagedCloudScope,
) -> ManagedCloudResult<Option<ManagedCloudResolvedHead>> {
    let row = tx
        .query_opt(
            "SELECT head.head_revision,head.current_transition_sha256,
                head.current_activation_sha256,head.current_manifest_sha256,
                activation.cohort_sha256,activation.trust_generation,
                activation.channel_sequence,manifest.release_id,
                manifest.release_sequence,activation.task_queue_sha256,
                activation.failure_converter_sha256,activation.expires_at_ms
           FROM jobs_managed_cloud_heads head
           JOIN jobs_managed_cloud_activations activation
             ON activation.activation_sha256=head.current_activation_sha256
            AND activation.manifest_sha256=head.current_manifest_sha256
            AND activation.environment=head.environment
            AND activation.region=head.region AND activation.channel=head.channel
           JOIN jobs_managed_cloud_manifests manifest
             ON manifest.manifest_sha256=activation.manifest_sha256
          WHERE head.environment=$1 AND head.region=$2 AND head.channel=$3
          FOR SHARE OF head,activation,manifest",
            &[&scope.environment, &scope.region, &scope.channel],
        )
        .map_err(managed_cloud_storage)?;
    Ok(row.map(|row| ManagedCloudResolvedHead {
        scope: scope.clone(),
        head_revision: row.get(0),
        transition_sha256: row.get(1),
        activation_sha256: row.get(2),
        manifest_sha256: row.get(3),
        cohort_sha256: row.get(4),
        trust_generation: row.get(5),
        channel_sequence: row.get(6),
        release_id: row.get(7),
        release_sequence: row.get(8),
        task_queue_sha256: row.get(9),
        failure_converter_sha256: row.get(10),
        activation_expires_at_ms: row.get(11),
    }))
}

fn managed_cloud_status_without_head(scope: &ManagedCloudScope) -> ManagedCloudReleaseStatus {
    ManagedCloudReleaseStatus {
        scope: scope.clone(),
        authority_ready: false,
        customer_admission: false,
        unavailability_reason: Some("no_active_release".to_string()),
        head_revision: 0,
        transition_sha256: None,
        activation_sha256: None,
        manifest_sha256: None,
        cohort_sha256: None,
        trust_generation: None,
        channel_sequence: None,
        release_id: None,
        release_sequence: None,
        task_queue_sha256: None,
        failure_converter_sha256: None,
        expires_at_ms: None,
    }
}

fn managed_cloud_status_from_head(
    head: &ManagedCloudResolvedHead,
    authority_ready: bool,
    customer_admission: bool,
    reason: Option<String>,
) -> ManagedCloudReleaseStatus {
    ManagedCloudReleaseStatus {
        scope: head.scope.clone(),
        authority_ready,
        customer_admission,
        unavailability_reason: reason,
        head_revision: head.head_revision,
        transition_sha256: Some(head.transition_sha256.clone()),
        activation_sha256: Some(head.activation_sha256.clone()),
        manifest_sha256: Some(head.manifest_sha256.clone()),
        cohort_sha256: Some(head.cohort_sha256.clone()),
        trust_generation: Some(head.trust_generation),
        channel_sequence: Some(head.channel_sequence),
        release_id: Some(head.release_id.clone()),
        release_sequence: Some(head.release_sequence),
        task_queue_sha256: Some(head.task_queue_sha256.clone()),
        failure_converter_sha256: Some(head.failure_converter_sha256.clone()),
        expires_at_ms: Some(head.activation_expires_at_ms),
    }
}

fn sqlite_managed_cloud_required_runtimes(
    tx: &rusqlite::Transaction<'_>,
    head: &ManagedCloudResolvedHead,
) -> ManagedCloudResult<Vec<ManagedCloudRequiredRuntime>> {
    tx.prepare(
        "SELECT requirement.role,capability.component_id,
                requirement.dependency_evidence_sha256,
                requirement.heartbeat_ttl_ms
           FROM jobs_managed_cloud_activation_requirements requirement
           JOIN jobs_managed_cloud_manifest_capabilities capability
             ON capability.manifest_sha256=?2
            AND capability.capability=requirement.role
          WHERE requirement.activation_sha256=?1 ORDER BY requirement.role",
    )
    .map_err(managed_cloud_storage)?
    .query_map(
        params![head.activation_sha256, head.manifest_sha256],
        |row| {
            Ok(ManagedCloudRequiredRuntime {
                role: row.get(0)?,
                component_id: row.get(1)?,
                dependency_evidence_sha256: row.get(2)?,
                heartbeat_ttl_ms: row.get(3)?,
            })
        },
    )
    .map_err(managed_cloud_storage)?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(managed_cloud_storage)
}

fn postgres_managed_cloud_required_runtimes(
    tx: &mut postgres::Transaction<'_>,
    head: &ManagedCloudResolvedHead,
) -> ManagedCloudResult<Vec<ManagedCloudRequiredRuntime>> {
    tx.query(
        "SELECT requirement.role,capability.component_id,
                requirement.dependency_evidence_sha256,
                requirement.heartbeat_ttl_ms
           FROM jobs_managed_cloud_activation_requirements requirement
           JOIN jobs_managed_cloud_manifest_capabilities capability
             ON capability.manifest_sha256=$2
            AND capability.capability=requirement.role
          WHERE requirement.activation_sha256=$1 ORDER BY requirement.role
          FOR SHARE OF requirement,capability",
        &[&head.activation_sha256, &head.manifest_sha256],
    )
    .map_err(managed_cloud_storage)
    .map(|rows| {
        rows.into_iter()
            .map(|row| ManagedCloudRequiredRuntime {
                role: row.get(0),
                component_id: row.get(1),
                dependency_evidence_sha256: row.get(2),
                heartbeat_ttl_ms: row.get(3),
            })
            .collect()
    })
}

fn managed_cloud_resolver_input(
    head: &ManagedCloudResolvedHead,
    runtime: &ManagedCloudRequiredRuntime,
) -> NewManagedCloudRuntimeGrant {
    NewManagedCloudRuntimeGrant {
        issuance_ref: "readiness-resolution-000000000000".to_string(),
        scope: head.scope.clone(),
        activation_sha256: head.activation_sha256.clone(),
        manifest_sha256: head.manifest_sha256.clone(),
        component_id: runtime.component_id.clone(),
        role: runtime.role.clone(),
        expected_worker_id: "readiness-worker-00000000000000".to_string(),
        authorization_ref: "readiness-authority-00000000000".to_string(),
        created_by: "managed-cloud-readiness".to_string(),
        ttl_ms: MANAGED_CLOUD_MIN_GRANT_TTL_MS,
    }
}

fn managed_cloud_phase611_runtime_set_exact(runtimes: &[ManagedCloudRequiredRuntime]) -> bool {
    runtimes.len() == MANAGED_CLOUD_BASE_RUNTIME_ROLES.len()
        && runtimes
            .iter()
            .map(|runtime| runtime.role.as_str())
            .eq(MANAGED_CLOUD_BASE_RUNTIME_ROLES)
}

fn sqlite_managed_cloud_runtime_readiness(
    tx: &rusqlite::Transaction<'_>,
    head: &ManagedCloudResolvedHead,
    runtime: &ManagedCloudRequiredRuntime,
    now_ms: i64,
) -> ManagedCloudResult<(bool, bool)> {
    tx.query_row(
        "SELECT
           EXISTS(
             SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats heartbeat
              WHERE heartbeat.activation_sha256=?1 AND heartbeat.manifest_sha256=?2
                AND heartbeat.role=?3 AND heartbeat.observed_head_revision=?4
                AND heartbeat.observed_transition_sha256=?5
           ),
           EXISTS(
             SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats heartbeat
             JOIN jobs_managed_cloud_runtime_instances instance
               ON instance.runtime_instance_id=heartbeat.runtime_instance_id
              AND instance.instance_epoch=heartbeat.instance_epoch
             JOIN jobs_managed_cloud_runtime_grants grant
               ON grant.grant_id=instance.grant_id
              AND grant.activation_sha256=heartbeat.activation_sha256
              AND grant.manifest_sha256=heartbeat.manifest_sha256
              AND grant.component_id=heartbeat.component_id
              AND grant.role=heartbeat.role
              AND grant.expected_worker_id=heartbeat.worker_id
              AND grant.expected_dependency_evidence_sha256=
                  heartbeat.dependency_evidence_sha256
              WHERE heartbeat.activation_sha256=?1 AND heartbeat.manifest_sha256=?2
                AND heartbeat.role=?3 AND heartbeat.observed_head_revision=?4
                AND heartbeat.observed_transition_sha256=?5
                AND heartbeat.dependency_evidence_sha256=?6
                AND heartbeat.health_state='ready' AND heartbeat.reason_code IS NULL
                AND heartbeat.heartbeat_at_ms+?7>?8 AND grant.expires_at_ms>?8
                AND NOT EXISTS(
                  SELECT 1 FROM jobs_managed_cloud_runtime_grant_revocations direct
                   WHERE direct.grant_id=grant.grant_id
                )
                AND NOT EXISTS(
                  SELECT 1 FROM jobs_managed_cloud_revocations revoked
                   WHERE revoked.effective_at_ms<=?8 AND (
                     (revoked.subject_kind='runtime_grant'
                       AND revoked.subject_id=grant.grant_id
                       AND revoked.subject_sha256=grant.token_sha256)
                     OR (revoked.subject_kind='runtime_instance'
                       AND revoked.subject_id=instance.runtime_instance_id
                       AND revoked.subject_sha256=instance.runtime_identity_sha256)
                   )
                )
           )",
        params![
            head.activation_sha256,
            head.manifest_sha256,
            runtime.role,
            head.head_revision,
            head.transition_sha256,
            runtime.dependency_evidence_sha256,
            runtime.heartbeat_ttl_ms,
            now_ms,
        ],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(managed_cloud_storage)
}

fn postgres_managed_cloud_runtime_readiness(
    tx: &mut postgres::Transaction<'_>,
    head: &ManagedCloudResolvedHead,
    runtime: &ManagedCloudRequiredRuntime,
    now_ms: i64,
) -> ManagedCloudResult<(bool, bool)> {
    tx.query_one(
        "SELECT
           EXISTS(
             SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats heartbeat
              WHERE heartbeat.activation_sha256=$1 AND heartbeat.manifest_sha256=$2
                AND heartbeat.role=$3 AND heartbeat.observed_head_revision=$4
                AND heartbeat.observed_transition_sha256=$5
           ),
           EXISTS(
             SELECT 1 FROM jobs_managed_cloud_runtime_heartbeats heartbeat
             JOIN jobs_managed_cloud_runtime_instances instance
               ON instance.runtime_instance_id=heartbeat.runtime_instance_id
              AND instance.instance_epoch=heartbeat.instance_epoch
             JOIN jobs_managed_cloud_runtime_grants grant
               ON grant.grant_id=instance.grant_id
              AND grant.activation_sha256=heartbeat.activation_sha256
              AND grant.manifest_sha256=heartbeat.manifest_sha256
              AND grant.component_id=heartbeat.component_id
              AND grant.role=heartbeat.role
              AND grant.expected_worker_id=heartbeat.worker_id
              AND grant.expected_dependency_evidence_sha256=
                  heartbeat.dependency_evidence_sha256
              WHERE heartbeat.activation_sha256=$1 AND heartbeat.manifest_sha256=$2
                AND heartbeat.role=$3 AND heartbeat.observed_head_revision=$4
                AND heartbeat.observed_transition_sha256=$5
                AND heartbeat.dependency_evidence_sha256=$6
                AND heartbeat.health_state='ready' AND heartbeat.reason_code IS NULL
                AND heartbeat.heartbeat_at_ms+$7>$8 AND grant.expires_at_ms>$8
                AND NOT EXISTS(
                  SELECT 1 FROM jobs_managed_cloud_runtime_grant_revocations direct
                   WHERE direct.grant_id=grant.grant_id
                )
                AND NOT EXISTS(
                  SELECT 1 FROM jobs_managed_cloud_revocations revoked
                   WHERE revoked.effective_at_ms<=$8 AND (
                     (revoked.subject_kind='runtime_grant'
                       AND revoked.subject_id=grant.grant_id
                       AND revoked.subject_sha256=grant.token_sha256)
                     OR (revoked.subject_kind='runtime_instance'
                       AND revoked.subject_id=instance.runtime_instance_id
                       AND revoked.subject_sha256=instance.runtime_identity_sha256)
                   )
                )
           )",
        &[
            &head.activation_sha256,
            &head.manifest_sha256,
            &runtime.role,
            &head.head_revision,
            &head.transition_sha256,
            &runtime.dependency_evidence_sha256,
            &runtime.heartbeat_ttl_ms,
            &now_ms,
        ],
    )
    .map(|row| (row.get(0), row.get(1)))
    .map_err(managed_cloud_storage)
}

fn managed_cloud_readiness_result(
    status: ManagedCloudReleaseStatus,
    evaluated_at_ms: i64,
    missing_roles: Vec<String>,
    stale_roles: Vec<String>,
    cohort_eligible: bool,
) -> ManagedCloudResult<ManagedCloudReadiness> {
    let readiness_sha256 = managed_cloud_digest(&ManagedCloudReadinessDigestAuthority {
        version: 1,
        audience: MANAGED_CLOUD_READINESS_AUDIENCE,
        status: &status,
        evaluated_at_ms,
        missing_roles: &missing_roles,
        stale_roles: &stale_roles,
        cohort_eligible,
    })?;
    Ok(ManagedCloudReadiness {
        status,
        readiness_sha256,
        evaluated_at_ms,
        missing_roles,
        stale_roles,
        cohort_eligible,
    })
}

fn resolve_sqlite_managed_cloud_readiness_tx(
    tx: &rusqlite::Transaction<'_>,
    query: &ManagedCloudReadinessQuery,
    now_ms: i64,
) -> ManagedCloudResult<ManagedCloudReadiness> {
    let Some(head) = sqlite_managed_cloud_resolved_head(tx, &query.scope)? else {
        return managed_cloud_readiness_result(
            managed_cloud_status_without_head(&query.scope),
            now_ms,
            Vec::new(),
            Vec::new(),
            false,
        );
    };
    let runtimes = sqlite_managed_cloud_required_runtimes(tx, &head)?;
    let mut authority_valid = managed_cloud_phase611_runtime_set_exact(&runtimes);
    if authority_valid {
        for runtime in &runtimes {
            match resolve_sqlite_managed_cloud_grant_authority(
                tx,
                &managed_cloud_resolver_input(&head, runtime),
                now_ms,
            ) {
                Ok(_) => {}
                Err(error @ ManagedCloudRegistryError::Storage(_)) => return Err(error),
                Err(_) => {
                    authority_valid = false;
                    break;
                }
            }
        }
    }
    let mut missing_roles = Vec::new();
    let mut stale_roles = Vec::new();
    if authority_valid {
        for runtime in &runtimes {
            let (has_heartbeat, ready) =
                sqlite_managed_cloud_runtime_readiness(tx, &head, runtime, now_ms)?;
            if !ready {
                if has_heartbeat {
                    stale_roles.push(runtime.role.clone());
                } else {
                    missing_roles.push(runtime.role.clone());
                }
            }
        }
    } else {
        missing_roles.extend(runtimes.iter().map(|runtime| runtime.role.clone()));
    }
    let cohort_eligible = match (head.scope.channel.as_str(), query.account_id.as_deref()) {
        ("shadow", _) => false,
        ("general", None) => true,
        ("canary", None) => tx
            .query_row(
                "SELECT 1 FROM jobs_managed_cloud_cohort_members
                  WHERE cohort_sha256=?1 LIMIT 1",
                params![head.cohort_sha256],
                |_| Ok(()),
            )
            .optional()
            .map_err(managed_cloud_storage)?
            .is_some(),
        ("general", Some(account_id)) => tx
            .query_row(
                "SELECT 1 FROM accounts WHERE id=?1",
                params![account_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(managed_cloud_storage)?
            .is_some(),
        ("canary", Some(account_id)) => tx
            .query_row(
                "SELECT 1 FROM jobs_managed_cloud_cohort_members
                  WHERE cohort_sha256=?1 AND account_id=?2",
                params![head.cohort_sha256, account_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(managed_cloud_storage)?
            .is_some(),
        _ => false,
    };
    let authority_ready = authority_valid
        && missing_roles.is_empty()
        && stale_roles.is_empty()
        && now_ms < head.activation_expires_at_ms;
    let customer_admission = authority_ready && head.scope.channel != "shadow" && cohort_eligible;
    let reason = if !authority_valid {
        Some("authority_invalid".to_string())
    } else if !authority_ready {
        Some("runtime_not_ready".to_string())
    } else if head.scope.channel == "shadow" {
        Some("shadow_only".to_string())
    } else if !cohort_eligible {
        Some("cohort_ineligible".to_string())
    } else {
        None
    };
    managed_cloud_readiness_result(
        managed_cloud_status_from_head(&head, authority_ready, customer_admission, reason),
        now_ms,
        missing_roles,
        stale_roles,
        cohort_eligible,
    )
}

fn resolve_postgres_managed_cloud_readiness_tx(
    tx: &mut postgres::Transaction<'_>,
    query: &ManagedCloudReadinessQuery,
    now_ms: i64,
) -> ManagedCloudResult<ManagedCloudReadiness> {
    let Some(head) = postgres_managed_cloud_resolved_head(tx, &query.scope)? else {
        return managed_cloud_readiness_result(
            managed_cloud_status_without_head(&query.scope),
            now_ms,
            Vec::new(),
            Vec::new(),
            false,
        );
    };
    let runtimes = postgres_managed_cloud_required_runtimes(tx, &head)?;
    let mut authority_valid = managed_cloud_phase611_runtime_set_exact(&runtimes);
    if authority_valid {
        for runtime in &runtimes {
            match resolve_postgres_managed_cloud_grant_authority(
                tx,
                &managed_cloud_resolver_input(&head, runtime),
                now_ms,
            ) {
                Ok(_) => {}
                Err(error @ ManagedCloudRegistryError::Storage(_)) => return Err(error),
                Err(_) => {
                    authority_valid = false;
                    break;
                }
            }
        }
    }
    let mut missing_roles = Vec::new();
    let mut stale_roles = Vec::new();
    if authority_valid {
        for runtime in &runtimes {
            let (has_heartbeat, ready) =
                postgres_managed_cloud_runtime_readiness(tx, &head, runtime, now_ms)?;
            if !ready {
                if has_heartbeat {
                    stale_roles.push(runtime.role.clone());
                } else {
                    missing_roles.push(runtime.role.clone());
                }
            }
        }
    } else {
        missing_roles.extend(runtimes.iter().map(|runtime| runtime.role.clone()));
    }
    let cohort_eligible = match (head.scope.channel.as_str(), query.account_id.as_deref()) {
        ("shadow", _) => false,
        ("general", None) => true,
        ("canary", None) => tx
            .query_opt(
                "SELECT 1 FROM jobs_managed_cloud_cohort_members
                  WHERE cohort_sha256=$1 LIMIT 1",
                &[&head.cohort_sha256],
            )
            .map_err(managed_cloud_storage)?
            .is_some(),
        ("general", Some(account_id)) => tx
            .query_opt("SELECT 1 FROM accounts WHERE id=$1", &[&account_id])
            .map_err(managed_cloud_storage)?
            .is_some(),
        ("canary", Some(account_id)) => tx
            .query_opt(
                "SELECT 1 FROM jobs_managed_cloud_cohort_members
                  WHERE cohort_sha256=$1 AND account_id=$2",
                &[&head.cohort_sha256, &account_id],
            )
            .map_err(managed_cloud_storage)?
            .is_some(),
        _ => false,
    };
    let authority_ready = authority_valid
        && missing_roles.is_empty()
        && stale_roles.is_empty()
        && now_ms < head.activation_expires_at_ms;
    let customer_admission = authority_ready && head.scope.channel != "shadow" && cohort_eligible;
    let reason = if !authority_valid {
        Some("authority_invalid".to_string())
    } else if !authority_ready {
        Some("runtime_not_ready".to_string())
    } else if head.scope.channel == "shadow" {
        Some("shadow_only".to_string())
    } else if !cohort_eligible {
        Some("cohort_ineligible".to_string())
    } else {
        None
    };
    managed_cloud_readiness_result(
        managed_cloud_status_from_head(&head, authority_ready, customer_admission, reason),
        now_ms,
        missing_roles,
        stale_roles,
        cohort_eligible,
    )
}

pub fn resolve_managed_cloud_readiness(
    pool: &DbPool,
    query: &ManagedCloudReadinessQuery,
) -> ManagedCloudResult<ManagedCloudReadiness> {
    validate_managed_cloud_scope(&query.scope)?;
    if query.account_id.as_ref().is_some_and(|account_id| {
        account_id.is_empty() || account_id.len() > 128 || account_id.trim() != account_id
    }) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection.transaction().map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            let Some(head) = sqlite_managed_cloud_resolved_head(&tx, &query.scope)? else {
                let status = managed_cloud_status_without_head(&query.scope);
                tx.commit().map_err(managed_cloud_storage)?;
                return managed_cloud_readiness_result(
                    status,
                    now_ms,
                    Vec::new(),
                    Vec::new(),
                    false,
                );
            };
            let runtimes = sqlite_managed_cloud_required_runtimes(&tx, &head)?;
            let mut authority_valid = managed_cloud_phase611_runtime_set_exact(&runtimes);
            if authority_valid {
                for runtime in &runtimes {
                    match resolve_sqlite_managed_cloud_grant_authority(
                        &tx,
                        &managed_cloud_resolver_input(&head, runtime),
                        now_ms,
                    ) {
                        Ok(_) => {}
                        Err(error @ ManagedCloudRegistryError::Storage(_)) => return Err(error),
                        Err(_) => {
                            authority_valid = false;
                            break;
                        }
                    }
                }
            }
            let mut missing_roles = Vec::new();
            let mut stale_roles = Vec::new();
            if authority_valid {
                for runtime in &runtimes {
                    let (has_heartbeat, ready) =
                        sqlite_managed_cloud_runtime_readiness(&tx, &head, runtime, now_ms)?;
                    if !ready {
                        if has_heartbeat {
                            stale_roles.push(runtime.role.clone());
                        } else {
                            missing_roles.push(runtime.role.clone());
                        }
                    }
                }
            } else {
                missing_roles.extend(runtimes.iter().map(|runtime| runtime.role.clone()));
            }
            let cohort_eligible = match (head.scope.channel.as_str(), query.account_id.as_deref()) {
                ("shadow", _) => false,
                ("general", None) => true,
                ("canary", None) => tx
                    .query_row(
                        "SELECT 1 FROM jobs_managed_cloud_cohort_members
                          WHERE cohort_sha256=?1 LIMIT 1",
                        params![head.cohort_sha256],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(managed_cloud_storage)?
                    .is_some(),
                ("general", Some(account_id)) => tx
                    .query_row(
                        "SELECT 1 FROM accounts WHERE id=?1",
                        params![account_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(managed_cloud_storage)?
                    .is_some(),
                ("canary", Some(account_id)) => tx
                    .query_row(
                        "SELECT 1 FROM jobs_managed_cloud_cohort_members
                          WHERE cohort_sha256=?1 AND account_id=?2",
                        params![head.cohort_sha256, account_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(managed_cloud_storage)?
                    .is_some(),
                _ => false,
            };
            let authority_ready = authority_valid
                && missing_roles.is_empty()
                && stale_roles.is_empty()
                && now_ms < head.activation_expires_at_ms;
            let customer_admission =
                authority_ready && head.scope.channel != "shadow" && cohort_eligible;
            let reason = if !authority_valid {
                Some("authority_invalid".to_string())
            } else if !authority_ready {
                Some("runtime_not_ready".to_string())
            } else if head.scope.channel == "shadow" {
                Some("shadow_only".to_string())
            } else if !cohort_eligible {
                Some("cohort_ineligible".to_string())
            } else {
                None
            };
            let status =
                managed_cloud_status_from_head(&head, authority_ready, customer_admission, reason);
            let result = managed_cloud_readiness_result(
                status,
                now_ms,
                missing_roles,
                stale_roles,
                cohort_eligible,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            let Some(head) = postgres_managed_cloud_resolved_head(&mut tx, &query.scope)? else {
                let status = managed_cloud_status_without_head(&query.scope);
                tx.commit().map_err(managed_cloud_storage)?;
                return managed_cloud_readiness_result(
                    status,
                    now_ms,
                    Vec::new(),
                    Vec::new(),
                    false,
                );
            };
            let runtimes = postgres_managed_cloud_required_runtimes(&mut tx, &head)?;
            let mut authority_valid = managed_cloud_phase611_runtime_set_exact(&runtimes);
            if authority_valid {
                for runtime in &runtimes {
                    match resolve_postgres_managed_cloud_grant_authority(
                        &mut tx,
                        &managed_cloud_resolver_input(&head, runtime),
                        now_ms,
                    ) {
                        Ok(_) => {}
                        Err(error @ ManagedCloudRegistryError::Storage(_)) => return Err(error),
                        Err(_) => {
                            authority_valid = false;
                            break;
                        }
                    }
                }
            }
            let mut missing_roles = Vec::new();
            let mut stale_roles = Vec::new();
            if authority_valid {
                for runtime in &runtimes {
                    let (has_heartbeat, ready) =
                        postgres_managed_cloud_runtime_readiness(&mut tx, &head, runtime, now_ms)?;
                    if !ready {
                        if has_heartbeat {
                            stale_roles.push(runtime.role.clone());
                        } else {
                            missing_roles.push(runtime.role.clone());
                        }
                    }
                }
            } else {
                missing_roles.extend(runtimes.iter().map(|runtime| runtime.role.clone()));
            }
            let cohort_eligible = match (head.scope.channel.as_str(), query.account_id.as_deref()) {
                ("shadow", _) => false,
                ("general", None) => true,
                ("canary", None) => tx
                    .query_opt(
                        "SELECT 1 FROM jobs_managed_cloud_cohort_members
                          WHERE cohort_sha256=$1 LIMIT 1",
                        &[&head.cohort_sha256],
                    )
                    .map_err(managed_cloud_storage)?
                    .is_some(),
                ("general", Some(account_id)) => tx
                    .query_opt("SELECT 1 FROM accounts WHERE id=$1", &[&account_id])
                    .map_err(managed_cloud_storage)?
                    .is_some(),
                ("canary", Some(account_id)) => tx
                    .query_opt(
                        "SELECT 1 FROM jobs_managed_cloud_cohort_members
                          WHERE cohort_sha256=$1 AND account_id=$2",
                        &[&head.cohort_sha256, &account_id],
                    )
                    .map_err(managed_cloud_storage)?
                    .is_some(),
                _ => false,
            };
            let authority_ready = authority_valid
                && missing_roles.is_empty()
                && stale_roles.is_empty()
                && now_ms < head.activation_expires_at_ms;
            let customer_admission =
                authority_ready && head.scope.channel != "shadow" && cohort_eligible;
            let reason = if !authority_valid {
                Some("authority_invalid".to_string())
            } else if !authority_ready {
                Some("runtime_not_ready".to_string())
            } else if head.scope.channel == "shadow" {
                Some("shadow_only".to_string())
            } else if !cohort_eligible {
                Some("cohort_ineligible".to_string())
            } else {
                None
            };
            let status =
                managed_cloud_status_from_head(&head, authority_ready, customer_admission, reason);
            let result = managed_cloud_readiness_result(
                status,
                now_ms,
                missing_roles,
                stale_roles,
                cohort_eligible,
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(result)
        }
    })
}

pub fn get_managed_cloud_release_status(
    pool: &DbPool,
    scope: &ManagedCloudScope,
) -> ManagedCloudResult<ManagedCloudReleaseStatus> {
    resolve_managed_cloud_readiness(
        pool,
        &ManagedCloudReadinessQuery {
            scope: scope.clone(),
            account_id: None,
        },
    )
    .map(|readiness| readiness.status)
}

fn managed_cloud_admission_from_readiness(
    readiness: &ManagedCloudReadiness,
) -> ManagedCloudResult<ManagedCloudAdmissionAuthority> {
    let status = &readiness.status;
    if !status.authority_ready
        || !status.customer_admission
        || !readiness.cohort_eligible
        || status.scope.channel == "shadow"
    {
        return Err(if readiness.cohort_eligible {
            ManagedCloudRegistryError::Unavailable
        } else {
            ManagedCloudRegistryError::CohortIneligible
        });
    }
    Ok(ManagedCloudAdmissionAuthority {
        scope: status.scope.clone(),
        head_revision: status.head_revision,
        transition_sha256: status
            .transition_sha256
            .clone()
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        activation_sha256: status
            .activation_sha256
            .clone()
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        manifest_sha256: status
            .manifest_sha256
            .clone()
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        cohort_sha256: status
            .cohort_sha256
            .clone()
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        trust_generation: status
            .trust_generation
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        channel_sequence: status
            .channel_sequence
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        release_id: status
            .release_id
            .clone()
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        release_sequence: status
            .release_sequence
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        task_queue_sha256: status
            .task_queue_sha256
            .clone()
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        failure_converter_sha256: status
            .failure_converter_sha256
            .clone()
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        readiness_sha256: readiness.readiness_sha256.clone(),
        activation_expires_at_ms: status
            .expires_at_ms
            .ok_or(ManagedCloudRegistryError::Unavailable)?,
        resolved_at_ms: readiness.evaluated_at_ms,
    })
}

pub fn resolve_managed_cloud_admission(
    pool: &DbPool,
    query: &ManagedCloudReadinessQuery,
) -> ManagedCloudResult<ManagedCloudAdmissionAuthority> {
    let readiness = resolve_managed_cloud_readiness(pool, query)?;
    managed_cloud_admission_from_readiness(&readiness)
}

fn managed_cloud_release_memo(
    binding_sha256: &str,
    admission: &ManagedCloudAdmissionAuthority,
) -> ManagedCloudResult<(ManagedCloudReleaseMemoAuthority, Vec<u8>, String, String)> {
    let memo = ManagedCloudReleaseMemoAuthority {
        version: 1,
        execution: ManagedCloudExecutionAuthority {
            binding_sha256: binding_sha256.to_string(),
            admission: admission.clone(),
        },
    };
    let mut bytes = managed_cloud_canonical_json(&memo)?;
    if bytes.pop() != Some(b'\n')
        || bytes.is_empty()
        || bytes.len() > MANAGED_CLOUD_MAX_ENVELOPE_BYTES
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let sha256 = managed_cloud_sha256(&bytes);
    let base64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes);
    Ok((memo, bytes, base64url, sha256))
}

pub fn parse_managed_cloud_release_memo_bytes(
    bytes: &[u8],
) -> ManagedCloudResult<ManagedCloudReleaseMemoAuthority> {
    if bytes.is_empty()
        || bytes.len() > MANAGED_CLOUD_MAX_ENVELOPE_BYTES
        || bytes.last() == Some(&b'\n')
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let mut canonical = bytes.to_vec();
    canonical.push(b'\n');
    let memo: ManagedCloudReleaseMemoAuthority = managed_cloud_parse_canonical(&canonical)?;
    let (expected, expected_bytes, _, _) =
        managed_cloud_release_memo(&memo.execution.binding_sha256, &memo.execution.admission)?;
    if expected != memo || expected_bytes != bytes {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    validate_managed_cloud_scope(&memo.execution.admission.scope)?;
    if memo.version != 1
        || !managed_cloud_hex64(&memo.execution.binding_sha256)
        || !managed_cloud_safe_integer(memo.execution.admission.head_revision, true)
        || !managed_cloud_hex64(&memo.execution.admission.transition_sha256)
        || !managed_cloud_hex64(&memo.execution.admission.activation_sha256)
        || !managed_cloud_hex64(&memo.execution.admission.manifest_sha256)
        || !managed_cloud_hex64(&memo.execution.admission.cohort_sha256)
        || !managed_cloud_safe_integer(memo.execution.admission.trust_generation, true)
        || !managed_cloud_safe_integer(memo.execution.admission.channel_sequence, true)
        || !managed_cloud_token(&memo.execution.admission.release_id, 128)
        || !managed_cloud_safe_integer(memo.execution.admission.release_sequence, true)
        || !managed_cloud_hex64(&memo.execution.admission.task_queue_sha256)
        || !managed_cloud_hex64(&memo.execution.admission.failure_converter_sha256)
        || !managed_cloud_hex64(&memo.execution.admission.readiness_sha256)
        || !managed_cloud_safe_integer(memo.execution.admission.activation_expires_at_ms, true)
        || !managed_cloud_safe_integer(memo.execution.admission.resolved_at_ms, false)
        || memo.execution.admission.resolved_at_ms
            >= memo.execution.admission.activation_expires_at_ms
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(memo)
}

pub fn managed_cloud_release_memo_sha256(
    memo: &ManagedCloudReleaseMemoAuthority,
) -> ManagedCloudResult<String> {
    let (expected, bytes, _, sha256) =
        managed_cloud_release_memo(&memo.execution.binding_sha256, &memo.execution.admission)?;
    if expected != *memo || parse_managed_cloud_release_memo_bytes(&bytes)? != *memo {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(sha256)
}

fn managed_cloud_workflow_binding_result(
    command_id: String,
    binding_sha256: String,
    release_memo_base64url: String,
    release_memo_sha256: String,
    admission: ManagedCloudAdmissionAuthority,
    replayed: bool,
) -> ManagedCloudWorkflowBinding {
    ManagedCloudWorkflowBinding {
        command_id,
        binding_sha256,
        release_memo_base64url,
        release_memo_sha256,
        admission,
        replayed,
    }
}

const MANAGED_CLOUD_WORKFLOW_BINDING_COLUMNS: &str =
    "command_id,binding_sha256,account_id_hmac_sha256,application_id_hmac_sha256,\
     run_id_hmac_sha256,workflow_id_hmac_sha256,environment,region,channel,\
     head_revision,transition_sha256,activation_sha256,manifest_sha256,cohort_sha256,\
     trust_generation,channel_sequence,release_id,release_sequence,task_queue_sha256,\
     failure_converter_sha256,readiness_sha256,resolved_at_ms,release_memo_base64url,\
     release_memo_sha256,activation_expires_at_ms,bound_at_ms";

fn managed_cloud_workflow_binding_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ManagedCloudStoredWorkflowBinding> {
    let bound_at_ms = row.get(25)?;
    Ok(ManagedCloudStoredWorkflowBinding {
        command_id: row.get(0)?,
        binding_sha256: row.get(1)?,
        account_id_hmac_sha256: row.get(2)?,
        application_id_hmac_sha256: row.get(3)?,
        run_id_hmac_sha256: row.get(4)?,
        workflow_id_hmac_sha256: row.get(5)?,
        admission: ManagedCloudAdmissionAuthority {
            scope: ManagedCloudScope {
                environment: row.get(6)?,
                region: row.get(7)?,
                channel: row.get(8)?,
            },
            head_revision: row.get(9)?,
            transition_sha256: row.get(10)?,
            activation_sha256: row.get(11)?,
            manifest_sha256: row.get(12)?,
            cohort_sha256: row.get(13)?,
            trust_generation: row.get(14)?,
            channel_sequence: row.get(15)?,
            release_id: row.get(16)?,
            release_sequence: row.get(17)?,
            task_queue_sha256: row.get(18)?,
            failure_converter_sha256: row.get(19)?,
            readiness_sha256: row.get(20)?,
            activation_expires_at_ms: row.get(24)?,
            resolved_at_ms: row.get(21)?,
        },
        release_memo_base64url: row.get(22)?,
        release_memo_sha256: row.get(23)?,
        bound_at_ms,
    })
}

fn managed_cloud_workflow_binding_from_postgres(
    row: &postgres::Row,
) -> ManagedCloudStoredWorkflowBinding {
    let bound_at_ms = row.get(25);
    ManagedCloudStoredWorkflowBinding {
        command_id: row.get(0),
        binding_sha256: row.get(1),
        account_id_hmac_sha256: row.get(2),
        application_id_hmac_sha256: row.get(3),
        run_id_hmac_sha256: row.get(4),
        workflow_id_hmac_sha256: row.get(5),
        admission: ManagedCloudAdmissionAuthority {
            scope: ManagedCloudScope {
                environment: row.get(6),
                region: row.get(7),
                channel: row.get(8),
            },
            head_revision: row.get(9),
            transition_sha256: row.get(10),
            activation_sha256: row.get(11),
            manifest_sha256: row.get(12),
            cohort_sha256: row.get(13),
            trust_generation: row.get(14),
            channel_sequence: row.get(15),
            release_id: row.get(16),
            release_sequence: row.get(17),
            task_queue_sha256: row.get(18),
            failure_converter_sha256: row.get(19),
            readiness_sha256: row.get(20),
            activation_expires_at_ms: row.get(24),
            resolved_at_ms: row.get(21),
        },
        release_memo_base64url: row.get(22),
        release_memo_sha256: row.get(23),
        bound_at_ms,
    }
}

fn sqlite_managed_cloud_workflow_binding(
    tx: &rusqlite::Transaction<'_>,
    command_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudStoredWorkflowBinding>> {
    tx.query_row(
        &format!(
            "SELECT {MANAGED_CLOUD_WORKFLOW_BINDING_COLUMNS}
               FROM jobs_managed_cloud_workflow_bindings WHERE command_id=?1"
        ),
        params![command_id],
        managed_cloud_workflow_binding_from_sqlite,
    )
    .optional()
    .map_err(managed_cloud_storage)
}

fn postgres_managed_cloud_workflow_binding(
    tx: &mut postgres::Transaction<'_>,
    command_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudStoredWorkflowBinding>> {
    tx.query_opt(
        &format!(
            "SELECT {MANAGED_CLOUD_WORKFLOW_BINDING_COLUMNS}
               FROM jobs_managed_cloud_workflow_bindings WHERE command_id=$1"
        ),
        &[&command_id],
    )
    .map(|row| {
        row.as_ref()
            .map(managed_cloud_workflow_binding_from_postgres)
    })
    .map_err(managed_cloud_storage)
}

fn require_exact_managed_cloud_workflow_binding(
    stored: &ManagedCloudStoredWorkflowBinding,
    input: &ManagedCloudWorkflowBindingInput,
) -> ManagedCloudResult<ManagedCloudWorkflowBinding> {
    let account_id_hmac_sha256 = managed_cloud_subject_hmac("account", &input.account_id)?;
    let application_id_hmac_sha256 =
        managed_cloud_subject_hmac("application", &input.application_id)?;
    let run_id_hmac_sha256 = managed_cloud_subject_hmac("run", &input.run_id)?;
    let workflow_id_hmac_sha256 = managed_cloud_subject_hmac("workflow", &input.workflow_id)?;
    if stored.command_id != input.command_id
        || stored.admission.scope != input.scope
        || require_managed_cloud_secret_match(
            &stored.account_id_hmac_sha256,
            &account_id_hmac_sha256,
        )
        .is_err()
        || require_managed_cloud_secret_match(
            &stored.application_id_hmac_sha256,
            &application_id_hmac_sha256,
        )
        .is_err()
        || require_managed_cloud_secret_match(&stored.run_id_hmac_sha256, &run_id_hmac_sha256)
            .is_err()
        || require_managed_cloud_secret_match(
            &stored.workflow_id_hmac_sha256,
            &workflow_id_hmac_sha256,
        )
        .is_err()
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let expected_binding_sha256 = managed_cloud_digest(&ManagedCloudWorkflowBindingAuthority {
        version: 1,
        audience: MANAGED_CLOUD_BINDING_AUDIENCE,
        command_id: &stored.command_id,
        account_id_hmac_sha256: &stored.account_id_hmac_sha256,
        application_id_hmac_sha256: &stored.application_id_hmac_sha256,
        run_id_hmac_sha256: &stored.run_id_hmac_sha256,
        workflow_id_hmac_sha256: &stored.workflow_id_hmac_sha256,
        admission: &stored.admission,
        bound_at_ms: stored.bound_at_ms,
    })?;
    let (_, expected_memo_bytes, expected_memo_base64url, expected_memo_sha256) =
        managed_cloud_release_memo(&stored.binding_sha256, &stored.admission)?;
    let stored_memo_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&stored.release_memo_base64url)
        .map_err(|_| ManagedCloudRegistryError::IdentityConflict)?;
    if expected_binding_sha256 != stored.binding_sha256
        || expected_memo_bytes != stored_memo_bytes
        || expected_memo_base64url != stored.release_memo_base64url
        || expected_memo_sha256 != stored.release_memo_sha256
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(managed_cloud_workflow_binding_result(
        stored.command_id.clone(),
        stored.binding_sha256.clone(),
        stored.release_memo_base64url.clone(),
        stored.release_memo_sha256.clone(),
        stored.admission.clone(),
        true,
    ))
}

fn new_managed_cloud_workflow_binding(
    input: &ManagedCloudWorkflowBindingInput,
    admission: ManagedCloudAdmissionAuthority,
    bound_at_ms: i64,
) -> ManagedCloudResult<ManagedCloudStoredWorkflowBinding> {
    if admission.resolved_at_ms > bound_at_ms {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let account_id_hmac_sha256 = managed_cloud_subject_hmac("account", &input.account_id)?;
    let application_id_hmac_sha256 =
        managed_cloud_subject_hmac("application", &input.application_id)?;
    let run_id_hmac_sha256 = managed_cloud_subject_hmac("run", &input.run_id)?;
    let workflow_id_hmac_sha256 = managed_cloud_subject_hmac("workflow", &input.workflow_id)?;
    let binding_sha256 = managed_cloud_digest(&ManagedCloudWorkflowBindingAuthority {
        version: 1,
        audience: MANAGED_CLOUD_BINDING_AUDIENCE,
        command_id: &input.command_id,
        account_id_hmac_sha256: &account_id_hmac_sha256,
        application_id_hmac_sha256: &application_id_hmac_sha256,
        run_id_hmac_sha256: &run_id_hmac_sha256,
        workflow_id_hmac_sha256: &workflow_id_hmac_sha256,
        admission: &admission,
        bound_at_ms,
    })?;
    let (_, _, release_memo_base64url, release_memo_sha256) =
        managed_cloud_release_memo(&binding_sha256, &admission)?;
    Ok(ManagedCloudStoredWorkflowBinding {
        command_id: input.command_id.clone(),
        binding_sha256,
        account_id_hmac_sha256,
        application_id_hmac_sha256,
        run_id_hmac_sha256,
        workflow_id_hmac_sha256,
        admission,
        release_memo_base64url,
        release_memo_sha256,
        bound_at_ms,
    })
}

fn insert_sqlite_managed_cloud_workflow_binding(
    tx: &rusqlite::Transaction<'_>,
    binding: &ManagedCloudStoredWorkflowBinding,
) -> ManagedCloudResult<()> {
    let admission = &binding.admission;
    tx.execute(
        "INSERT INTO jobs_managed_cloud_workflow_bindings(
           command_id,binding_sha256,account_id_hmac_sha256,application_id_hmac_sha256,
           run_id_hmac_sha256,workflow_id_hmac_sha256,environment,region,channel,
           head_revision,transition_sha256,activation_sha256,manifest_sha256,cohort_sha256,
           trust_generation,channel_sequence,release_id,release_sequence,task_queue_sha256,
           failure_converter_sha256,readiness_sha256,resolved_at_ms,release_memo_base64url,
           release_memo_sha256,activation_expires_at_ms,bound_at_ms
         ) VALUES(
           ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,
           ?19,?20,?21,?22,?23,?24,?25,?26
         )",
        params![
            binding.command_id,
            binding.binding_sha256,
            binding.account_id_hmac_sha256,
            binding.application_id_hmac_sha256,
            binding.run_id_hmac_sha256,
            binding.workflow_id_hmac_sha256,
            admission.scope.environment,
            admission.scope.region,
            admission.scope.channel,
            admission.head_revision,
            admission.transition_sha256,
            admission.activation_sha256,
            admission.manifest_sha256,
            admission.cohort_sha256,
            admission.trust_generation,
            admission.channel_sequence,
            admission.release_id,
            admission.release_sequence,
            admission.task_queue_sha256,
            admission.failure_converter_sha256,
            admission.readiness_sha256,
            admission.resolved_at_ms,
            binding.release_memo_base64url,
            binding.release_memo_sha256,
            admission.activation_expires_at_ms,
            binding.bound_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    Ok(())
}

fn insert_postgres_managed_cloud_workflow_binding(
    tx: &mut postgres::Transaction<'_>,
    binding: &ManagedCloudStoredWorkflowBinding,
) -> ManagedCloudResult<()> {
    let admission = &binding.admission;
    tx.execute(
        "INSERT INTO jobs_managed_cloud_workflow_bindings(
           command_id,binding_sha256,account_id_hmac_sha256,application_id_hmac_sha256,
           run_id_hmac_sha256,workflow_id_hmac_sha256,environment,region,channel,
           head_revision,transition_sha256,activation_sha256,manifest_sha256,cohort_sha256,
           trust_generation,channel_sequence,release_id,release_sequence,task_queue_sha256,
           failure_converter_sha256,readiness_sha256,resolved_at_ms,release_memo_base64url,
           release_memo_sha256,activation_expires_at_ms,bound_at_ms
         ) VALUES(
           $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,
           $19,$20,$21,$22,$23,$24,$25,$26
         )",
        &[
            &binding.command_id,
            &binding.binding_sha256,
            &binding.account_id_hmac_sha256,
            &binding.application_id_hmac_sha256,
            &binding.run_id_hmac_sha256,
            &binding.workflow_id_hmac_sha256,
            &admission.scope.environment,
            &admission.scope.region,
            &admission.scope.channel,
            &admission.head_revision,
            &admission.transition_sha256,
            &admission.activation_sha256,
            &admission.manifest_sha256,
            &admission.cohort_sha256,
            &admission.trust_generation,
            &admission.channel_sequence,
            &admission.release_id,
            &admission.release_sequence,
            &admission.task_queue_sha256,
            &admission.failure_converter_sha256,
            &admission.readiness_sha256,
            &admission.resolved_at_ms,
            &binding.release_memo_base64url,
            &binding.release_memo_sha256,
            &admission.activation_expires_at_ms,
            &binding.bound_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    Ok(())
}

fn validate_managed_cloud_workflow_binding_input(
    input: &ManagedCloudWorkflowBindingInput,
) -> ManagedCloudResult<()> {
    validate_managed_cloud_scope(&input.scope)?;
    if !managed_cloud_route_id(&input.command_id)
        || input.account_id.is_empty()
        || input.account_id.len() > 128
        || input.application_id.is_empty()
        || input.application_id.len() > 128
        || !managed_cloud_route_id(&input.run_id)
        || !(20..=192).contains(&input.workflow_id.len())
        || !input
            .workflow_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || input.scope.channel == "shadow"
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    Ok(())
}

fn managed_cloud_binding_input_from_lease(
    lease: &JobsWorkflowCommandLease,
    scope: &ManagedCloudScope,
) -> ManagedCloudWorkflowBindingInput {
    ManagedCloudWorkflowBindingInput {
        command_id: lease.command.id.clone(),
        account_id: lease.command.account_id.clone(),
        application_id: lease.command.application_id.clone(),
        run_id: lease.command.run_id.clone(),
        workflow_id: lease.command.workflow_id.clone(),
        scope: scope.clone(),
    }
}

pub(crate) fn lock_managed_cloud_release_registry_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    scope: &ManagedCloudScope,
) -> ManagedCloudResult<()> {
    validate_managed_cloud_scope(scope)?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
           hashtextextended('jobs-managed-cloud-release-registry',0))",
        &[],
    )
    .map_err(managed_cloud_storage)?;
    let lock_key = format!(
        "managed-cloud:{}:{}:{}",
        scope.environment, scope.region, scope.channel
    );
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        &[&lock_key],
    )
    .map_err(managed_cloud_storage)?;
    Ok(())
}

pub(crate) fn lock_managed_cloud_workflow_admission_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    scope: &ManagedCloudScope,
) -> ManagedCloudResult<()> {
    lock_operational_hold_shared_postgres_tx(tx).map_err(map_managed_cloud_operational_error)?;
    lock_managed_cloud_release_registry_postgres_tx(tx, scope)?;
    lock_postgres_ats_certification(tx).map_err(managed_cloud_storage)?;
    tx.query_opt(
        "SELECT singleton_id FROM jobs_runner_volume_fleet_state
          WHERE singleton_id=1 FOR SHARE",
        &[],
    )
    .map_err(managed_cloud_storage)?;
    Ok(())
}

fn sqlite_managed_cloud_command_marker(
    tx: &rusqlite::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
) -> ManagedCloudResult<(bool, String)> {
    let row = tx
        .query_row(
            "SELECT account_id,application_id,run_id,workflow_id,
                    managed_cloud_authority_required,command_kind
               FROM jobs_workflow_commands WHERE id=?1",
            params![input.command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)? != 0,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    if row.0 != input.account_id
        || row.1 != input.application_id
        || row.2 != input.run_id
        || row.3 != input.workflow_id
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok((row.4, row.5))
}

fn postgres_managed_cloud_command_marker(
    tx: &mut postgres::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
) -> ManagedCloudResult<(bool, String)> {
    let row = tx
        .query_opt(
            "SELECT account_id,application_id,run_id,workflow_id,
                    managed_cloud_authority_required,command_kind
               FROM jobs_workflow_commands WHERE id=$1",
            &[&input.command_id],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    if row.get::<_, String>(0) != input.account_id
        || row.get::<_, String>(1) != input.application_id
        || row.get::<_, String>(2) != input.run_id
        || row.get::<_, String>(3) != input.workflow_id
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok((row.get(4), row.get(5)))
}

fn sqlite_managed_cloud_command_requires_authority(
    tx: &rusqlite::Transaction<'_>,
    command_id: &str,
) -> ManagedCloudResult<bool> {
    tx.query_row(
        "SELECT managed_cloud_authority_required
           FROM jobs_workflow_commands WHERE id=?1",
        params![command_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(managed_cloud_storage)?
    .map(|value| value != 0)
    .ok_or(ManagedCloudRegistryError::NotFound)
}

fn postgres_managed_cloud_command_requires_authority(
    tx: &mut postgres::Transaction<'_>,
    command_id: &str,
) -> ManagedCloudResult<bool> {
    tx.query_opt(
        "SELECT managed_cloud_authority_required
           FROM jobs_workflow_commands WHERE id=$1",
        &[&command_id],
    )
    .map_err(managed_cloud_storage)?
    .map(|row| row.get(0))
    .ok_or(ManagedCloudRegistryError::NotFound)
}

pub(crate) fn managed_cloud_request_start_preflight_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    command_id: &str,
) -> ManagedCloudResult<ManagedCloudRequestStartPreflight> {
    let row = tx
        .query_opt(
            "SELECT managed_cloud_authority_required,command_kind
               FROM jobs_workflow_commands WHERE id=$1",
            &[&command_id],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let authority_required = row.get::<_, bool>(0);
    let command_kind = row.get::<_, String>(1);
    let has_binding = postgres_managed_cloud_workflow_binding(tx, command_id)?.is_some();
    let has_stored_authority =
        postgres_managed_cloud_stored_request_start_authority(tx, command_id)?.is_some();
    if !authority_required {
        if has_binding || has_stored_authority {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        return Ok(ManagedCloudRequestStartPreflight::Historical);
    }
    if !matches!(command_kind.as_str(), "start" | "resume") || !has_binding {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(if has_stored_authority {
        ManagedCloudRequestStartPreflight::Reconcile
    } else {
        ManagedCloudRequestStartPreflight::FreshEffect
    })
}

fn map_managed_cloud_operational_error(error: OperationalHoldError) -> ManagedCloudRegistryError {
    match error {
        OperationalHoldError::Storage(error) => ManagedCloudRegistryError::Storage(error),
        _ => ManagedCloudRegistryError::Unavailable,
    }
}

fn require_sqlite_managed_cloud_effect_admission(
    tx: &rusqlite::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
    now_ms: i64,
) -> ManagedCloudResult<ManagedCloudAdmissionAuthority> {
    if crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission().as_ref()
        != Some(&input.scope)
        || !cloud_distribution_ready_sqlite_tx(tx).map_err(managed_cloud_storage)?
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(tx, &input.account_id)
        .map_err(managed_cloud_storage)?;
    require_no_workflow_cleanup_sqlite_tx(tx, &input.account_id).map_err(managed_cloud_storage)?;
    let entitled = tx
        .query_row(
            "SELECT cloud_browser FROM jobs_entitlements WHERE account_id=?1",
            params![input.account_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .is_some_and(|enabled| enabled != 0);
    if !entitled {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    let (application, _) =
        load_stage_application_sqlite_tx(tx, &input.account_id, &input.application_id)
            .map_err(managed_cloud_storage)?;
    if !current_execution_authorized_sqlite(
        tx,
        &input.account_id,
        &application,
        ExecutionAuthorityRunner::Cloud,
    )
    .map_err(managed_cloud_storage)?
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    let hold_context = operational_hold_context_for_application_sqlite_tx(
        tx,
        &input.account_id,
        &input.application_id,
        Some("cloud"),
        None,
        None,
    )
    .map_err(map_managed_cloud_operational_error)?;
    require_operational_capability_sqlite_tx(
        tx,
        OperationalCapability::ApplicationQueue,
        &hold_context,
    )
    .map_err(map_managed_cloud_operational_error)?;
    let readiness = resolve_sqlite_managed_cloud_readiness_tx(
        tx,
        &ManagedCloudReadinessQuery {
            scope: input.scope.clone(),
            account_id: Some(input.account_id.clone()),
        },
        now_ms,
    )?;
    managed_cloud_admission_from_readiness(&readiness)
}

fn require_postgres_managed_cloud_effect_admission(
    tx: &mut postgres::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
    now_ms: i64,
) -> ManagedCloudResult<ManagedCloudAdmissionAuthority> {
    lock_managed_cloud_workflow_admission_postgres_tx(tx, &input.scope)?;
    require_postgres_managed_cloud_effect_admission_after_prelock(tx, input, now_ms)
}

fn require_postgres_managed_cloud_effect_admission_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
    now_ms: i64,
) -> ManagedCloudResult<ManagedCloudAdmissionAuthority> {
    if crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission().as_ref()
        != Some(&input.scope)
        || !cloud_distribution_ready_postgres_tx(tx).map_err(managed_cloud_storage)?
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
        tx,
        &input.account_id,
    )
    .map_err(managed_cloud_storage)?;
    require_no_workflow_cleanup_postgres_tx(tx, &input.account_id)
        .map_err(managed_cloud_storage)?;
    let entitled = tx
        .query_opt(
            "SELECT cloud_browser FROM jobs_entitlements
              WHERE account_id=$1 FOR UPDATE",
            &[&input.account_id],
        )
        .map_err(managed_cloud_storage)?
        .is_some_and(|row| row.get::<_, bool>(0));
    if !entitled {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    let (application, _) =
        load_stage_application_postgres_tx(tx, &input.account_id, &input.application_id)
            .map_err(managed_cloud_storage)?;
    if !current_execution_authorized_postgres(
        tx,
        &input.account_id,
        &application,
        ExecutionAuthorityRunner::Cloud,
    )
    .map_err(managed_cloud_storage)?
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    let hold_context = operational_hold_context_for_application_postgres_tx(
        tx,
        &input.account_id,
        &input.application_id,
        Some("cloud"),
        None,
        None,
    )
    .map_err(map_managed_cloud_operational_error)?;
    require_operational_capability_postgres_tx(
        tx,
        OperationalCapability::ApplicationQueue,
        &hold_context,
    )
    .map_err(map_managed_cloud_operational_error)?;
    let readiness = resolve_postgres_managed_cloud_readiness_tx(
        tx,
        &ManagedCloudReadinessQuery {
            scope: input.scope.clone(),
            account_id: Some(input.account_id.clone()),
        },
        now_ms,
    )?;
    managed_cloud_admission_from_readiness(&readiness)
}

fn managed_cloud_current_is_frozen(
    current: &ManagedCloudAdmissionAuthority,
    frozen: &ManagedCloudAdmissionAuthority,
) -> bool {
    current.scope == frozen.scope
        && current.head_revision == frozen.head_revision
        && current.transition_sha256 == frozen.transition_sha256
        && current.activation_sha256 == frozen.activation_sha256
        && current.manifest_sha256 == frozen.manifest_sha256
        && current.activation_expires_at_ms == frozen.activation_expires_at_ms
        && current.task_queue_sha256 == frozen.task_queue_sha256
        && current.failure_converter_sha256 == frozen.failure_converter_sha256
}

fn sqlite_managed_cloud_recovery_accepts(
    tx: &rusqlite::Transaction<'_>,
    current: &ManagedCloudAdmissionAuthority,
    frozen: &ManagedCloudAdmissionAuthority,
    now_ms: i64,
) -> ManagedCloudResult<bool> {
    if managed_cloud_current_is_frozen(current, frozen) {
        return Ok(true);
    }
    tx.query_row(
        "SELECT 1
           FROM jobs_managed_cloud_activation_recovery_acceptances acceptance
           JOIN jobs_managed_cloud_activations activation
             ON activation.activation_sha256=acceptance.recovery_activation_sha256
            AND activation.manifest_sha256=acceptance.recovery_manifest_sha256
            AND activation.environment=?4 AND activation.region=?5
            AND activation.channel=?6 AND activation.trust_generation=?7
            AND activation.channel_sequence=?8
            AND activation.task_queue_sha256=?9
            AND activation.failure_converter_sha256=?10
           JOIN jobs_managed_cloud_manifests manifest
             ON manifest.manifest_sha256=activation.manifest_sha256
            AND manifest.trust_generation=activation.trust_generation
            AND manifest.release_id=?11 AND manifest.release_sequence=?12
           JOIN jobs_managed_cloud_cohorts cohort
             ON cohort.cohort_sha256=activation.cohort_sha256
            AND cohort.authorization_signature_set_sha256=
                activation.cohort_signature_set_sha256
            AND cohort.trust_generation=activation.trust_generation
            AND cohort.environment=activation.environment
            AND cohort.region=activation.region AND cohort.channel=activation.channel
           JOIN jobs_managed_cloud_trust_policies policy
             ON policy.trust_generation=activation.trust_generation
           JOIN jobs_managed_cloud_head_transitions transition
             ON transition.transition_sha256=?13
            AND transition.environment=activation.environment
            AND transition.region=activation.region AND transition.channel=activation.channel
            AND transition.head_revision=?14
            AND transition.next_activation_sha256=activation.activation_sha256
            AND transition.next_manifest_sha256=manifest.manifest_sha256
            AND transition.next_trust_generation=activation.trust_generation
            AND transition.next_channel_sequence=activation.channel_sequence
           LEFT JOIN jobs_managed_cloud_rollbacks rollback
             ON rollback.rollback_sha256=transition.rollback_authority_sha256
            AND rollback.trust_generation=activation.trust_generation
          WHERE acceptance.activation_sha256=?1
            AND acceptance.recovery_activation_sha256=?2
            AND acceptance.recovery_manifest_sha256=?3
            AND activation.cohort_sha256=?15
            AND (transition.transition_kind='activation'
                 OR rollback.rollback_sha256 IS NOT NULL)
            AND NOT EXISTS(
              SELECT 1 FROM jobs_managed_cloud_revocations revoked
               WHERE revoked.effective_at_ms<=?16 AND (
                 (revoked.subject_kind='activation'
                   AND revoked.subject_sha256=activation.activation_sha256)
                 OR (revoked.subject_kind='manifest'
                   AND revoked.subject_sha256=manifest.manifest_sha256)
                 OR (revoked.subject_kind='cohort'
                   AND revoked.subject_sha256=cohort.cohort_sha256)
                 OR (revoked.subject_kind='component' AND EXISTS(
                   SELECT 1 FROM jobs_managed_cloud_manifest_components component
                    WHERE component.manifest_sha256=manifest.manifest_sha256
                      AND component.component_id=revoked.subject_id
                      AND component.artifact_sha256=revoked.subject_sha256
                 ))
                 OR (revoked.subject_kind='release'
                   AND revoked.subject_id=manifest.release_id
                   AND revoked.subject_sha256=manifest.manifest_sha256)
                 OR (revoked.subject_kind='rollback'
                   AND rollback.rollback_sha256 IS NOT NULL
                   AND revoked.subject_id=rollback.rollback_id
                   AND revoked.subject_sha256=rollback.rollback_sha256)
                 OR (revoked.subject_kind='trust_policy'
                   AND revoked.subject_id=policy.policy_id
                   AND revoked.subject_sha256=policy.policy_sha256)
                 OR (revoked.subject_kind='signing_key' AND revoked.subject_id IN (
                   SELECT signature.key_id FROM jobs_managed_cloud_signatures signature
                    WHERE signature.signature_set_sha256 IN (
                      activation.authorization_signature_set_sha256,
                      manifest.authorization_signature_set_sha256,
                      cohort.authorization_signature_set_sha256,
                      policy.authorization_signature_set_sha256
                    ) OR signature.signature_set_sha256=
                         rollback.authorization_signature_set_sha256
                 ))
               )
            )",
        params![
            current.activation_sha256,
            frozen.activation_sha256,
            frozen.manifest_sha256,
            frozen.scope.environment,
            frozen.scope.region,
            frozen.scope.channel,
            frozen.trust_generation,
            frozen.channel_sequence,
            frozen.task_queue_sha256,
            frozen.failure_converter_sha256,
            frozen.release_id,
            frozen.release_sequence,
            frozen.transition_sha256,
            frozen.head_revision,
            frozen.cohort_sha256,
            now_ms,
        ],
        |_| Ok(()),
    )
    .optional()
    .map(|row| row.is_some())
    .map_err(managed_cloud_storage)
}

fn postgres_managed_cloud_recovery_accepts(
    tx: &mut postgres::Transaction<'_>,
    current: &ManagedCloudAdmissionAuthority,
    frozen: &ManagedCloudAdmissionAuthority,
    now_ms: i64,
) -> ManagedCloudResult<bool> {
    if managed_cloud_current_is_frozen(current, frozen) {
        return Ok(true);
    }
    tx.query_opt(
        "SELECT 1
           FROM jobs_managed_cloud_activation_recovery_acceptances acceptance
           JOIN jobs_managed_cloud_activations activation
             ON activation.activation_sha256=acceptance.recovery_activation_sha256
            AND activation.manifest_sha256=acceptance.recovery_manifest_sha256
            AND activation.environment=$4 AND activation.region=$5
            AND activation.channel=$6 AND activation.trust_generation=$7
            AND activation.channel_sequence=$8
            AND activation.task_queue_sha256=$9
            AND activation.failure_converter_sha256=$10
           JOIN jobs_managed_cloud_manifests manifest
             ON manifest.manifest_sha256=activation.manifest_sha256
            AND manifest.trust_generation=activation.trust_generation
            AND manifest.release_id=$11 AND manifest.release_sequence=$12
           JOIN jobs_managed_cloud_cohorts cohort
             ON cohort.cohort_sha256=activation.cohort_sha256
            AND cohort.authorization_signature_set_sha256=
                activation.cohort_signature_set_sha256
            AND cohort.trust_generation=activation.trust_generation
            AND cohort.environment=activation.environment
            AND cohort.region=activation.region AND cohort.channel=activation.channel
           JOIN jobs_managed_cloud_trust_policies policy
             ON policy.trust_generation=activation.trust_generation
           JOIN jobs_managed_cloud_head_transitions transition
             ON transition.transition_sha256=$13
            AND transition.environment=activation.environment
            AND transition.region=activation.region AND transition.channel=activation.channel
            AND transition.head_revision=$14
            AND transition.next_activation_sha256=activation.activation_sha256
            AND transition.next_manifest_sha256=manifest.manifest_sha256
            AND transition.next_trust_generation=activation.trust_generation
            AND transition.next_channel_sequence=activation.channel_sequence
           LEFT JOIN jobs_managed_cloud_rollbacks rollback
             ON rollback.rollback_sha256=transition.rollback_authority_sha256
            AND rollback.trust_generation=activation.trust_generation
          WHERE acceptance.activation_sha256=$1
            AND acceptance.recovery_activation_sha256=$2
            AND acceptance.recovery_manifest_sha256=$3
            AND activation.cohort_sha256=$15
            AND (transition.transition_kind='activation'
                 OR rollback.rollback_sha256 IS NOT NULL)
            AND NOT EXISTS(
              SELECT 1 FROM jobs_managed_cloud_revocations revoked
               WHERE revoked.effective_at_ms<=$16 AND (
                 (revoked.subject_kind='activation'
                   AND revoked.subject_sha256=activation.activation_sha256)
                 OR (revoked.subject_kind='manifest'
                   AND revoked.subject_sha256=manifest.manifest_sha256)
                 OR (revoked.subject_kind='cohort'
                   AND revoked.subject_sha256=cohort.cohort_sha256)
                 OR (revoked.subject_kind='component' AND EXISTS(
                   SELECT 1 FROM jobs_managed_cloud_manifest_components component
                    WHERE component.manifest_sha256=manifest.manifest_sha256
                      AND component.component_id=revoked.subject_id
                      AND component.artifact_sha256=revoked.subject_sha256
                 ))
                 OR (revoked.subject_kind='release'
                   AND revoked.subject_id=manifest.release_id
                   AND revoked.subject_sha256=manifest.manifest_sha256)
                 OR (revoked.subject_kind='rollback'
                   AND rollback.rollback_sha256 IS NOT NULL
                   AND revoked.subject_id=rollback.rollback_id
                   AND revoked.subject_sha256=rollback.rollback_sha256)
                 OR (revoked.subject_kind='trust_policy'
                   AND revoked.subject_id=policy.policy_id
                   AND revoked.subject_sha256=policy.policy_sha256)
                 OR (revoked.subject_kind='signing_key' AND revoked.subject_id IN (
                   SELECT signature.key_id FROM jobs_managed_cloud_signatures signature
                    WHERE signature.signature_set_sha256 IN (
                      activation.authorization_signature_set_sha256,
                      manifest.authorization_signature_set_sha256,
                      cohort.authorization_signature_set_sha256,
                      policy.authorization_signature_set_sha256
                    ) OR signature.signature_set_sha256=
                         rollback.authorization_signature_set_sha256
                 ))
               )
            )",
        &[
            &current.activation_sha256,
            &frozen.activation_sha256,
            &frozen.manifest_sha256,
            &frozen.scope.environment,
            &frozen.scope.region,
            &frozen.scope.channel,
            &frozen.trust_generation,
            &frozen.channel_sequence,
            &frozen.task_queue_sha256,
            &frozen.failure_converter_sha256,
            &frozen.release_id,
            &frozen.release_sequence,
            &frozen.transition_sha256,
            &frozen.head_revision,
            &frozen.cohort_sha256,
            &now_ms,
        ],
    )
    .map(|row| row.is_some())
    .map_err(managed_cloud_storage)
}

fn sqlite_managed_cloud_start_binding_for_resume(
    tx: &rusqlite::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
) -> ManagedCloudResult<ManagedCloudStoredWorkflowBinding> {
    let mut statement = tx
        .prepare(&format!(
            "SELECT {MANAGED_CLOUD_WORKFLOW_BINDING_COLUMNS}
               FROM jobs_managed_cloud_workflow_bindings binding
               JOIN jobs_workflow_commands command ON command.id=binding.command_id
              WHERE command.account_id=?1 AND command.application_id=?2
                AND command.run_id=?3 AND command.workflow_id=?4
                AND command.command_kind='start'
                AND command.managed_cloud_authority_required=1
                AND command.first_request_started_at_ms IS NOT NULL
                AND EXISTS(
                  SELECT 1 FROM jobs_workflow_command_attempt_events event
                   WHERE event.command_id=command.id
                     AND event.event_kind='request_started'
                )
              ORDER BY command.created_at_ms,command.id LIMIT 2"
        ))
        .map_err(managed_cloud_storage)?;
    let rows = statement
        .query_map(
            params![
                input.account_id,
                input.application_id,
                input.run_id,
                input.workflow_id,
            ],
            managed_cloud_workflow_binding_from_sqlite,
        )
        .map_err(managed_cloud_storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(managed_cloud_storage)?;
    if rows.len() != 1 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let stored = rows
        .into_iter()
        .next()
        .ok_or(ManagedCloudRegistryError::IdentityConflict)?;
    let start_input = ManagedCloudWorkflowBindingInput {
        command_id: stored.command_id.clone(),
        account_id: input.account_id.clone(),
        application_id: input.application_id.clone(),
        run_id: input.run_id.clone(),
        workflow_id: input.workflow_id.clone(),
        scope: input.scope.clone(),
    };
    require_exact_managed_cloud_workflow_binding(&stored, &start_input)?;
    Ok(stored)
}

fn postgres_managed_cloud_start_binding_for_resume(
    tx: &mut postgres::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
    lock_rows: bool,
) -> ManagedCloudResult<ManagedCloudStoredWorkflowBinding> {
    let lock_clause = if lock_rows {
        "FOR SHARE OF binding,command"
    } else {
        ""
    };
    let rows = tx
        .query(
            &format!(
                "SELECT {MANAGED_CLOUD_WORKFLOW_BINDING_COLUMNS}
                   FROM jobs_managed_cloud_workflow_bindings binding
                   JOIN jobs_workflow_commands command ON command.id=binding.command_id
                  WHERE command.account_id=$1 AND command.application_id=$2
                    AND command.run_id=$3 AND command.workflow_id=$4
                    AND command.command_kind='start'
                    AND command.managed_cloud_authority_required
                    AND command.first_request_started_at_ms IS NOT NULL
                    AND EXISTS(
                      SELECT 1 FROM jobs_workflow_command_attempt_events event
                       WHERE event.command_id=command.id
                         AND event.event_kind='request_started'
                    )
                  ORDER BY command.created_at_ms,command.id
                  {lock_clause} LIMIT 2"
            ),
            &[
                &input.account_id,
                &input.application_id,
                &input.run_id,
                &input.workflow_id,
            ],
        )
        .map_err(managed_cloud_storage)?;
    if rows.len() != 1 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let stored = managed_cloud_workflow_binding_from_postgres(&rows[0]);
    let start_input = ManagedCloudWorkflowBindingInput {
        command_id: stored.command_id.clone(),
        account_id: input.account_id.clone(),
        application_id: input.application_id.clone(),
        run_id: input.run_id.clone(),
        workflow_id: input.workflow_id.clone(),
        scope: input.scope.clone(),
    };
    require_exact_managed_cloud_workflow_binding(&stored, &start_input)?;
    Ok(stored)
}

pub(crate) fn bind_managed_cloud_workflow_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
) -> ManagedCloudResult<ManagedCloudWorkflowBinding> {
    validate_managed_cloud_workflow_binding_input(input)?;
    let (authority_required, command_kind) = sqlite_managed_cloud_command_marker(tx, input)?;
    if !authority_required || !matches!(command_kind.as_str(), "start" | "resume") {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    if let Some(existing) = sqlite_managed_cloud_workflow_binding(tx, &input.command_id)? {
        return require_exact_managed_cloud_workflow_binding(&existing, input);
    }
    let now_ms = managed_cloud_db_now_sqlite(tx)?;
    let current_admission = require_sqlite_managed_cloud_effect_admission(tx, input, now_ms)?;
    let frozen_admission = if command_kind == "resume" {
        let start = sqlite_managed_cloud_start_binding_for_resume(tx, input)?;
        if !sqlite_managed_cloud_recovery_accepts(tx, &current_admission, &start.admission, now_ms)?
        {
            return Err(ManagedCloudRegistryError::RecoveryNotAccepted);
        }
        start.admission
    } else {
        current_admission
    };
    let binding = new_managed_cloud_workflow_binding(input, frozen_admission, now_ms)?;
    insert_sqlite_managed_cloud_workflow_binding(tx, &binding)?;
    let stored = sqlite_managed_cloud_workflow_binding(tx, &input.command_id)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let mut result = require_exact_managed_cloud_workflow_binding(&stored, input)?;
    result.replayed = false;
    Ok(result)
}

pub(crate) fn bind_managed_cloud_workflow_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
) -> ManagedCloudResult<ManagedCloudWorkflowBinding> {
    validate_managed_cloud_workflow_binding_input(input)?;
    let (authority_required, command_kind) = postgres_managed_cloud_command_marker(tx, input)?;
    if !authority_required || !matches!(command_kind.as_str(), "start" | "resume") {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    if let Some(existing) = postgres_managed_cloud_workflow_binding(tx, &input.command_id)? {
        return require_exact_managed_cloud_workflow_binding(&existing, input);
    }
    let now_ms = managed_cloud_db_now_postgres(tx)?;
    let current_admission = require_postgres_managed_cloud_effect_admission(tx, input, now_ms)?;
    if let Some(existing) = postgres_managed_cloud_workflow_binding(tx, &input.command_id)? {
        return require_exact_managed_cloud_workflow_binding(&existing, input);
    }
    let frozen_admission = if command_kind == "resume" {
        let start = postgres_managed_cloud_start_binding_for_resume(tx, input, true)?;
        if !postgres_managed_cloud_recovery_accepts(
            tx,
            &current_admission,
            &start.admission,
            now_ms,
        )? {
            return Err(ManagedCloudRegistryError::RecoveryNotAccepted);
        }
        start.admission
    } else {
        current_admission
    };
    let binding = new_managed_cloud_workflow_binding(input, frozen_admission, now_ms)?;
    insert_postgres_managed_cloud_workflow_binding(tx, &binding)?;
    let stored = postgres_managed_cloud_workflow_binding(tx, &input.command_id)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let mut result = require_exact_managed_cloud_workflow_binding(&stored, input)?;
    result.replayed = false;
    Ok(result)
}

pub(crate) fn require_managed_cloud_workflow_binding_replay_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
) -> ManagedCloudResult<ManagedCloudWorkflowBinding> {
    validate_managed_cloud_workflow_binding_input(input)?;
    let (authority_required, command_kind) = sqlite_managed_cloud_command_marker(tx, input)?;
    if !authority_required || !matches!(command_kind.as_str(), "start" | "resume") {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let stored = sqlite_managed_cloud_workflow_binding(tx, &input.command_id)?
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    require_exact_managed_cloud_workflow_binding(&stored, input)
}

pub(crate) fn require_managed_cloud_workflow_binding_replay_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
) -> ManagedCloudResult<ManagedCloudWorkflowBinding> {
    validate_managed_cloud_workflow_binding_input(input)?;
    let (authority_required, command_kind) = postgres_managed_cloud_command_marker(tx, input)?;
    if !authority_required || !matches!(command_kind.as_str(), "start" | "resume") {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let stored = postgres_managed_cloud_workflow_binding(tx, &input.command_id)?
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    require_exact_managed_cloud_workflow_binding(&stored, input)
}

fn require_sqlite_managed_cloud_command_lease(
    tx: &rusqlite::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    let stored = tx
        .query_row(
            &format!(
                "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                  WHERE id=?1 AND account_id=?2"
            ),
            params![lease.command.id, lease.command.account_id],
            sqlite_workflow_command_row,
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    validate_workflow_command_lease(&stored, lease, JobsWorkflowCommandState::Claimed, now_ms)
        .map_err(|_| ManagedCloudRegistryError::Unavailable)?;
    if stored.application_id != lease.command.application_id
        || stored.run_id != lease.command.run_id
        || stored.workflow_id != lease.command.workflow_id
        || stored.command_kind != lease.command.command_kind.as_str()
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn require_postgres_managed_cloud_command_lease(
    tx: &mut postgres::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                  WHERE id=$1 AND account_id=$2 FOR SHARE"
            ),
            &[&lease.command.id, &lease.command.account_id],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let stored = postgres_workflow_command_row(&row);
    validate_workflow_command_lease(&stored, lease, JobsWorkflowCommandState::Claimed, now_ms)
        .map_err(|_| ManagedCloudRegistryError::Unavailable)?;
    if stored.application_id != lease.command.application_id
        || stored.run_id != lease.command.run_id
        || stored.workflow_id != lease.command.workflow_id
        || stored.command_kind != lease.command.command_kind.as_str()
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn require_sqlite_managed_cloud_command_lease_replay(
    tx: &rusqlite::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    let stored = tx
        .query_row(
            &format!(
                "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                  WHERE id=?1 AND account_id=?2"
            ),
            params![lease.command.id, lease.command.account_id],
            sqlite_workflow_command_row,
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let valid =
        validate_workflow_command_lease(&stored, lease, JobsWorkflowCommandState::Claimed, now_ms)
            .is_ok()
            || validate_workflow_command_lease(
                &stored,
                lease,
                JobsWorkflowCommandState::Delivering,
                now_ms,
            )
            .is_ok();
    if !valid {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    if stored.application_id != lease.command.application_id
        || stored.run_id != lease.command.run_id
        || stored.workflow_id != lease.command.workflow_id
        || stored.command_kind != lease.command.command_kind.as_str()
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn require_postgres_managed_cloud_command_lease_replay(
    tx: &mut postgres::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                  WHERE id=$1 AND account_id=$2 FOR SHARE"
            ),
            &[&lease.command.id, &lease.command.account_id],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let stored = postgres_workflow_command_row(&row);
    let valid =
        validate_workflow_command_lease(&stored, lease, JobsWorkflowCommandState::Claimed, now_ms)
            .is_ok()
            || validate_workflow_command_lease(
                &stored,
                lease,
                JobsWorkflowCommandState::Delivering,
                now_ms,
            )
            .is_ok();
    if !valid {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    if stored.application_id != lease.command.application_id
        || stored.run_id != lease.command.run_id
        || stored.workflow_id != lease.command.workflow_id
        || stored.command_kind != lease.command.command_kind.as_str()
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn lock_postgres_managed_cloud_request_start_attempt(
    tx: &mut postgres::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
) -> ManagedCloudResult<()> {
    let token_sha256 = workflow_command_lease_token_sha256(&lease.lease_token);
    let locked = tx
        .query_opt(
            "SELECT 1 FROM jobs_workflow_command_attempts
              WHERE id=$1 AND account_id=$2 AND command_id=$3 AND fence=$4
                AND lease_owner=$5 AND lease_token_sha256=$6
                AND lease_expires_at_ms=$7
              FOR UPDATE",
            &[
                &lease.attempt_id,
                &lease.command.account_id,
                &lease.command.id,
                &lease.fence,
                &lease.lease_owner,
                &token_sha256,
                &lease.lease_expires_at_ms,
            ],
        )
        .map_err(managed_cloud_storage)?
        .is_some();
    if !locked {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(())
}

fn sqlite_managed_cloud_request_execution_binding(
    tx: &rusqlite::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
    command_kind: &str,
    command_binding: &ManagedCloudWorkflowBinding,
) -> ManagedCloudResult<ManagedCloudWorkflowBinding> {
    if command_kind == "start" {
        return Ok(command_binding.clone());
    }
    if command_kind != "resume" {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let start = sqlite_managed_cloud_start_binding_for_resume(tx, input)?;
    if start.admission != command_binding.admission {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let start_input = ManagedCloudWorkflowBindingInput {
        command_id: start.command_id.clone(),
        account_id: input.account_id.clone(),
        application_id: input.application_id.clone(),
        run_id: input.run_id.clone(),
        workflow_id: input.workflow_id.clone(),
        scope: input.scope.clone(),
    };
    require_exact_managed_cloud_workflow_binding(&start, &start_input)
}

fn postgres_managed_cloud_request_execution_binding(
    tx: &mut postgres::Transaction<'_>,
    input: &ManagedCloudWorkflowBindingInput,
    command_kind: &str,
    command_binding: &ManagedCloudWorkflowBinding,
    lock_rows: bool,
) -> ManagedCloudResult<ManagedCloudWorkflowBinding> {
    if command_kind == "start" {
        return Ok(command_binding.clone());
    }
    if command_kind != "resume" {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let start = postgres_managed_cloud_start_binding_for_resume(tx, input, lock_rows)?;
    if start.admission != command_binding.admission {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let start_input = ManagedCloudWorkflowBindingInput {
        command_id: start.command_id.clone(),
        account_id: input.account_id.clone(),
        application_id: input.application_id.clone(),
        run_id: input.run_id.clone(),
        workflow_id: input.workflow_id.clone(),
        scope: input.scope.clone(),
    };
    require_exact_managed_cloud_workflow_binding(&start, &start_input)
}

fn managed_cloud_request_start_authority(
    binding: ManagedCloudWorkflowBinding,
    execution_binding: ManagedCloudWorkflowBinding,
    current: ManagedCloudAdmissionAuthority,
) -> ManagedCloudResult<ManagedCloudRequestStartAuthority> {
    let recovery_accepted =
        !managed_cloud_current_is_frozen(&current, &execution_binding.admission);
    let authorized_at_ms = current.resolved_at_ms;
    if authorized_at_ms < execution_binding.admission.resolved_at_ms
        || authorized_at_ms >= current.activation_expires_at_ms
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    let recovery_authorization_sha256 =
        managed_cloud_digest(&ManagedCloudRecoveryAuthorizationDigest {
            version: 1,
            audience: MANAGED_CLOUD_RECOVERY_AUTHORIZATION_AUDIENCE,
            binding_sha256: &execution_binding.binding_sha256,
            current_head_revision: current.head_revision,
            current_transition_sha256: &current.transition_sha256,
            current_activation_sha256: &current.activation_sha256,
            current_manifest_sha256: &current.manifest_sha256,
            current_activation_expires_at_ms: current.activation_expires_at_ms,
            current_task_queue_sha256: &current.task_queue_sha256,
            current_failure_converter_sha256: &current.failure_converter_sha256,
            current_readiness_sha256: &current.readiness_sha256,
            frozen_activation_sha256: &execution_binding.admission.activation_sha256,
            frozen_manifest_sha256: &execution_binding.admission.manifest_sha256,
            recovery_accepted,
            authorized_at_ms,
        })?;
    Ok(ManagedCloudRequestStartAuthority {
        managed_cloud: ManagedCloudGatewayAuthority {
            version: 1,
            execution: ManagedCloudExecutionAuthority {
                binding_sha256: execution_binding.binding_sha256,
                admission: execution_binding.admission,
            },
            authorization: ManagedCloudCurrentAuthorization {
                current_head_revision: current.head_revision,
                current_transition_sha256: current.transition_sha256,
                current_activation_sha256: current.activation_sha256,
                current_manifest_sha256: current.manifest_sha256,
                current_activation_expires_at_ms: current.activation_expires_at_ms,
                current_task_queue_sha256: current.task_queue_sha256,
                current_failure_converter_sha256: current.failure_converter_sha256,
                current_readiness_sha256: current.readiness_sha256,
                recovery_accepted,
                recovery_authorization_sha256,
                authorized_at_ms,
            },
        },
        binding,
        replayed: false,
        reconcile_only: false,
        attempt_replayed: false,
    })
}

fn validate_managed_cloud_gateway_authority(
    authority: &ManagedCloudGatewayAuthority,
    execution_binding: &ManagedCloudWorkflowBinding,
) -> ManagedCloudResult<()> {
    let expected_execution = ManagedCloudExecutionAuthority {
        binding_sha256: execution_binding.binding_sha256.clone(),
        admission: execution_binding.admission.clone(),
    };
    let authorization = &authority.authorization;
    let exact_current = authority.execution.admission.head_revision
        == authorization.current_head_revision
        && authority.execution.admission.transition_sha256
            == authorization.current_transition_sha256
        && authority.execution.admission.activation_sha256
            == authorization.current_activation_sha256
        && authority.execution.admission.manifest_sha256 == authorization.current_manifest_sha256
        && authority.execution.admission.activation_expires_at_ms
            == authorization.current_activation_expires_at_ms
        && authority.execution.admission.task_queue_sha256
            == authorization.current_task_queue_sha256
        && authority.execution.admission.failure_converter_sha256
            == authorization.current_failure_converter_sha256;
    if authority.version != 1
        || authority.execution != expected_execution
        || !managed_cloud_safe_integer(authorization.current_head_revision, true)
        || !managed_cloud_hex64(&authorization.current_transition_sha256)
        || !managed_cloud_hex64(&authorization.current_activation_sha256)
        || !managed_cloud_hex64(&authorization.current_manifest_sha256)
        || !managed_cloud_safe_integer(authorization.current_activation_expires_at_ms, true)
        || !managed_cloud_hex64(&authorization.current_task_queue_sha256)
        || !managed_cloud_hex64(&authorization.current_failure_converter_sha256)
        || !managed_cloud_hex64(&authorization.current_readiness_sha256)
        || !managed_cloud_hex64(&authorization.recovery_authorization_sha256)
        || !managed_cloud_safe_integer(authorization.authorized_at_ms, false)
        || authorization.authorized_at_ms < authority.execution.admission.resolved_at_ms
        || authorization.authorized_at_ms >= authorization.current_activation_expires_at_ms
        || authorization.recovery_accepted == exact_current
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let recovery_authorization_sha256 =
        managed_cloud_digest(&ManagedCloudRecoveryAuthorizationDigest {
            version: 1,
            audience: MANAGED_CLOUD_RECOVERY_AUTHORIZATION_AUDIENCE,
            binding_sha256: &authority.execution.binding_sha256,
            current_head_revision: authorization.current_head_revision,
            current_transition_sha256: &authorization.current_transition_sha256,
            current_activation_sha256: &authorization.current_activation_sha256,
            current_manifest_sha256: &authorization.current_manifest_sha256,
            current_activation_expires_at_ms: authorization.current_activation_expires_at_ms,
            current_task_queue_sha256: &authorization.current_task_queue_sha256,
            current_failure_converter_sha256: &authorization.current_failure_converter_sha256,
            current_readiness_sha256: &authorization.current_readiness_sha256,
            frozen_activation_sha256: &authority.execution.admission.activation_sha256,
            frozen_manifest_sha256: &authority.execution.admission.manifest_sha256,
            recovery_accepted: authorization.recovery_accepted,
            authorized_at_ms: authorization.authorized_at_ms,
        })?;
    if recovery_authorization_sha256 != authorization.recovery_authorization_sha256 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

const MANAGED_CLOUD_REQUEST_START_AUTHORITY_COLUMNS: &str =
    "attempt_id,account_id,command_id,fence,binding_sha256,\
     gateway_authority_base64url,gateway_authority_sha256,authorized_at_ms";

fn managed_cloud_request_start_authority_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ManagedCloudStoredRequestStartAuthority> {
    Ok(ManagedCloudStoredRequestStartAuthority {
        attempt_id: row.get(0)?,
        account_id: row.get(1)?,
        command_id: row.get(2)?,
        fence: row.get(3)?,
        binding_sha256: row.get(4)?,
        gateway_authority_base64url: row.get(5)?,
        gateway_authority_sha256: row.get(6)?,
        authorized_at_ms: row.get(7)?,
    })
}

fn managed_cloud_request_start_authority_from_postgres(
    row: &postgres::Row,
) -> ManagedCloudStoredRequestStartAuthority {
    ManagedCloudStoredRequestStartAuthority {
        attempt_id: row.get(0),
        account_id: row.get(1),
        command_id: row.get(2),
        fence: row.get(3),
        binding_sha256: row.get(4),
        gateway_authority_base64url: row.get(5),
        gateway_authority_sha256: row.get(6),
        authorized_at_ms: row.get(7),
    }
}

fn sqlite_managed_cloud_stored_request_start_authority(
    tx: &rusqlite::Transaction<'_>,
    command_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudStoredRequestStartAuthority>> {
    tx.query_row(
        &format!(
            "SELECT {MANAGED_CLOUD_REQUEST_START_AUTHORITY_COLUMNS}
               FROM jobs_managed_cloud_request_start_authorities
              WHERE command_id=?1"
        ),
        params![command_id],
        managed_cloud_request_start_authority_from_sqlite,
    )
    .optional()
    .map_err(managed_cloud_storage)
}

fn postgres_managed_cloud_stored_request_start_authority(
    tx: &mut postgres::Transaction<'_>,
    command_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudStoredRequestStartAuthority>> {
    tx.query_opt(
        &format!(
            "SELECT {MANAGED_CLOUD_REQUEST_START_AUTHORITY_COLUMNS}
               FROM jobs_managed_cloud_request_start_authorities
              WHERE command_id=$1"
        ),
        &[&command_id],
    )
    .map(|row| {
        row.as_ref()
            .map(managed_cloud_request_start_authority_from_postgres)
    })
    .map_err(managed_cloud_storage)
}

fn require_exact_managed_cloud_stored_request_start_authority(
    stored: &ManagedCloudStoredRequestStartAuthority,
    lease: &JobsWorkflowCommandLease,
    binding: ManagedCloudWorkflowBinding,
    execution_binding: &ManagedCloudWorkflowBinding,
) -> ManagedCloudResult<ManagedCloudRequestStartAuthority> {
    if stored.account_id != lease.command.account_id
        || stored.command_id != lease.command.id
        || stored.binding_sha256 != binding.binding_sha256
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let bytes = managed_cloud_decode_base64url(&stored.gateway_authority_base64url)?;
    if managed_cloud_sha256(&bytes) != stored.gateway_authority_sha256 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let managed_cloud: ManagedCloudGatewayAuthority = managed_cloud_parse_canonical(&bytes)?;
    validate_managed_cloud_gateway_authority(&managed_cloud, execution_binding)?;
    if managed_cloud.authorization.authorized_at_ms != stored.authorized_at_ms {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(ManagedCloudRequestStartAuthority {
        binding,
        managed_cloud,
        replayed: true,
        reconcile_only: true,
        attempt_replayed: stored.attempt_id == lease.attempt_id && stored.fence == lease.fence,
    })
}

fn resolve_postgres_managed_cloud_stored_request_start_authority(
    tx: &mut postgres::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
    stored: &ManagedCloudStoredRequestStartAuthority,
) -> ManagedCloudResult<ManagedCloudRequestStartAuthority> {
    let now_ms = managed_cloud_db_now_postgres(tx)?;
    let bytes = managed_cloud_decode_base64url(&stored.gateway_authority_base64url)?;
    let managed_cloud: ManagedCloudGatewayAuthority = managed_cloud_parse_canonical(&bytes)?;
    let input =
        managed_cloud_binding_input_from_lease(lease, &managed_cloud.execution.admission.scope);
    validate_managed_cloud_workflow_binding_input(&input)?;
    let (authority_required, command_kind) = postgres_managed_cloud_command_marker(tx, &input)?;
    if !authority_required || !matches!(command_kind.as_str(), "start" | "resume") {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    require_postgres_managed_cloud_command_lease_replay(tx, lease, now_ms)?;
    let binding = postgres_managed_cloud_workflow_binding(tx, &input.command_id)?
        .as_ref()
        .map(|binding| require_exact_managed_cloud_workflow_binding(binding, &input))
        .transpose()?
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    let execution_binding = postgres_managed_cloud_request_execution_binding(
        tx,
        &input,
        &command_kind,
        &binding,
        true,
    )?;
    require_exact_managed_cloud_stored_request_start_authority(
        stored,
        lease,
        binding,
        &execution_binding,
    )
}

fn insert_sqlite_managed_cloud_request_start_authority(
    tx: &rusqlite::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
    authority: &ManagedCloudRequestStartAuthority,
) -> ManagedCloudResult<()> {
    let bytes = managed_cloud_canonical_json(&authority.managed_cloud)?;
    let base64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes);
    tx.execute(
        "INSERT INTO jobs_managed_cloud_request_start_authorities(
           attempt_id,account_id,command_id,fence,binding_sha256,
           gateway_authority_base64url,gateway_authority_sha256,authorized_at_ms
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            lease.attempt_id,
            lease.command.account_id,
            lease.command.id,
            lease.fence,
            authority.binding.binding_sha256,
            base64url,
            managed_cloud_sha256(&bytes),
            authority.managed_cloud.authorization.authorized_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    Ok(())
}

fn insert_postgres_managed_cloud_request_start_authority(
    tx: &mut postgres::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
    authority: &ManagedCloudRequestStartAuthority,
) -> ManagedCloudResult<()> {
    let bytes = managed_cloud_canonical_json(&authority.managed_cloud)?;
    let base64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes);
    let sha256 = managed_cloud_sha256(&bytes);
    tx.execute(
        "INSERT INTO jobs_managed_cloud_request_start_authorities(
           attempt_id,account_id,command_id,fence,binding_sha256,
           gateway_authority_base64url,gateway_authority_sha256,authorized_at_ms
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        &[
            &lease.attempt_id,
            &lease.command.account_id,
            &lease.command.id,
            &lease.fence,
            &authority.binding.binding_sha256,
            &base64url,
            &sha256,
            &authority.managed_cloud.authorization.authorized_at_ms,
        ],
    )
    .map_err(managed_cloud_storage)?;
    Ok(())
}

pub(crate) fn resolve_managed_cloud_request_start_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
    scope: Option<&ManagedCloudScope>,
) -> ManagedCloudResult<Option<ManagedCloudRequestStartAuthority>> {
    let now_ms = managed_cloud_db_now_sqlite(tx)?;
    let authority_required =
        sqlite_managed_cloud_command_requires_authority(tx, &lease.command.id)?;
    if !authority_required {
        if sqlite_managed_cloud_workflow_binding(tx, &lease.command.id)?.is_some()
            || sqlite_managed_cloud_stored_request_start_authority(tx, &lease.command.id)?.is_some()
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        require_sqlite_managed_cloud_command_lease(tx, lease, now_ms)?;
        return Ok(None);
    }
    if let Some(stored) =
        sqlite_managed_cloud_stored_request_start_authority(tx, &lease.command.id)?
    {
        let bytes = managed_cloud_decode_base64url(&stored.gateway_authority_base64url)?;
        let managed_cloud: ManagedCloudGatewayAuthority = managed_cloud_parse_canonical(&bytes)?;
        let input =
            managed_cloud_binding_input_from_lease(lease, &managed_cloud.execution.admission.scope);
        validate_managed_cloud_workflow_binding_input(&input)?;
        let (authority_required, command_kind) = sqlite_managed_cloud_command_marker(tx, &input)?;
        if !authority_required || !matches!(command_kind.as_str(), "start" | "resume") {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        require_sqlite_managed_cloud_command_lease_replay(tx, lease, now_ms)?;
        let binding = sqlite_managed_cloud_workflow_binding(tx, &input.command_id)?
            .as_ref()
            .map(|binding| require_exact_managed_cloud_workflow_binding(binding, &input))
            .transpose()?
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
        let execution_binding =
            sqlite_managed_cloud_request_execution_binding(tx, &input, &command_kind, &binding)?;
        return require_exact_managed_cloud_stored_request_start_authority(
            &stored,
            lease,
            binding,
            &execution_binding,
        )
        .map(Some);
    }
    let scope = scope.ok_or(ManagedCloudRegistryError::Unavailable)?;
    let input = managed_cloud_binding_input_from_lease(lease, scope);
    validate_managed_cloud_workflow_binding_input(&input)?;
    let stored_binding = sqlite_managed_cloud_workflow_binding(tx, &input.command_id)?;
    let binding = stored_binding
        .as_ref()
        .map(|stored| require_exact_managed_cloud_workflow_binding(stored, &input))
        .transpose()?;
    let (authority_required, command_kind) = sqlite_managed_cloud_command_marker(tx, &input)?;
    require_sqlite_managed_cloud_command_lease(tx, lease, now_ms)?;
    if !authority_required || !matches!(command_kind.as_str(), "start" | "resume") {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let binding = binding.ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    let execution_binding =
        sqlite_managed_cloud_request_execution_binding(tx, &input, &command_kind, &binding)?;
    let current = require_sqlite_managed_cloud_effect_admission(tx, &input, now_ms)?;
    if !sqlite_managed_cloud_recovery_accepts(tx, &current, &execution_binding.admission, now_ms)? {
        return Err(ManagedCloudRegistryError::RecoveryNotAccepted);
    }
    let authority = managed_cloud_request_start_authority(binding, execution_binding, current)?;
    insert_sqlite_managed_cloud_request_start_authority(tx, lease, &authority)?;
    Ok(Some(authority))
}

pub(crate) fn resolve_managed_cloud_request_start_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    lease: &JobsWorkflowCommandLease,
    scope: Option<&ManagedCloudScope>,
) -> ManagedCloudResult<Option<ManagedCloudRequestStartAuthority>> {
    let authority_required =
        postgres_managed_cloud_command_requires_authority(tx, &lease.command.id)?;
    if !authority_required {
        if postgres_managed_cloud_workflow_binding(tx, &lease.command.id)?.is_some()
            || postgres_managed_cloud_stored_request_start_authority(tx, &lease.command.id)?
                .is_some()
        {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        let now_ms = managed_cloud_db_now_postgres(tx)?;
        require_postgres_managed_cloud_command_lease(tx, lease, now_ms)?;
        return Ok(None);
    }
    if let Some(stored) =
        postgres_managed_cloud_stored_request_start_authority(tx, &lease.command.id)?
    {
        lock_postgres_managed_cloud_request_start_attempt(tx, lease)?;
        return resolve_postgres_managed_cloud_stored_request_start_authority(tx, lease, &stored)
            .map(Some);
    }
    let scope = scope.ok_or(ManagedCloudRegistryError::Unavailable)?;
    let input = managed_cloud_binding_input_from_lease(lease, scope);
    validate_managed_cloud_workflow_binding_input(&input)?;
    lock_managed_cloud_workflow_admission_postgres_tx(tx, scope)?;
    if let Some(stored) =
        postgres_managed_cloud_stored_request_start_authority(tx, &lease.command.id)?
    {
        lock_postgres_managed_cloud_request_start_attempt(tx, lease)?;
        return resolve_postgres_managed_cloud_stored_request_start_authority(tx, lease, &stored)
            .map(Some);
    }
    let preliminary_now_ms = managed_cloud_db_now_postgres(tx)?;
    require_postgres_managed_cloud_effect_admission(tx, &input, preliminary_now_ms)?;
    lock_postgres_managed_cloud_request_start_attempt(tx, lease)?;
    if let Some(stored) =
        postgres_managed_cloud_stored_request_start_authority(tx, &lease.command.id)?
    {
        return resolve_postgres_managed_cloud_stored_request_start_authority(tx, lease, &stored)
            .map(Some);
    }
    let now_ms = managed_cloud_db_now_postgres(tx)?;
    let current = require_postgres_managed_cloud_effect_admission(tx, &input, now_ms)?;
    require_postgres_managed_cloud_command_lease(tx, lease, now_ms)?;
    let (authority_required, command_kind) = postgres_managed_cloud_command_marker(tx, &input)?;
    if !authority_required || !matches!(command_kind.as_str(), "start" | "resume") {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let stored_binding = postgres_managed_cloud_workflow_binding(tx, &input.command_id)?;
    let binding = stored_binding
        .as_ref()
        .map(|stored| require_exact_managed_cloud_workflow_binding(stored, &input))
        .transpose()?
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    let execution_binding = postgres_managed_cloud_request_execution_binding(
        tx,
        &input,
        &command_kind,
        &binding,
        true,
    )?;
    if !postgres_managed_cloud_recovery_accepts(tx, &current, &execution_binding.admission, now_ms)?
    {
        return Err(ManagedCloudRegistryError::RecoveryNotAccepted);
    }
    let authority = managed_cloud_request_start_authority(binding, execution_binding, current)?;
    insert_postgres_managed_cloud_request_start_authority(tx, lease, &authority)?;
    Ok(Some(authority))
}

fn require_managed_cloud_workflow_command_identity(
    stored: &JobsWorkflowCommand,
    expected: &JobsWorkflowCommand,
) -> ManagedCloudResult<()> {
    if stored.id != expected.id
        || stored.account_id != expected.account_id
        || stored.application_id != expected.application_id
        || stored.run_id != expected.run_id
        || stored.workflow_id != expected.workflow_id
        || stored.intervention_id != expected.intervention_id
        || stored.command_kind != expected.command_kind
        || stored.protocol_version != expected.protocol_version
        || !workflow_command_hmac_matches(
            &stored.idempotency_key_hmac_sha256,
            &expected.idempotency_key_hmac_sha256,
        )
        || stored.request_id != expected.request_id
        || !workflow_command_hmac_matches(
            &stored.request_hmac_sha256,
            &expected.request_hmac_sha256,
        )
        || !workflow_command_hmac_matches(
            &stored.payload_hmac_sha256,
            &expected.payload_hmac_sha256,
        )
        || stored.envelope != expected.envelope
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn sqlite_managed_cloud_workflow_release_memo(
    tx: &rusqlite::Transaction<'_>,
    command: &JobsWorkflowCommand,
) -> ManagedCloudResult<Option<ManagedCloudReleaseMemoAuthority>> {
    let stored = tx
        .query_row(
            &format!(
                "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                  WHERE id=?1 AND account_id=?2"
            ),
            params![command.id, command.account_id],
            sqlite_workflow_command_row,
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let stored = workflow_command_from_stored(stored)
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    require_managed_cloud_workflow_command_identity(&stored, command)?;
    let authority_required = sqlite_managed_cloud_command_requires_authority(tx, &command.id)?;
    let binding = sqlite_managed_cloud_workflow_binding(tx, &command.id)?;
    let request_start = sqlite_managed_cloud_stored_request_start_authority(tx, &command.id)?;
    if !authority_required {
        if binding.is_some() || request_start.is_some() {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        return Ok(None);
    }
    let binding = binding.ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    let input = ManagedCloudWorkflowBindingInput {
        command_id: command.id.clone(),
        account_id: command.account_id.clone(),
        application_id: command.application_id.clone(),
        run_id: command.run_id.clone(),
        workflow_id: command.workflow_id.clone(),
        scope: binding.admission.scope.clone(),
    };
    validate_managed_cloud_workflow_binding_input(&input)?;
    let (authority_required, command_kind) = sqlite_managed_cloud_command_marker(tx, &input)?;
    if !authority_required || command_kind != command.command_kind.as_str() {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let binding = require_exact_managed_cloud_workflow_binding(&binding, &input)?;
    let execution_binding =
        sqlite_managed_cloud_request_execution_binding(tx, &input, &command_kind, &binding)?;
    managed_cloud_release_memo(
        &execution_binding.binding_sha256,
        &execution_binding.admission,
    )
    .map(|(memo, _, _, _)| Some(memo))
}

fn postgres_managed_cloud_workflow_release_memo(
    tx: &mut postgres::Transaction<'_>,
    command: &JobsWorkflowCommand,
) -> ManagedCloudResult<Option<ManagedCloudReleaseMemoAuthority>> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                  WHERE id=$1 AND account_id=$2"
            ),
            &[&command.id, &command.account_id],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let stored = workflow_command_from_stored(postgres_workflow_command_row(&row))
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    require_managed_cloud_workflow_command_identity(&stored, command)?;
    let authority_required = postgres_managed_cloud_command_requires_authority(tx, &command.id)?;
    let binding = postgres_managed_cloud_workflow_binding(tx, &command.id)?;
    let request_start = postgres_managed_cloud_stored_request_start_authority(tx, &command.id)?;
    if !authority_required {
        if binding.is_some() || request_start.is_some() {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        }
        return Ok(None);
    }
    let binding = binding.ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    let input = ManagedCloudWorkflowBindingInput {
        command_id: command.id.clone(),
        account_id: command.account_id.clone(),
        application_id: command.application_id.clone(),
        run_id: command.run_id.clone(),
        workflow_id: command.workflow_id.clone(),
        scope: binding.admission.scope.clone(),
    };
    validate_managed_cloud_workflow_binding_input(&input)?;
    let (authority_required, command_kind) = postgres_managed_cloud_command_marker(tx, &input)?;
    if !authority_required || command_kind != command.command_kind.as_str() {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let binding = require_exact_managed_cloud_workflow_binding(&binding, &input)?;
    let execution_binding = postgres_managed_cloud_request_execution_binding(
        tx,
        &input,
        &command_kind,
        &binding,
        true,
    )?;
    managed_cloud_release_memo(
        &execution_binding.binding_sha256,
        &execution_binding.admission,
    )
    .map(|(memo, _, _, _)| Some(memo))
}

pub fn get_managed_cloud_workflow_release_memo(
    pool: &DbPool,
    command: &JobsWorkflowCommand,
) -> ManagedCloudResult<Option<ManagedCloudReleaseMemoAuthority>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection.transaction().map_err(managed_cloud_storage)?;
            let memo = sqlite_managed_cloud_workflow_release_memo(&tx, command)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(memo)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            let memo = postgres_managed_cloud_workflow_release_memo(&mut tx, command)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(memo)
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedCloudExecutionCommandAuthority {
    request_command_id: String,
    execution_binding: ManagedCloudWorkflowBinding,
    binding_input: ManagedCloudWorkflowBindingInput,
}

fn require_managed_cloud_execution_input(
    managed: bool,
    input: Option<&ManagedCloudExecutionLeaseClaimInput>,
) -> ManagedCloudResult<Option<&ManagedCloudExecutionLeaseClaimInput>> {
    match (managed, input) {
        (false, None) => Ok(None),
        (true, Some(input)) => Ok(Some(input)),
        _ => Err(ManagedCloudRegistryError::IdentityConflict),
    }
}

fn sqlite_managed_cloud_execution_is_managed(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ManagedCloudResult<bool> {
    let (complete_start, any_managed_authority): (bool, bool) = tx
        .query_row(
            "SELECT
               EXISTS(
                 SELECT 1 FROM jobs_workflow_commands command
                  JOIN jobs_managed_cloud_workflow_bindings binding
                    ON binding.command_id=command.id
                  JOIN jobs_managed_cloud_request_start_authorities request_start
                    ON request_start.command_id=command.id
                 WHERE command.account_id=?1 AND command.application_id=?2
                   AND command.run_id=?3 AND command.command_kind='start'
                   AND command.managed_cloud_authority_required=1
                   AND command.first_request_started_at_ms IS NOT NULL
               ),
               EXISTS(
                 SELECT 1 FROM jobs_workflow_commands command
                 WHERE command.account_id=?1 AND command.application_id=?2
                   AND command.run_id=?3
                   AND (command.managed_cloud_authority_required=1
                     OR EXISTS(
                       SELECT 1 FROM jobs_managed_cloud_workflow_bindings binding
                        WHERE binding.command_id=command.id)
                     OR EXISTS(
                       SELECT 1 FROM jobs_managed_cloud_request_start_authorities request_start
                        WHERE request_start.command_id=command.id))
               )",
            params![account_id, application_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(managed_cloud_storage)?;
    if !complete_start && any_managed_authority {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(complete_start)
}

fn postgres_managed_cloud_execution_is_managed(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ManagedCloudResult<bool> {
    let row = tx
        .query_one(
            "SELECT
               EXISTS(
                 SELECT 1 FROM jobs_workflow_commands command
                  JOIN jobs_managed_cloud_workflow_bindings binding
                    ON binding.command_id=command.id
                  JOIN jobs_managed_cloud_request_start_authorities request_start
                    ON request_start.command_id=command.id
                 WHERE command.account_id=$1 AND command.application_id=$2
                   AND command.run_id=$3 AND command.command_kind='start'
                   AND command.managed_cloud_authority_required
                   AND command.first_request_started_at_ms IS NOT NULL
               ),
               EXISTS(
                 SELECT 1 FROM jobs_workflow_commands command
                 WHERE command.account_id=$1 AND command.application_id=$2
                   AND command.run_id=$3
                   AND (command.managed_cloud_authority_required
                     OR EXISTS(
                       SELECT 1 FROM jobs_managed_cloud_workflow_bindings binding
                        WHERE binding.command_id=command.id)
                     OR EXISTS(
                       SELECT 1 FROM jobs_managed_cloud_request_start_authorities request_start
                        WHERE request_start.command_id=command.id))
               )",
            &[&account_id, &application_id, &run_id],
        )
        .map_err(managed_cloud_storage)?;
    let complete_start: bool = row.get(0);
    let any_managed_authority: bool = row.get(1);
    if !complete_start && any_managed_authority {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(complete_start)
}

fn validate_managed_cloud_execution_lease_claim_input(
    input: &ManagedCloudExecutionLeaseClaimInput,
) -> ManagedCloudResult<()> {
    if !managed_cloud_workflow_request_id(&input.workflow_request_id)
        || !managed_cloud_hex64(&input.managed_cloud_release_sha256)
        || !managed_cloud_route_id(&input.managed_cloud_runtime_instance_id)
        || !managed_cloud_safe_integer(input.managed_cloud_runtime_instance_epoch, true)
        || managed_cloud_release_memo_sha256(&input.managed_cloud_release)?
            != input.managed_cloud_release_sha256
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    Ok(())
}

fn managed_cloud_workflow_request_id(value: &str) -> bool {
    let Some(uuid) = value.strip_prefix("wfreq-v2-") else {
        return false;
    };
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'),
        })
        && uuid.as_bytes()[14] == b'5'
        && matches!(uuid.as_bytes()[19], b'8' | b'9' | b'a' | b'b')
}

fn sqlite_managed_cloud_execution_command_authority(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    input: &ManagedCloudExecutionLeaseClaimInput,
) -> ManagedCloudResult<ManagedCloudExecutionCommandAuthority> {
    let mut statement = tx
        .prepare(&format!(
            "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands command
              WHERE command.account_id=?1 AND command.application_id=?2
                AND command.run_id=?3 AND command.request_id=?4
                AND command.managed_cloud_authority_required=1
                AND command.command_kind IN ('start','resume')
                AND command.first_request_started_at_ms IS NOT NULL
                AND EXISTS(
                  SELECT 1 FROM jobs_workflow_command_attempt_events event
                   WHERE event.command_id=command.id
                     AND event.event_kind='request_started'
                )
              ORDER BY command.created_at_ms,command.id LIMIT 2"
        ))
        .map_err(managed_cloud_storage)?;
    let rows = statement
        .query_map(
            params![
                account_id,
                application_id,
                run_id,
                input.workflow_request_id,
            ],
            sqlite_workflow_command_row,
        )
        .map_err(managed_cloud_storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(managed_cloud_storage)?;
    if rows.len() != 1 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let command = workflow_command_from_stored(
        rows.into_iter()
            .next()
            .ok_or(ManagedCloudRegistryError::IdentityConflict)?,
    )
    .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    let scope = input
        .managed_cloud_release
        .execution
        .admission
        .scope
        .clone();
    let binding_input = ManagedCloudWorkflowBindingInput {
        command_id: command.id.clone(),
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        run_id: run_id.to_string(),
        workflow_id: command.workflow_id,
        scope,
    };
    validate_managed_cloud_workflow_binding_input(&binding_input)?;
    let binding = sqlite_managed_cloud_workflow_binding(tx, &binding_input.command_id)?
        .as_ref()
        .map(|stored| require_exact_managed_cloud_workflow_binding(stored, &binding_input))
        .transpose()?
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    let execution_binding = sqlite_managed_cloud_request_execution_binding(
        tx,
        &binding_input,
        command.command_kind.as_str(),
        &binding,
    )?;
    let (memo, _, memo_base64url, memo_sha256) = managed_cloud_release_memo(
        &execution_binding.binding_sha256,
        &execution_binding.admission,
    )?;
    if memo != input.managed_cloud_release
        || memo_sha256 != input.managed_cloud_release_sha256
        || memo_base64url != execution_binding.release_memo_base64url
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(ManagedCloudExecutionCommandAuthority {
        request_command_id: binding_input.command_id.clone(),
        execution_binding,
        binding_input,
    })
}

fn postgres_managed_cloud_execution_command_authority(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    input: &ManagedCloudExecutionLeaseClaimInput,
    lock_command: bool,
) -> ManagedCloudResult<ManagedCloudExecutionCommandAuthority> {
    let lock_clause = if lock_command {
        "FOR SHARE OF command"
    } else {
        ""
    };
    let rows = tx
        .query(
            &format!(
                "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands command
                  WHERE command.account_id=$1 AND command.application_id=$2
                    AND command.run_id=$3 AND command.request_id=$4
                    AND command.managed_cloud_authority_required
                    AND command.command_kind IN ('start','resume')
                    AND command.first_request_started_at_ms IS NOT NULL
                    AND EXISTS(
                      SELECT 1 FROM jobs_workflow_command_attempt_events event
                       WHERE event.command_id=command.id
                         AND event.event_kind='request_started'
                  )
                  ORDER BY command.created_at_ms,command.id
                  {lock_clause} LIMIT 2"
            ),
            &[
                &account_id,
                &application_id,
                &run_id,
                &input.workflow_request_id,
            ],
        )
        .map_err(managed_cloud_storage)?;
    if rows.len() != 1 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let command = workflow_command_from_stored(postgres_workflow_command_row(&rows[0]))
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    let scope = input
        .managed_cloud_release
        .execution
        .admission
        .scope
        .clone();
    let binding_input = ManagedCloudWorkflowBindingInput {
        command_id: command.id.clone(),
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        run_id: run_id.to_string(),
        workflow_id: command.workflow_id,
        scope,
    };
    validate_managed_cloud_workflow_binding_input(&binding_input)?;
    if lock_command {
        tx.query_opt(
            "SELECT 1 FROM jobs_managed_cloud_workflow_bindings
              WHERE command_id=$1 FOR SHARE",
            &[&binding_input.command_id],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    }
    let binding = postgres_managed_cloud_workflow_binding(tx, &binding_input.command_id)?
        .as_ref()
        .map(|stored| require_exact_managed_cloud_workflow_binding(stored, &binding_input))
        .transpose()?
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    let execution_binding = postgres_managed_cloud_request_execution_binding(
        tx,
        &binding_input,
        command.command_kind.as_str(),
        &binding,
        lock_command,
    )?;
    let (memo, _, memo_base64url, memo_sha256) = managed_cloud_release_memo(
        &execution_binding.binding_sha256,
        &execution_binding.admission,
    )?;
    if memo != input.managed_cloud_release
        || memo_sha256 != input.managed_cloud_release_sha256
        || memo_base64url != execution_binding.release_memo_base64url
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(ManagedCloudExecutionCommandAuthority {
        request_command_id: binding_input.command_id.clone(),
        execution_binding,
        binding_input,
    })
}

fn require_managed_cloud_runner_instance_matches_current(
    instance: &ManagedCloudRuntimeInstance,
    current: &ManagedCloudAdmissionAuthority,
    runtime_instance_id: &str,
    runtime_instance_epoch: i64,
    authenticated_worker_id: &str,
    volume_worker_id: &str,
) -> ManagedCloudResult<()> {
    if !managed_cloud_route_id(authenticated_worker_id)
        || !managed_cloud_route_id(volume_worker_id)
        || authenticated_worker_id
            .as_bytes()
            .ct_eq(volume_worker_id.as_bytes())
            .unwrap_u8()
            != 1
        || instance
            .worker_id
            .as_bytes()
            .ct_eq(authenticated_worker_id.as_bytes())
            .unwrap_u8()
            != 1
        || instance.runtime_instance_id != runtime_instance_id
        || instance.instance_epoch != runtime_instance_epoch
        || instance.component_id != "jobs-runner"
        || instance.role != "managed_runner"
        || instance.scope != current.scope
        || instance.head_revision != current.head_revision
        || instance.transition_sha256 != current.transition_sha256
        || instance.activation_sha256 != current.activation_sha256
        || instance.manifest_sha256 != current.manifest_sha256
        || instance.task_queue_sha256 != current.task_queue_sha256
        || instance.failure_converter_sha256 != current.failure_converter_sha256
        || instance.activation_expires_at_ms != current.activation_expires_at_ms
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(())
}

fn require_sqlite_managed_cloud_runner_instance_ready(
    tx: &rusqlite::Transaction<'_>,
    current: &ManagedCloudAdmissionAuthority,
    runtime_instance_id: &str,
    runtime_instance_epoch: i64,
    authenticated_worker_id: &str,
    volume_worker_id: &str,
    now_ms: i64,
) -> ManagedCloudResult<ManagedCloudRuntimeInstance> {
    let (instance, _) =
        sqlite_managed_cloud_runtime_instance_by_runtime_id(tx, runtime_instance_id)?
            .ok_or(ManagedCloudRegistryError::Unavailable)?;
    require_managed_cloud_runner_instance_matches_current(
        &instance,
        current,
        runtime_instance_id,
        runtime_instance_epoch,
        authenticated_worker_id,
        volume_worker_id,
    )?;
    require_sqlite_managed_cloud_runtime_authority_active(tx, &instance, now_ms)?;
    let ready = tx
        .query_row(
            "SELECT 1
               FROM jobs_managed_cloud_runtime_heartbeats heartbeat
               JOIN jobs_managed_cloud_activation_requirements requirement
                 ON requirement.activation_sha256=heartbeat.activation_sha256
                AND requirement.role=heartbeat.role
                AND requirement.dependency_evidence_sha256=
                    heartbeat.dependency_evidence_sha256
              WHERE heartbeat.runtime_instance_id=?1 AND heartbeat.instance_epoch=?2
                AND heartbeat.activation_sha256=?3 AND heartbeat.manifest_sha256=?4
                AND heartbeat.component_id='jobs-runner'
                AND heartbeat.role='managed_runner'
                AND heartbeat.worker_id=?5 AND heartbeat.artifact_sha256=?6
                AND heartbeat.observed_head_revision=?7
                AND heartbeat.observed_transition_sha256=?8
                AND heartbeat.migration_set_sha256=?9
                AND heartbeat.config_schema_sha256=?10
                AND heartbeat.protocol_set_sha256=?11
                AND heartbeat.task_queue_sha256=?12
                AND heartbeat.failure_converter_sha256=?13
                AND heartbeat.dependency_evidence_sha256=?14
                AND heartbeat.health_state='ready' AND heartbeat.reason_code IS NULL
                AND heartbeat.heartbeat_at_ms+requirement.heartbeat_ttl_ms>?15",
            params![
                instance.runtime_instance_id,
                instance.instance_epoch,
                instance.activation_sha256,
                instance.manifest_sha256,
                instance.worker_id,
                instance.artifact_sha256,
                instance.head_revision,
                instance.transition_sha256,
                instance.migration_set_sha256,
                instance.config_schema_sha256,
                instance.protocol_set_sha256,
                instance.task_queue_sha256,
                instance.failure_converter_sha256,
                instance.dependency_evidence_sha256,
                now_ms,
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .is_some();
    if !ready {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(instance)
}

fn require_postgres_managed_cloud_runner_instance_ready(
    tx: &mut postgres::Transaction<'_>,
    current: &ManagedCloudAdmissionAuthority,
    runtime_instance_id: &str,
    runtime_instance_epoch: i64,
    authenticated_worker_id: &str,
    volume_worker_id: &str,
    now_ms: i64,
) -> ManagedCloudResult<ManagedCloudRuntimeInstance> {
    let row = tx
        .query_opt(
            &format!(
                "SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS}
                   FROM jobs_managed_cloud_runtime_instances
                  WHERE runtime_instance_id=$1 AND instance_epoch=$2 FOR SHARE"
            ),
            &[&runtime_instance_id, &runtime_instance_epoch],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::Unavailable)?;
    let (instance, _) = managed_cloud_runtime_instance_from_postgres(&row);
    require_managed_cloud_runner_instance_matches_current(
        &instance,
        current,
        runtime_instance_id,
        runtime_instance_epoch,
        authenticated_worker_id,
        volume_worker_id,
    )?;
    tx.query_opt(
        "SELECT 1 FROM jobs_managed_cloud_runtime_grants
          WHERE grant_id=$1 FOR SHARE",
        &[&instance.grant_id],
    )
    .map_err(managed_cloud_storage)?
    .ok_or(ManagedCloudRegistryError::Unavailable)?;
    require_postgres_managed_cloud_runtime_authority_active(tx, &instance, now_ms)?;
    let ready = tx
        .query_opt(
            "SELECT 1
               FROM jobs_managed_cloud_runtime_heartbeats heartbeat
               JOIN jobs_managed_cloud_activation_requirements requirement
                 ON requirement.activation_sha256=heartbeat.activation_sha256
                AND requirement.role=heartbeat.role
                AND requirement.dependency_evidence_sha256=
                    heartbeat.dependency_evidence_sha256
              WHERE heartbeat.runtime_instance_id=$1 AND heartbeat.instance_epoch=$2
                AND heartbeat.activation_sha256=$3 AND heartbeat.manifest_sha256=$4
                AND heartbeat.component_id='jobs-runner'
                AND heartbeat.role='managed_runner'
                AND heartbeat.worker_id=$5 AND heartbeat.artifact_sha256=$6
                AND heartbeat.observed_head_revision=$7
                AND heartbeat.observed_transition_sha256=$8
                AND heartbeat.migration_set_sha256=$9
                AND heartbeat.config_schema_sha256=$10
                AND heartbeat.protocol_set_sha256=$11
                AND heartbeat.task_queue_sha256=$12
                AND heartbeat.failure_converter_sha256=$13
                AND heartbeat.dependency_evidence_sha256=$14
                AND heartbeat.health_state='ready' AND heartbeat.reason_code IS NULL
                AND heartbeat.heartbeat_at_ms+requirement.heartbeat_ttl_ms>$15
              FOR SHARE OF heartbeat,requirement",
            &[
                &instance.runtime_instance_id,
                &instance.instance_epoch,
                &instance.activation_sha256,
                &instance.manifest_sha256,
                &instance.worker_id,
                &instance.artifact_sha256,
                &instance.head_revision,
                &instance.transition_sha256,
                &instance.migration_set_sha256,
                &instance.config_schema_sha256,
                &instance.protocol_set_sha256,
                &instance.task_queue_sha256,
                &instance.failure_converter_sha256,
                &instance.dependency_evidence_sha256,
                &now_ms,
            ],
        )
        .map_err(managed_cloud_storage)?
        .is_some();
    if !ready {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(instance)
}

fn managed_cloud_execution_lease_authority(
    command: ManagedCloudExecutionCommandAuthority,
    input: &ManagedCloudExecutionLeaseClaimInput,
    current: ManagedCloudAdmissionAuthority,
    worker_id: &str,
) -> ManagedCloudResult<ManagedCloudExecutionLeaseAuthority> {
    let execution_binding = command.execution_binding;
    let request_start = managed_cloud_request_start_authority(
        execution_binding.clone(),
        execution_binding.clone(),
        current,
    )?;
    let gateway_bytes = managed_cloud_canonical_json(&request_start.managed_cloud)?;
    let gateway_authority_base64url =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&gateway_bytes);
    Ok(ManagedCloudExecutionLeaseAuthority {
        managed_cloud: request_start.managed_cloud,
        managed_cloud_workflow_request_id: input.workflow_request_id.clone(),
        request_command_id: command.request_command_id,
        execution_command_id: execution_binding.command_id,
        binding_sha256: execution_binding.binding_sha256,
        release_memo_base64url: execution_binding.release_memo_base64url,
        release_sha256: execution_binding.release_memo_sha256,
        managed_cloud_runtime_instance_id: input.managed_cloud_runtime_instance_id.clone(),
        managed_cloud_runtime_instance_epoch: input.managed_cloud_runtime_instance_epoch,
        managed_cloud_worker_id: worker_id.to_string(),
        gateway_authority_base64url,
        gateway_authority_sha256: managed_cloud_sha256(&gateway_bytes),
    })
}

pub(crate) fn resolve_managed_cloud_execution_lease_claim_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    input: Option<&ManagedCloudExecutionLeaseClaimInput>,
    authenticated_worker_id: &str,
    volume_worker_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudExecutionLeaseAuthority>> {
    let managed =
        sqlite_managed_cloud_execution_is_managed(tx, account_id, application_id, run_id)?;
    let Some(input) = require_managed_cloud_execution_input(managed, input)? else {
        return Ok(None);
    };
    validate_managed_cloud_execution_lease_claim_input(input)?;
    let command = sqlite_managed_cloud_execution_command_authority(
        tx,
        account_id,
        application_id,
        run_id,
        input,
    )?;
    let now_ms = managed_cloud_db_now_sqlite(tx)?;
    let current =
        require_sqlite_managed_cloud_effect_admission(tx, &command.binding_input, now_ms)?;
    if !sqlite_managed_cloud_recovery_accepts(
        tx,
        &current,
        &command.execution_binding.admission,
        now_ms,
    )? {
        return Err(ManagedCloudRegistryError::RecoveryNotAccepted);
    }
    require_sqlite_managed_cloud_runner_instance_ready(
        tx,
        &current,
        &input.managed_cloud_runtime_instance_id,
        input.managed_cloud_runtime_instance_epoch,
        authenticated_worker_id,
        volume_worker_id,
        now_ms,
    )?;
    managed_cloud_execution_lease_authority(command, input, current, authenticated_worker_id)
        .map(Some)
}

/// The caller must acquire `lock_managed_cloud_workflow_admission_postgres_tx`
/// before ATS/fleet child locks, then call this helper after the account lock
/// and before locking the execution lease.
pub(crate) fn resolve_managed_cloud_execution_lease_claim_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    input: Option<&ManagedCloudExecutionLeaseClaimInput>,
    authenticated_worker_id: &str,
    volume_worker_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudExecutionLeaseAuthority>> {
    let managed =
        postgres_managed_cloud_execution_is_managed(tx, account_id, application_id, run_id)?;
    let Some(input) = require_managed_cloud_execution_input(managed, input)? else {
        return Ok(None);
    };
    validate_managed_cloud_execution_lease_claim_input(input)?;
    let discovered_command = postgres_managed_cloud_execution_command_authority(
        tx,
        account_id,
        application_id,
        run_id,
        input,
        false,
    )?;
    let now_ms = managed_cloud_db_now_postgres(tx)?;
    let current = require_postgres_managed_cloud_effect_admission_after_prelock(
        tx,
        &discovered_command.binding_input,
        now_ms,
    )?;
    let command = postgres_managed_cloud_execution_command_authority(
        tx,
        account_id,
        application_id,
        run_id,
        input,
        true,
    )?;
    if command != discovered_command {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    if !postgres_managed_cloud_recovery_accepts(
        tx,
        &current,
        &command.execution_binding.admission,
        now_ms,
    )? {
        return Err(ManagedCloudRegistryError::RecoveryNotAccepted);
    }
    require_postgres_managed_cloud_runner_instance_ready(
        tx,
        &current,
        &input.managed_cloud_runtime_instance_id,
        input.managed_cloud_runtime_instance_epoch,
        authenticated_worker_id,
        volume_worker_id,
        now_ms,
    )?;
    managed_cloud_execution_lease_authority(command, input, current, authenticated_worker_id)
        .map(Some)
}

pub(crate) fn bind_managed_cloud_execution_lease_authority(
    authority: ManagedCloudExecutionLeaseAuthority,
    run_id: &str,
    fence: i64,
    lease_token_sha256: &str,
) -> ManagedCloudResult<BoundManagedCloudExecutionLeaseAuthority> {
    if !managed_cloud_route_id(run_id)
        || !managed_cloud_safe_integer(fence, true)
        || !managed_cloud_hex64(lease_token_sha256)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let lease_authority_sha256 =
        managed_cloud_digest(&ManagedCloudExecutionLeaseAuthorityDigest {
            version: 1,
            audience: MANAGED_CLOUD_EXECUTION_LEASE_AUTHORITY_AUDIENCE,
            run_id,
            fence,
            lease_token_sha256,
            workflow_request_id: &authority.managed_cloud_workflow_request_id,
            request_command_id: &authority.request_command_id,
            execution_command_id: &authority.execution_command_id,
            binding_sha256: &authority.binding_sha256,
            release_sha256: &authority.release_sha256,
            runtime_instance_id: &authority.managed_cloud_runtime_instance_id,
            runtime_instance_epoch: authority.managed_cloud_runtime_instance_epoch,
            worker_id: &authority.managed_cloud_worker_id,
            gateway_authority_sha256: &authority.gateway_authority_sha256,
        })?;
    Ok(BoundManagedCloudExecutionLeaseAuthority {
        authority,
        lease_authority_sha256,
    })
}

const MANAGED_CLOUD_EXECUTION_LEASE_AUTHORITY_COLUMNS: &str =
    "managed_cloud_workflow_request_id,managed_cloud_request_command_id,\
     managed_cloud_execution_command_id,managed_cloud_binding_sha256,\
     managed_cloud_release_memo_base64url,managed_cloud_release_sha256,\
     managed_cloud_runtime_instance_id,managed_cloud_runtime_instance_epoch,\
     managed_cloud_worker_id,\
     managed_cloud_gateway_authority_base64url,managed_cloud_gateway_authority_sha256,\
     managed_cloud_lease_authority_sha256";

fn managed_cloud_execution_lease_authority_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ManagedCloudStoredExecutionLeaseAuthority> {
    Ok(ManagedCloudStoredExecutionLeaseAuthority {
        workflow_request_id: row.get(0)?,
        request_command_id: row.get(1)?,
        execution_command_id: row.get(2)?,
        binding_sha256: row.get(3)?,
        release_memo_base64url: row.get(4)?,
        release_sha256: row.get(5)?,
        runtime_instance_id: row.get(6)?,
        runtime_instance_epoch: row.get(7)?,
        worker_id: row.get(8)?,
        gateway_authority_base64url: row.get(9)?,
        gateway_authority_sha256: row.get(10)?,
        lease_authority_sha256: row.get(11)?,
    })
}

fn managed_cloud_execution_lease_authority_from_postgres(
    row: &postgres::Row,
) -> ManagedCloudStoredExecutionLeaseAuthority {
    ManagedCloudStoredExecutionLeaseAuthority {
        workflow_request_id: row.get(0),
        request_command_id: row.get(1),
        execution_command_id: row.get(2),
        binding_sha256: row.get(3),
        release_memo_base64url: row.get(4),
        release_sha256: row.get(5),
        runtime_instance_id: row.get(6),
        runtime_instance_epoch: row.get(7),
        worker_id: row.get(8),
        gateway_authority_base64url: row.get(9),
        gateway_authority_sha256: row.get(10),
        lease_authority_sha256: row.get(11),
    }
}

fn managed_cloud_execution_lease_authority_present(
    stored: &ManagedCloudStoredExecutionLeaseAuthority,
) -> ManagedCloudResult<bool> {
    let present = [
        stored.workflow_request_id.is_some(),
        stored.request_command_id.is_some(),
        stored.execution_command_id.is_some(),
        stored.binding_sha256.is_some(),
        stored.release_memo_base64url.is_some(),
        stored.release_sha256.is_some(),
        stored.runtime_instance_id.is_some(),
        stored.runtime_instance_epoch.is_some(),
        stored.worker_id.is_some(),
        stored.gateway_authority_base64url.is_some(),
        stored.gateway_authority_sha256.is_some(),
        stored.lease_authority_sha256.is_some(),
    ]
    .into_iter()
    .filter(|value| *value)
    .count();
    match present {
        0 => Ok(false),
        12 => Ok(true),
        _ => Err(ManagedCloudRegistryError::InvalidAuthority),
    }
}

struct ManagedCloudStoredEffectContext<'a> {
    run_id: &'a str,
    fence: i64,
    lease_token_sha256: &'a str,
    authenticated_worker_id: &'a str,
    volume_worker_id: &'a str,
}

fn require_managed_cloud_stored_claim_allows_effect(
    stored: &ManagedCloudStoredExecutionLeaseAuthority,
    command: &ManagedCloudExecutionCommandAuthority,
    input: &ManagedCloudExecutionLeaseClaimInput,
    context: ManagedCloudStoredEffectContext<'_>,
) -> ManagedCloudResult<()> {
    let ManagedCloudStoredEffectContext {
        run_id,
        fence,
        lease_token_sha256,
        authenticated_worker_id,
        volume_worker_id,
    } = context;
    if !managed_cloud_execution_lease_authority_present(stored)?
        || !managed_cloud_route_id(authenticated_worker_id)
        || !managed_cloud_route_id(volume_worker_id)
        || authenticated_worker_id
            .as_bytes()
            .ct_eq(volume_worker_id.as_bytes())
            .unwrap_u8()
            != 1
        || stored.worker_id.as_deref() != Some(authenticated_worker_id)
        || stored.runtime_instance_id.as_deref()
            != Some(input.managed_cloud_runtime_instance_id.as_str())
        || stored.runtime_instance_epoch != Some(input.managed_cloud_runtime_instance_epoch)
        || stored.binding_sha256.as_deref()
            != Some(command.execution_binding.binding_sha256.as_str())
        || stored.execution_command_id.as_deref()
            != Some(command.execution_binding.command_id.as_str())
        || stored.release_sha256.as_deref() != Some(input.managed_cloud_release_sha256.as_str())
        || stored.release_memo_base64url.as_deref()
            != Some(command.execution_binding.release_memo_base64url.as_str())
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let release_bytes = managed_cloud_decode_base64url(
        stored
            .release_memo_base64url
            .as_deref()
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
    )?;
    if parse_managed_cloud_release_memo_bytes(&release_bytes)? != input.managed_cloud_release
        || managed_cloud_sha256(&release_bytes) != input.managed_cloud_release_sha256
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let gateway_authority_base64url = stored
        .gateway_authority_base64url
        .as_deref()
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    let gateway_bytes = managed_cloud_decode_base64url(gateway_authority_base64url)?;
    let gateway: ManagedCloudGatewayAuthority = managed_cloud_parse_canonical(&gateway_bytes)?;
    validate_managed_cloud_gateway_authority(&gateway, &command.execution_binding)?;
    let gateway_authority_sha256 = managed_cloud_sha256(&gateway_bytes);
    if stored.gateway_authority_sha256.as_deref() != Some(gateway_authority_sha256.as_str()) {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let stored_authority = ManagedCloudExecutionLeaseAuthority {
        managed_cloud: gateway,
        managed_cloud_workflow_request_id: stored
            .workflow_request_id
            .clone()
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
        request_command_id: stored
            .request_command_id
            .clone()
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
        execution_command_id: stored
            .execution_command_id
            .clone()
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
        binding_sha256: stored
            .binding_sha256
            .clone()
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
        release_memo_base64url: stored
            .release_memo_base64url
            .clone()
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
        release_sha256: stored
            .release_sha256
            .clone()
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
        managed_cloud_runtime_instance_id: stored
            .runtime_instance_id
            .clone()
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
        managed_cloud_runtime_instance_epoch: stored
            .runtime_instance_epoch
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
        managed_cloud_worker_id: stored
            .worker_id
            .clone()
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?,
        gateway_authority_base64url: gateway_authority_base64url.to_string(),
        gateway_authority_sha256,
    };
    let bound = bind_managed_cloud_execution_lease_authority(
        stored_authority,
        run_id,
        fence,
        lease_token_sha256,
    )?;
    if stored.lease_authority_sha256.as_deref() != Some(bound.lease_authority_sha256.as_str()) {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_managed_cloud_execution_effect_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    fence: i64,
    lease_token_sha256: &str,
    input: Option<&ManagedCloudExecutionLeaseClaimInput>,
    authenticated_worker_id: &str,
    volume_worker_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudExecutionLeaseAuthority>> {
    let managed =
        sqlite_managed_cloud_execution_is_managed(tx, account_id, application_id, run_id)?;
    let Some(input) = require_managed_cloud_execution_input(managed, input)? else {
        return Ok(None);
    };
    validate_managed_cloud_execution_lease_claim_input(input)?;
    let command = sqlite_managed_cloud_execution_command_authority(
        tx,
        account_id,
        application_id,
        run_id,
        input,
    )?;
    let now_ms = managed_cloud_db_now_sqlite(tx)?;
    let current =
        require_sqlite_managed_cloud_effect_admission(tx, &command.binding_input, now_ms)?;
    let stored = tx
        .query_row(
            &format!(
                "SELECT {MANAGED_CLOUD_EXECUTION_LEASE_AUTHORITY_COLUMNS}
                   FROM jobs_execution_leases
                  WHERE run_id=?1 AND account_id=?2 AND application_id=?3
                    AND fence=?4 AND lease_token_sha256=?5"
            ),
            params![
                run_id,
                account_id,
                application_id,
                fence,
                lease_token_sha256,
            ],
            managed_cloud_execution_lease_authority_from_sqlite,
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    require_managed_cloud_stored_claim_allows_effect(
        &stored,
        &command,
        input,
        ManagedCloudStoredEffectContext {
            run_id,
            fence,
            lease_token_sha256,
            authenticated_worker_id,
            volume_worker_id,
        },
    )?;
    if !sqlite_managed_cloud_recovery_accepts(
        tx,
        &current,
        &command.execution_binding.admission,
        now_ms,
    )? {
        return Err(ManagedCloudRegistryError::RecoveryNotAccepted);
    }
    require_sqlite_managed_cloud_runner_instance_ready(
        tx,
        &current,
        &input.managed_cloud_runtime_instance_id,
        input.managed_cloud_runtime_instance_epoch,
        authenticated_worker_id,
        volume_worker_id,
        now_ms,
    )?;
    managed_cloud_execution_lease_authority(command, input, current, authenticated_worker_id)
        .map(Some)
}

/// The caller must acquire the managed-cloud global admission locks before
/// ATS/fleet locks and call this helper after the account lock but before the
/// execution lease is locked. The helper locks entitlement/application,
/// command/binding, then the exact lease in that order.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_managed_cloud_execution_effect_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    fence: i64,
    lease_token_sha256: &str,
    input: Option<&ManagedCloudExecutionLeaseClaimInput>,
    authenticated_worker_id: &str,
    volume_worker_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudExecutionLeaseAuthority>> {
    let managed =
        postgres_managed_cloud_execution_is_managed(tx, account_id, application_id, run_id)?;
    let Some(input) = require_managed_cloud_execution_input(managed, input)? else {
        return Ok(None);
    };
    validate_managed_cloud_execution_lease_claim_input(input)?;
    let discovered_command = postgres_managed_cloud_execution_command_authority(
        tx,
        account_id,
        application_id,
        run_id,
        input,
        false,
    )?;
    let now_ms = managed_cloud_db_now_postgres(tx)?;
    let current = require_postgres_managed_cloud_effect_admission_after_prelock(
        tx,
        &discovered_command.binding_input,
        now_ms,
    )?;
    let command = postgres_managed_cloud_execution_command_authority(
        tx,
        account_id,
        application_id,
        run_id,
        input,
        true,
    )?;
    if command != discovered_command {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let row = tx
        .query_opt(
            &format!(
                "SELECT {MANAGED_CLOUD_EXECUTION_LEASE_AUTHORITY_COLUMNS}
                   FROM jobs_execution_leases
                  WHERE run_id=$1 AND account_id=$2 AND application_id=$3
                    AND fence=$4 AND lease_token_sha256=$5 FOR SHARE"
            ),
            &[
                &run_id,
                &account_id,
                &application_id,
                &fence,
                &lease_token_sha256,
            ],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let stored = managed_cloud_execution_lease_authority_from_postgres(&row);
    require_managed_cloud_stored_claim_allows_effect(
        &stored,
        &command,
        input,
        ManagedCloudStoredEffectContext {
            run_id,
            fence,
            lease_token_sha256,
            authenticated_worker_id,
            volume_worker_id,
        },
    )?;
    if !postgres_managed_cloud_recovery_accepts(
        tx,
        &current,
        &command.execution_binding.admission,
        now_ms,
    )? {
        return Err(ManagedCloudRegistryError::RecoveryNotAccepted);
    }
    require_postgres_managed_cloud_runner_instance_ready(
        tx,
        &current,
        &input.managed_cloud_runtime_instance_id,
        input.managed_cloud_runtime_instance_epoch,
        authenticated_worker_id,
        volume_worker_id,
        now_ms,
    )?;
    managed_cloud_execution_lease_authority(command, input, current, authenticated_worker_id)
        .map(Some)
}

const MANAGED_CLOUD_IRREVERSIBLE_RECEIPT_COLUMNS: &str =
    "workflow_request_id,request_command_id,execution_command_id,binding_sha256,\
     release_memo_base64url,release_sha256,runtime_instance_id,runtime_instance_epoch,\
     worker_id,gateway_authority_base64url,gateway_authority_sha256,receipt_sha256";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedCloudStoredIrreversibleEffectReceipt {
    workflow_request_id: String,
    request_command_id: String,
    execution_command_id: String,
    binding_sha256: String,
    release_memo_base64url: String,
    release_sha256: String,
    runtime_instance_id: String,
    runtime_instance_epoch: i64,
    worker_id: String,
    gateway_authority_base64url: String,
    gateway_authority_sha256: String,
    receipt_sha256: String,
}

fn managed_cloud_irreversible_receipt_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ManagedCloudStoredIrreversibleEffectReceipt> {
    Ok(ManagedCloudStoredIrreversibleEffectReceipt {
        workflow_request_id: row.get(0)?,
        request_command_id: row.get(1)?,
        execution_command_id: row.get(2)?,
        binding_sha256: row.get(3)?,
        release_memo_base64url: row.get(4)?,
        release_sha256: row.get(5)?,
        runtime_instance_id: row.get(6)?,
        runtime_instance_epoch: row.get(7)?,
        worker_id: row.get(8)?,
        gateway_authority_base64url: row.get(9)?,
        gateway_authority_sha256: row.get(10)?,
        receipt_sha256: row.get(11)?,
    })
}

fn managed_cloud_irreversible_receipt_from_postgres(
    row: &postgres::Row,
) -> ManagedCloudStoredIrreversibleEffectReceipt {
    ManagedCloudStoredIrreversibleEffectReceipt {
        workflow_request_id: row.get(0),
        request_command_id: row.get(1),
        execution_command_id: row.get(2),
        binding_sha256: row.get(3),
        release_memo_base64url: row.get(4),
        release_sha256: row.get(5),
        runtime_instance_id: row.get(6),
        runtime_instance_epoch: row.get(7),
        worker_id: row.get(8),
        gateway_authority_base64url: row.get(9),
        gateway_authority_sha256: row.get(10),
        receipt_sha256: row.get(11),
    }
}

fn validate_managed_cloud_irreversible_receipt_authority(
    authority: &ManagedCloudExecutionLeaseAuthority,
) -> ManagedCloudResult<()> {
    let release_bytes = managed_cloud_decode_base64url(&authority.release_memo_base64url)?;
    let release = parse_managed_cloud_release_memo_bytes(&release_bytes)?;
    let gateway_bytes = managed_cloud_decode_base64url(&authority.gateway_authority_base64url)?;
    let gateway: ManagedCloudGatewayAuthority = managed_cloud_parse_canonical(&gateway_bytes)?;
    let input = ManagedCloudExecutionLeaseClaimInput {
        workflow_request_id: authority.managed_cloud_workflow_request_id.clone(),
        managed_cloud_release: release.clone(),
        managed_cloud_release_sha256: authority.release_sha256.clone(),
        managed_cloud_runtime_instance_id: authority.managed_cloud_runtime_instance_id.clone(),
        managed_cloud_runtime_instance_epoch: authority.managed_cloud_runtime_instance_epoch,
    };
    validate_managed_cloud_execution_lease_claim_input(&input)?;
    let binding = ManagedCloudWorkflowBinding {
        command_id: authority.execution_command_id.clone(),
        binding_sha256: authority.binding_sha256.clone(),
        release_memo_base64url: authority.release_memo_base64url.clone(),
        release_memo_sha256: authority.release_sha256.clone(),
        admission: release.execution.admission.clone(),
        replayed: true,
    };
    validate_managed_cloud_gateway_authority(&gateway, &binding)?;
    if !managed_cloud_route_id(&authority.request_command_id)
        || !managed_cloud_route_id(&authority.execution_command_id)
        || !managed_cloud_route_id(&authority.managed_cloud_worker_id)
        || authority.binding_sha256 != release.execution.binding_sha256
        || authority.managed_cloud != gateway
        || authority.managed_cloud.execution != release.execution
        || authority.release_sha256 != managed_cloud_sha256(&release_bytes)
        || authority.gateway_authority_sha256 != managed_cloud_sha256(&gateway_bytes)
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn managed_cloud_irreversible_receipt_sha256(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    fence: i64,
    lease_token_sha256: &str,
    authority: &ManagedCloudExecutionLeaseAuthority,
) -> ManagedCloudResult<String> {
    validate_managed_cloud_irreversible_receipt_authority(authority)?;
    if !managed_cloud_route_id(run_id)
        || !managed_cloud_safe_integer(fence, true)
        || !managed_cloud_hex64(lease_token_sha256)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let account_id_hmac_sha256 = managed_cloud_subject_hmac("account", account_id)?;
    let application_id_hmac_sha256 = managed_cloud_subject_hmac("application", application_id)?;
    managed_cloud_digest(&ManagedCloudIrreversibleEffectReceiptDigest {
        version: 1,
        audience: MANAGED_CLOUD_IRREVERSIBLE_EFFECT_RECEIPT_AUDIENCE,
        run_id,
        fence,
        lease_token_sha256,
        account_id_hmac_sha256: &account_id_hmac_sha256,
        application_id_hmac_sha256: &application_id_hmac_sha256,
        workflow_request_id: &authority.managed_cloud_workflow_request_id,
        request_command_id: &authority.request_command_id,
        execution_command_id: &authority.execution_command_id,
        binding_sha256: &authority.binding_sha256,
        release_sha256: &authority.release_sha256,
        runtime_instance_id: &authority.managed_cloud_runtime_instance_id,
        runtime_instance_epoch: authority.managed_cloud_runtime_instance_epoch,
        worker_id: &authority.managed_cloud_worker_id,
        gateway_authority_sha256: &authority.gateway_authority_sha256,
    })
}

struct ManagedCloudIrreversibleReceiptContext<'a> {
    account_id: &'a str,
    application_id: &'a str,
    run_id: &'a str,
    fence: i64,
    lease_token_sha256: &'a str,
    authenticated_worker_id: &'a str,
}

fn managed_cloud_irreversible_receipt_authority(
    stored: ManagedCloudStoredIrreversibleEffectReceipt,
    input: &ManagedCloudExecutionLeaseClaimInput,
    context: ManagedCloudIrreversibleReceiptContext<'_>,
) -> ManagedCloudResult<ManagedCloudIrreversibleEffectReceipt> {
    let ManagedCloudIrreversibleReceiptContext {
        account_id,
        application_id,
        run_id,
        fence,
        lease_token_sha256,
        authenticated_worker_id,
    } = context;
    let release_bytes = managed_cloud_decode_base64url(&stored.release_memo_base64url)?;
    let release = parse_managed_cloud_release_memo_bytes(&release_bytes)?;
    let gateway_bytes = managed_cloud_decode_base64url(&stored.gateway_authority_base64url)?;
    let gateway: ManagedCloudGatewayAuthority = managed_cloud_parse_canonical(&gateway_bytes)?;
    let authority = ManagedCloudExecutionLeaseAuthority {
        managed_cloud: gateway,
        managed_cloud_workflow_request_id: stored.workflow_request_id,
        request_command_id: stored.request_command_id,
        execution_command_id: stored.execution_command_id,
        binding_sha256: stored.binding_sha256,
        release_memo_base64url: stored.release_memo_base64url,
        release_sha256: stored.release_sha256,
        managed_cloud_runtime_instance_id: stored.runtime_instance_id,
        managed_cloud_runtime_instance_epoch: stored.runtime_instance_epoch,
        managed_cloud_worker_id: stored.worker_id,
        gateway_authority_base64url: stored.gateway_authority_base64url,
        gateway_authority_sha256: stored.gateway_authority_sha256,
    };
    validate_managed_cloud_irreversible_receipt_authority(&authority)?;
    if !managed_cloud_route_id(authenticated_worker_id)
        || authority.managed_cloud_workflow_request_id != input.workflow_request_id
        || release != input.managed_cloud_release
        || authority.release_sha256 != input.managed_cloud_release_sha256
        || authority.managed_cloud_runtime_instance_id != input.managed_cloud_runtime_instance_id
        || authority.managed_cloud_runtime_instance_epoch
            != input.managed_cloud_runtime_instance_epoch
        || authority
            .managed_cloud_worker_id
            .as_bytes()
            .ct_eq(authenticated_worker_id.as_bytes())
            .unwrap_u8()
            != 1
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let receipt_sha256 = managed_cloud_irreversible_receipt_sha256(
        account_id,
        application_id,
        run_id,
        fence,
        lease_token_sha256,
        &authority,
    )?;
    if receipt_sha256 != stored.receipt_sha256 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(ManagedCloudIrreversibleEffectReceipt { authority })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_managed_cloud_irreversible_effect_receipt_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    fence: i64,
    lease_token_sha256: &str,
    authority: &ManagedCloudExecutionLeaseAuthority,
) -> ManagedCloudResult<ManagedCloudIrreversibleEffectReceipt> {
    let receipt_sha256 = managed_cloud_irreversible_receipt_sha256(
        account_id,
        application_id,
        run_id,
        fence,
        lease_token_sha256,
        authority,
    )?;
    let committed_at_ms = managed_cloud_db_now_sqlite(tx)?;
    tx.execute(
        "INSERT INTO jobs_managed_cloud_irreversible_effect_receipts(
           run_id,fence,account_id,application_id,workflow_request_id,
           request_command_id,execution_command_id,binding_sha256,
           release_memo_base64url,release_sha256,runtime_instance_id,
           runtime_instance_epoch,worker_id,gateway_authority_base64url,
           gateway_authority_sha256,receipt_sha256,committed_at_ms
         ) SELECT ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17
            WHERE EXISTS(
              SELECT 1 FROM jobs_execution_leases lease
               WHERE lease.run_id=?1 AND lease.account_id=?3
                 AND lease.application_id=?4 AND lease.fence=?2
                 AND lease.lease_token_sha256=?18 AND lease.phase='click_started')",
        params![
            run_id,
            fence,
            account_id,
            application_id,
            authority.managed_cloud_workflow_request_id,
            authority.request_command_id,
            authority.execution_command_id,
            authority.binding_sha256,
            authority.release_memo_base64url,
            authority.release_sha256,
            authority.managed_cloud_runtime_instance_id,
            authority.managed_cloud_runtime_instance_epoch,
            authority.managed_cloud_worker_id,
            authority.gateway_authority_base64url,
            authority.gateway_authority_sha256,
            receipt_sha256,
            committed_at_ms,
            lease_token_sha256,
        ],
    )
    .map_err(managed_cloud_storage)
    .and_then(|inserted| {
        if inserted == 1 {
            Ok(ManagedCloudIrreversibleEffectReceipt {
                authority: authority.clone(),
            })
        } else {
            Err(ManagedCloudRegistryError::IdentityConflict)
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_managed_cloud_irreversible_effect_receipt_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    fence: i64,
    lease_token_sha256: &str,
    authority: &ManagedCloudExecutionLeaseAuthority,
) -> ManagedCloudResult<ManagedCloudIrreversibleEffectReceipt> {
    let receipt_sha256 = managed_cloud_irreversible_receipt_sha256(
        account_id,
        application_id,
        run_id,
        fence,
        lease_token_sha256,
        authority,
    )?;
    let committed_at_ms = managed_cloud_db_now_postgres(tx)?;
    let inserted = tx
        .execute(
            "INSERT INTO jobs_managed_cloud_irreversible_effect_receipts(
               run_id,fence,account_id,application_id,workflow_request_id,
               request_command_id,execution_command_id,binding_sha256,
               release_memo_base64url,release_sha256,runtime_instance_id,
               runtime_instance_epoch,worker_id,gateway_authority_base64url,
               gateway_authority_sha256,receipt_sha256,committed_at_ms
             ) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17
                WHERE EXISTS(
                  SELECT 1 FROM jobs_execution_leases lease
                   WHERE lease.run_id=$1 AND lease.account_id=$3
                     AND lease.application_id=$4 AND lease.fence=$2
                     AND lease.lease_token_sha256=$18 AND lease.phase='click_started')",
            &[
                &run_id,
                &fence,
                &account_id,
                &application_id,
                &authority.managed_cloud_workflow_request_id,
                &authority.request_command_id,
                &authority.execution_command_id,
                &authority.binding_sha256,
                &authority.release_memo_base64url,
                &authority.release_sha256,
                &authority.managed_cloud_runtime_instance_id,
                &authority.managed_cloud_runtime_instance_epoch,
                &authority.managed_cloud_worker_id,
                &authority.gateway_authority_base64url,
                &authority.gateway_authority_sha256,
                &receipt_sha256,
                &committed_at_ms,
                &lease_token_sha256,
            ],
        )
        .map_err(managed_cloud_storage)?;
    if inserted != 1 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(ManagedCloudIrreversibleEffectReceipt {
        authority: authority.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_managed_cloud_irreversible_effect_receipt_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    fence: i64,
    lease_token_sha256: &str,
    input: Option<&ManagedCloudExecutionLeaseClaimInput>,
    authenticated_worker_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudIrreversibleEffectReceipt>> {
    let managed: Option<bool> = tx
        .query_row(
            "SELECT managed_cloud_lease_authority_sha256 IS NOT NULL
               FROM jobs_execution_leases
              WHERE run_id=?1 AND account_id=?2 AND application_id=?3
                AND fence=?4 AND lease_token_sha256=?5 AND phase='click_started'",
            params![
                run_id,
                account_id,
                application_id,
                fence,
                lease_token_sha256
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(managed_cloud_storage)?;
    let managed = managed.ok_or(ManagedCloudRegistryError::NotFound)?;
    let stored = tx
        .query_row(
            &format!(
                "SELECT {MANAGED_CLOUD_IRREVERSIBLE_RECEIPT_COLUMNS}
                   FROM jobs_managed_cloud_irreversible_effect_receipts
                  WHERE run_id=?1 AND fence=?2 AND account_id=?3 AND application_id=?4"
            ),
            params![run_id, fence, account_id, application_id],
            managed_cloud_irreversible_receipt_from_sqlite,
        )
        .optional()
        .map_err(managed_cloud_storage)?;
    match (managed, input, stored) {
        (false, None, None) => Ok(None),
        (true, Some(input), Some(stored)) => managed_cloud_irreversible_receipt_authority(
            stored,
            input,
            ManagedCloudIrreversibleReceiptContext {
                account_id,
                application_id,
                run_id,
                fence,
                lease_token_sha256,
                authenticated_worker_id,
            },
        )
        .map(Some),
        _ => Err(ManagedCloudRegistryError::IdentityConflict),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_managed_cloud_irreversible_effect_receipt_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    fence: i64,
    lease_token_sha256: &str,
    input: Option<&ManagedCloudExecutionLeaseClaimInput>,
    authenticated_worker_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudIrreversibleEffectReceipt>> {
    let row = tx
        .query_opt(
            "SELECT managed_cloud_lease_authority_sha256 IS NOT NULL
               FROM jobs_execution_leases
              WHERE run_id=$1 AND account_id=$2 AND application_id=$3
                AND fence=$4 AND lease_token_sha256=$5 AND phase='click_started'
              FOR SHARE",
            &[
                &run_id,
                &account_id,
                &application_id,
                &fence,
                &lease_token_sha256,
            ],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::NotFound)?;
    let managed: bool = row.get(0);
    let stored = tx
        .query_opt(
            &format!(
                "SELECT {MANAGED_CLOUD_IRREVERSIBLE_RECEIPT_COLUMNS}
                   FROM jobs_managed_cloud_irreversible_effect_receipts
                  WHERE run_id=$1 AND fence=$2 AND account_id=$3 AND application_id=$4
                  FOR SHARE"
            ),
            &[&run_id, &fence, &account_id, &application_id],
        )
        .map_err(managed_cloud_storage)?
        .as_ref()
        .map(managed_cloud_irreversible_receipt_from_postgres);
    match (managed, input, stored) {
        (false, None, None) => Ok(None),
        (true, Some(input), Some(stored)) => managed_cloud_irreversible_receipt_authority(
            stored,
            input,
            ManagedCloudIrreversibleReceiptContext {
                account_id,
                application_id,
                run_id,
                fence,
                lease_token_sha256,
                authenticated_worker_id,
            },
        )
        .map(Some),
        _ => Err(ManagedCloudRegistryError::IdentityConflict),
    }
}

fn validate_new_managed_cloud_runtime_grant(
    input: &NewManagedCloudRuntimeGrant,
) -> ManagedCloudResult<()> {
    validate_managed_cloud_scope(&input.scope)?;
    if !managed_cloud_route_id(&input.issuance_ref)
        || !managed_cloud_hex64(&input.activation_sha256)
        || !managed_cloud_hex64(&input.manifest_sha256)
        || !matches!(
            input.component_id.as_str(),
            "jobs-api" | "jobs-runner" | "jobs-workflows"
        )
        || !managed_cloud_runtime_role(&input.role)
        || input.role == "original_source_verifier"
        || !managed_cloud_route_id(&input.expected_worker_id)
        || !managed_cloud_route_id(&input.authorization_ref)
        || !managed_cloud_actor(&input.created_by)
        || !(MANAGED_CLOUD_MIN_GRANT_TTL_MS..=MANAGED_CLOUD_MAX_GRANT_TTL_MS)
            .contains(&input.ttl_ms)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    Ok(())
}

fn generate_managed_cloud_runtime_grant_credentials() -> (String, String) {
    let mut id_bytes = [0_u8; 24];
    let mut token_bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut id_bytes);
    rand::thread_rng().fill_bytes(&mut token_bytes);
    (
        format!(
            "mcg_{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id_bytes)
        ),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes),
    )
}

fn validate_claim_managed_cloud_runtime_grant(
    input: &ClaimManagedCloudRuntimeGrant,
) -> ManagedCloudResult<()> {
    if !managed_cloud_route_id(&input.grant_id)
        || !managed_cloud_base64url_32(&input.grant_token)
        || !managed_cloud_route_id(&input.runtime_instance_id)
        || !managed_cloud_route_id(&input.worker_id)
        || !managed_cloud_base64url_32(&input.session_token)
        || !managed_cloud_hex64(&input.runtime_identity_sha256)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let expected = derive_managed_cloud_runtime_session_token(
        &input.grant_token,
        &input.grant_id,
        &input.runtime_instance_id,
    )?;
    if expected
        .as_bytes()
        .ct_eq(input.session_token.as_bytes())
        .unwrap_u8()
        != 1
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    Ok(())
}

fn validate_managed_cloud_runtime_heartbeat(
    input: &ManagedCloudRuntimeHeartbeatInput,
) -> ManagedCloudResult<()> {
    if !managed_cloud_route_id(&input.runtime_instance_id)
        || !managed_cloud_route_id(&input.worker_id)
        || !managed_cloud_base64url_32(&input.session_token)
        || !managed_cloud_safe_integer(input.heartbeat_sequence, true)
        || !managed_cloud_safe_integer(input.observed_head_revision, true)
        || !managed_cloud_hex64(&input.observed_transition_sha256)
        || !managed_cloud_hex64(&input.activation_sha256)
        || !managed_cloud_hex64(&input.manifest_sha256)
        || !managed_cloud_token(&input.component_id, 128)
        || !managed_cloud_runtime_role(&input.role)
        || !managed_cloud_hex64(&input.artifact_sha256)
        || !managed_cloud_hex64(&input.migration_set_sha256)
        || !managed_cloud_hex64(&input.config_schema_sha256)
        || !managed_cloud_hex64(&input.protocol_set_sha256)
        || !managed_cloud_hex64(&input.task_queue_sha256)
        || !managed_cloud_hex64(&input.failure_converter_sha256)
        || !managed_cloud_hex64(&input.dependency_evidence_sha256)
        || !managed_cloud_health(&input.health_state, input.reason_code.as_deref())
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    if managed_cloud_dependency_evidence_sha256(
        &input.role,
        &input.activation_sha256,
        &input.manifest_sha256,
        &input.component_id,
        &input.artifact_sha256,
        &input.task_queue_sha256,
        &input.failure_converter_sha256,
    )? != input.dependency_evidence_sha256
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    Ok(())
}

fn managed_cloud_token_sha256(token: &str) -> String {
    managed_cloud_sha256(token.as_bytes())
}

fn managed_cloud_runtime_grant_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ManagedCloudRuntimeGrant> {
    Ok(ManagedCloudRuntimeGrant {
        grant_id: row.get(0)?,
        grant_token: None,
        token_sha256: row.get(1)?,
        issuance_ref: row.get(2)?,
        scope: ManagedCloudScope {
            environment: row.get(3)?,
            region: row.get(4)?,
            channel: row.get(5)?,
        },
        activation_sha256: row.get(6)?,
        manifest_sha256: row.get(7)?,
        component_id: row.get(8)?,
        role: row.get(9)?,
        head_revision: row.get(10)?,
        transition_sha256: row.get(11)?,
        artifact_sha256: row.get(12)?,
        config_schema_sha256: row.get(13)?,
        migration_set_sha256: row.get(14)?,
        protocol_set_sha256: row.get(15)?,
        task_queue_sha256: row.get(16)?,
        failure_converter_sha256: row.get(17)?,
        dependency_evidence_sha256: row.get(18)?,
        expected_runtime_identity_sha256: row.get(19)?,
        expected_worker_id: row.get(20)?,
        authorization_ref: row.get(21)?,
        created_by: row.get(22)?,
        activation_expires_at_ms: row.get(23)?,
        expires_at_ms: row.get(24)?,
        created_at_ms: row.get(25)?,
        replayed: false,
    })
}

fn managed_cloud_runtime_grant_from_postgres(row: &postgres::Row) -> ManagedCloudRuntimeGrant {
    ManagedCloudRuntimeGrant {
        grant_id: row.get(0),
        grant_token: None,
        token_sha256: row.get(1),
        issuance_ref: row.get(2),
        scope: ManagedCloudScope {
            environment: row.get(3),
            region: row.get(4),
            channel: row.get(5),
        },
        activation_sha256: row.get(6),
        manifest_sha256: row.get(7),
        component_id: row.get(8),
        role: row.get(9),
        head_revision: row.get(10),
        transition_sha256: row.get(11),
        artifact_sha256: row.get(12),
        config_schema_sha256: row.get(13),
        migration_set_sha256: row.get(14),
        protocol_set_sha256: row.get(15),
        task_queue_sha256: row.get(16),
        failure_converter_sha256: row.get(17),
        dependency_evidence_sha256: row.get(18),
        expected_runtime_identity_sha256: row.get(19),
        expected_worker_id: row.get(20),
        authorization_ref: row.get(21),
        created_by: row.get(22),
        activation_expires_at_ms: row.get(23),
        expires_at_ms: row.get(24),
        created_at_ms: row.get(25),
        replayed: false,
    }
}

const MANAGED_CLOUD_RUNTIME_GRANT_COLUMNS: &str =
    "grant_id,token_sha256,issuance_ref,environment,region,channel,activation_sha256,\
     manifest_sha256,component_id,role,head_revision,transition_sha256,\
     artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,\
     task_queue_sha256,failure_converter_sha256,expected_dependency_evidence_sha256,\
     expected_runtime_identity_sha256,\
     expected_worker_id,authorization_ref,created_by,activation_expires_at_ms,expires_at_ms,created_at_ms";

fn require_exact_managed_cloud_runtime_grant_replay(
    existing: &ManagedCloudRuntimeGrant,
    input: &NewManagedCloudRuntimeGrant,
) -> ManagedCloudResult<()> {
    if existing.issuance_ref != input.issuance_ref
        || existing.scope != input.scope
        || existing.activation_sha256 != input.activation_sha256
        || existing.manifest_sha256 != input.manifest_sha256
        || existing.component_id != input.component_id
        || existing.role != input.role
        || existing.expected_worker_id != input.expected_worker_id
        || existing.authorization_ref != input.authorization_ref
        || existing.created_by != input.created_by
        || existing.expires_at_ms - existing.created_at_ms != input.ttl_ms
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn sqlite_managed_cloud_runtime_grant_by_issuance_ref(
    tx: &rusqlite::Transaction<'_>,
    issuance_ref: &str,
) -> ManagedCloudResult<Option<ManagedCloudRuntimeGrant>> {
    tx.query_row(
        &format!(
            "SELECT {MANAGED_CLOUD_RUNTIME_GRANT_COLUMNS}
               FROM jobs_managed_cloud_runtime_grants WHERE issuance_ref=?1"
        ),
        params![issuance_ref],
        managed_cloud_runtime_grant_from_sqlite,
    )
    .optional()
    .map_err(managed_cloud_storage)
}

fn postgres_managed_cloud_runtime_grant_by_issuance_ref(
    tx: &mut postgres::Transaction<'_>,
    issuance_ref: &str,
) -> ManagedCloudResult<Option<ManagedCloudRuntimeGrant>> {
    tx.query_opt(
        &format!(
            "SELECT {MANAGED_CLOUD_RUNTIME_GRANT_COLUMNS}
               FROM jobs_managed_cloud_runtime_grants WHERE issuance_ref=$1 FOR UPDATE"
        ),
        &[&issuance_ref],
    )
    .map(|row| row.as_ref().map(managed_cloud_runtime_grant_from_postgres))
    .map_err(managed_cloud_storage)
}

fn sqlite_managed_cloud_runtime_grant_by_id(
    tx: &rusqlite::Transaction<'_>,
    grant_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudRuntimeGrant>> {
    tx.query_row(
        &format!(
            "SELECT {MANAGED_CLOUD_RUNTIME_GRANT_COLUMNS} \
               FROM jobs_managed_cloud_runtime_grants WHERE grant_id=?1"
        ),
        params![grant_id],
        managed_cloud_runtime_grant_from_sqlite,
    )
    .optional()
    .map_err(managed_cloud_storage)
}

fn postgres_managed_cloud_runtime_grant_by_id(
    tx: &mut postgres::Transaction<'_>,
    grant_id: &str,
) -> ManagedCloudResult<Option<ManagedCloudRuntimeGrant>> {
    tx.query_opt(
        &format!(
            "SELECT {MANAGED_CLOUD_RUNTIME_GRANT_COLUMNS} \
               FROM jobs_managed_cloud_runtime_grants WHERE grant_id=$1"
        ),
        &[&grant_id],
    )
    .map(|row| row.as_ref().map(managed_cloud_runtime_grant_from_postgres))
    .map_err(managed_cloud_storage)
}

fn managed_cloud_runtime_grant_result(
    mut grant: ManagedCloudRuntimeGrant,
    grant_token: Option<&str>,
    replayed: bool,
) -> ManagedCloudRuntimeGrant {
    grant.grant_token = grant_token.map(str::to_string);
    grant.replayed = replayed;
    grant
}

fn encrypt_managed_cloud_runtime_grant_token(
    grant_id: &str,
    issuance_ref: &str,
    grant_token: &str,
) -> ManagedCloudResult<String> {
    if !managed_cloud_route_id(grant_id)
        || !managed_cloud_route_id(issuance_ref)
        || !managed_cloud_base64url_32(grant_token)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let plain = format!(
        "{MANAGED_CLOUD_RUNTIME_GRANT_TOKEN_DOMAIN}\0{grant_id}\0{issuance_ref}\0{grant_token}"
    );
    encrypt_payload(&plain).map_err(managed_cloud_storage)
}

fn decrypt_managed_cloud_runtime_grant_token(
    ciphertext: &str,
    grant: &ManagedCloudRuntimeGrant,
) -> ManagedCloudResult<String> {
    let plain = decrypt_payload(ciphertext).map_err(managed_cloud_storage)?;
    let prefix = format!(
        "{MANAGED_CLOUD_RUNTIME_GRANT_TOKEN_DOMAIN}\0{}\0{}\0",
        grant.grant_id, grant.issuance_ref
    );
    let token = plain
        .strip_prefix(&prefix)
        .filter(|value| managed_cloud_base64url_32(value))
        .ok_or(ManagedCloudRegistryError::InvalidRequest)?;
    require_managed_cloud_secret_match(&grant.token_sha256, &managed_cloud_token_sha256(token))?;
    Ok(token.to_string())
}

fn sqlite_managed_cloud_replay_grant_token(
    tx: &rusqlite::Transaction<'_>,
    grant: &ManagedCloudRuntimeGrant,
    now_ms: i64,
) -> ManagedCloudResult<Option<String>> {
    let (ciphertext, consumed, revoked): (Option<String>, bool, bool) = tx
        .query_row(
            "SELECT grant.grant_token_ciphertext,
                    EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_instances instance
                            WHERE instance.grant_id=grant.grant_id),
                    EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_grant_revocations revoked
                            WHERE revoked.grant_id=grant.grant_id)
               FROM jobs_managed_cloud_runtime_grants grant WHERE grant.grant_id=?1",
            params![grant.grant_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(managed_cloud_storage)?;
    if grant.expires_at_ms <= now_ms || consumed || revoked {
        if ciphertext.is_some() {
            tx.execute(
                "UPDATE jobs_managed_cloud_runtime_grants
                    SET grant_token_ciphertext=NULL
                  WHERE grant_id=?1 AND grant_token_ciphertext IS NOT NULL",
                params![grant.grant_id],
            )
            .map_err(managed_cloud_storage)?;
        }
        if grant.expires_at_ms <= now_ms {
            return Err(ManagedCloudRegistryError::GrantExpired);
        }
        return Ok(None);
    }
    ciphertext
        .as_deref()
        .map(|value| decrypt_managed_cloud_runtime_grant_token(value, grant))
        .transpose()?
        .ok_or_else(|| {
            ManagedCloudRegistryError::Storage(anyhow::anyhow!(
                "unconsumed managed cloud runtime grant token was scrubbed"
            ))
        })
        .map(Some)
}

fn postgres_managed_cloud_replay_grant_token(
    tx: &mut postgres::Transaction<'_>,
    grant: &ManagedCloudRuntimeGrant,
    now_ms: i64,
) -> ManagedCloudResult<Option<String>> {
    let row = tx
        .query_one(
            "SELECT grant.grant_token_ciphertext,
                    EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_instances instance
                            WHERE instance.grant_id=grant.grant_id),
                    EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_grant_revocations revoked
                            WHERE revoked.grant_id=grant.grant_id)
               FROM jobs_managed_cloud_runtime_grants grant WHERE grant.grant_id=$1",
            &[&grant.grant_id],
        )
        .map_err(managed_cloud_storage)?;
    let ciphertext: Option<String> = row.get(0);
    let consumed: bool = row.get(1);
    let revoked: bool = row.get(2);
    if grant.expires_at_ms <= now_ms || consumed || revoked {
        if ciphertext.is_some() {
            tx.execute(
                "UPDATE jobs_managed_cloud_runtime_grants
                    SET grant_token_ciphertext=NULL
                  WHERE grant_id=$1 AND grant_token_ciphertext IS NOT NULL",
                &[&grant.grant_id],
            )
            .map_err(managed_cloud_storage)?;
        }
        if grant.expires_at_ms <= now_ms {
            return Err(ManagedCloudRegistryError::GrantExpired);
        }
        return Ok(None);
    }
    ciphertext
        .as_deref()
        .map(|value| decrypt_managed_cloud_runtime_grant_token(value, grant))
        .transpose()?
        .ok_or_else(|| {
            ManagedCloudRegistryError::Storage(anyhow::anyhow!(
                "unconsumed managed cloud runtime grant token was scrubbed"
            ))
        })
        .map(Some)
}

#[derive(Debug, Clone)]
struct ManagedCloudGrantAuthority {
    head_revision: i64,
    transition_sha256: String,
    artifact_sha256: String,
    config_schema_sha256: String,
    migration_set_sha256: String,
    protocol_set_sha256: String,
    task_queue_sha256: String,
    failure_converter_sha256: String,
    dependency_evidence_sha256: String,
    runtime_identity_sha256: String,
    activation_expires_at_ms: i64,
}

fn managed_cloud_grant_authority_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ManagedCloudGrantAuthority> {
    Ok(ManagedCloudGrantAuthority {
        head_revision: row.get(0)?,
        transition_sha256: row.get(1)?,
        artifact_sha256: row.get(2)?,
        config_schema_sha256: row.get(3)?,
        migration_set_sha256: row.get(4)?,
        protocol_set_sha256: row.get(5)?,
        task_queue_sha256: row.get(6)?,
        failure_converter_sha256: row.get(7)?,
        dependency_evidence_sha256: row.get(8)?,
        runtime_identity_sha256: row.get(9)?,
        activation_expires_at_ms: row.get(10)?,
    })
}

fn managed_cloud_grant_authority_from_postgres(row: &postgres::Row) -> ManagedCloudGrantAuthority {
    ManagedCloudGrantAuthority {
        head_revision: row.get(0),
        transition_sha256: row.get(1),
        artifact_sha256: row.get(2),
        config_schema_sha256: row.get(3),
        migration_set_sha256: row.get(4),
        protocol_set_sha256: row.get(5),
        task_queue_sha256: row.get(6),
        failure_converter_sha256: row.get(7),
        dependency_evidence_sha256: row.get(8),
        runtime_identity_sha256: row.get(9),
        activation_expires_at_ms: row.get(10),
    }
}

fn resolve_sqlite_managed_cloud_grant_authority(
    tx: &rusqlite::Transaction<'_>,
    input: &NewManagedCloudRuntimeGrant,
    now_ms: i64,
) -> ManagedCloudResult<ManagedCloudGrantAuthority> {
    tx.query_row(
        "SELECT head.head_revision,head.current_transition_sha256,
                component.artifact_sha256,manifest.config_schema_sha256,
                manifest.migration_set_sha256,manifest.protocol_set_sha256,
                activation.task_queue_sha256,activation.failure_converter_sha256,
                requirement.dependency_evidence_sha256,
                identity.runtime_identity_sha256,activation.expires_at_ms
           FROM jobs_managed_cloud_heads head
           JOIN jobs_managed_cloud_head_transitions transition
             ON transition.transition_sha256=head.current_transition_sha256
            AND transition.environment=head.environment AND transition.region=head.region
            AND transition.channel=head.channel AND transition.head_revision=head.head_revision
            AND transition.next_activation_sha256=head.current_activation_sha256
            AND transition.next_manifest_sha256=head.current_manifest_sha256
           JOIN jobs_managed_cloud_activations activation
             ON activation.activation_sha256=head.current_activation_sha256
            AND activation.manifest_sha256=head.current_manifest_sha256
            AND activation.environment=head.environment AND activation.region=head.region
            AND activation.channel=head.channel
           JOIN jobs_managed_cloud_manifests manifest
             ON manifest.manifest_sha256=activation.manifest_sha256
            AND manifest.trust_generation=activation.trust_generation
           JOIN jobs_managed_cloud_cohorts cohort
             ON cohort.cohort_sha256=activation.cohort_sha256
            AND cohort.trust_generation=activation.trust_generation
            AND cohort.environment=activation.environment AND cohort.region=activation.region
            AND cohort.channel=activation.channel
           JOIN jobs_managed_cloud_trust_policies policy
             ON policy.trust_generation=activation.trust_generation
           LEFT JOIN jobs_managed_cloud_rollbacks rollback
             ON rollback.rollback_sha256=transition.rollback_authority_sha256
            AND rollback.trust_generation=activation.trust_generation
            AND rollback.environment=activation.environment AND rollback.region=activation.region
            AND rollback.channel=activation.channel
           JOIN jobs_managed_cloud_manifest_capabilities capability
             ON capability.manifest_sha256=manifest.manifest_sha256
            AND capability.component_id=?7 AND capability.capability=?8
           JOIN jobs_managed_cloud_manifest_components component
             ON component.manifest_sha256=capability.manifest_sha256
            AND component.component_id=capability.component_id
           JOIN jobs_managed_cloud_activation_requirements requirement
             ON requirement.activation_sha256=activation.activation_sha256
            AND requirement.role=capability.capability
           JOIN jobs_managed_cloud_manifest_runtime_identities identity
             ON identity.manifest_sha256=manifest.manifest_sha256
            AND identity.component_id=capability.component_id
            AND identity.role=capability.capability
          WHERE head.environment=?1 AND head.region=?2 AND head.channel=?3
            AND activation.activation_sha256=?4 AND activation.manifest_sha256=?5
            AND activation.not_before_ms<=?6 AND activation.expires_at_ms>?6
            AND cohort.not_before_ms<=?6 AND cohort.expires_at_ms>?6
            AND policy.valid_from_ms<=?6 AND policy.expires_at_ms>?6
            AND activation.cloud_distribution_enabled=1
            AND activation.workflow_command_dispatch_enabled=1
            AND activation.workflow_cleanup_enabled=1
            AND activation.direct_discovery_enabled=0
            AND activation.global_discovery_enabled=0
            AND activation.source_verification_enabled=0
            AND (transition.transition_kind='activation'
                 OR rollback.rollback_sha256 IS NOT NULL)
            AND NOT EXISTS(
              SELECT 1 FROM jobs_managed_cloud_revocations revoked
               WHERE revoked.effective_at_ms<=?6 AND (
                 (revoked.subject_kind='activation'
                   AND revoked.subject_sha256=activation.activation_sha256)
                 OR (revoked.subject_kind='manifest'
                   AND revoked.subject_sha256=manifest.manifest_sha256)
                 OR (revoked.subject_kind='cohort'
                   AND revoked.subject_sha256=cohort.cohort_sha256)
                 OR (revoked.subject_kind='component'
                   AND revoked.subject_id=component.component_id
                   AND revoked.subject_sha256=component.artifact_sha256)
                 OR (revoked.subject_kind='release'
                   AND revoked.subject_id=manifest.release_id
                   AND revoked.subject_sha256=manifest.manifest_sha256)
                 OR (revoked.subject_kind='rollback'
                   AND rollback.rollback_sha256 IS NOT NULL
                   AND revoked.subject_id=rollback.rollback_id
                   AND revoked.subject_sha256=rollback.rollback_sha256)
                 OR (revoked.subject_kind='trust_policy'
                   AND revoked.subject_id=policy.policy_id
                   AND revoked.subject_sha256=policy.policy_sha256)
                 OR (revoked.subject_kind='signing_key' AND revoked.subject_id IN (
                   SELECT signature.key_id FROM jobs_managed_cloud_signatures signature
                    WHERE signature.signature_set_sha256 IN (
                      activation.authorization_signature_set_sha256,
                      manifest.authorization_signature_set_sha256,
                      cohort.authorization_signature_set_sha256,
                      policy.authorization_signature_set_sha256
                    ) OR signature.signature_set_sha256=
                         rollback.authorization_signature_set_sha256
                 ))
               )
            )",
        params![
            input.scope.environment,
            input.scope.region,
            input.scope.channel,
            input.activation_sha256,
            input.manifest_sha256,
            now_ms,
            input.component_id,
            input.role,
        ],
        managed_cloud_grant_authority_from_sqlite,
    )
    .optional()
    .map_err(managed_cloud_storage)?
    .ok_or(ManagedCloudRegistryError::Unavailable)
}

fn resolve_postgres_managed_cloud_grant_authority(
    tx: &mut postgres::Transaction<'_>,
    input: &NewManagedCloudRuntimeGrant,
    now_ms: i64,
) -> ManagedCloudResult<ManagedCloudGrantAuthority> {
    tx.query_opt(
        "SELECT head.head_revision,head.current_transition_sha256,
                component.artifact_sha256,manifest.config_schema_sha256,
                manifest.migration_set_sha256,manifest.protocol_set_sha256,
                activation.task_queue_sha256,activation.failure_converter_sha256,
                requirement.dependency_evidence_sha256,
                identity.runtime_identity_sha256,activation.expires_at_ms
           FROM jobs_managed_cloud_heads head
           JOIN jobs_managed_cloud_head_transitions transition
             ON transition.transition_sha256=head.current_transition_sha256
            AND transition.environment=head.environment AND transition.region=head.region
            AND transition.channel=head.channel AND transition.head_revision=head.head_revision
            AND transition.next_activation_sha256=head.current_activation_sha256
            AND transition.next_manifest_sha256=head.current_manifest_sha256
           JOIN jobs_managed_cloud_activations activation
             ON activation.activation_sha256=head.current_activation_sha256
            AND activation.manifest_sha256=head.current_manifest_sha256
            AND activation.environment=head.environment AND activation.region=head.region
            AND activation.channel=head.channel
           JOIN jobs_managed_cloud_manifests manifest
             ON manifest.manifest_sha256=activation.manifest_sha256
            AND manifest.trust_generation=activation.trust_generation
           JOIN jobs_managed_cloud_cohorts cohort
             ON cohort.cohort_sha256=activation.cohort_sha256
            AND cohort.trust_generation=activation.trust_generation
            AND cohort.environment=activation.environment AND cohort.region=activation.region
            AND cohort.channel=activation.channel
           JOIN jobs_managed_cloud_trust_policies policy
             ON policy.trust_generation=activation.trust_generation
           LEFT JOIN jobs_managed_cloud_rollbacks rollback
             ON rollback.rollback_sha256=transition.rollback_authority_sha256
            AND rollback.trust_generation=activation.trust_generation
            AND rollback.environment=activation.environment AND rollback.region=activation.region
            AND rollback.channel=activation.channel
           JOIN jobs_managed_cloud_manifest_capabilities capability
             ON capability.manifest_sha256=manifest.manifest_sha256
            AND capability.component_id=$7 AND capability.capability=$8
           JOIN jobs_managed_cloud_manifest_components component
             ON component.manifest_sha256=capability.manifest_sha256
            AND component.component_id=capability.component_id
           JOIN jobs_managed_cloud_activation_requirements requirement
             ON requirement.activation_sha256=activation.activation_sha256
            AND requirement.role=capability.capability
           JOIN jobs_managed_cloud_manifest_runtime_identities identity
             ON identity.manifest_sha256=manifest.manifest_sha256
            AND identity.component_id=capability.component_id
            AND identity.role=capability.capability
          WHERE head.environment=$1 AND head.region=$2 AND head.channel=$3
            AND activation.activation_sha256=$4 AND activation.manifest_sha256=$5
            AND activation.not_before_ms<=$6 AND activation.expires_at_ms>$6
            AND cohort.not_before_ms<=$6 AND cohort.expires_at_ms>$6
            AND policy.valid_from_ms<=$6 AND policy.expires_at_ms>$6
            AND activation.cloud_distribution_enabled
            AND activation.workflow_command_dispatch_enabled
            AND activation.workflow_cleanup_enabled
            AND NOT activation.direct_discovery_enabled
            AND NOT activation.global_discovery_enabled
            AND NOT activation.source_verification_enabled
            AND (transition.transition_kind='activation'
                 OR rollback.rollback_sha256 IS NOT NULL)
            AND NOT EXISTS(
              SELECT 1 FROM jobs_managed_cloud_revocations revoked
               WHERE revoked.effective_at_ms<=$6 AND (
                 (revoked.subject_kind='activation'
                   AND revoked.subject_sha256=activation.activation_sha256)
                 OR (revoked.subject_kind='manifest'
                   AND revoked.subject_sha256=manifest.manifest_sha256)
                 OR (revoked.subject_kind='cohort'
                   AND revoked.subject_sha256=cohort.cohort_sha256)
                 OR (revoked.subject_kind='component'
                   AND revoked.subject_id=component.component_id
                   AND revoked.subject_sha256=component.artifact_sha256)
                 OR (revoked.subject_kind='release'
                   AND revoked.subject_id=manifest.release_id
                   AND revoked.subject_sha256=manifest.manifest_sha256)
                 OR (revoked.subject_kind='rollback'
                   AND rollback.rollback_sha256 IS NOT NULL
                   AND revoked.subject_id=rollback.rollback_id
                   AND revoked.subject_sha256=rollback.rollback_sha256)
                 OR (revoked.subject_kind='trust_policy'
                   AND revoked.subject_id=policy.policy_id
                   AND revoked.subject_sha256=policy.policy_sha256)
                 OR (revoked.subject_kind='signing_key' AND revoked.subject_id IN (
                   SELECT signature.key_id FROM jobs_managed_cloud_signatures signature
                    WHERE signature.signature_set_sha256 IN (
                      activation.authorization_signature_set_sha256,
                      manifest.authorization_signature_set_sha256,
                      cohort.authorization_signature_set_sha256,
                      policy.authorization_signature_set_sha256
                    ) OR signature.signature_set_sha256=
                         rollback.authorization_signature_set_sha256
                 ))
               )
            )
          FOR UPDATE OF head",
        &[
            &input.scope.environment,
            &input.scope.region,
            &input.scope.channel,
            &input.activation_sha256,
            &input.manifest_sha256,
            &now_ms,
            &input.component_id,
            &input.role,
        ],
    )
    .map_err(managed_cloud_storage)?
    .map(|row| managed_cloud_grant_authority_from_postgres(&row))
    .ok_or(ManagedCloudRegistryError::Unavailable)
}

pub fn issue_managed_cloud_runtime_grant(
    pool: &DbPool,
    input: &NewManagedCloudRuntimeGrant,
) -> ManagedCloudResult<ManagedCloudRuntimeGrant> {
    validate_new_managed_cloud_runtime_grant(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            if let Some(existing) =
                sqlite_managed_cloud_runtime_grant_by_issuance_ref(&tx, &input.issuance_ref)?
            {
                require_exact_managed_cloud_runtime_grant_replay(&existing, input)?;
                let now_ms = managed_cloud_db_now_sqlite(&tx)?;
                let grant_token =
                    match sqlite_managed_cloud_replay_grant_token(&tx, &existing, now_ms) {
                        Ok(token) => token,
                        Err(ManagedCloudRegistryError::GrantExpired) => {
                            tx.commit().map_err(managed_cloud_storage)?;
                            return Err(ManagedCloudRegistryError::GrantExpired);
                        }
                        Err(error) => return Err(error),
                    };
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_runtime_grant_result(
                    existing,
                    grant_token.as_deref(),
                    true,
                ));
            }
            let (grant_id, grant_token) = generate_managed_cloud_runtime_grant_credentials();
            let token_sha256 = managed_cloud_token_sha256(&grant_token);
            let grant_token_ciphertext = encrypt_managed_cloud_runtime_grant_token(
                &grant_id,
                &input.issuance_ref,
                &grant_token,
            )?;
            if tx
                .query_row(
                    "SELECT 1 FROM jobs_managed_cloud_runtime_grants WHERE token_sha256=?1",
                    params![token_sha256],
                    |_| Ok(()),
                )
                .optional()
                .map_err(managed_cloud_storage)?
                .is_some()
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            let derived = resolve_sqlite_managed_cloud_grant_authority(&tx, input, now_ms)?;
            let expires_at_ms = now_ms
                .checked_add(input.ttl_ms)
                .filter(|expiry| {
                    *expiry <= MANAGED_CLOUD_MAX_SAFE_INTEGER
                        && *expiry <= derived.activation_expires_at_ms
                })
                .ok_or(ManagedCloudRegistryError::InvalidRequest)?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_runtime_grants(
                   grant_id,token_sha256,grant_token_ciphertext,issuance_ref,
                   environment,region,channel,activation_sha256,
                   manifest_sha256,component_id,role,head_revision,transition_sha256,
                   artifact_sha256,config_schema_sha256,migration_set_sha256,
                   protocol_set_sha256,task_queue_sha256,failure_converter_sha256,
                   expected_dependency_evidence_sha256,
                   expected_runtime_identity_sha256,expected_worker_id,authorization_ref,
                   created_by,activation_expires_at_ms,expires_at_ms,created_at_ms
                 ) VALUES(
                   ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,
                   ?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27
                 )",
                params![
                    grant_id,
                    token_sha256,
                    grant_token_ciphertext,
                    input.issuance_ref,
                    input.scope.environment,
                    input.scope.region,
                    input.scope.channel,
                    input.activation_sha256,
                    input.manifest_sha256,
                    input.component_id,
                    input.role,
                    derived.head_revision,
                    derived.transition_sha256,
                    derived.artifact_sha256,
                    derived.config_schema_sha256,
                    derived.migration_set_sha256,
                    derived.protocol_set_sha256,
                    derived.task_queue_sha256,
                    derived.failure_converter_sha256,
                    derived.dependency_evidence_sha256,
                    derived.runtime_identity_sha256,
                    input.expected_worker_id,
                    input.authorization_ref,
                    input.created_by,
                    derived.activation_expires_at_ms,
                    expires_at_ms,
                    now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            let grant = sqlite_managed_cloud_runtime_grant_by_id(&tx, &grant_id)?.ok_or(
                ManagedCloudRegistryError::Storage(anyhow::anyhow!(
                    "managed cloud runtime grant insert disappeared"
                )),
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_runtime_grant_result(
                grant,
                Some(&grant_token),
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            if let Some(existing) =
                postgres_managed_cloud_runtime_grant_by_issuance_ref(&mut tx, &input.issuance_ref)?
            {
                require_exact_managed_cloud_runtime_grant_replay(&existing, input)?;
                let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
                let grant_token =
                    match postgres_managed_cloud_replay_grant_token(&mut tx, &existing, now_ms) {
                        Ok(token) => token,
                        Err(ManagedCloudRegistryError::GrantExpired) => {
                            tx.commit().map_err(managed_cloud_storage)?;
                            return Err(ManagedCloudRegistryError::GrantExpired);
                        }
                        Err(error) => return Err(error),
                    };
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_runtime_grant_result(
                    existing,
                    grant_token.as_deref(),
                    true,
                ));
            }
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                   hashtextextended('jobs-managed-cloud-release-registry',0))",
                &[],
            )
            .map_err(managed_cloud_storage)?;
            let lock_key = format!(
                "managed-cloud:{}:{}:{}",
                input.scope.environment, input.scope.region, input.scope.channel
            );
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
                &[&lock_key],
            )
            .map_err(managed_cloud_storage)?;
            if let Some(existing) =
                postgres_managed_cloud_runtime_grant_by_issuance_ref(&mut tx, &input.issuance_ref)?
            {
                require_exact_managed_cloud_runtime_grant_replay(&existing, input)?;
                let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
                let grant_token =
                    match postgres_managed_cloud_replay_grant_token(&mut tx, &existing, now_ms) {
                        Ok(token) => token,
                        Err(ManagedCloudRegistryError::GrantExpired) => {
                            tx.commit().map_err(managed_cloud_storage)?;
                            return Err(ManagedCloudRegistryError::GrantExpired);
                        }
                        Err(error) => return Err(error),
                    };
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(managed_cloud_runtime_grant_result(
                    existing,
                    grant_token.as_deref(),
                    true,
                ));
            }
            let (grant_id, grant_token) = generate_managed_cloud_runtime_grant_credentials();
            let token_sha256 = managed_cloud_token_sha256(&grant_token);
            let grant_token_ciphertext = encrypt_managed_cloud_runtime_grant_token(
                &grant_id,
                &input.issuance_ref,
                &grant_token,
            )?;
            if tx
                .query_opt(
                    "SELECT 1 FROM jobs_managed_cloud_runtime_grants WHERE token_sha256=$1",
                    &[&token_sha256],
                )
                .map_err(managed_cloud_storage)?
                .is_some()
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            let derived = resolve_postgres_managed_cloud_grant_authority(&mut tx, input, now_ms)?;
            let expires_at_ms = now_ms
                .checked_add(input.ttl_ms)
                .filter(|expiry| {
                    *expiry <= MANAGED_CLOUD_MAX_SAFE_INTEGER
                        && *expiry <= derived.activation_expires_at_ms
                })
                .ok_or(ManagedCloudRegistryError::InvalidRequest)?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_runtime_grants(
                   grant_id,token_sha256,grant_token_ciphertext,issuance_ref,
                   environment,region,channel,activation_sha256,
                   manifest_sha256,component_id,role,head_revision,transition_sha256,
                   artifact_sha256,config_schema_sha256,migration_set_sha256,
                   protocol_set_sha256,task_queue_sha256,failure_converter_sha256,
                   expected_dependency_evidence_sha256,
                   expected_runtime_identity_sha256,expected_worker_id,authorization_ref,
                   created_by,activation_expires_at_ms,expires_at_ms,created_at_ms
                 ) VALUES(
                   $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
                   $17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27
                 )",
                &[
                    &grant_id,
                    &token_sha256,
                    &grant_token_ciphertext,
                    &input.issuance_ref,
                    &input.scope.environment,
                    &input.scope.region,
                    &input.scope.channel,
                    &input.activation_sha256,
                    &input.manifest_sha256,
                    &input.component_id,
                    &input.role,
                    &derived.head_revision,
                    &derived.transition_sha256,
                    &derived.artifact_sha256,
                    &derived.config_schema_sha256,
                    &derived.migration_set_sha256,
                    &derived.protocol_set_sha256,
                    &derived.task_queue_sha256,
                    &derived.failure_converter_sha256,
                    &derived.dependency_evidence_sha256,
                    &derived.runtime_identity_sha256,
                    &input.expected_worker_id,
                    &input.authorization_ref,
                    &input.created_by,
                    &derived.activation_expires_at_ms,
                    &expires_at_ms,
                    &now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            let grant = postgres_managed_cloud_runtime_grant_by_id(&mut tx, &grant_id)?.ok_or(
                ManagedCloudRegistryError::Storage(anyhow::anyhow!(
                    "managed cloud runtime grant insert disappeared"
                )),
            )?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(managed_cloud_runtime_grant_result(
                grant,
                Some(&grant_token),
                false,
            ))
        }
    })
}

pub fn revoke_managed_cloud_runtime_grant(
    pool: &DbPool,
    input: &RevokeManagedCloudRuntimeGrant,
) -> ManagedCloudResult<ManagedCloudRuntimeGrantRevocation> {
    if !managed_cloud_route_id(&input.grant_id)
        || !managed_cloud_route_id(&input.reason_ref)
        || !managed_cloud_actor(&input.revoked_by)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let existing = tx
                .query_row(
                    "SELECT reason_ref,revoked_by,revoked_at_ms
                       FROM jobs_managed_cloud_runtime_grant_revocations
                      WHERE grant_id=?1",
                    params![input.grant_id],
                    |row| {
                        Ok(ManagedCloudRuntimeGrantRevocation {
                            grant_id: input.grant_id.clone(),
                            reason_ref: row.get(0)?,
                            revoked_by: row.get(1)?,
                            revoked_at_ms: row.get(2)?,
                            replayed: true,
                        })
                    },
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            if let Some(existing) = existing {
                if existing.reason_ref != input.reason_ref
                    || existing.revoked_by != input.revoked_by
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                tx.execute(
                    "UPDATE jobs_managed_cloud_runtime_grants
                        SET grant_token_ciphertext=NULL
                      WHERE grant_id=?1 AND grant_token_ciphertext IS NOT NULL",
                    params![input.grant_id],
                )
                .map_err(managed_cloud_storage)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(existing);
            }
            if sqlite_managed_cloud_runtime_grant_by_id(&tx, &input.grant_id)?.is_none() {
                return Err(ManagedCloudRegistryError::NotFound);
            }
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_runtime_grant_revocations(
                   grant_id,reason_ref,revoked_by,revoked_at_ms
                 ) VALUES(?1,?2,?3,?4)",
                params![input.grant_id, input.reason_ref, input.revoked_by, now_ms],
            )
            .map_err(managed_cloud_storage)?;
            tx.execute(
                "UPDATE jobs_managed_cloud_runtime_grants
                    SET grant_token_ciphertext=NULL
                  WHERE grant_id=?1 AND grant_token_ciphertext IS NOT NULL",
                params![input.grant_id],
            )
            .map_err(managed_cloud_storage)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(ManagedCloudRuntimeGrantRevocation {
                grant_id: input.grant_id.clone(),
                reason_ref: input.reason_ref.clone(),
                revoked_by: input.revoked_by.clone(),
                revoked_at_ms: now_ms,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            if tx
                .query_opt(
                    "SELECT 1 FROM jobs_managed_cloud_runtime_grants
                      WHERE grant_id=$1 FOR UPDATE",
                    &[&input.grant_id],
                )
                .map_err(managed_cloud_storage)?
                .is_none()
            {
                return Err(ManagedCloudRegistryError::NotFound);
            }
            let existing = tx
                .query_opt(
                    "SELECT reason_ref,revoked_by,revoked_at_ms
                       FROM jobs_managed_cloud_runtime_grant_revocations
                      WHERE grant_id=$1",
                    &[&input.grant_id],
                )
                .map_err(managed_cloud_storage)?;
            if let Some(row) = existing {
                let existing = ManagedCloudRuntimeGrantRevocation {
                    grant_id: input.grant_id.clone(),
                    reason_ref: row.get(0),
                    revoked_by: row.get(1),
                    revoked_at_ms: row.get(2),
                    replayed: true,
                };
                if existing.reason_ref != input.reason_ref
                    || existing.revoked_by != input.revoked_by
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                tx.execute(
                    "UPDATE jobs_managed_cloud_runtime_grants
                        SET grant_token_ciphertext=NULL
                      WHERE grant_id=$1 AND grant_token_ciphertext IS NOT NULL",
                    &[&input.grant_id],
                )
                .map_err(managed_cloud_storage)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(existing);
            }
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_runtime_grant_revocations(
                   grant_id,reason_ref,revoked_by,revoked_at_ms
                 ) VALUES($1,$2,$3,$4)",
                &[
                    &input.grant_id,
                    &input.reason_ref,
                    &input.revoked_by,
                    &now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            tx.execute(
                "UPDATE jobs_managed_cloud_runtime_grants
                    SET grant_token_ciphertext=NULL
                  WHERE grant_id=$1 AND grant_token_ciphertext IS NOT NULL",
                &[&input.grant_id],
            )
            .map_err(managed_cloud_storage)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(ManagedCloudRuntimeGrantRevocation {
                grant_id: input.grant_id.clone(),
                reason_ref: input.reason_ref.clone(),
                revoked_by: input.revoked_by.clone(),
                revoked_at_ms: now_ms,
                replayed: false,
            })
        }
    })
}

const MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS: &str =
    "grant_id,runtime_instance_id,runtime_identity_sha256,worker_id,environment,region,channel,\
     activation_sha256,manifest_sha256,component_id,role,head_revision,transition_sha256,\
     artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,\
     task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,\
     activation_expires_at_ms,instance_epoch,claimed_at_ms,\
     session_proof_hmac_sha256";

fn managed_cloud_runtime_instance_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(ManagedCloudRuntimeInstance, String)> {
    Ok((
        ManagedCloudRuntimeInstance {
            grant_id: row.get(0)?,
            runtime_instance_id: row.get(1)?,
            runtime_identity_sha256: row.get(2)?,
            worker_id: row.get(3)?,
            scope: ManagedCloudScope {
                environment: row.get(4)?,
                region: row.get(5)?,
                channel: row.get(6)?,
            },
            activation_sha256: row.get(7)?,
            manifest_sha256: row.get(8)?,
            component_id: row.get(9)?,
            role: row.get(10)?,
            head_revision: row.get(11)?,
            transition_sha256: row.get(12)?,
            artifact_sha256: row.get(13)?,
            config_schema_sha256: row.get(14)?,
            migration_set_sha256: row.get(15)?,
            protocol_set_sha256: row.get(16)?,
            task_queue_sha256: row.get(17)?,
            failure_converter_sha256: row.get(18)?,
            dependency_evidence_sha256: row.get(19)?,
            activation_expires_at_ms: row.get(20)?,
            instance_epoch: row.get(21)?,
            next_heartbeat_sequence: 1,
            claimed_at_ms: row.get(22)?,
            replayed: false,
        },
        row.get(23)?,
    ))
}

fn managed_cloud_runtime_instance_from_postgres(
    row: &postgres::Row,
) -> (ManagedCloudRuntimeInstance, String) {
    (
        ManagedCloudRuntimeInstance {
            grant_id: row.get(0),
            runtime_instance_id: row.get(1),
            runtime_identity_sha256: row.get(2),
            worker_id: row.get(3),
            scope: ManagedCloudScope {
                environment: row.get(4),
                region: row.get(5),
                channel: row.get(6),
            },
            activation_sha256: row.get(7),
            manifest_sha256: row.get(8),
            component_id: row.get(9),
            role: row.get(10),
            head_revision: row.get(11),
            transition_sha256: row.get(12),
            artifact_sha256: row.get(13),
            config_schema_sha256: row.get(14),
            migration_set_sha256: row.get(15),
            protocol_set_sha256: row.get(16),
            task_queue_sha256: row.get(17),
            failure_converter_sha256: row.get(18),
            dependency_evidence_sha256: row.get(19),
            activation_expires_at_ms: row.get(20),
            instance_epoch: row.get(21),
            next_heartbeat_sequence: 1,
            claimed_at_ms: row.get(22),
            replayed: false,
        },
        row.get(23),
    )
}

fn require_managed_cloud_secret_match(actual: &str, expected: &str) -> ManagedCloudResult<()> {
    if actual.len() != expected.len()
        || actual.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() != 1
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    Ok(())
}

fn require_exact_managed_cloud_runtime_instance_replay(
    existing: &(ManagedCloudRuntimeInstance, String),
    input: &ClaimManagedCloudRuntimeGrant,
) -> ManagedCloudResult<()> {
    let instance = &existing.0;
    if instance.grant_id != input.grant_id
        || instance.runtime_instance_id != input.runtime_instance_id
        || instance.runtime_identity_sha256 != input.runtime_identity_sha256
        || instance.worker_id != input.worker_id
    {
        return Err(ManagedCloudRegistryError::GrantConsumed);
    }
    let proof = managed_cloud_runtime_session_proof_hmac(
        &input.session_token,
        &input.grant_id,
        &input.worker_id,
        &input.runtime_instance_id,
        instance.instance_epoch,
    )?;
    require_managed_cloud_secret_match(&existing.1, &proof)
}

fn require_sqlite_managed_cloud_grant_authority_active(
    tx: &rusqlite::Transaction<'_>,
    grant: &ManagedCloudRuntimeGrant,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    let row = tx
        .query_row(
            "SELECT activation.not_before_ms,activation.expires_at_ms,
                    cohort.not_before_ms,cohort.expires_at_ms,
                    policy.valid_from_ms,policy.expires_at_ms,
                    head.current_transition_sha256,
                    EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_grant_revocations direct
                            WHERE direct.grant_id=grant.grant_id)
                    OR EXISTS(
                      SELECT 1 FROM jobs_managed_cloud_revocations revoked
                       WHERE revoked.effective_at_ms<=?2 AND (
                         (revoked.subject_kind='runtime_grant'
                           AND revoked.subject_id=grant.grant_id
                           AND revoked.subject_sha256=grant.token_sha256)
                         OR (revoked.subject_kind='activation'
                           AND revoked.subject_sha256=activation.activation_sha256)
                         OR (revoked.subject_kind='manifest'
                           AND revoked.subject_sha256=manifest.manifest_sha256)
                         OR (revoked.subject_kind='cohort'
                           AND revoked.subject_sha256=cohort.cohort_sha256)
                         OR (revoked.subject_kind='component'
                           AND revoked.subject_id=grant.component_id
                           AND revoked.subject_sha256=grant.artifact_sha256)
                         OR (revoked.subject_kind='release'
                           AND revoked.subject_id=manifest.release_id
                           AND revoked.subject_sha256=manifest.manifest_sha256)
                         OR (revoked.subject_kind='rollback'
                           AND rollback.rollback_sha256 IS NOT NULL
                           AND revoked.subject_id=rollback.rollback_id
                           AND revoked.subject_sha256=rollback.rollback_sha256)
                         OR (revoked.subject_kind='trust_policy'
                           AND revoked.subject_id=policy.policy_id
                           AND revoked.subject_sha256=policy.policy_sha256)
                         OR (revoked.subject_kind='signing_key' AND revoked.subject_id IN (
                           SELECT signature.key_id
                             FROM jobs_managed_cloud_signatures signature
                            WHERE signature.signature_set_sha256 IN (
                              activation.authorization_signature_set_sha256,
                              manifest.authorization_signature_set_sha256,
                              cohort.authorization_signature_set_sha256,
                              policy.authorization_signature_set_sha256
                            )
                            OR signature.signature_set_sha256=
                               rollback.authorization_signature_set_sha256
                         ))
                       )
                    ) AS revoked
               FROM jobs_managed_cloud_runtime_grants grant
               JOIN jobs_managed_cloud_activations activation
                 ON activation.activation_sha256=grant.activation_sha256
                AND activation.manifest_sha256=grant.manifest_sha256
               JOIN jobs_managed_cloud_manifests manifest
                 ON manifest.manifest_sha256=grant.manifest_sha256
               JOIN jobs_managed_cloud_cohorts cohort
                 ON cohort.cohort_sha256=activation.cohort_sha256
                AND cohort.trust_generation=activation.trust_generation
                AND cohort.environment=activation.environment
                AND cohort.region=activation.region AND cohort.channel=activation.channel
               JOIN jobs_managed_cloud_trust_policies policy
                 ON policy.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_heads head
                 ON head.environment=grant.environment AND head.region=grant.region
                AND head.channel=grant.channel AND head.head_revision=grant.head_revision
                AND head.current_transition_sha256=grant.transition_sha256
                AND head.current_activation_sha256=grant.activation_sha256
                AND head.current_manifest_sha256=grant.manifest_sha256
               JOIN jobs_managed_cloud_head_transitions transition
                 ON transition.transition_sha256=head.current_transition_sha256
                AND transition.environment=head.environment
                AND transition.region=head.region AND transition.channel=head.channel
                AND transition.head_revision=head.head_revision
               LEFT JOIN jobs_managed_cloud_rollbacks rollback
                 ON rollback.rollback_sha256=transition.rollback_authority_sha256
                AND rollback.trust_generation=activation.trust_generation
              WHERE grant.grant_id=?1
                AND (transition.transition_kind='activation'
                     OR rollback.rollback_sha256 IS NOT NULL)
                AND activation.cloud_distribution_enabled=1
                AND activation.workflow_command_dispatch_enabled=1
                AND activation.workflow_cleanup_enabled=1
                AND activation.direct_discovery_enabled=0
                AND activation.global_discovery_enabled=0
                AND activation.source_verification_enabled=0",
            params![grant.grant_id, now_ms],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, bool>(7)?,
                ))
            },
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::Unavailable)?;
    if row.7 {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    if row.0 > now_ms
        || row.1 <= now_ms
        || row.2 > now_ms
        || row.3 <= now_ms
        || row.4 > now_ms
        || row.5 <= now_ms
        || grant.activation_expires_at_ms != row.1
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(())
}

fn require_postgres_managed_cloud_grant_authority_active(
    tx: &mut postgres::Transaction<'_>,
    grant: &ManagedCloudRuntimeGrant,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    let row = tx
        .query_opt(
            "SELECT activation.not_before_ms,activation.expires_at_ms,
                    cohort.not_before_ms,cohort.expires_at_ms,
                    policy.valid_from_ms,policy.expires_at_ms,
                    head.current_transition_sha256,
                    EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_grant_revocations direct
                            WHERE direct.grant_id=grant.grant_id)
                    OR EXISTS(
                      SELECT 1 FROM jobs_managed_cloud_revocations revoked
                       WHERE revoked.effective_at_ms<=$2 AND (
                         (revoked.subject_kind='runtime_grant'
                           AND revoked.subject_id=grant.grant_id
                           AND revoked.subject_sha256=grant.token_sha256)
                         OR (revoked.subject_kind='activation'
                           AND revoked.subject_sha256=activation.activation_sha256)
                         OR (revoked.subject_kind='manifest'
                           AND revoked.subject_sha256=manifest.manifest_sha256)
                         OR (revoked.subject_kind='cohort'
                           AND revoked.subject_sha256=cohort.cohort_sha256)
                         OR (revoked.subject_kind='component'
                           AND revoked.subject_id=grant.component_id
                           AND revoked.subject_sha256=grant.artifact_sha256)
                         OR (revoked.subject_kind='release'
                           AND revoked.subject_id=manifest.release_id
                           AND revoked.subject_sha256=manifest.manifest_sha256)
                         OR (revoked.subject_kind='rollback'
                           AND rollback.rollback_sha256 IS NOT NULL
                           AND revoked.subject_id=rollback.rollback_id
                           AND revoked.subject_sha256=rollback.rollback_sha256)
                         OR (revoked.subject_kind='trust_policy'
                           AND revoked.subject_id=policy.policy_id
                           AND revoked.subject_sha256=policy.policy_sha256)
                         OR (revoked.subject_kind='signing_key' AND revoked.subject_id IN (
                           SELECT signature.key_id
                             FROM jobs_managed_cloud_signatures signature
                            WHERE signature.signature_set_sha256 IN (
                              activation.authorization_signature_set_sha256,
                              manifest.authorization_signature_set_sha256,
                              cohort.authorization_signature_set_sha256,
                              policy.authorization_signature_set_sha256
                            )
                            OR signature.signature_set_sha256=
                               rollback.authorization_signature_set_sha256
                         ))
                       )
                    ) AS revoked
               FROM jobs_managed_cloud_runtime_grants grant
               JOIN jobs_managed_cloud_activations activation
                 ON activation.activation_sha256=grant.activation_sha256
                AND activation.manifest_sha256=grant.manifest_sha256
               JOIN jobs_managed_cloud_manifests manifest
                 ON manifest.manifest_sha256=grant.manifest_sha256
               JOIN jobs_managed_cloud_cohorts cohort
                 ON cohort.cohort_sha256=activation.cohort_sha256
                AND cohort.trust_generation=activation.trust_generation
                AND cohort.environment=activation.environment
                AND cohort.region=activation.region AND cohort.channel=activation.channel
               JOIN jobs_managed_cloud_trust_policies policy
                 ON policy.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_heads head
                 ON head.environment=grant.environment AND head.region=grant.region
                AND head.channel=grant.channel AND head.head_revision=grant.head_revision
                AND head.current_transition_sha256=grant.transition_sha256
                AND head.current_activation_sha256=grant.activation_sha256
                AND head.current_manifest_sha256=grant.manifest_sha256
               JOIN jobs_managed_cloud_head_transitions transition
                 ON transition.transition_sha256=head.current_transition_sha256
                AND transition.environment=head.environment
                AND transition.region=head.region AND transition.channel=head.channel
                AND transition.head_revision=head.head_revision
               LEFT JOIN jobs_managed_cloud_rollbacks rollback
                 ON rollback.rollback_sha256=transition.rollback_authority_sha256
                AND rollback.trust_generation=activation.trust_generation
              WHERE grant.grant_id=$1
                AND (transition.transition_kind='activation'
                     OR rollback.rollback_sha256 IS NOT NULL)
                AND activation.cloud_distribution_enabled
                AND activation.workflow_command_dispatch_enabled
                AND activation.workflow_cleanup_enabled
                AND NOT activation.direct_discovery_enabled
                AND NOT activation.global_discovery_enabled
                AND NOT activation.source_verification_enabled
              FOR SHARE OF head",
            &[&grant.grant_id, &now_ms],
        )
        .map_err(managed_cloud_storage)?
        .ok_or(ManagedCloudRegistryError::Unavailable)?;
    if row.get::<_, bool>(7) {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    let not_before_ms: i64 = row.get(0);
    let expires_at_ms: i64 = row.get(1);
    let cohort_from_ms: i64 = row.get(2);
    let cohort_expires_ms: i64 = row.get(3);
    let policy_from_ms: i64 = row.get(4);
    let policy_expires_ms: i64 = row.get(5);
    if not_before_ms > now_ms
        || expires_at_ms <= now_ms
        || cohort_from_ms > now_ms
        || cohort_expires_ms <= now_ms
        || policy_from_ms > now_ms
        || policy_expires_ms <= now_ms
        || grant.activation_expires_at_ms != expires_at_ms
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(())
}

pub fn claim_managed_cloud_runtime_grant(
    pool: &DbPool,
    input: &ClaimManagedCloudRuntimeGrant,
) -> ManagedCloudResult<ManagedCloudRuntimeInstance> {
    validate_claim_managed_cloud_runtime_grant(input)?;
    let token_sha256 = managed_cloud_token_sha256(&input.grant_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let grant = sqlite_managed_cloud_runtime_grant_by_id(&tx, &input.grant_id)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            require_managed_cloud_secret_match(&grant.token_sha256, &token_sha256)?;
            if grant.expected_runtime_identity_sha256 != input.runtime_identity_sha256
                || grant.expected_worker_id != input.worker_id
            {
                return Err(ManagedCloudRegistryError::InvalidRequest);
            }
            let existing = tx
                .query_row(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS}
                           FROM jobs_managed_cloud_runtime_instances WHERE grant_id=?1"
                    ),
                    params![input.grant_id],
                    managed_cloud_runtime_instance_from_sqlite,
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            if let Some(mut existing) = existing {
                require_exact_managed_cloud_runtime_instance_replay(&existing, input)?;
                tx.execute(
                    "UPDATE jobs_managed_cloud_runtime_grants
                        SET grant_token_ciphertext=NULL
                      WHERE grant_id=?1 AND grant_token_ciphertext IS NOT NULL",
                    params![input.grant_id],
                )
                .map_err(managed_cloud_storage)?;
                existing.0.replayed = true;
                existing.0.next_heartbeat_sequence = tx
                    .query_row(
                        "SELECT COALESCE((
                           SELECT heartbeat_sequence+1
                             FROM jobs_managed_cloud_runtime_heartbeats
                            WHERE runtime_instance_id=?1 AND instance_epoch=?2
                         ),1)",
                        params![existing.0.runtime_instance_id, existing.0.instance_epoch],
                        |row| row.get(0),
                    )
                    .map_err(managed_cloud_storage)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(existing.0);
            }
            if tx
                .query_row(
                    "SELECT 1 FROM jobs_managed_cloud_runtime_instances
                      WHERE runtime_instance_id=?1",
                    params![input.runtime_instance_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(managed_cloud_storage)?
                .is_some()
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            if grant.expires_at_ms <= now_ms {
                tx.execute(
                    "UPDATE jobs_managed_cloud_runtime_grants
                        SET grant_token_ciphertext=NULL
                      WHERE grant_id=?1 AND grant_token_ciphertext IS NOT NULL",
                    params![input.grant_id],
                )
                .map_err(managed_cloud_storage)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Err(ManagedCloudRegistryError::GrantExpired);
            }
            require_sqlite_managed_cloud_grant_authority_active(&tx, &grant, now_ms)?;
            let instance_epoch = 1_i64;
            let session_proof_hmac_sha256 = managed_cloud_runtime_session_proof_hmac(
                &input.session_token,
                &input.grant_id,
                &input.worker_id,
                &input.runtime_instance_id,
                instance_epoch,
            )?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_runtime_instances(
                   grant_id,runtime_instance_id,runtime_identity_sha256,worker_id,
                   session_proof_hmac_sha256,environment,region,channel,activation_sha256,
                   manifest_sha256,component_id,role,head_revision,transition_sha256,
                   artifact_sha256,config_schema_sha256,migration_set_sha256,
                   protocol_set_sha256,task_queue_sha256,failure_converter_sha256,
                   dependency_evidence_sha256,activation_expires_at_ms,
                   instance_epoch,claimed_at_ms
                 ) VALUES(
                   ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                   ?18,?19,?20,?21,?22,?23,?24
                 )",
                params![
                    grant.grant_id,
                    input.runtime_instance_id,
                    input.runtime_identity_sha256,
                    input.worker_id,
                    session_proof_hmac_sha256,
                    grant.scope.environment,
                    grant.scope.region,
                    grant.scope.channel,
                    grant.activation_sha256,
                    grant.manifest_sha256,
                    grant.component_id,
                    grant.role,
                    grant.head_revision,
                    grant.transition_sha256,
                    grant.artifact_sha256,
                    grant.config_schema_sha256,
                    grant.migration_set_sha256,
                    grant.protocol_set_sha256,
                    grant.task_queue_sha256,
                    grant.failure_converter_sha256,
                    grant.dependency_evidence_sha256,
                    grant.activation_expires_at_ms,
                    instance_epoch,
                    now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            tx.execute(
                "UPDATE jobs_managed_cloud_runtime_grants
                    SET grant_token_ciphertext=NULL WHERE grant_id=?1",
                params![input.grant_id],
            )
            .map_err(managed_cloud_storage)?;
            let mut instance = tx
                .query_row(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS}
                           FROM jobs_managed_cloud_runtime_instances WHERE grant_id=?1"
                    ),
                    params![input.grant_id],
                    managed_cloud_runtime_instance_from_sqlite,
                )
                .map_err(managed_cloud_storage)?
                .0;
            instance.replayed = false;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(instance)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            let grant_row = tx
                .query_opt(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_GRANT_COLUMNS}
                           FROM jobs_managed_cloud_runtime_grants
                          WHERE grant_id=$1 FOR UPDATE"
                    ),
                    &[&input.grant_id],
                )
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            let grant = managed_cloud_runtime_grant_from_postgres(&grant_row);
            require_managed_cloud_secret_match(&grant.token_sha256, &token_sha256)?;
            if grant.expected_runtime_identity_sha256 != input.runtime_identity_sha256
                || grant.expected_worker_id != input.worker_id
            {
                return Err(ManagedCloudRegistryError::InvalidRequest);
            }
            let existing = tx
                .query_opt(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS}
                           FROM jobs_managed_cloud_runtime_instances
                          WHERE grant_id=$1 FOR UPDATE"
                    ),
                    &[&input.grant_id],
                )
                .map_err(managed_cloud_storage)?;
            if let Some(row) = existing {
                let mut existing = managed_cloud_runtime_instance_from_postgres(&row);
                require_exact_managed_cloud_runtime_instance_replay(&existing, input)?;
                tx.execute(
                    "UPDATE jobs_managed_cloud_runtime_grants
                        SET grant_token_ciphertext=NULL
                      WHERE grant_id=$1 AND grant_token_ciphertext IS NOT NULL",
                    &[&input.grant_id],
                )
                .map_err(managed_cloud_storage)?;
                existing.0.replayed = true;
                existing.0.next_heartbeat_sequence = tx
                    .query_one(
                        "SELECT COALESCE((
                           SELECT heartbeat_sequence+1
                             FROM jobs_managed_cloud_runtime_heartbeats
                            WHERE runtime_instance_id=$1 AND instance_epoch=$2
                         ),1)",
                        &[&existing.0.runtime_instance_id, &existing.0.instance_epoch],
                    )
                    .map_err(managed_cloud_storage)?
                    .get(0);
                tx.commit().map_err(managed_cloud_storage)?;
                return Ok(existing.0);
            }
            if tx
                .query_opt(
                    "SELECT 1 FROM jobs_managed_cloud_runtime_instances
                      WHERE runtime_instance_id=$1 FOR UPDATE",
                    &[&input.runtime_instance_id],
                )
                .map_err(managed_cloud_storage)?
                .is_some()
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            if grant.expires_at_ms <= now_ms {
                tx.execute(
                    "UPDATE jobs_managed_cloud_runtime_grants
                        SET grant_token_ciphertext=NULL
                      WHERE grant_id=$1 AND grant_token_ciphertext IS NOT NULL",
                    &[&input.grant_id],
                )
                .map_err(managed_cloud_storage)?;
                tx.commit().map_err(managed_cloud_storage)?;
                return Err(ManagedCloudRegistryError::GrantExpired);
            }
            require_postgres_managed_cloud_grant_authority_active(&mut tx, &grant, now_ms)?;
            let instance_epoch = 1_i64;
            let session_proof_hmac_sha256 = managed_cloud_runtime_session_proof_hmac(
                &input.session_token,
                &input.grant_id,
                &input.worker_id,
                &input.runtime_instance_id,
                instance_epoch,
            )?;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_runtime_instances(
                   grant_id,runtime_instance_id,runtime_identity_sha256,worker_id,
                   session_proof_hmac_sha256,environment,region,channel,activation_sha256,
                   manifest_sha256,component_id,role,head_revision,transition_sha256,
                   artifact_sha256,config_schema_sha256,migration_set_sha256,
                   protocol_set_sha256,task_queue_sha256,failure_converter_sha256,
                   dependency_evidence_sha256,activation_expires_at_ms,
                   instance_epoch,claimed_at_ms
                 ) VALUES(
                   $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                   $18,$19,$20,$21,$22,$23,$24
                 )",
                &[
                    &grant.grant_id,
                    &input.runtime_instance_id,
                    &input.runtime_identity_sha256,
                    &input.worker_id,
                    &session_proof_hmac_sha256,
                    &grant.scope.environment,
                    &grant.scope.region,
                    &grant.scope.channel,
                    &grant.activation_sha256,
                    &grant.manifest_sha256,
                    &grant.component_id,
                    &grant.role,
                    &grant.head_revision,
                    &grant.transition_sha256,
                    &grant.artifact_sha256,
                    &grant.config_schema_sha256,
                    &grant.migration_set_sha256,
                    &grant.protocol_set_sha256,
                    &grant.task_queue_sha256,
                    &grant.failure_converter_sha256,
                    &grant.dependency_evidence_sha256,
                    &grant.activation_expires_at_ms,
                    &instance_epoch,
                    &now_ms,
                ],
            )
            .map_err(managed_cloud_storage)?;
            tx.execute(
                "UPDATE jobs_managed_cloud_runtime_grants
                    SET grant_token_ciphertext=NULL WHERE grant_id=$1",
                &[&input.grant_id],
            )
            .map_err(managed_cloud_storage)?;
            let row = tx
                .query_one(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS}
                           FROM jobs_managed_cloud_runtime_instances WHERE grant_id=$1"
                    ),
                    &[&input.grant_id],
                )
                .map_err(managed_cloud_storage)?;
            let mut instance = managed_cloud_runtime_instance_from_postgres(&row).0;
            instance.replayed = false;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(instance)
        }
    })
}

const MANAGED_CLOUD_RUNTIME_HEARTBEAT_COLUMNS: &str =
    "runtime_instance_id,worker_id,instance_epoch,heartbeat_sequence,\
     observed_head_revision,observed_transition_sha256,activation_sha256,manifest_sha256,\
     component_id,role,artifact_sha256,migration_set_sha256,config_schema_sha256,\
     protocol_set_sha256,task_queue_sha256,failure_converter_sha256,\
     dependency_evidence_sha256,health_state,reason_code,heartbeat_at_ms";

fn managed_cloud_runtime_heartbeat_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ManagedCloudRuntimeHeartbeat> {
    Ok(ManagedCloudRuntimeHeartbeat {
        runtime_instance_id: row.get(0)?,
        worker_id: row.get(1)?,
        instance_epoch: row.get(2)?,
        heartbeat_sequence: row.get(3)?,
        observed_head_revision: row.get(4)?,
        observed_transition_sha256: row.get(5)?,
        activation_sha256: row.get(6)?,
        manifest_sha256: row.get(7)?,
        component_id: row.get(8)?,
        role: row.get(9)?,
        artifact_sha256: row.get(10)?,
        migration_set_sha256: row.get(11)?,
        config_schema_sha256: row.get(12)?,
        protocol_set_sha256: row.get(13)?,
        task_queue_sha256: row.get(14)?,
        failure_converter_sha256: row.get(15)?,
        dependency_evidence_sha256: row.get(16)?,
        health_state: row.get(17)?,
        reason_code: row.get(18)?,
        heartbeat_at_ms: row.get(19)?,
        replayed: false,
    })
}

fn managed_cloud_runtime_heartbeat_from_postgres(
    row: &postgres::Row,
) -> ManagedCloudRuntimeHeartbeat {
    ManagedCloudRuntimeHeartbeat {
        runtime_instance_id: row.get(0),
        worker_id: row.get(1),
        instance_epoch: row.get(2),
        heartbeat_sequence: row.get(3),
        observed_head_revision: row.get(4),
        observed_transition_sha256: row.get(5),
        activation_sha256: row.get(6),
        manifest_sha256: row.get(7),
        component_id: row.get(8),
        role: row.get(9),
        artifact_sha256: row.get(10),
        migration_set_sha256: row.get(11),
        config_schema_sha256: row.get(12),
        protocol_set_sha256: row.get(13),
        task_queue_sha256: row.get(14),
        failure_converter_sha256: row.get(15),
        dependency_evidence_sha256: row.get(16),
        health_state: row.get(17),
        reason_code: row.get(18),
        heartbeat_at_ms: row.get(19),
        replayed: false,
    }
}

fn require_managed_cloud_runtime_heartbeat_identity(
    instance: &ManagedCloudRuntimeInstance,
    input: &ManagedCloudRuntimeHeartbeatInput,
) -> ManagedCloudResult<()> {
    if instance.runtime_instance_id != input.runtime_instance_id
        || instance.worker_id != input.worker_id
        || instance.head_revision != input.observed_head_revision
        || instance.transition_sha256 != input.observed_transition_sha256
        || instance.activation_sha256 != input.activation_sha256
        || instance.manifest_sha256 != input.manifest_sha256
        || instance.component_id != input.component_id
        || instance.role != input.role
        || instance.artifact_sha256 != input.artifact_sha256
        || instance.migration_set_sha256 != input.migration_set_sha256
        || instance.config_schema_sha256 != input.config_schema_sha256
        || instance.protocol_set_sha256 != input.protocol_set_sha256
        || instance.task_queue_sha256 != input.task_queue_sha256
        || instance.failure_converter_sha256 != input.failure_converter_sha256
        || instance.dependency_evidence_sha256 != input.dependency_evidence_sha256
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    Ok(())
}

fn require_exact_managed_cloud_heartbeat_replay(
    existing: &ManagedCloudRuntimeHeartbeat,
    input: &ManagedCloudRuntimeHeartbeatInput,
) -> ManagedCloudResult<()> {
    if existing.runtime_instance_id != input.runtime_instance_id
        || existing.worker_id != input.worker_id
        || existing.heartbeat_sequence != input.heartbeat_sequence
        || existing.observed_head_revision != input.observed_head_revision
        || existing.observed_transition_sha256 != input.observed_transition_sha256
        || existing.activation_sha256 != input.activation_sha256
        || existing.manifest_sha256 != input.manifest_sha256
        || existing.component_id != input.component_id
        || existing.role != input.role
        || existing.artifact_sha256 != input.artifact_sha256
        || existing.migration_set_sha256 != input.migration_set_sha256
        || existing.config_schema_sha256 != input.config_schema_sha256
        || existing.protocol_set_sha256 != input.protocol_set_sha256
        || existing.task_queue_sha256 != input.task_queue_sha256
        || existing.failure_converter_sha256 != input.failure_converter_sha256
        || existing.dependency_evidence_sha256 != input.dependency_evidence_sha256
        || existing.health_state != input.health_state
        || existing.reason_code != input.reason_code
    {
        return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
    }
    Ok(())
}

fn sqlite_managed_cloud_runtime_instance_by_runtime_id(
    tx: &rusqlite::Transaction<'_>,
    runtime_instance_id: &str,
) -> ManagedCloudResult<Option<(ManagedCloudRuntimeInstance, String)>> {
    tx.query_row(
        &format!(
            "SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS}
               FROM jobs_managed_cloud_runtime_instances WHERE runtime_instance_id=?1"
        ),
        params![runtime_instance_id],
        managed_cloud_runtime_instance_from_sqlite,
    )
    .optional()
    .map_err(managed_cloud_storage)
}

fn require_sqlite_managed_cloud_runtime_authority_active(
    tx: &rusqlite::Transaction<'_>,
    instance: &ManagedCloudRuntimeInstance,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    let revoked = tx
        .query_row(
            "SELECT 1
               FROM jobs_managed_cloud_runtime_instances instance
               JOIN jobs_managed_cloud_runtime_grants grant
                 ON grant.grant_id=instance.grant_id
                AND grant.environment=instance.environment
                AND grant.region=instance.region AND grant.channel=instance.channel
                AND grant.activation_sha256=instance.activation_sha256
                AND grant.manifest_sha256=instance.manifest_sha256
                AND grant.component_id=instance.component_id AND grant.role=instance.role
                AND grant.head_revision=instance.head_revision
                AND grant.transition_sha256=instance.transition_sha256
                AND grant.artifact_sha256=instance.artifact_sha256
                AND grant.config_schema_sha256=instance.config_schema_sha256
                AND grant.migration_set_sha256=instance.migration_set_sha256
                AND grant.protocol_set_sha256=instance.protocol_set_sha256
                AND grant.task_queue_sha256=instance.task_queue_sha256
                AND grant.failure_converter_sha256=instance.failure_converter_sha256
                AND grant.expected_dependency_evidence_sha256=
                    instance.dependency_evidence_sha256
                AND grant.expected_runtime_identity_sha256=
                    instance.runtime_identity_sha256
                AND grant.expected_worker_id=instance.worker_id
                AND grant.activation_expires_at_ms=instance.activation_expires_at_ms
               JOIN jobs_managed_cloud_activations activation
                 ON activation.activation_sha256=instance.activation_sha256
                AND activation.manifest_sha256=instance.manifest_sha256
                AND activation.environment=instance.environment
                AND activation.region=instance.region AND activation.channel=instance.channel
               JOIN jobs_managed_cloud_manifests manifest
                 ON manifest.manifest_sha256=instance.manifest_sha256
                AND manifest.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_cohorts cohort
                 ON cohort.cohort_sha256=activation.cohort_sha256
                AND cohort.authorization_signature_set_sha256=
                    activation.cohort_signature_set_sha256
                AND cohort.trust_generation=activation.trust_generation
                AND cohort.environment=activation.environment
                AND cohort.region=activation.region AND cohort.channel=activation.channel
               JOIN jobs_managed_cloud_trust_policies policy
                 ON policy.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_head_transitions transition
                 ON transition.transition_sha256=instance.transition_sha256
                AND transition.environment=instance.environment
                AND transition.region=instance.region AND transition.channel=instance.channel
                AND transition.head_revision=instance.head_revision
                AND transition.next_activation_sha256=instance.activation_sha256
                AND transition.next_manifest_sha256=instance.manifest_sha256
               LEFT JOIN jobs_managed_cloud_rollbacks rollback
                 ON rollback.rollback_sha256=transition.rollback_authority_sha256
                AND rollback.trust_generation=activation.trust_generation
              WHERE instance.runtime_instance_id=?1
                AND (transition.transition_kind='activation'
                     OR rollback.rollback_sha256 IS NOT NULL)
                AND EXISTS(
                SELECT 1 FROM jobs_managed_cloud_runtime_grant_revocations direct
                 WHERE direct.grant_id=grant.grant_id
                UNION ALL
                SELECT 1 FROM jobs_managed_cloud_revocations revoked
                 WHERE revoked.effective_at_ms<=?2 AND (
                   (revoked.subject_kind='runtime_grant'
                     AND revoked.subject_id=grant.grant_id
                     AND revoked.subject_sha256=grant.token_sha256)
                   OR (revoked.subject_kind='runtime_instance'
                     AND revoked.subject_id=instance.runtime_instance_id
                     AND revoked.subject_sha256=instance.runtime_identity_sha256)
                   OR (revoked.subject_kind='activation'
                     AND revoked.subject_sha256=activation.activation_sha256)
                   OR (revoked.subject_kind='manifest'
                     AND revoked.subject_sha256=manifest.manifest_sha256)
                   OR (revoked.subject_kind='cohort'
                     AND revoked.subject_sha256=cohort.cohort_sha256)
                   OR (revoked.subject_kind='component'
                     AND revoked.subject_id=instance.component_id
                     AND revoked.subject_sha256=instance.artifact_sha256)
                   OR (revoked.subject_kind='release'
                     AND revoked.subject_id=manifest.release_id
                     AND revoked.subject_sha256=manifest.manifest_sha256)
                   OR (revoked.subject_kind='rollback'
                     AND rollback.rollback_sha256 IS NOT NULL
                     AND revoked.subject_id=rollback.rollback_id
                     AND revoked.subject_sha256=rollback.rollback_sha256)
                   OR (revoked.subject_kind='trust_policy'
                     AND revoked.subject_id=policy.policy_id
                     AND revoked.subject_sha256=policy.policy_sha256)
                   OR (revoked.subject_kind='signing_key' AND revoked.subject_id IN (
                     SELECT signature.key_id FROM jobs_managed_cloud_signatures signature
                      WHERE signature.signature_set_sha256 IN (
                        activation.authorization_signature_set_sha256,
                        manifest.authorization_signature_set_sha256,
                        cohort.authorization_signature_set_sha256,
                        policy.authorization_signature_set_sha256
                      )
                      OR signature.signature_set_sha256=
                         rollback.authorization_signature_set_sha256
                   ))
                 )
              ) LIMIT 1",
            params![instance.runtime_instance_id, now_ms],
            |_| Ok(()),
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .is_some();
    if revoked {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    let active = tx
        .query_row(
            "SELECT 1
               FROM jobs_managed_cloud_runtime_instances instance
               JOIN jobs_managed_cloud_runtime_grants grant
                 ON grant.grant_id=instance.grant_id
                AND grant.environment=instance.environment
                AND grant.region=instance.region AND grant.channel=instance.channel
                AND grant.activation_sha256=instance.activation_sha256
                AND grant.manifest_sha256=instance.manifest_sha256
                AND grant.component_id=instance.component_id AND grant.role=instance.role
                AND grant.head_revision=instance.head_revision
                AND grant.transition_sha256=instance.transition_sha256
                AND grant.artifact_sha256=instance.artifact_sha256
                AND grant.config_schema_sha256=instance.config_schema_sha256
                AND grant.migration_set_sha256=instance.migration_set_sha256
                AND grant.protocol_set_sha256=instance.protocol_set_sha256
                AND grant.task_queue_sha256=instance.task_queue_sha256
                AND grant.failure_converter_sha256=instance.failure_converter_sha256
                AND grant.expected_dependency_evidence_sha256=
                    instance.dependency_evidence_sha256
                AND grant.expected_runtime_identity_sha256=
                    instance.runtime_identity_sha256
                AND grant.expected_worker_id=instance.worker_id
                AND grant.activation_expires_at_ms=instance.activation_expires_at_ms
               JOIN jobs_managed_cloud_activations activation
                 ON activation.activation_sha256=instance.activation_sha256
                AND activation.manifest_sha256=instance.manifest_sha256
                AND activation.environment=instance.environment
                AND activation.region=instance.region AND activation.channel=instance.channel
               JOIN jobs_managed_cloud_cohorts cohort
                 ON cohort.cohort_sha256=activation.cohort_sha256
                AND cohort.authorization_signature_set_sha256=
                    activation.cohort_signature_set_sha256
                AND cohort.trust_generation=activation.trust_generation
                AND cohort.environment=activation.environment
                AND cohort.region=activation.region AND cohort.channel=activation.channel
               JOIN jobs_managed_cloud_trust_policies policy
                 ON policy.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_head_transitions transition
                 ON transition.transition_sha256=instance.transition_sha256
                AND transition.environment=instance.environment
                AND transition.region=instance.region AND transition.channel=instance.channel
                AND transition.head_revision=instance.head_revision
                AND transition.next_activation_sha256=instance.activation_sha256
                AND transition.next_manifest_sha256=instance.manifest_sha256
               LEFT JOIN jobs_managed_cloud_rollbacks rollback
                 ON rollback.rollback_sha256=transition.rollback_authority_sha256
                AND rollback.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_heads head
                 ON head.environment=instance.environment AND head.region=instance.region
                AND head.channel=instance.channel
                AND head.head_revision=instance.head_revision
                AND head.current_transition_sha256=instance.transition_sha256
                AND head.current_activation_sha256=instance.activation_sha256
                AND head.current_manifest_sha256=instance.manifest_sha256
              WHERE instance.runtime_instance_id=?1
                AND (transition.transition_kind='activation'
                     OR rollback.rollback_sha256 IS NOT NULL)
                AND activation.not_before_ms<=?2 AND activation.expires_at_ms>?2
                AND activation.expires_at_ms=instance.activation_expires_at_ms
                AND grant.expires_at_ms>?2
                AND cohort.not_before_ms<=?2 AND cohort.expires_at_ms>?2
                AND policy.valid_from_ms<=?2 AND policy.expires_at_ms>?2
                AND activation.cloud_distribution_enabled=1
                AND activation.workflow_command_dispatch_enabled=1
                AND activation.workflow_cleanup_enabled=1
                AND activation.direct_discovery_enabled=0
                AND activation.global_discovery_enabled=0
                AND activation.source_verification_enabled=0",
            params![instance.runtime_instance_id, now_ms],
            |_| Ok(()),
        )
        .optional()
        .map_err(managed_cloud_storage)?
        .is_some();
    if !active {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(())
}

fn require_postgres_managed_cloud_runtime_authority_active(
    tx: &mut postgres::Transaction<'_>,
    instance: &ManagedCloudRuntimeInstance,
    now_ms: i64,
) -> ManagedCloudResult<()> {
    let revoked = tx
        .query_opt(
            "SELECT 1
               FROM jobs_managed_cloud_runtime_instances instance
               JOIN jobs_managed_cloud_runtime_grants grant
                 ON grant.grant_id=instance.grant_id
                AND grant.environment=instance.environment
                AND grant.region=instance.region AND grant.channel=instance.channel
                AND grant.activation_sha256=instance.activation_sha256
                AND grant.manifest_sha256=instance.manifest_sha256
                AND grant.component_id=instance.component_id AND grant.role=instance.role
                AND grant.head_revision=instance.head_revision
                AND grant.transition_sha256=instance.transition_sha256
                AND grant.artifact_sha256=instance.artifact_sha256
                AND grant.config_schema_sha256=instance.config_schema_sha256
                AND grant.migration_set_sha256=instance.migration_set_sha256
                AND grant.protocol_set_sha256=instance.protocol_set_sha256
                AND grant.task_queue_sha256=instance.task_queue_sha256
                AND grant.failure_converter_sha256=instance.failure_converter_sha256
                AND grant.expected_dependency_evidence_sha256=
                    instance.dependency_evidence_sha256
                AND grant.expected_runtime_identity_sha256=
                    instance.runtime_identity_sha256
                AND grant.expected_worker_id=instance.worker_id
                AND grant.activation_expires_at_ms=instance.activation_expires_at_ms
               JOIN jobs_managed_cloud_activations activation
                 ON activation.activation_sha256=instance.activation_sha256
                AND activation.manifest_sha256=instance.manifest_sha256
                AND activation.environment=instance.environment
                AND activation.region=instance.region AND activation.channel=instance.channel
               JOIN jobs_managed_cloud_manifests manifest
                 ON manifest.manifest_sha256=instance.manifest_sha256
                AND manifest.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_cohorts cohort
                 ON cohort.cohort_sha256=activation.cohort_sha256
                AND cohort.authorization_signature_set_sha256=
                    activation.cohort_signature_set_sha256
                AND cohort.trust_generation=activation.trust_generation
                AND cohort.environment=activation.environment
                AND cohort.region=activation.region AND cohort.channel=activation.channel
               JOIN jobs_managed_cloud_trust_policies policy
                 ON policy.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_head_transitions transition
                 ON transition.transition_sha256=instance.transition_sha256
                AND transition.environment=instance.environment
                AND transition.region=instance.region AND transition.channel=instance.channel
                AND transition.head_revision=instance.head_revision
                AND transition.next_activation_sha256=instance.activation_sha256
                AND transition.next_manifest_sha256=instance.manifest_sha256
               LEFT JOIN jobs_managed_cloud_rollbacks rollback
                 ON rollback.rollback_sha256=transition.rollback_authority_sha256
                AND rollback.trust_generation=activation.trust_generation
              WHERE instance.runtime_instance_id=$1
                AND (transition.transition_kind='activation'
                     OR rollback.rollback_sha256 IS NOT NULL)
                AND EXISTS(
                SELECT 1 FROM jobs_managed_cloud_runtime_grant_revocations direct
                 WHERE direct.grant_id=grant.grant_id
                UNION ALL
                SELECT 1 FROM jobs_managed_cloud_revocations revoked
                 WHERE revoked.effective_at_ms<=$2 AND (
                   (revoked.subject_kind='runtime_grant'
                     AND revoked.subject_id=grant.grant_id
                     AND revoked.subject_sha256=grant.token_sha256)
                   OR (revoked.subject_kind='runtime_instance'
                     AND revoked.subject_id=instance.runtime_instance_id
                     AND revoked.subject_sha256=instance.runtime_identity_sha256)
                   OR (revoked.subject_kind='activation'
                     AND revoked.subject_sha256=activation.activation_sha256)
                   OR (revoked.subject_kind='manifest'
                     AND revoked.subject_sha256=manifest.manifest_sha256)
                   OR (revoked.subject_kind='cohort'
                     AND revoked.subject_sha256=cohort.cohort_sha256)
                   OR (revoked.subject_kind='component'
                     AND revoked.subject_id=instance.component_id
                     AND revoked.subject_sha256=instance.artifact_sha256)
                   OR (revoked.subject_kind='release'
                     AND revoked.subject_id=manifest.release_id
                     AND revoked.subject_sha256=manifest.manifest_sha256)
                   OR (revoked.subject_kind='rollback'
                     AND rollback.rollback_sha256 IS NOT NULL
                     AND revoked.subject_id=rollback.rollback_id
                     AND revoked.subject_sha256=rollback.rollback_sha256)
                   OR (revoked.subject_kind='trust_policy'
                     AND revoked.subject_id=policy.policy_id
                     AND revoked.subject_sha256=policy.policy_sha256)
                   OR (revoked.subject_kind='signing_key' AND revoked.subject_id IN (
                     SELECT signature.key_id FROM jobs_managed_cloud_signatures signature
                      WHERE signature.signature_set_sha256 IN (
                        activation.authorization_signature_set_sha256,
                        manifest.authorization_signature_set_sha256,
                        cohort.authorization_signature_set_sha256,
                        policy.authorization_signature_set_sha256
                      )
                      OR signature.signature_set_sha256=
                         rollback.authorization_signature_set_sha256
                   ))
                 )
              ) LIMIT 1",
            &[&instance.runtime_instance_id, &now_ms],
        )
        .map_err(managed_cloud_storage)?
        .is_some();
    if revoked {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    let active = tx
        .query_opt(
            "SELECT 1
               FROM jobs_managed_cloud_runtime_instances instance
               JOIN jobs_managed_cloud_runtime_grants grant
                 ON grant.grant_id=instance.grant_id
                AND grant.environment=instance.environment
                AND grant.region=instance.region AND grant.channel=instance.channel
                AND grant.activation_sha256=instance.activation_sha256
                AND grant.manifest_sha256=instance.manifest_sha256
                AND grant.component_id=instance.component_id AND grant.role=instance.role
                AND grant.head_revision=instance.head_revision
                AND grant.transition_sha256=instance.transition_sha256
                AND grant.artifact_sha256=instance.artifact_sha256
                AND grant.config_schema_sha256=instance.config_schema_sha256
                AND grant.migration_set_sha256=instance.migration_set_sha256
                AND grant.protocol_set_sha256=instance.protocol_set_sha256
                AND grant.task_queue_sha256=instance.task_queue_sha256
                AND grant.failure_converter_sha256=instance.failure_converter_sha256
                AND grant.expected_dependency_evidence_sha256=
                    instance.dependency_evidence_sha256
                AND grant.expected_runtime_identity_sha256=
                    instance.runtime_identity_sha256
                AND grant.expected_worker_id=instance.worker_id
                AND grant.activation_expires_at_ms=instance.activation_expires_at_ms
               JOIN jobs_managed_cloud_activations activation
                 ON activation.activation_sha256=instance.activation_sha256
                AND activation.manifest_sha256=instance.manifest_sha256
                AND activation.environment=instance.environment
                AND activation.region=instance.region AND activation.channel=instance.channel
               JOIN jobs_managed_cloud_cohorts cohort
                 ON cohort.cohort_sha256=activation.cohort_sha256
                AND cohort.authorization_signature_set_sha256=
                    activation.cohort_signature_set_sha256
                AND cohort.trust_generation=activation.trust_generation
                AND cohort.environment=activation.environment
                AND cohort.region=activation.region AND cohort.channel=activation.channel
               JOIN jobs_managed_cloud_trust_policies policy
                 ON policy.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_head_transitions transition
                 ON transition.transition_sha256=instance.transition_sha256
                AND transition.environment=instance.environment
                AND transition.region=instance.region AND transition.channel=instance.channel
                AND transition.head_revision=instance.head_revision
                AND transition.next_activation_sha256=instance.activation_sha256
                AND transition.next_manifest_sha256=instance.manifest_sha256
               LEFT JOIN jobs_managed_cloud_rollbacks rollback
                 ON rollback.rollback_sha256=transition.rollback_authority_sha256
                AND rollback.trust_generation=activation.trust_generation
               JOIN jobs_managed_cloud_heads head
                 ON head.environment=instance.environment AND head.region=instance.region
                AND head.channel=instance.channel
                AND head.head_revision=instance.head_revision
                AND head.current_transition_sha256=instance.transition_sha256
                AND head.current_activation_sha256=instance.activation_sha256
                AND head.current_manifest_sha256=instance.manifest_sha256
              WHERE instance.runtime_instance_id=$1
                AND (transition.transition_kind='activation'
                     OR rollback.rollback_sha256 IS NOT NULL)
                AND activation.not_before_ms<=$2 AND activation.expires_at_ms>$2
                AND activation.expires_at_ms=instance.activation_expires_at_ms
                AND grant.expires_at_ms>$2
                AND cohort.not_before_ms<=$2 AND cohort.expires_at_ms>$2
                AND policy.valid_from_ms<=$2 AND policy.expires_at_ms>$2
                AND activation.cloud_distribution_enabled
                AND activation.workflow_command_dispatch_enabled
                AND activation.workflow_cleanup_enabled
                AND NOT activation.direct_discovery_enabled
                AND NOT activation.global_discovery_enabled
                AND NOT activation.source_verification_enabled
              FOR SHARE OF head",
            &[&instance.runtime_instance_id, &now_ms],
        )
        .map_err(managed_cloud_storage)?
        .is_some();
    if !active {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(())
}

pub fn record_managed_cloud_runtime_heartbeat(
    pool: &DbPool,
    input: &ManagedCloudRuntimeHeartbeatInput,
) -> ManagedCloudResult<ManagedCloudRuntimeHeartbeat> {
    validate_managed_cloud_runtime_heartbeat(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let (instance, session_proof) = sqlite_managed_cloud_runtime_instance_by_runtime_id(
                &tx,
                &input.runtime_instance_id,
            )?
            .ok_or(ManagedCloudRegistryError::NotFound)?;
            require_managed_cloud_runtime_heartbeat_identity(&instance, input)?;
            let expected_proof = managed_cloud_runtime_session_proof_hmac(
                &input.session_token,
                &instance.grant_id,
                &input.worker_id,
                &input.runtime_instance_id,
                instance.instance_epoch,
            )?;
            require_managed_cloud_secret_match(&session_proof, &expected_proof)?;
            let current = tx
                .query_row(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_HEARTBEAT_COLUMNS}
                           FROM jobs_managed_cloud_runtime_heartbeats
                          WHERE runtime_instance_id=?1 AND instance_epoch=?2"
                    ),
                    params![input.runtime_instance_id, instance.instance_epoch],
                    managed_cloud_runtime_heartbeat_from_sqlite,
                )
                .optional()
                .map_err(managed_cloud_storage)?;
            let had_current = current.is_some();
            if let Some(mut current) = current {
                if input.heartbeat_sequence == current.heartbeat_sequence {
                    require_exact_managed_cloud_heartbeat_replay(&current, input)?;
                    current.replayed = true;
                    tx.commit().map_err(managed_cloud_storage)?;
                    return Ok(current);
                }
                if input.heartbeat_sequence != current.heartbeat_sequence + 1 {
                    return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
                }
            } else if input.heartbeat_sequence != 1 {
                return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
            }
            let now_ms = managed_cloud_db_now_sqlite(&tx)?;
            require_sqlite_managed_cloud_runtime_authority_active(&tx, &instance, now_ms)?;
            let values = params![
                input.runtime_instance_id,
                instance.instance_epoch,
                input.heartbeat_sequence,
                input.activation_sha256,
                input.manifest_sha256,
                input.component_id,
                input.role,
                input.worker_id,
                input.artifact_sha256,
                input.observed_head_revision,
                input.observed_transition_sha256,
                input.migration_set_sha256,
                input.config_schema_sha256,
                input.protocol_set_sha256,
                input.task_queue_sha256,
                input.failure_converter_sha256,
                input.dependency_evidence_sha256,
                input.health_state,
                input.reason_code,
                now_ms,
            ];
            let changed = if had_current {
                tx.execute(
                    "UPDATE jobs_managed_cloud_runtime_heartbeats SET
                       heartbeat_sequence=?3,activation_sha256=?4,manifest_sha256=?5,
                       component_id=?6,role=?7,worker_id=?8,artifact_sha256=?9,
                       observed_head_revision=?10,observed_transition_sha256=?11,
                       migration_set_sha256=?12,config_schema_sha256=?13,
                       protocol_set_sha256=?14,task_queue_sha256=?15,
                       failure_converter_sha256=?16,dependency_evidence_sha256=?17,
                       health_state=?18,reason_code=?19,heartbeat_at_ms=?20
                     WHERE runtime_instance_id=?1 AND instance_epoch=?2
                       AND heartbeat_sequence=?3-1",
                    values,
                )
            } else {
                tx.execute(
                    "INSERT INTO jobs_managed_cloud_runtime_heartbeats(
                       runtime_instance_id,instance_epoch,heartbeat_sequence,
                       activation_sha256,manifest_sha256,component_id,role,worker_id,
                       artifact_sha256,observed_head_revision,observed_transition_sha256,
                       migration_set_sha256,config_schema_sha256,protocol_set_sha256,
                       task_queue_sha256,failure_converter_sha256,
                       dependency_evidence_sha256,health_state,reason_code,heartbeat_at_ms
                     ) VALUES(
                       ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,
                       ?17,?18,?19,?20
                     )",
                    values,
                )
            }
            .map_err(managed_cloud_storage)?;
            if changed != 1 {
                return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
            }
            tx.execute(
                "INSERT INTO jobs_managed_cloud_runtime_heartbeat_audit(
                   runtime_instance_id,instance_epoch,heartbeat_sequence,activation_sha256,
                   manifest_sha256,component_id,role,worker_id,artifact_sha256,
                   observed_head_revision,observed_transition_sha256,migration_set_sha256,
                   config_schema_sha256,protocol_set_sha256,task_queue_sha256,
                   failure_converter_sha256,dependency_evidence_sha256,health_state,
                   reason_code,heartbeat_at_ms
                 ) SELECT runtime_instance_id,instance_epoch,heartbeat_sequence,
                          activation_sha256,manifest_sha256,component_id,role,worker_id,
                          artifact_sha256,observed_head_revision,observed_transition_sha256,
                          migration_set_sha256,config_schema_sha256,protocol_set_sha256,
                          task_queue_sha256,failure_converter_sha256,
                          dependency_evidence_sha256,health_state,reason_code,heartbeat_at_ms
                     FROM jobs_managed_cloud_runtime_heartbeats
                    WHERE runtime_instance_id=?1 AND instance_epoch=?2",
                params![input.runtime_instance_id, instance.instance_epoch],
            )
            .map_err(managed_cloud_storage)?;
            tx.execute(
                "DELETE FROM jobs_managed_cloud_runtime_heartbeat_audit
                  WHERE runtime_instance_id=?1 AND instance_epoch=?2
                    AND heartbeat_sequence<=?3-?4",
                params![
                    input.runtime_instance_id,
                    instance.instance_epoch,
                    input.heartbeat_sequence,
                    MANAGED_CLOUD_HEARTBEAT_AUDIT_LIMIT,
                ],
            )
            .map_err(managed_cloud_storage)?;
            let heartbeat = tx
                .query_row(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_HEARTBEAT_COLUMNS}
                           FROM jobs_managed_cloud_runtime_heartbeats
                          WHERE runtime_instance_id=?1 AND instance_epoch=?2"
                    ),
                    params![input.runtime_instance_id, instance.instance_epoch],
                    managed_cloud_runtime_heartbeat_from_sqlite,
                )
                .map_err(managed_cloud_storage)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(heartbeat)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            let instance_row = tx
                .query_opt(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS}
                           FROM jobs_managed_cloud_runtime_instances
                          WHERE runtime_instance_id=$1 FOR UPDATE"
                    ),
                    &[&input.runtime_instance_id],
                )
                .map_err(managed_cloud_storage)?
                .ok_or(ManagedCloudRegistryError::NotFound)?;
            let (instance, session_proof) =
                managed_cloud_runtime_instance_from_postgres(&instance_row);
            require_managed_cloud_runtime_heartbeat_identity(&instance, input)?;
            let expected_proof = managed_cloud_runtime_session_proof_hmac(
                &input.session_token,
                &instance.grant_id,
                &input.worker_id,
                &input.runtime_instance_id,
                instance.instance_epoch,
            )?;
            require_managed_cloud_secret_match(&session_proof, &expected_proof)?;
            let current = tx
                .query_opt(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_HEARTBEAT_COLUMNS}
                           FROM jobs_managed_cloud_runtime_heartbeats
                          WHERE runtime_instance_id=$1 AND instance_epoch=$2 FOR UPDATE"
                    ),
                    &[&input.runtime_instance_id, &instance.instance_epoch],
                )
                .map_err(managed_cloud_storage)?
                .map(|row| managed_cloud_runtime_heartbeat_from_postgres(&row));
            let had_current = current.is_some();
            if let Some(mut current) = current {
                if input.heartbeat_sequence == current.heartbeat_sequence {
                    require_exact_managed_cloud_heartbeat_replay(&current, input)?;
                    current.replayed = true;
                    tx.commit().map_err(managed_cloud_storage)?;
                    return Ok(current);
                }
                if input.heartbeat_sequence != current.heartbeat_sequence + 1 {
                    return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
                }
            } else if input.heartbeat_sequence != 1 {
                return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
            }
            let now_ms = managed_cloud_db_now_postgres(&mut tx)?;
            require_postgres_managed_cloud_runtime_authority_active(&mut tx, &instance, now_ms)?;
            let parameters: [&(dyn postgres::types::ToSql + Sync); 20] = [
                &input.runtime_instance_id,
                &instance.instance_epoch,
                &input.heartbeat_sequence,
                &input.activation_sha256,
                &input.manifest_sha256,
                &input.component_id,
                &input.role,
                &input.worker_id,
                &input.artifact_sha256,
                &input.observed_head_revision,
                &input.observed_transition_sha256,
                &input.migration_set_sha256,
                &input.config_schema_sha256,
                &input.protocol_set_sha256,
                &input.task_queue_sha256,
                &input.failure_converter_sha256,
                &input.dependency_evidence_sha256,
                &input.health_state,
                &input.reason_code,
                &now_ms,
            ];
            let changed = if had_current {
                tx.execute(
                    "UPDATE jobs_managed_cloud_runtime_heartbeats SET
                       heartbeat_sequence=$3,activation_sha256=$4,manifest_sha256=$5,
                       component_id=$6,role=$7,worker_id=$8,artifact_sha256=$9,
                       observed_head_revision=$10,observed_transition_sha256=$11,
                       migration_set_sha256=$12,config_schema_sha256=$13,
                       protocol_set_sha256=$14,task_queue_sha256=$15,
                       failure_converter_sha256=$16,dependency_evidence_sha256=$17,
                       health_state=$18,reason_code=$19,heartbeat_at_ms=$20
                     WHERE runtime_instance_id=$1 AND instance_epoch=$2
                       AND heartbeat_sequence=$3-1",
                    &parameters,
                )
            } else {
                tx.execute(
                    "INSERT INTO jobs_managed_cloud_runtime_heartbeats(
                       runtime_instance_id,instance_epoch,heartbeat_sequence,
                       activation_sha256,manifest_sha256,component_id,role,worker_id,
                       artifact_sha256,observed_head_revision,observed_transition_sha256,
                       migration_set_sha256,config_schema_sha256,protocol_set_sha256,
                       task_queue_sha256,failure_converter_sha256,
                       dependency_evidence_sha256,health_state,reason_code,heartbeat_at_ms
                     ) VALUES(
                       $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
                       $17,$18,$19,$20
                     )",
                    &parameters,
                )
            }
            .map_err(managed_cloud_storage)?;
            if changed != 1 {
                return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
            }
            tx.execute(
                "INSERT INTO jobs_managed_cloud_runtime_heartbeat_audit(
                   runtime_instance_id,instance_epoch,heartbeat_sequence,activation_sha256,
                   manifest_sha256,component_id,role,worker_id,artifact_sha256,
                   observed_head_revision,observed_transition_sha256,migration_set_sha256,
                   config_schema_sha256,protocol_set_sha256,task_queue_sha256,
                   failure_converter_sha256,dependency_evidence_sha256,health_state,
                   reason_code,heartbeat_at_ms
                 ) SELECT runtime_instance_id,instance_epoch,heartbeat_sequence,
                          activation_sha256,manifest_sha256,component_id,role,worker_id,
                          artifact_sha256,observed_head_revision,observed_transition_sha256,
                          migration_set_sha256,config_schema_sha256,protocol_set_sha256,
                          task_queue_sha256,failure_converter_sha256,
                          dependency_evidence_sha256,health_state,reason_code,heartbeat_at_ms
                     FROM jobs_managed_cloud_runtime_heartbeats
                    WHERE runtime_instance_id=$1 AND instance_epoch=$2",
                &[&input.runtime_instance_id, &instance.instance_epoch],
            )
            .map_err(managed_cloud_storage)?;
            tx.execute(
                "DELETE FROM jobs_managed_cloud_runtime_heartbeat_audit
                  WHERE runtime_instance_id=$1 AND instance_epoch=$2
                    AND heartbeat_sequence<=$3-$4",
                &[
                    &input.runtime_instance_id,
                    &instance.instance_epoch,
                    &input.heartbeat_sequence,
                    &MANAGED_CLOUD_HEARTBEAT_AUDIT_LIMIT,
                ],
            )
            .map_err(managed_cloud_storage)?;
            let row = tx
                .query_one(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_HEARTBEAT_COLUMNS}
                           FROM jobs_managed_cloud_runtime_heartbeats
                          WHERE runtime_instance_id=$1 AND instance_epoch=$2"
                    ),
                    &[&input.runtime_instance_id, &instance.instance_epoch],
                )
                .map_err(managed_cloud_storage)?;
            let heartbeat = managed_cloud_runtime_heartbeat_from_postgres(&row);
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(heartbeat)
        }
    })
}

#[cfg(test)]
mod managed_cloud_release_authority_tests {
    use super::*;

    #[test]
    fn managed_cloud_canonical_json_matches_cross_runtime_golden() {
        let value = json!({
            "z": "é",
            "a": {"β": 2, "a": -7},
            "n": 0,
            "array": [{"b": true, "a": null}, "雪"],
        });
        let bytes = managed_cloud_canonical_json(&value).expect("canonical managed-cloud JSON");
        assert_eq!(
            bytes,
            "{\"a\":{\"a\":-7,\"β\":2},\"array\":[{\"a\":null,\"b\":true},\"雪\"],\"n\":0,\"z\":\"é\"}\n"
                .as_bytes()
        );
        assert_eq!(
            managed_cloud_sha256(&bytes),
            "e0e886e66d3fea4d9571caa85b779ff5a7a25f41138d6ce936244f148867d2ee"
        );
    }

    #[test]
    fn managed_cloud_canonical_json_rejects_unsafe_numbers_and_noncanonical_bytes() {
        for bytes in [
            b"-0\n".as_slice(),
            b"1.5\n".as_slice(),
            b"9007199254740992\n".as_slice(),
            b"-9007199254740992\n".as_slice(),
            b"{\"z\":1,\"a\":2}\n".as_slice(),
            b"{\"a\":1,\"a\":1}\n".as_slice(),
        ] {
            assert!(managed_cloud_parse_canonical::<Value>(bytes).is_err());
        }
        assert!(managed_cloud_parse_canonical::<Value>(b"-9007199254740991\n").is_ok());
        assert!(managed_cloud_parse_canonical::<Value>(b"9007199254740991\n").is_ok());
    }

    #[test]
    fn managed_cloud_runtime_derivations_match_cross_runtime_goldens() {
        assert_eq!(
            derive_managed_cloud_runtime_session_token(
                &"A".repeat(43),
                "cloud-runtime-grant-1234567890",
                "cloud-runtime-instance-1234567890",
            )
            .expect("derive managed-cloud session token"),
            "0IRP57ngXPzC59kCx1RDz3YzR5srjKmnSnR3B2joUFY"
        );
        assert_eq!(
            managed_cloud_task_queue_sha256("bluey-prod", "bluey-jobs-applications")
                .expect("derive managed-cloud task queue digest"),
            "1551b746f88eded4598f4e0817382253d97bd1d8aa8bcc74b4f0339bd118cfb0"
        );
        assert_eq!(
            managed_cloud_failure_converter_sha256(b"converter\n"),
            "4b57c07fe3edb9cb6068615fb27d286931d8d6d547c0e4de3426fcc81ed17aa0"
        );
    }

    #[test]
    fn managed_cloud_dependency_evidence_includes_the_final_nul() {
        assert_eq!(
            managed_cloud_dependency_evidence_sha256(
                "jobs_api",
                &"a".repeat(64),
                &"b".repeat(64),
                "jobs-api",
                &"c".repeat(64),
                &"d".repeat(64),
                &"e".repeat(64),
            )
            .expect("derive managed-cloud dependency evidence"),
            "a6936c1e63191b5edbeff6f87e8cc55a40eda40e12940b06c601ac4b5d69564d"
        );
    }

    #[test]
    fn managed_cloud_artifact_refs_are_content_addressed() {
        let digest = "a".repeat(64);
        assert!(managed_cloud_artifact_ref(
            "oci_image",
            &format!("ghcr.io/bluey/jobs-api@sha256:{digest}"),
            "release-611",
            &digest,
        ));
        assert!(!managed_cloud_artifact_ref(
            "oci_image",
            "ghcr.io/bluey/jobs-api:latest",
            "release-611",
            &digest,
        ));
        assert!(managed_cloud_artifact_ref(
            "static_bundle",
            &format!("https://artifacts.bluey.sh/releases/release-611/{digest}.tar"),
            "release-611",
            &digest,
        ));
        assert!(!managed_cloud_artifact_ref(
            "static_bundle",
            "https://artifacts.bluey.sh/releases/release-611/portal.tar",
            "release-611",
            &digest,
        ));
    }

    #[test]
    fn managed_cloud_task_queue_rejects_replacement_characters() {
        assert!(managed_cloud_task_queue_sha256("bluey-prod", "queue\u{fffd}").is_err());
    }

    #[test]
    fn managed_cloud_workflow_request_ids_require_lowercase_uuid_v5() {
        assert!(managed_cloud_workflow_request_id(
            "wfreq-v2-12345678-1234-5abc-8def-123456789abc"
        ));
        for invalid in [
            "wfreq-v2-12345678-1234-4abc-8def-123456789abc",
            "wfreq-v2-12345678-1234-5abc-7def-123456789abc",
            "wfreq-v2-12345678-1234-5ABC-8def-123456789abc",
            "12345678-1234-5abc-8def-123456789abc",
        ] {
            assert!(!managed_cloud_workflow_request_id(invalid));
        }
    }

    fn managed_cloud_execution_test_admission() -> ManagedCloudAdmissionAuthority {
        ManagedCloudAdmissionAuthority {
            scope: ManagedCloudScope {
                environment: "production".to_string(),
                region: "us-east-1".to_string(),
                channel: "canary".to_string(),
            },
            head_revision: 1,
            transition_sha256: "a".repeat(64),
            activation_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            cohort_sha256: "d".repeat(64),
            trust_generation: 1,
            channel_sequence: 1,
            release_id: "release-611".to_string(),
            release_sequence: 1,
            task_queue_sha256: "e".repeat(64),
            failure_converter_sha256: "f".repeat(64),
            readiness_sha256: "1".repeat(64),
            activation_expires_at_ms: 2_000,
            resolved_at_ms: 1_000,
        }
    }

    fn managed_cloud_execution_test_authority() -> ManagedCloudExecutionLeaseAuthority {
        let admission = managed_cloud_execution_test_admission();
        let binding = managed_cloud_execution_test_binding(admission.clone());
        let mut current = admission;
        current.readiness_sha256 = "3".repeat(64);
        current.resolved_at_ms = 1_100;
        let managed_cloud =
            managed_cloud_request_start_authority(binding.clone(), binding.clone(), current)
                .expect("canonical managed-cloud gateway authority")
                .managed_cloud;
        let gateway_bytes = managed_cloud_canonical_json(&managed_cloud)
            .expect("canonical managed-cloud gateway bytes");
        ManagedCloudExecutionLeaseAuthority {
            managed_cloud,
            managed_cloud_workflow_request_id: "wfreq-v2-12345678-1234-5abc-8def-123456789abc"
                .to_string(),
            request_command_id: "request-command-1234567890".to_string(),
            execution_command_id: binding.command_id,
            binding_sha256: binding.binding_sha256,
            release_memo_base64url: binding.release_memo_base64url,
            release_sha256: binding.release_memo_sha256,
            managed_cloud_runtime_instance_id: "runtime-instance-123456789".to_string(),
            managed_cloud_runtime_instance_epoch: 7,
            managed_cloud_worker_id: "managed-worker-1234567890".to_string(),
            gateway_authority_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(&gateway_bytes),
            gateway_authority_sha256: managed_cloud_sha256(&gateway_bytes),
        }
    }

    fn managed_cloud_execution_test_binding(
        admission: ManagedCloudAdmissionAuthority,
    ) -> ManagedCloudWorkflowBinding {
        let binding_sha256 = "2".repeat(64);
        let (_, _, release_memo_base64url, release_memo_sha256) =
            managed_cloud_release_memo(&binding_sha256, &admission)
                .expect("canonical managed-cloud release memo");
        ManagedCloudWorkflowBinding {
            command_id: "execution-command-123456".to_string(),
            binding_sha256,
            release_memo_base64url,
            release_memo_sha256,
            admission,
            replayed: false,
        }
    }

    fn managed_cloud_execution_test_input() -> ManagedCloudExecutionLeaseClaimInput {
        let managed_cloud_release = ManagedCloudReleaseMemoAuthority {
            version: 1,
            execution: ManagedCloudExecutionAuthority {
                binding_sha256: "2".repeat(64),
                admission: managed_cloud_execution_test_admission(),
            },
        };
        let managed_cloud_release_sha256 =
            managed_cloud_release_memo_sha256(&managed_cloud_release)
                .expect("managed-cloud release memo digest");
        ManagedCloudExecutionLeaseClaimInput {
            workflow_request_id: "wfreq-v2-12345678-1234-5abc-8def-123456789abc".to_string(),
            managed_cloud_release,
            managed_cloud_release_sha256,
            managed_cloud_runtime_instance_id: "runtime-instance-123456789".to_string(),
            managed_cloud_runtime_instance_epoch: 7,
        }
    }

    fn managed_cloud_execution_test_runtime(
        admission: &ManagedCloudAdmissionAuthority,
    ) -> ManagedCloudRuntimeInstance {
        ManagedCloudRuntimeInstance {
            grant_id: "managed-grant-1234567890".to_string(),
            runtime_instance_id: "runtime-instance-123456789".to_string(),
            runtime_identity_sha256: "b".repeat(64),
            worker_id: "managed-worker-1234567890".to_string(),
            scope: admission.scope.clone(),
            activation_sha256: admission.activation_sha256.clone(),
            manifest_sha256: admission.manifest_sha256.clone(),
            component_id: "jobs-runner".to_string(),
            role: "managed_runner".to_string(),
            head_revision: admission.head_revision,
            transition_sha256: admission.transition_sha256.clone(),
            artifact_sha256: "c".repeat(64),
            config_schema_sha256: "d".repeat(64),
            migration_set_sha256: "e".repeat(64),
            protocol_set_sha256: "f".repeat(64),
            task_queue_sha256: admission.task_queue_sha256.clone(),
            failure_converter_sha256: admission.failure_converter_sha256.clone(),
            dependency_evidence_sha256: "0".repeat(64),
            activation_expires_at_ms: admission.activation_expires_at_ms,
            instance_epoch: 7,
            next_heartbeat_sequence: 2,
            claimed_at_ms: 1_000,
            replayed: false,
        }
    }

    #[test]
    fn managed_cloud_execution_input_is_present_if_and_only_if_run_is_managed() {
        let input = managed_cloud_execution_test_input();
        assert!(require_managed_cloud_execution_input(false, None)
            .expect("historical omission")
            .is_none());
        assert!(require_managed_cloud_execution_input(true, Some(&input))
            .expect("managed authority")
            .is_some());
        assert!(require_managed_cloud_execution_input(true, None).is_err());
        assert!(require_managed_cloud_execution_input(false, Some(&input)).is_err());
    }

    #[test]
    fn managed_cloud_runner_effect_requires_exact_role_epoch_release_and_worker() {
        let admission = managed_cloud_execution_test_admission();
        let runtime = managed_cloud_execution_test_runtime(&admission);
        let require = |runtime: &ManagedCloudRuntimeInstance,
                       epoch: i64,
                       authenticated_worker_id: &str,
                       volume_worker_id: &str| {
            require_managed_cloud_runner_instance_matches_current(
                runtime,
                &admission,
                "runtime-instance-123456789",
                epoch,
                authenticated_worker_id,
                volume_worker_id,
            )
        };
        require(
            &runtime,
            7,
            "managed-worker-1234567890",
            "managed-worker-1234567890",
        )
        .expect("exact managed runner");
        assert!(require(
            &runtime,
            8,
            "managed-worker-1234567890",
            "managed-worker-1234567890",
        )
        .is_err());
        assert!(require(
            &runtime,
            7,
            "different-worker-12345678",
            "managed-worker-1234567890",
        )
        .is_err());
        let mut wrong_role = runtime.clone();
        wrong_role.role = "workflow_worker".to_string();
        assert!(require(
            &wrong_role,
            7,
            "managed-worker-1234567890",
            "managed-worker-1234567890",
        )
        .is_err());
        let mut stale = runtime;
        stale.head_revision += 1;
        assert!(require(
            &stale,
            7,
            "managed-worker-1234567890",
            "managed-worker-1234567890",
        )
        .is_err());
    }

    #[test]
    fn managed_cloud_latest_resume_reuses_claim_a_without_rewriting_claim_identity() {
        let frozen = managed_cloud_execution_test_admission();
        let binding = managed_cloud_execution_test_binding(frozen.clone());
        let mut current = frozen.clone();
        current.readiness_sha256 = "8".repeat(64);
        current.resolved_at_ms += 1;
        let gateway =
            managed_cloud_request_start_authority(binding.clone(), binding.clone(), current)
                .expect("claim gateway authority")
                .managed_cloud;
        let gateway_bytes = managed_cloud_canonical_json(&gateway).expect("gateway bytes");
        let gateway_authority_base64url =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&gateway_bytes);
        let (release, _, release_memo_base64url, release_sha256) =
            managed_cloud_release_memo(&binding.binding_sha256, &binding.admission)
                .expect("release memo");
        let claim = ManagedCloudExecutionLeaseAuthority {
            managed_cloud: gateway,
            managed_cloud_workflow_request_id: "wfreq-v2-12345678-1234-5abc-8def-123456789abc"
                .to_string(),
            request_command_id: binding.command_id.clone(),
            execution_command_id: binding.command_id.clone(),
            binding_sha256: binding.binding_sha256.clone(),
            release_memo_base64url: release_memo_base64url.clone(),
            release_sha256: release_sha256.clone(),
            managed_cloud_runtime_instance_id: "runtime-instance-123456789".to_string(),
            managed_cloud_runtime_instance_epoch: 7,
            managed_cloud_worker_id: "managed-worker-1234567890".to_string(),
            gateway_authority_base64url: gateway_authority_base64url.clone(),
            gateway_authority_sha256: managed_cloud_sha256(&gateway_bytes),
        };
        let bound = bind_managed_cloud_execution_lease_authority(
            claim.clone(),
            "managed-run-1234567890",
            3,
            &"7".repeat(64),
        )
        .expect("bound claim authority");
        let stored = ManagedCloudStoredExecutionLeaseAuthority {
            workflow_request_id: Some(claim.managed_cloud_workflow_request_id),
            request_command_id: Some(claim.request_command_id),
            execution_command_id: Some(claim.execution_command_id),
            binding_sha256: Some(claim.binding_sha256),
            release_memo_base64url: Some(release_memo_base64url),
            release_sha256: Some(release_sha256.clone()),
            runtime_instance_id: Some(claim.managed_cloud_runtime_instance_id),
            runtime_instance_epoch: Some(claim.managed_cloud_runtime_instance_epoch),
            worker_id: Some(claim.managed_cloud_worker_id),
            gateway_authority_base64url: Some(gateway_authority_base64url),
            gateway_authority_sha256: Some(claim.gateway_authority_sha256),
            lease_authority_sha256: Some(bound.lease_authority_sha256),
        };
        let resume_input = ManagedCloudExecutionLeaseClaimInput {
            workflow_request_id: "wfreq-v2-abcdef12-1234-5abc-8def-123456789abc".to_string(),
            managed_cloud_release: release,
            managed_cloud_release_sha256: release_sha256,
            managed_cloud_runtime_instance_id: "runtime-instance-123456789".to_string(),
            managed_cloud_runtime_instance_epoch: 7,
        };
        let resume = ManagedCloudExecutionCommandAuthority {
            request_command_id: "resume-command-123456789".to_string(),
            execution_binding: binding,
            binding_input: ManagedCloudWorkflowBindingInput {
                command_id: "resume-command-123456789".to_string(),
                account_id: "managed-account-123456".to_string(),
                application_id: "managed-application-123".to_string(),
                run_id: "managed-run-1234567890".to_string(),
                workflow_id: "managed-workflow-123456".to_string(),
                scope: frozen.scope,
            },
        };
        require_managed_cloud_stored_claim_allows_effect(
            &stored,
            &resume,
            &resume_input,
            ManagedCloudStoredEffectContext {
                run_id: "managed-run-1234567890",
                fence: 3,
                lease_token_sha256: &"7".repeat(64),
                authenticated_worker_id: "managed-worker-1234567890",
                volume_worker_id: "managed-worker-1234567890",
            },
        )
        .expect("latest resume may reuse immutable claim A");
        assert_ne!(
            stored.workflow_request_id.as_deref(),
            Some(resume_input.workflow_request_id.as_str())
        );
    }

    #[test]
    fn managed_cloud_execution_authority_distinguishes_exact_and_recovery_admission() {
        let frozen = managed_cloud_execution_test_admission();
        let binding = managed_cloud_execution_test_binding(frozen.clone());
        let mut exact_current = frozen.clone();
        exact_current.readiness_sha256 = "8".repeat(64);
        exact_current.resolved_at_ms = frozen.resolved_at_ms + 1;
        let exact =
            managed_cloud_request_start_authority(binding.clone(), binding.clone(), exact_current)
                .expect("exact-current managed authority");
        assert!(!exact.managed_cloud.authorization.recovery_accepted);
        validate_managed_cloud_gateway_authority(&exact.managed_cloud, &binding)
            .expect("validate exact-current authority");

        let mut successor = frozen;
        successor.head_revision += 1;
        successor.transition_sha256 = "9".repeat(64);
        successor.activation_sha256 = "0".repeat(64);
        successor.manifest_sha256 = "a".repeat(64);
        successor.resolved_at_ms += 2;
        let recovery =
            managed_cloud_request_start_authority(binding.clone(), binding.clone(), successor)
                .expect("recovery-compatible managed authority");
        assert!(recovery.managed_cloud.authorization.recovery_accepted);
        validate_managed_cloud_gateway_authority(&recovery.managed_cloud, &binding)
            .expect("validate recovery authority");
    }

    #[test]
    fn managed_cloud_runtime_measurement_file_bound_is_closed() {
        assert!(managed_cloud_runtime_measurement_file_count_valid(1));
        assert!(managed_cloud_runtime_measurement_file_count_valid(512));
        assert!(!managed_cloud_runtime_measurement_file_count_valid(0));
        assert!(!managed_cloud_runtime_measurement_file_count_valid(513));
    }

    #[test]
    fn managed_cloud_node_paths_match_each_pinned_base_image() {
        assert_eq!(MANAGED_CLOUD_RUNNER_NODE_EXECUTABLE, "/usr/local/bin/node");
        assert_eq!(
            MANAGED_CLOUD_RUNNER_NODE_INVENTORY_PATH,
            "usr/local/bin/node"
        );
        assert_eq!(
            MANAGED_CLOUD_WORKFLOWS_NODE_EXECUTABLE,
            "/usr/local/bin/node"
        );
        assert_eq!(
            MANAGED_CLOUD_WORKFLOWS_NODE_INVENTORY_PATH,
            "usr/local/bin/node"
        );
        assert!(managed_cloud_runtime_measurement_inventory_path(
            "jobs-runner",
            "app/automation"
        ));
        assert!(managed_cloud_runtime_measurement_inventory_path(
            "jobs-runner",
            "app/automation/dist/index.js"
        ));
        assert!(managed_cloud_runtime_measurement_inventory_path(
            "jobs-runner",
            "usr/local/bin/node"
        ));
        assert!(!managed_cloud_runtime_measurement_inventory_path(
            "jobs-runner",
            "app/automation-hostile/dist/index.js"
        ));
    }

    #[test]
    fn managed_cloud_execution_response_and_digest_bind_correlation_identity() {
        let authority = managed_cloud_execution_test_authority();
        let serialized = serde_json::to_value(&authority).expect("serialize execution authority");
        let object = serialized.as_object().expect("execution authority object");
        assert_eq!(object.len(), 5);
        for key in [
            "managedCloud",
            "managedCloudWorkflowRequestId",
            "managedCloudRuntimeInstanceId",
            "managedCloudRuntimeInstanceEpoch",
            "managedCloudWorkerId",
        ] {
            assert!(object.contains_key(key), "missing response key {key}");
        }
        let first = bind_managed_cloud_execution_lease_authority(
            authority.clone(),
            "managed-run-1234567890",
            1,
            &"7".repeat(64),
        )
        .expect("bind execution authority");
        let mut changed = authority;
        changed.managed_cloud_workflow_request_id =
            "wfreq-v2-abcdef12-1234-5abc-8def-123456789abc".to_string();
        changed.managed_cloud_worker_id = "managed-worker-abcdefghij".to_string();
        let second = bind_managed_cloud_execution_lease_authority(
            changed,
            "managed-run-1234567890",
            1,
            &"7".repeat(64),
        )
        .expect("bind changed execution authority");
        assert_ne!(first.lease_authority_sha256, second.lease_authority_sha256);
    }

    #[test]
    fn managed_cloud_irreversible_receipt_rejects_changed_canonical_authority() {
        let authority = managed_cloud_execution_test_authority();
        validate_managed_cloud_irreversible_receipt_authority(&authority)
            .expect("exact irreversible authority");

        let mut changed_gateway = authority.clone();
        changed_gateway.gateway_authority_sha256 = "0".repeat(64);
        assert!(validate_managed_cloud_irreversible_receipt_authority(&changed_gateway).is_err());

        let mut changed_release = authority.clone();
        changed_release.release_sha256 = "1".repeat(64);
        assert!(validate_managed_cloud_irreversible_receipt_authority(&changed_release).is_err());

        let mut changed_worker = authority;
        changed_worker.managed_cloud_worker_id = "short".to_string();
        assert!(validate_managed_cloud_irreversible_receipt_authority(&changed_worker).is_err());
    }

    #[test]
    fn managed_cloud_execution_schema_is_paired_replay_safe_and_token_deletable() {
        let sqlite = include_str!(
            "../../../../infra/sqlite/server-runtime/055_jobs_managed_cloud_release_authority.sql"
        );
        let postgres = include_str!(
            "../../../../infra/postgres/server-runtime/033_jobs_managed_cloud_release_authority.sql"
        );
        let sqlite_bootstrap = include_str!("../mod.rs");
        for schema in [postgres, sqlite_bootstrap] {
            assert!(schema.contains("managed_cloud_worker_id"));
            assert!(schema.contains("managed_cloud_runtime_instance_epoch"));
            assert!(schema.contains("managed_cloud_lease_authority_sha256"));
            assert!(schema.contains("jobs_workflow_cleanup_hard_delete_cascade_tokens"));
            assert!(schema.contains("trg_jobs_managed_cloud_command_hard_delete"));
            assert!(schema.contains("jobs_managed_cloud_irreversible_effect_receipts"));
            assert!(schema.contains("trg_jobs_managed_cloud_irreversible_receipt_insert"));
        }
        assert!(postgres.contains("trg_jobs_managed_cloud_irreversible_receipt_delete"));
        for schema in [sqlite, sqlite_bootstrap] {
            assert!(schema.contains("trg_jobs_managed_cloud_irreversible_receipt_no_delete"));
        }
        for schema in [sqlite, postgres, sqlite_bootstrap] {
            assert!(schema.contains("idx_jobs_managed_cloud_runtime_instance_epoch_worker"));
        }
        let sqlite_runtime_instance_worker_index = sqlite
            .find("idx_jobs_managed_cloud_runtime_instance_epoch_worker")
            .expect("standalone SQLite runtime instance/epoch/worker unique index");
        let sqlite_receipt_table = sqlite
            .find("CREATE TABLE IF NOT EXISTS jobs_managed_cloud_irreversible_effect_receipts")
            .expect("standalone SQLite irreversible receipt table");
        assert!(sqlite_runtime_instance_worker_index < sqlite_receipt_table);
        assert!(postgres.contains("num_nonnulls("));
        assert!(postgres.contains(") IN (0,12)"));
        assert!(postgres.contains("ON DELETE CASCADE"));
        assert!(
            sqlite.contains("DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_command_hard_delete")
        );
        assert!(sqlite_bootstrap
            .contains("DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_execution_lease_insert"));
        assert!(sqlite_bootstrap
            .contains("DROP TRIGGER IF EXISTS trg_jobs_managed_cloud_execution_lease_update"));
        assert!(sqlite_bootstrap.contains("NOT IN (0, 12)"));
        assert!(sqlite.contains("PRIMARY KEY(run_id, fence)"));
        assert!(postgres.contains("PRIMARY KEY(run_id,fence)"));
        assert!(sqlite.contains("lease.phase='click_started'"));
        assert!(postgres.contains("lease.phase='click_started'"));
    }

    #[test]
    fn managed_cloud_execution_effect_is_fresh_without_rewriting_claim_tuple() {
        let source = include_str!("managed_cloud_release_authority.rs");
        let effect = source
            .split_once("pub(crate) fn resolve_managed_cloud_execution_effect_sqlite_tx(")
            .expect("SQLite execution-effect resolver")
            .1
            .split_once("fn validate_new_managed_cloud_runtime_grant(")
            .expect("execution-effect resolver boundary")
            .0;
        assert!(effect.contains("sqlite_managed_cloud_execution_is_managed"));
        assert!(effect.contains("postgres_managed_cloud_execution_is_managed"));
        assert!(effect.contains("require_managed_cloud_stored_claim_allows_effect"));
        assert!(effect.contains("require_sqlite_managed_cloud_runner_instance_ready"));
        assert!(effect.contains("require_postgres_managed_cloud_runner_instance_ready"));
        assert!(!effect.contains("UPDATE jobs_execution_leases"));
        assert!(source.contains("heartbeat.heartbeat_at_ms+requirement.heartbeat_ttl_ms>?15"));
        assert!(source.contains("heartbeat.heartbeat_at_ms+requirement.heartbeat_ttl_ms>$15"));

        let recovery = source
            .split_once("fn sqlite_managed_cloud_recovery_accepts(")
            .expect("SQLite recovery verifier")
            .1
            .split_once("fn postgres_managed_cloud_recovery_accepts(")
            .expect("PostgreSQL recovery verifier boundary")
            .0;
        for revoked_subject in [
            "subject_kind='activation'",
            "subject_kind='manifest'",
            "subject_kind='cohort'",
            "subject_kind='component'",
            "subject_kind='release'",
            "subject_kind='rollback'",
            "subject_kind='trust_policy'",
            "subject_kind='signing_key'",
        ] {
            assert!(recovery.contains(revoked_subject));
        }

        let postgres_effect = effect
            .split_once(
                "pub(crate) fn resolve_managed_cloud_execution_effect_postgres_tx_after_prelock(",
            )
            .expect("PostgreSQL execution-effect resolver")
            .1;
        let discovery = postgres_effect
            .find("false,")
            .expect("nonlocking command discovery");
        let admission = postgres_effect
            .find("require_postgres_managed_cloud_effect_admission_after_prelock")
            .expect("fresh effect admission");
        let locked = postgres_effect[admission..]
            .find("true,")
            .map(|offset| admission + offset)
            .expect("locked command revalidation");
        assert!(discovery < admission && admission < locked);
    }
}
